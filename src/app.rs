//! 应用状态层：把"界面状态"和"播放状态"装在一起，
//! 并提供键盘处理和每帧推进。

use ratatui::{crossterm::event::KeyCode, widgets::ListState};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use crate::{
    audio::Audio,
    library::{Track, TrackMeta, read_meta},
    util::BoxError,
};

pub struct App {
    // ---- 界面状态：ui 层需要读，所以是 pub(crate) ----
    pub(crate) playlist: Vec<Track>,
    pub(crate) cursor: usize,
    /// 跨帧存活，列表才能正确滚动（每帧新建会丢掉滚动偏移）
    pub(crate) list_state: ListState,
    /// 正在播放的曲目下标
    pub(crate) playing: Option<usize>,
    /// 顶部状态文字
    pub(crate) status: String,

    // ---- 音频后端：保持私有，只通过下面两个方法暴露 ----
    audio: Audio,

    // ---- 后台标签加载：接收端 ----
    meta_rx: Receiver<(usize, TrackMeta)>,
}

impl App {
    pub fn new(playlist: Vec<Track>) -> Result<Self, BoxError> {
        let meta_rx = spawn_meta_loader(&playlist);

        Ok(App {
            playlist,
            cursor: 0,
            list_state: ListState::default(),
            playing: None,
            status: "Enter: play  ·  Space: pause".to_string(),
            audio: Audio::new()?,
            meta_rx,
        })
    }

    // ---------- 播放控制 ----------

    /// 播放第 idx 首。
    ///
    /// 这里故意不返回 `Result`：在 TUI 里，错误应该变成界面上的提示，
    /// 而不是让整个程序退出——用户点了一首坏文件，不该有这种代价。
    fn play_index(&mut self, idx: usize) {
        // 后台线程可能还没排到这首，先同步补读，这样状态栏立刻就是对的
        self.playlist[idx].ensure_meta();
        let label = self.playlist[idx].label();

        // `&mut self.audio` 和 `&self.playlist` 借用的是不同字段，
        // 编译器允许这种"不相交借用"，所以不需要 clone 路径
        match self.audio.play(self.playlist[idx].path()) {
            Ok(()) => {
                self.playing = Some(idx);
                self.status = format!("▶ {label}");
            }
            Err(e) => {
                self.status = format!("⚠ Cannot open {}: {e}", self.playlist[idx].path().display());
            }
        }
    }

    /// 光标移动后立刻读一次当前这首的标签，不用等后台线程
    fn load_cursor_track(&mut self) {
        let idx = self.cursor;
        if let Some(track) = self.playlist.get_mut(idx) {
            track.ensure_meta();
        }
    }

    // ---------- 每帧更新 ----------

    /// 每帧调用一次：收下后台读好的标签，并处理"一首歌自然播完"。
    pub fn tick(&mut self) {
        self.apply_loaded_meta();

        let Some(idx) = self.playing else { return };
        if !self.audio.is_finished() {
            return;
        }

        // 队列空了 → 当前曲目结束，自动下一首
        if idx + 1 < self.playlist.len() {
            self.play_index(idx + 1);
        } else {
            self.playing = None;
            self.status = "Playback finished".to_string();
        }
    }

    /// 把后台线程已读好的标签应用到列表上。
    ///
    /// 非阻塞的 `try_recv`：有就收，没有就继续画下一帧。
    fn apply_loaded_meta(&mut self) {
        while let Ok((idx, meta)) = self.meta_rx.try_recv() {
            if let Some(track) = self.playlist.get_mut(idx) {
                track.set_meta(meta);
            }
        }
    }

    // ---------- 键盘 ----------

    /// 返回 true 表示要退出。
    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,

            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor + 1 < self.playlist.len() {
                    self.cursor += 1;
                    self.load_cursor_track();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                self.load_cursor_track();
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.load_cursor_track();
            }
            KeyCode::End => {
                self.cursor = self.playlist.len().saturating_sub(1);
                self.load_cursor_track();
            }

            KeyCode::Enter => self.play_index(self.cursor),
            KeyCode::Char(' ') => {
                if self.playing.is_some() {
                    self.audio.toggle_pause();
                }
            }
            KeyCode::Left => self.audio.seek_by(-5.0),
            KeyCode::Right => self.audio.seek_by(5.0),

            _ => {}
        }
        false
    }

    /// 当且仅当正在播放时，返回 (当前位置, 总时长, 进度)
    pub fn progress(&self) -> Option<(Duration, Duration, f64)> {
        self.playing?; // Option 的 ? ：None 就直接返回 None
        self.audio.progress()
    }

    pub fn device_info(&self) -> String {
        self.audio.device_info()
    }
}

/// 启动后台线程，逐个读标签，通过 channel 把结果送回主线程。
///
/// 线程是 detached 的：主线程退出时进程结束，它会一起消失，
/// 所以不需要 join。
fn spawn_meta_loader(playlist: &[Track]) -> Receiver<(usize, TrackMeta)> {
    let (tx, rx) = mpsc::channel();

    let paths: Vec<PathBuf> = playlist.iter().map(|t| t.path().to_path_buf()).collect();

    thread::spawn(move || {
        for (idx, path) in paths.into_iter().enumerate() {
            // send 失败说明接收端已经被 drop（App 退出了），别再白干活
            if tx.send((idx, read_meta(&path))).is_err() {
                break;
            }
        }
    });

    rx
}
