//! 频谱分析：把正在播放的音频分流一份出来，做 FFT，得到每个频段的能量。
//!
//! # 数据流
//!
//! ```text
//!   解码器 ──► SpectrumTap ──► Player ──► Mixer ──► cpal 回调 ──► 声卡
//!                  │
//!                  │ push（无锁）
//!                  ▼
//!            rtrb 环形缓冲
//!                  │ pop（每帧一次）
//!                  ▼
//!            Analyzer：加窗 → FFT → 分频段 → 回落平滑
//! ```
//!
//! # 为什么要用环形缓冲
//!
//! `SpectrumTap::next` 跑在 **音频回调线程** 上。那条线程一旦卡住，
//! 声音就会爆音甚至断掉。所以它里面**不能加锁、不能分配内存、不能打印**。
//! `rtrb` 是一个无锁的单生产者单消费者队列，正好满足这个约束：
//! 满了就丢样本（`push` 返回 Err），绝不等待。

use std::{sync::Arc, time::Duration};

use rodio::{ChannelCount, Sample, SampleRate, Source, source::SeekError};
use rtrb::{Consumer, Producer, RingBuffer};
use rustfft::{Fft, FftPlanner, num_complex::Complex};

/// 环形缓冲容量（单声道样本数）。
/// 44.1kHz 下一帧（100ms）约 4410 个样本，这里留了 ~370ms 的余量。
const RING_CAPACITY: usize = 16_384;

/// FFT 窗口大小。越大频率分辨率越高、时间分辨率越低。
pub const FFT_SIZE: usize = 2048;

/// 显示成多少根柱子
pub const BANDS: usize = 48;

/// 频谱覆盖范围（Hz）。人耳对频率的感知是对数的，所以按对数分频段。
const F_MIN: f32 = 40.0;
const F_MAX: f32 = 16_000.0;

/// 每帧的回落系数：越小掉得越快。用来避免柱子疯狂闪烁。
const DECAY: f32 = 0.80;

/// 动态范围，低于这个分贝当没有
const FLOOR_DB: f32 = -60.0;

/// 建立一对环形缓冲（生产者给音频线程，消费者给 UI 线程）。
pub fn ring() -> (Producer<Sample>, Consumer<Sample>) {
    RingBuffer::new(RING_CAPACITY)
}

// ============================================================
//  分流器：实现成 rodio 的 Source，插在解码器后面
// ============================================================

/// 把源分成两路：音频原样往上传给播放器，同时把**单声道**样本塞进环形缓冲。
///
/// 做成单声道是因为 FFT 只对单个时间序列有意义。如果直接把左右声道
/// 交错塞进去，频率轴会被拉成两倍，还会出现镜像干扰。
pub struct SpectrumTap<S> {
    inner: S,
    producer: Producer<Sample>,
    channels: usize,
    /// 当前帧累加值（用来把多声道平均成单声道）
    accum: f32,
    /// 当前帧已经累加了几个声道
    count: usize,
}

impl<S: Source> SpectrumTap<S> {
    pub fn new(inner: S, producer: Producer<Sample>) -> Self {
        let channels = inner.channels().get() as usize;
        SpectrumTap {
            inner,
            producer,
            channels: channels.max(1),
            accum: 0.0,
            count: 0,
        }
    }
}

impl<S: Source> Iterator for SpectrumTap<S> {
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

impl<S: Source> Source for SpectrumTap<S> {
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
//  分析器：消费端，每帧做一次 FFT
// ============================================================

pub struct Analyzer {
    consumer: Consumer<Sample>,
    sample_rate: f32,

    /// 还没够一个窗口的样本
    pending: Vec<f32>,

    /// Hann 窗，用来减少频谱泄漏
    window: Vec<f32>,

    /// FFT 输入/输出缓冲
    fft_buf: Vec<Complex<f32>>,
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex<f32>>,

    /// 上半边幅度谱（FFT 结果是对称的，只需一半）
    mags: Vec<f32>,

    /// 输出：每根柱子的高度 0.0~1.0
    levels: Vec<f32>,
}

impl Analyzer {
    pub fn new(consumer: Consumer<Sample>, sample_rate: f32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch_len = fft.get_inplace_scratch_len();

        // Hann 窗：w[i] = 0.5 - 0.5*cos(2πi/(N-1))
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                let x = std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32;
                0.5 - 0.5 * x.cos()
            })
            .collect();

        Analyzer {
            consumer,
            sample_rate,
            pending: Vec::with_capacity(FFT_SIZE * 4),
            window,
            fft_buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            fft,
            scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            mags: vec![0.0; FFT_SIZE / 2],
            levels: vec![0.0; BANDS],
        }
    }

