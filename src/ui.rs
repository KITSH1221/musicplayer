//! 表现层：只负责把 App 的状态画到终端上。
//!
//! 这里没有任何业务逻辑 —— 出了问题永远是"传进来的状态不对"，
//! 而不是"画图函数干了别的事"。

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Gauge, List, Paragraph},
};

use crate::{app::App, util::fmt_time};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [header, body, gauge_area, help] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    // ---- 顶部：状态 + 输出设备信息 ----
    let header_text = format!("{}\n{}", app.status, app.device_info());
    frame.render_widget(
        Paragraph::new(header_text).block(Block::bordered().title(" musicplayer ")),
        header,
    );

    // ---- 中部：歌曲列表，正在播放的那首前面加 ♪ ----
    let items: Vec<String> = app
        .playlist
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mark = if app.playing == Some(i) { "♪ " } else { "  " };
            format!("{mark}{}", t.display_line())
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title(" Playlist "))
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    app.list_state.select(Some(app.cursor));
    frame.render_stateful_widget(list, body, &mut app.list_state);

    // ---- 进度条 ----
    let gauge = match app.progress() {
        Some((pos, total, ratio)) => Gauge::default()
            .block(Block::bordered().title(" Progress "))
            .gauge_style(Style::new().fg(Color::Green))
            .ratio(ratio)
            .label(format!("{} / {}", fmt_time(pos), fmt_time(total))),
        None => Gauge::default()
            .block(Block::bordered().title(" Progress "))
            .ratio(0.0)
            .label("--:-- / --:--"),
    };
    frame.render_widget(gauge, gauge_area);

    // ---- 底部：按键提示 ----
    frame.render_widget(
        Paragraph::new("Enter play  ·  Space pause  ·  ←/→ ±5s  ·  ↑/↓ select  ·  q quit"),
        help,
    );
}
