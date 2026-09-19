//! musicplayer —— 终端音乐播放器
//!
//! 模块分层：
//! - `library` 数据层：Track 与目录扫描
//! - `audio`   音频后端：封装 rodio
//! - `app`     状态层：App 状态、键盘处理、每帧推进
//! - `ui`      表现层：把状态画到终端
//! - `util`    小工具

mod app;
mod audio;
mod library;
mod ui;
mod util;
mod waveform;

use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
};
use std::{env, path::PathBuf, time::Duration};

use crate::{
    app::App,
    library::{Track, scan_audio},
    util::BoxError,
};

fn main() -> Result<(), BoxError> {
    let target = env::args()
        .nth(1)
        .ok_or("usage: musicplayer <file or directory>")?;
    let target = PathBuf::from(target);

    let playlist = if target.is_dir() {
        scan_audio(&target)?
    } else {
        vec![Track::new(target)]
    };

    if playlist.is_empty() {
        eprintln!("no audio files found");
        std::process::exit(1);
    }

    // 先建音频。失败时终端还没被改动，错误能正常打印
    let mut app = App::new(playlist)?;

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore(); // 即使 run 出错，也必须先恢复终端再返回
    result?;
    Ok(())
}

/// 事件循环：推进状态 → 重绘 → 处理按键
fn run(terminal: &mut DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    loop {
        app.tick();
        terminal.draw(|frame| ui::draw(frame, app))?;

        // poll 带超时：没有按键也要转一圈，界面才会刷新
        if event::poll(Duration::from_millis(100))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.handle_key(key) {
                break;
            }
        }
    }
    Ok(())
}
