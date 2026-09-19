//! 波形可视化：把正在播放的音频分流一份出来，算出一条**振幅包络**。
//!
//! # 数据流
//!
//! ```text
//!   解码器 ──► AudioTap ──► Player ──► Mixer ──► cpal 回调 ──► 声卡
//!                 │
//!                 │ push（无锁）
//!                 ▼
//!           rtrb 环形缓冲
//!                 │ pop（每帧一次）
//!                 ▼
//!           Waveform：取最近一段 → 分桶取峰值 → 时间平滑
//!                 │
//!                 ▼
//!           ui：用盲文点阵画成一条平滑的波浪
//! ```
//!
//! 注意这里算的是**包络**（每个小段里的最大振幅），不是原始采样点。
//! 直接把 44100Hz 的采样点丢给几十列宽的屏幕会得到一团抖动的噪点；
//! 取峰值包络才有平滑流动的"波浪"观感。
//!
//! # 为什么要用环形缓冲
//!
//! `AudioTap::next` 跑在 **音频回调线程** 上。那条线程一旦卡住，
//! 声音就会爆音甚至断掉。所以它里面**不能加锁、不能分配内存、不能打印**。
//! `rtrb` 是无锁的单生产者单消费者队列：满了就丢样本，绝不等待。

use std::time::Duration;

use rodio::{ChannelCount, Sample, SampleRate, Source, source::SeekError};
use rtrb::{Consumer, Producer, RingBuffer};

/// 环形缓冲容量（单声道样本数）。
/// 44.1kHz 下一帧（100ms）约 4410 个样本，这里留了 ~370ms 的余量。
const RING_CAPACITY: usize = 16_384;

/// 波形的点数。UI 再按终端宽度重采样，所以这里和屏幕宽度解耦。
pub const POINTS: usize = 256;

/// 每帧参与计算的最大样本数（约 93ms @44.1kHz）
const WINDOW_SAMPLES: usize = 4096;

/// 显示增益。音乐峰值一般到不了 1.0，略微放大一点看起来更饱满；
/// 但不要设得太大，否则包络线会顶满整个区域、失去呼吸感（1.0 左右刚好）。
const GAIN: f32 = 1.0;

/// 时间平滑系数：上升快、回落慢，避免波形每帧乱抖。
const ATTACK: f32 = 0.60;
const RELEASE: f32 = 0.25;

/// 完全静音时每帧的衰减系数
const DECAY: f32 = 0.85;

/// 建立一对环形缓冲（生产者给音频线程，消费者给 UI 线程）。
pub fn ring() -> (Producer<Sample>, Consumer<Sample>) {
    RingBuffer::new(RING_CAPACITY)
}

// ============================================================
//  分流器：实现成 rodio 的 Source，插在解码器后面
// ============================================================

/// 把源分成两路：音频原样往上传给播放器，同时把**单声道**样本塞进环形缓冲。
///
/// 做成单声道是因为波形只关心振幅随时间的变化；左右声道交错塞进去
/// 会让包络算错（相邻样本来自不同声道）。
pub struct AudioTap<S> {
    inner: S,
    producer: Producer<Sample>,
    channels: usize,
    /// 当前帧累加值（用来把多声道平均成单声道）
    accum: f32,
    /// 当前帧已经累加了几个声道
    count: usize,
}

impl<S: Source> AudioTap<S> {
    pub fn new(inner: S, producer: Producer<Sample>) -> Self {
        let channels = inner.channels().get() as usize;
        AudioTap {
            inner,
            producer,
            channels: channels.max(1),
            accum: 0.0,
            count: 0,
        }
    }
}

impl<S: Source> Iterator for AudioTap<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let sample = self.inner.next()?;

        self.accum += sample;
        self.count += 1;
        if self.count >= self.channels {
            // push 失败 = 环形缓冲满了（UI 没跟上）。丢掉即可，绝不能等待
            let _ = self.producer.push(self.accum / self.count as f32);
            self.accum = 0.0;
            self.count = 0;
        }

        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for AudioTap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    /// ⚠️ 必须转发！`Source::try_seek` 有默认实现，那个实现直接返回
    /// `NotSupported`。不转发的话，套上一层 tap 就会让 seek 整体失效。
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

// ============================================================
//  分析器：消费端，每帧算一次包络
// ============================================================

pub struct Waveform {
    consumer: Consumer<Sample>,
    /// 还没凑够窗口的样本
    pending: Vec<f32>,
    /// 输出：`POINTS` 个 0.0~1.0 的包络值
    levels: Vec<f32>,
}

impl Waveform {
    pub fn new(consumer: Consumer<Sample>) -> Self {
        Waveform {
            consumer,
            pending: Vec::with_capacity(WINDOW_SAMPLES * 2),
            levels: vec![0.0; POINTS],
        }
    }

    /// 每帧调用一次：把缓冲里的样本取出来，更新包络。
    pub fn update(&mut self) {
        // 借用技巧：先把 pending 整个"拿走"，这样后面调用 &mut self 的方法
        // 就不会和它的借用冲突了。结束时还回去，容量也保住了。
        let mut pending = std::mem::take(&mut self.pending);

        while let Ok(sample) = self.consumer.pop() {
            pending.push(sample);
        }

        if pending.len() >= POINTS {
            // 只分析最近的一个窗口
            let start = pending.len().saturating_sub(WINDOW_SAMPLES);
            self.compute(&pending[start..]);
            pending.clear();
        } else {
            // 样本不够（暂停中、刚开始、放完了），让波形自然回落成一条直线
            self.decay();
        }

        self.pending = pending;
    }

