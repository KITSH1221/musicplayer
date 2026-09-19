//! 表现层：极简风。
//!
//! # 三条设计原则
//!
//! 1. **不用边框**。边框是 TUI 里最大的视觉噪音来源；用留白来分组。
//! 2. **只用两种颜色**：一个强调色 + 一个暗色。层级靠"亮/暗、粗/细"表达。
//! 3. **对齐代替标签**。右对齐的次要信息本身就是分组，不需要写 "Artist:"。
//!
//! 这里没有任何业务逻辑 —— 出了问题永远是"传进来的状态不对"，
//! 而不是"画图函数干了别的事"。

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{LineGauge, List, ListItem, Paragraph},
};

use crate::{app::App, library::Track, util::fmt_time};

/// 唯一的强调色
const ACCENT: Color = Color::Cyan;
/// 次要信息、未激活状态
const DIM: Color = Color::DarkGray;
/// 出错才用
const ERROR: Color = Color::Red;

/// 列表左侧给"选中标记"预留的宽度
const MARKER_WIDTH: u16 = 2;

pub fn draw(frame: &mut Frame, app: &mut App) {
    // 整体留白。界面立刻"松"下来，是极简风格最廉价也最有效的一招。
    let area = frame.area().inner(Margin::new(2, 1));

    let [
        header,
        now,
        _,
        list_area,
        _,
        spectrum_area,
        progress_area,
        help,
    ] = Layout::vertical([
        Constraint::Length(1), // 标题 + 状态
        Constraint::Length(1), // 正在播放
        Constraint::Length(1), // 留白
        Constraint::Min(1),    // 列表
        Constraint::Length(1), // 留白
        Constraint::Length(5), // 频谱
        Constraint::Length(1), // 进度
        Constraint::Length(1), // 帮助
    ])
    .areas(area);

    frame.render_widget(Paragraph::new(header_line(app, area.width)), header);
    frame.render_widget(Paragraph::new(now_playing(app)), now);

    draw_list(frame, app, list_area);
    draw_spectrum(frame, app.spectrum(), spectrum_area);
    draw_progress(frame, app, progress_area);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "enter play   space pause   ←/→ ±5s   ↑/↓ select   q quit",
            Style::new().fg(DIM),
        )),
        help,
    );
}

// ============================================================
//  顶部
// ============================================================

/// 左边是应用名，右边是状态：有通知显示通知，否则显示设备信息 / 加载进度。
fn header_line(app: &App, width: u16) -> Line<'static> {
    let title = Span::styled(
        "musicplayer",
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
    );

    let (loaded, total) = app.load_progress();

    let right = if let Some(notice) = app.notice() {
        Span::styled(notice.to_string(), Style::new().fg(ERROR))
    } else if loaded < total {
        Span::styled(format!("loading {loaded}/{total}"), Style::new().fg(ACCENT))
    } else {
        Span::styled(app.device_info(), Style::new().fg(DIM))
    };

    justified(title, right, width)
}

/// "正在播放"那一行：标题亮、艺术家暗
fn now_playing(app: &App) -> Line<'static> {
    let Some(idx) = app.playing else {
        return Line::from(Span::styled("—", Style::new().fg(DIM)));
    };

    let track = &app.playlist[idx];
    let mut spans = vec![Span::styled("♪  ", Style::new().fg(ACCENT))];
    spans.push(Span::styled(
        track.title(),
        Style::new().add_modifier(Modifier::BOLD),
    ));

    if let Some(artist) = track.artist() {
        spans.push(Span::styled(format!("  ·  {artist}"), Style::new().fg(DIM)));
    }

    Line::from(spans)
}

// ============================================================
//  列表
// ============================================================

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    // 减掉选中标记占的宽度，剩下的才是每行可用的宽度
    let row_width = area.width.saturating_sub(MARKER_WIDTH) as usize;

    let items: Vec<ListItem> = app
        .playlist
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let line = track_line(track, app.playing == Some(i), row_width);
            if track.is_loaded() {
                ListItem::new(line)
            } else {
                // 标签还没读到的行整行压暗，加载过程在界面上是"可见"的
                ListItem::new(line).style(Style::new().fg(DIM))
            }
        })
        .collect();

    let list = List::new(items)
        .highlight_symbol(Span::styled("▸ ", Style::new().fg(ACCENT)))
        // 选中不用背景色块，只用"加粗 + 强调色"，更干净
        .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD));

    app.list_state.select(Some(app.cursor));
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

