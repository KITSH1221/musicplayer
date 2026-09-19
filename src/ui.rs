//! 表现层：只负责把 App 的状态画到终端上。
//!
//! 这里没有任何业务逻辑 —— 出了问题永远是"传进来的状态不对"，
//! 而不是"画图函数干了别的事"。

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Gauge, List, ListItem, Paragraph},
};

use crate::{app::App, util::fmt_time};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [header, body, spectrum_area, gauge_area, help] = Layout::vertical([
        Constraint::Length(4), // 边框占 2 行，内容区剩 2 行，刚好放状态 + 设备信息
        Constraint::Min(1),
        Constraint::Length(8), // 频谱
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
    // 标签还没读到的行先显示文件名，并用暗色区分
    let items: Vec<ListItem> = app
        .playlist
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mark = if app.playing == Some(i) { "♪ " } else { "  " };
            let item = ListItem::new(format!("{mark}{}", t.display_line()));
            if t.is_loaded() {
                item
            } else {
                item.style(Style::new().fg(Color::DarkGray))
            }
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Playlist ")
                .title_top(load_badge(app)),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    app.list_state.select(Some(app.cursor));
    frame.render_stateful_widget(list, body, &mut app.list_state);

    // ---- 频谱 ----
    draw_spectrum(frame, app.spectrum(), spectrum_area);

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

/// 半格字符：下标 = 这一格从底部算起被填了 1/8 的几份
const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// 手绘频谱。
///
/// 没有用 `BarChart`：那个部件是给"带坐标轴的统计图"设计的，带宽、间隔、
/// 标签都不适合频谱。直接写 buffer 反而更短、更可控。
fn draw_spectrum(frame: &mut Frame, levels: &[f32], area: Rect) {
    let block = Block::bordered().title(" Spectrum ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let cols = inner.width as usize;
    let rows = inner.height as usize;
    let sub_rows = rows * 8; // 每格 8 级，所以总共有 rows*8 级高度
    let buf = frame.buffer_mut();

    for c in 0..cols {
        let level = column_level(levels, c, cols);
        let filled = (level * sub_rows as f32).round() as usize;

        for r in 0..rows {
            // r = 0 是最上面那一行；从底部往上算这格应该填多少
            let from_bottom = (rows - 1 - r) * 8;
            let sub = filled.saturating_sub(from_bottom).min(8);
            if sub == 0 {
                continue;
            }

            let position = (inner.x + c as u16, inner.y + r as u16);
            if let Some(cell) = buf.cell_mut(position) {
                cell.set_char(BLOCKS[sub]);
                cell.set_style(Style::new().fg(bar_color(r, rows)));
            }
        }
    }
}

/// 把 `BANDS` 个频段重采样成 `cols` 列（列比频段多就复用，少就取最大）
fn column_level(levels: &[f32], c: usize, cols: usize) -> f32 {
    if levels.is_empty() {
        return 0.0;
    }
    let start = c * levels.len() / cols;
    let end = ((c + 1) * levels.len() / cols)
        .max(start + 1)
        .min(levels.len());

    levels[start..end].iter().copied().fold(0.0, f32::max)
}

/// 越靠上的格子越"热"，像真的电平表
fn bar_color(row: usize, rows: usize) -> Color {
    let frac = (row + 1) as f32 / rows as f32; // 顶部 → 接近 0
    if frac < 0.34 {
        Color::Red
    } else if frac < 0.67 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// 右上角角标：加载中显示 `loading 342/2000`，加载完显示总数
fn load_badge(app: &App) -> Line<'static> {
    let (loaded, total) = app.load_progress();

    let (text, color) = if loaded < total {
        (format!(" loading {loaded}/{total} "), Color::Yellow)
    } else {
        (format!(" {total} tracks "), Color::DarkGray)
    };

    Line::from(Span::styled(text, Style::new().fg(color))).right_aligned()
}
