use lofty::prelude::*;
use lofty::read_from_path;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, List, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

fn fmt_time(d: Duration) -> String {
    format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
}

struct Track {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    duration: Option<Duration>,
}

impl Track {
    fn from_path(path: PathBuf) -> Self {
        // ① 先用文件名做兜底标题——注意必须在 path 被移进结构体之前算
        let fallback = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut track = Track {
            path,
            title: fallback,
            artist: "unknown".to_string(),
            album: "unknown".to_string(),
            duration: None,
        };
        if let Ok(tagged) = read_from_path(&track.path) {
            track.duration = Some(tagged.properties().duration());
            // ② 从标签中提取标题、艺术家、专辑
            if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
                if let Some(t) = tag.title() {
                    track.title = t.into_owned();
                }
                if let Some(a) = tag.artist() {
                    track.artist = a.into_owned();
                }
                if let Some(a) = tag.album() {
                    track.album = a.into_owned();
                }
            }
        }
        track
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("use musicplayer <path>");
            std::process::exit(1);
        }
    };
    let target = PathBuf::from(path);
    let playlist = if target.is_dir() {
        scan_audio(&target)?
    } else {
        vec![Track::from_path(target)]
    };
    if playlist.is_empty() {
        eprintln!("no audio files found");
        std::process::exit(1);
    }
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &playlist);
    ratatui::restore(); // 无论如何都要恢复终端
    result?;
    Ok(())
}

fn run(terminal: &mut DefaultTerminal, playlist: &[Track]) -> std::io::Result<()> {
    let mut cursor = 0;
    loop {
        terminal.draw(|frame| draw(frame, playlist, cursor))?;
        if event::poll(Duration::from_millis(100))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Down | KeyCode::Char('j') => {
                    if cursor + 1 < playlist.len() {
                        cursor += 1
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    cursor = cursor.saturating_sub(1);
                }
                KeyCode::Home => cursor = 0,
                KeyCode::End => cursor = playlist.len().saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame, playlist: &[Track], cursor: usize) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    frame.render_widget(
        Paragraph::new("musicplayer").block(Block::bordered().title("player")),
        header,
    );
    let items: Vec<String> = playlist
        .iter()
        .map(|t| {
            let dur = t.duration.map(fmt_time).unwrap_or_else(|| "--:--".into());
            format!("{} — {}  [{}]", t.artist, t.title, dur)
        })
        .collect();
    let list = List::new(items)
        .block(Block::bordered().title(" PlayList "))
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(Some(cursor));
    frame.render_stateful_widget(list, body, &mut state);

    frame.render_widget(Paragraph::new("[↑/↓ 或 k/j] select    [q] quit"), footer);
}

fn is_audio(path: &Path) -> bool {
    // match macro => matches!(expression,pattern)
    matches! {
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
        Some("mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac")
    }
}
fn scan_audio(dir: &Path) -> std::io::Result<Vec<Track>> {
    let mut tracks = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_audio(&path) {
            tracks.push(Track::from_path(path));
        }
    }
    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(tracks)
}
