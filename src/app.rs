//! 应用状态层：把"界面状态"和"播放状态"装在一起，
//! 并提供键盘处理和每帧推进。

use ratatui::{crossterm::event::KeyCode, widgets::ListState};
use std::time::Duration;

use crate::audio::Audio;
use crate::library::Track;
use crate::util::BoxError;

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
}

impl App {
    pub fn new(playlist: Vec<Track>) -> Result<Self, BoxError> {
        Ok(App {
            playlist,
            cursor: 0,
            list_state: ListState::default(),
            playing: None,
            status: "Enter: play  ·  Space: pause".to_string(),
            audio: Audio::new()?,
        })
    }

    /// 播放第 idx 首。
    ///
    /// 这里故意不返回 `Result`：在 TUI 里，错误应该变成界面上的提示，
    /// 而不是让整个程序退出——用户点了一首坏文件，不该有这种代价。
    fn play_index(&mut self, idx: usize) {
        let label = self.playlist[idx].label();

        // `&mut self.audio` 和 `&self.playlist` 借用的是不同字段，
        // 编译器允许这种"不相交借用"，所以不需要 clone 路径
        match self.audio.play(self.playlist[idx].path()) {
            Ok(()) => {
                self.playing = Some(idx);
                self.status = format!("▶ {label}");
            }
            Err(e) => {
                self.status = format!("Cannot open {}: {e}", self.playlist[idx].path().display());
            }
        }
    }

    /// 每帧调用一次：处理"一首歌自然播完"。
    pub fn tick(&mut self) {
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

    /// 返回 true 表示要退出。
    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,

            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor + 1 < self.playlist.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.playlist.len().saturating_sub(1),

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