/// 一行列表：左边标题，右边艺术家 + 时长（右对齐）
fn track_line(track: &Track, is_playing: bool, width: usize) -> Line<'static> {
    let marker = if is_playing {
        Span::styled("♪ ", Style::new().fg(ACCENT))
    } else {
        Span::raw("  ")
    };

    // 右边：艺术家 + 时长。没读到的信息就留空，不显示占位符
    let right = match (track.artist(), track.duration()) {
        (Some(artist), Some(dur)) => format!("{artist}  {}", fmt_time(dur)),
        (Some(artist), None) => artist.to_string(),
        _ => String::new(),
    };

    let title = track.title();
    let left_w = marker.width() + Span::raw(title.clone()).width();
    let right_w = Span::raw(right.clone()).width();
    // 中间至少留 2 个空格，否则左右会贴在一起
    let pad = width.saturating_sub(left_w + right_w + 2).max(1);

    let mut spans = vec![marker, Span::raw(title), Span::raw(" ".repeat(pad))];
    if !right.is_empty() {
        spans.push(Span::styled(right, Style::new().fg(DIM)));
    }

    Line::from(spans)
}

// ============================================================
//  频谱
// ============================================================

/// 半格字符：下标 = 这一格从底部算起被填了 1/8 的几份
const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn draw_spectrum(frame: &mut Frame, levels: &[f32], area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let cols = area.width as usize;
    let rows = area.height as usize;
    let sub_rows = rows * 8; // 每格 8 级，所以总共有 rows*8 级高度
    let buf = frame.buffer_mut();

    for c in 0..cols {
        let level = column_level(levels, c, cols);
        let filled = (level * sub_rows as f32).round() as usize;

        for r in 0..rows {
            // r = 0 是最上面一行；从底部往上算这一格该填多少
            let from_bottom = (rows - 1 - r) * 8;
            let sub = filled.saturating_sub(from_bottom).min(8);
            if sub == 0 {
                continue;
            }

            let position = (area.x + c as u16, area.y + r as u16);
            if let Some(cell) = buf.cell_mut(position) {
                cell.set_char(BLOCKS[sub]);
                cell.set_style(Style::new().fg(ACCENT));
            }
        }
    }
}

/// 把 `BANDS` 个频段重采样成 `cols` 列（列多就复用，列少就取最大）
fn column_level(levels: &[f32], c: usize, cols: usize) -> f32 {
    if levels.is_empty() || cols == 0 {
        return 0.0;
    }
    let start = c * levels.len() / cols;
    let end = ((c + 1) * levels.len() / cols)
        .max(start + 1)
        .min(levels.len());

    levels[start..end].iter().copied().fold(0.0, f32::max)
}

// ============================================================
//  进度
// ============================================================

/// `01:23   ─────────────────────  04:08`
///
/// 用 `LineGauge`（一条细线）而不是 `Gauge`（一整块实心条），
/// 这是极简和"仪表盘风"最大的区别。
fn draw_progress(frame: &mut Frame, app: &App, area: Rect) {
    let [elapsed_area, bar_area, total_area] = Layout::horizontal([
        Constraint::Length(6),
        Constraint::Min(4),
        Constraint::Length(7),
    ])
    .areas(area);

    let (elapsed, total, ratio) = match app.progress() {
        Some((pos, total, ratio)) => (fmt_time(pos), fmt_time(total), ratio),
        None => ("--:--".to_string(), "--:--".to_string(), 0.0),
    };

    // 时间右对齐、贴着进度条
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(elapsed, Style::new().fg(ACCENT))).right_aligned()),
        elapsed_area,
    );

    frame.render_widget(
        LineGauge::default()
            .ratio(ratio)
            .filled_style(Style::new().fg(ACCENT))
            .unfilled_style(Style::new().fg(DIM)),
        bar_area,
    );

    frame.render_widget(
        Paragraph::new(Span::styled(format!(" {total}"), Style::new().fg(DIM))),
        total_area,
    );
}

// ============================================================
//  小工具
// ============================================================

/// 把两个片段拉成左右两端对齐的一行
fn justified(left: Span<'static>, right: Span<'static>, width: u16) -> Line<'static> {
    let pad = (width as usize)
        .saturating_sub(left.width() + right.width())
        .max(1);
    Line::from(vec![left, Span::raw(" ".repeat(pad)), right])
}