    pub fn levels(&self) -> &[f32] {
        &self.levels
    }

    /// 把一段样本分桶，每桶取峰值振幅，再按时间平滑。
    fn compute(&mut self, samples: &[f32]) {
        let n = samples.len();
        debug_assert!(n >= POINTS);

        for (i, level) in self.levels.iter_mut().enumerate() {
            let a = i * n / POINTS;
            let b = ((i + 1) * n / POINTS).max(a + 1).min(n);

            // 取绝对值最大的那个样本 —— 峰值包络
            let peak = samples[a..b].iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let target = (peak * GAIN).min(1.0);

            // 上升比回落快：视觉上更有"跟手"的感觉，又不会闪
            let k = if target > *level { ATTACK } else { RELEASE };
            *level += (target - *level) * k;
        }
    }

    fn decay(&mut self) {
        for level in &mut self.levels {
            *level *= DECAY;
            if *level < 0.001 {
                *level = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_forwards_audio_and_fills_ring() {
        use rodio::buffer::SamplesBuffer;

        let (producer, mut consumer) = RingBuffer::<f32>::new(1024);
        let channels = ChannelCount::new(2).unwrap();
        let sample_rate = SampleRate::new(44_100).unwrap();

        // 200 个样本 = 100 个立体声帧
        let data: Vec<f32> = (0..200).map(|i| i as f32 / 200.0).collect();
        let source = SamplesBuffer::new(channels, sample_rate, data);

        let forwarded: Vec<f32> = AudioTap::new(source, producer).collect();

        // 原样转发，一个不多一个不少
        assert_eq!(forwarded.len(), 200);

        // 环形缓冲里是降混后的单声道：100 个
        let mut mono = Vec::new();
        while let Ok(s) = consumer.pop() {
            mono.push(s);
        }
        assert_eq!(mono.len(), 100);

        // 降混是取平均，不是取左声道
        assert!((mono[0] - (0.0 + 0.005) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn envelope_tracks_amplitude() {
        let (_producer, consumer) = RingBuffer::<f32>::new(64);
        let mut wave = Waveform::new(consumer);

        // 先把平滑拉到稳态：反复喂满幅方波
        let loud = vec![1.0f32; WINDOW_SAMPLES];
        for _ in 0..50 {
            wave.compute(&loud);
        }
        // 增益 1.6，满幅会被 clamp 到 1.0
        assert!(
            wave.levels().iter().all(|&l| l > 0.95),
            "满幅信号应该让所有点接近 1.0"
        );

        // 换成很轻的信号，包络应该明显下来
        let quiet = vec![0.05f32; WINDOW_SAMPLES];
        for _ in 0..50 {
            wave.compute(&quiet);
        }
        assert!(
            wave.levels().iter().all(|&l| l < 0.2),
            "轻信号应该让包络降下来"
        );
    }

    #[test]
    fn envelope_stays_in_range() {
        let (_producer, consumer) = RingBuffer::<f32>::new(64);
        let mut wave = Waveform::new(consumer);

        // 故意给超出范围的样本（理论上不该发生，但要保证不会画到屏幕外）
        let hot = vec![9.0f32; WINDOW_SAMPLES];
        for _ in 0..50 {
            wave.compute(&hot);
        }
        assert!(wave.levels().iter().all(|&l| (0.0..=1.0).contains(&l)));

        let negative = vec![-9.0f32; WINDOW_SAMPLES];
        for _ in 0..50 {
            wave.compute(&negative);
        }
        assert!(wave.levels().iter().all(|&l| (0.0..=1.0).contains(&l)));
    }

    #[test]
    fn silence_brings_the_wave_back_to_a_flat_line() {
        let (_producer, consumer) = RingBuffer::<f32>::new(64);
        let mut wave = Waveform::new(consumer);

        let loud = vec![1.0f32; WINDOW_SAMPLES];
        for _ in 0..50 {
            wave.compute(&loud);
        }
        assert!(wave.levels().iter().any(|&l| l > 0.5));

        // 没有新样本时反复 update，波形应该衰减成一条直线
        for _ in 0..200 {
            wave.update();
        }
        assert!(wave.levels().iter().all(|&l| l == 0.0));
    }

    #[test]
    fn a_burst_only_lifts_the_matching_points() {
        let (_producer, consumer) = RingBuffer::<f32>::new(64);
        let mut wave = Waveform::new(consumer);

        // 只在窗口的后半段放一个脉冲
        let mut buf = vec![0.0f32; WINDOW_SAMPLES];
        let pump = WINDOW_SAMPLES * 3 / 4;
        buf[pump] = 1.0;

        wave.compute(&buf);

        // 前 1/4 应该是安静的，脉冲附近应该被抬起来
        let first_quarter = &wave.levels()[..POINTS / 4];
        assert!(first_quarter.iter().all(|&l| l == 0.0));

        let around = &wave.levels()[POINTS / 2..];
        assert!(around.iter().any(|&l| l > 0.5));
    }
}