    /// 每帧调用一次：把缓冲里的样本取出来，做一次 FFT，更新柱子高度。
    pub fn update(&mut self) {
        // 借用技巧：先把 pending 整个"拿走"，这样后面调用 &mut self 的方法
        // 就不会和它的借用冲突了。结束时还回去，容量也保住了。
        let mut pending = std::mem::take(&mut self.pending);

        // 把这段时间积累的样本全部取出来（不留残余，避免越积越多）
        while let Ok(sample) = self.consumer.pop() {
            pending.push(sample);
        }

        if pending.len() >= FFT_SIZE {
            let start = pending.len() - FFT_SIZE; // 只分析最近的一个窗口
            self.compute_bands(&pending[start..]);
            pending.clear();
        } else {
            // 样本不够（暂停中、刚开始、放完了），只让柱子自然回落
            self.decay();
        }

        self.pending = pending;
    }

    pub fn levels(&self) -> &[f32] {
        &self.levels
    }

    /// 对恰好 `FFT_SIZE` 个样本做一次分析，更新 `levels`。
    fn compute_bands(&mut self, samples: &[f32]) {
        debug_assert_eq!(samples.len(), FFT_SIZE);

        // ① 加窗
        for (i, &s) in samples.iter().enumerate() {
            self.fft_buf[i] = Complex::new(s * self.window[i], 0.0);
        }

        // ② FFT
        self.fft
            .process_with_scratch(&mut self.fft_buf, &mut self.scratch);

        // ③ 幅度谱。除以 N/2 是为了归一化：满幅正弦应该得到约 1.0
        let norm = 2.0 / (FFT_SIZE as f32 * 0.5);
        for (i, c) in self.fft_buf[..FFT_SIZE / 2].iter().enumerate() {
            self.mags[i] = c.norm() * norm;
        }

        // ④ 把线性排列的频率 bin 归并成对数排列的柱子
        let bin_hz = self.sample_rate / FFT_SIZE as f32;
        let nbins = self.mags.len();
        let ratio = F_MAX / F_MIN;

        for band in 0..BANDS {
            let f0 = F_MIN * ratio.powf(band as f32 / BANDS as f32);
            let f1 = F_MIN * ratio.powf((band + 1) as f32 / BANDS as f32);

            let i0 = ((f0 / bin_hz) as usize).min(nbins - 1);
            let i1 = (((f1 / bin_hz) as usize) + 1).clamp(i0 + 1, nbins);

            // 取这段里的峰值：峰值比平均值更符合"看得见"的直觉
            let peak = self.mags[i0..i1].iter().copied().fold(0.0, f32::max);

            // ⑤ 转成 dB 再映射到 0..1，这样小信号也看得见
            let db = 20.0 * peak.max(1e-6).log10();
            let target = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);

            // ⑥ 平滑：上升立刻跟上，回落慢一点
            self.levels[band] = target.max(self.levels[band] * DECAY);
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

        let forwarded: Vec<f32> = SpectrumTap::new(source, producer).collect();

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
    fn peak_lands_in_the_expected_band() {
        let (_producer, consumer) = RingBuffer::<f32>::new(64);
        let mut analyzer = Analyzer::new(consumer, 44_100.0);

        // 生成 1000Hz 满幅正弦
        let freq = 1000.0;
        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|i| (std::f32::consts::TAU * freq * i as f32 / 44_100.0).sin())
            .collect();

        analyzer.compute_bands(&samples);

        let peak = analyzer
            .levels
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        // 1000Hz 应该落在对数频段的哪个位置
        let expected = ((freq / F_MIN).ln() / (F_MAX / F_MIN).ln() * BANDS as f32) as usize;

        assert!(
            peak.abs_diff(expected) <= 2,
            "峰值落在第 {peak} 根柱子，预期接近第 {expected} 根"
        );
    }

    #[test]
    fn decay_brings_levels_back_to_zero() {
        let (_producer, consumer) = RingBuffer::<f32>::new(64);
        let mut analyzer = Analyzer::new(consumer, 44_100.0);

        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|i| (std::f32::consts::TAU * 1000.0 * i as f32 / 44_100.0).sin())
            .collect();
        analyzer.compute_bands(&samples);
        assert!(analyzer.levels().iter().any(|&l| l > 0.5));

        // 没有新样本时反复 update，柱子应该衰减到 0
        for _ in 0..200 {
            analyzer.update();
        }
        assert!(analyzer.levels().iter().all(|&l| l == 0.0));
    }
}
