//! 音频后端：把 rodio 包成一个 `Audio` 类型。
//!
//! 上层（app）只跟 `Audio` 打交道，不需要知道 rodio 的存在。
//! 这样做的好处：以后换播放库，只改这一个文件。

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::{fs::File, path::Path, time::Duration};

use crate::{
    util::BoxError,
    waveform::{self, AudioTap, Waveform},
};

pub struct Audio {
    player: Player,

    /// ⚠️ 这个字段看起来"没被用到"，但绝不能删。
    /// 它拥有底层的 cpal 音频流，被 drop 掉声音就停。
    /// （`device_info()` 会读它，所以也不会触发 dead_code 警告。）
    sink: MixerDeviceSink,

    /// 当前曲目的总时长
    total: Option<Duration>,

    /// 波形分析器。每首歌换一个新的，绑定到那首歌的环形缓冲
    waveform: Waveform,
}

impl Audio {
    /// 打开默认输出设备，失败会返回错误。
    pub fn new() -> Result<Self, BoxError> {
        let mut sink = DeviceSinkBuilder::open_default_sink()?;
        sink.log_on_drop(false);
        let player = Player::connect_new(sink.mixer());

        // 还没开始播，先给一个没有音频流的分析器（波形会一直是一条直线）
        let (_producer, consumer) = waveform::ring();

        Ok(Audio {
            player,
            sink,
            total: None,
            waveform: Waveform::new(consumer),
        })
    }

    /// 播放一个文件，替换掉当前正在播放的内容。
    ///
    /// 注意：`stop()` 只是标记停止，`append()` 之后会重新激活，
    /// 所以"停上一首"和"放下一首"可以这样连着写。
    pub fn play(&mut self, path: &Path) -> Result<(), BoxError> {
        let file = File::open(path)?;
        // Decoder::try_from(File) 自动设置 byte_len + seekable，所以能 seek
        let source = Decoder::try_from(file)?;

        // 给这首歌建一个新的环形缓冲，把解码器包上一层分流器
        let (producer, consumer) = waveform::ring();
        let tap = AudioTap::new(source, producer);

        self.total = tap.total_duration();
        self.player.stop();
        self.player.append(tap);
        self.player.play(); // 清掉可能残留的暂停状态

        // 换掉旧的分析器（旧的那个会连同旧环形缓冲一起被丢弃）
        self.waveform = Waveform::new(consumer);
        Ok(())
    }

    /// 每帧调用一次，推进波形计算。
    pub fn update_waveform(&mut self) {
        self.waveform.update();
    }

    /// 波形的振幅包络，每点 0.0~1.0
    pub fn waveform(&self) -> &[f32] {
        self.waveform.levels()
    }

    pub fn toggle_pause(&mut self) {
        if self.player.is_paused() {
            self.player.play();
        } else {
            self.player.pause();
        }
    }

    /// 相对当前位置跳转 seconds 秒（可正可负）。
    pub fn seek_by(&mut self, seconds: f64) {
        let Some(total) = self.total else { return };
        let pos = self.player.get_pos();

        let target = if seconds >= 0.0 {
            (pos + Duration::from_secs_f64(seconds)).min(total)
        } else {
            pos.saturating_sub(Duration::from_secs_f64(-seconds))
        };

        let _ = self.player.try_seek(target);
    }

    /// 队列空了 = 当前曲目放完了
    pub fn is_finished(&self) -> bool {
        self.player.empty()
    }

    /// 返回 (当前位置, 总时长, 进度 0.0~1.0)。没有在播放时返回 None。
    pub fn progress(&self) -> Option<(Duration, Duration, f64)> {
        let total = self.total?;
        if total.is_zero() {
            return None;
        }
        let pos = self.player.get_pos().min(total);
        let ratio = (pos.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0);
        Some((pos, total, ratio))
    }

    /// 输出设备信息，例如 `48000 Hz · 2 ch`
    pub fn device_info(&self) -> String {
        let cfg = self.sink.config();
        format!("{} Hz · {} ch", cfg.sample_rate(), cfg.channel_count())
    }
}
