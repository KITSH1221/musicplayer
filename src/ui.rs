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

    let [header, now, _, wave_area, _, list_area, progress_area, help] = Layout::vertical([
        Constraint::Length(1), // 标题 + 状态
        Constraint::Length(1), // 正在播放
        Constraint::Length(1), // 留白
        Constraint::Length(6), // 波形 ← 放在播放列表上面
        Constraint::Length(1), // 留白
        Constraint::Min(1),    // 列表
        Constraint::Length(1), // 进度
        Constraint::Length(1), // 帮助
    ])
    .areas(area);

    frame.render_widget(Paragraph::new(header_line(app, area.width)), header);
    frame.render_widget(Paragraph::new(now_playing(app)), now);

    draw_waveform(frame, app.waveform(), wave_area);
    draw_list(frame, app, list_area);
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
//  波形
// ============================================================
//
// 用**盲文点阵**画，而不是方块字符。
//
// 一个盲文字符（U+2800 起）内部是 2 列 × 4 行的点阵，所以：
//   - 纵向分辨率 = 每格 4 级（方块字符只有 1 级）
//   - 横向分辨率 = 每格 2 列（直接翻倍）
//
// 结果是能画出一条**平滑的细线**，而不是一堆方块。
// 代价是要求终端字体包含盲文点阵（Cascadia / JetBrains Mono / Fira Code /
// DejaVu Sans Mono 都有）。

/// 把包络重采样到点阵，算出每个字符格的盲文点位。
///
/// 返回值长度 = `width * height`，每个字节是 8 个点的位掩码。
/// 抽成纯函数是为了能脱离终端做单元测试。
fn braille_cells(levels: &[f32], width: usize, height: usize) -> Vec<u8> {
    let mut cells = vec![0u8; width * height];
    if width == 0 || height == 0 {
        return cells;
    }

    let dot_cols = width * 2;
    let dot_rows = height * 4;
    let center = dot_rows / 2; // 中线（从上往下数的点行号）

    let up_room = center; // 中线以上还有几行可用
    let down_room = dot_rows - 1 - center; // 中线以下还有几行可用

    for x in 0..dot_cols {
        let amp = resample(levels, x, dot_cols).clamp(0.0, 1.0);
        let up = (amp * up_room as f32).round() as usize;
        let down = (amp * down_room as f32).round() as usize;

        // 上下各画一条包络线，中间自然形成一个"波浪带"
        set_dot(&mut cells, width, height, x, center - up);
        set_dot(&mut cells, width, height, x, center + down);
    }

    cells
}

/// 点亮一个点。坐标超出范围就忽略（防御性写法，避免 panic）。
fn set_dot(cells: &mut [u8], width: usize, height: usize, x: usize, y: usize) {
    let cx = x / 2;
    let cy = y / 4;
    if cx >= width || cy >= height {
        return;
    }
    cells[cy * width + cx] |= dot_bit(x % 2, y % 4);
}

/// 盲文点阵的位序（来自 Unicode 标准）：
///
/// ```text
///   列0 列1
///   0x01 0x08   ← 第 0 行
///   0x02 0x10   ← 第 1 行
///   0x04 0x20   ← 第 2 行
///   0x40 0x80   ← 第 3 行
/// ```
fn dot_bit(dx: usize, dy: usize) -> u8 {
    match (dx, dy) {
        (0, 0) => 0x01,
        (1, 0) => 0x08,
        (0, 1) => 0x02,
        (1, 1) => 0x10,
        (0, 2) => 0x04,
        (1, 2) => 0x20,
        (0, 3) => 0x40,
        (1, 3) => 0x80,
        _ => 0,
    }
}

fn braille_char(bits: u8) -> char {
    char::from_u32(0x2800 + bits as u32).unwrap_or(' ')
}

fn draw_waveform(frame: &mut Frame, levels: &[f32], area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let width = area.width as usize;
    let height = area.height as usize;
    let cells = braille_cells(levels, width, height);

    let buf = frame.buffer_mut();
    for cy in 0..height {
        for cx in 0..width {
            let bits = cells[cy * width + cx];
            if bits == 0 {
                continue;
            }
            let position = (area.x + cx as u16, area.y + cy as u16);
            if let Some(cell) = buf.cell_mut(position) {
                cell.set_char(braille_char(bits));
                cell.set_style(Style::new().fg(ACCENT));
            }
        }
    }
}

/// 把 `levels` 重采样成 `n` 个值：某一段里取最大值。
/// 目标比源短就归并（取峰值），比源长就复用。
fn resample(levels: &[f32], i: usize, n: usize) -> f32 {
    if levels.is_empty() || n == 0 {
        return 0.0;
    }
    let a = i * levels.len() / n;
    let b = ((i + 1) * levels.len() / n).max(a + 1).min(levels.len());
    levels[a..b].iter().copied().fold(0.0, f32::max)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waveform::POINTS;

    #[test]
    fn braille_dot_bits_match_the_unicode_layout() {
        let cases = [
            ((0, 0), 0x01),
            ((1, 0), 0x08),
            ((0, 1), 0x02),
            ((1, 1), 0x10),
            ((0, 2), 0x04),
            ((1, 2), 0x20),
            ((0, 3), 0x40),
            ((1, 3), 0x80),
        ];
        for ((dx, dy), expected) in cases {
            let mut cells = vec![0u8; 1];
            set_dot(&mut cells, 1, 1, dx, dy);
            assert_eq!(cells[0], expected, "点 ({dx},{dy}) 的位不对");
        }
    }

    #[test]
    fn braille_char_starts_at_u2800() {
        assert_eq!(braille_char(0x00), '\u{2800}');
        assert_eq!(braille_char(0x01), '\u{2801}');
        assert_eq!(braille_char(0xFF), '\u{28FF}');
    }

    #[test]
    fn silence_draws_a_flat_line_through_the_centre() {
        let levels = vec![0.0f32; POINTS];
        let (w, h) = (4usize, 2usize);
        let cells = braille_cells(&levels, w, h);

        let center = h * 4 / 2; // 点行号
        let cy = center / 4;
        let dy = center % 4;

        for cx in 0..w {
            for dx in 0..2 {
                assert!(
                    cells[cy * w + cx] & dot_bit(dx, dy) != 0,
                    "第 {} 列的中线没画出来",
                    cx * 2 + dx
                );
            }
        }
    }

    #[test]
    fn full_amplitude_reaches_top_and_bottom_rows() {
        let levels = vec![1.0f32; POINTS];
        let (w, h) = (4usize, 2usize);
        let cells = braille_cells(&levels, w, h);

        assert!(cells[..w].iter().any(|&c| c != 0), "顶行没画到");
        assert!(cells[(h - 1) * w..].iter().any(|&c| c != 0), "底行没画到");
    }

    #[test]
    fn out_of_range_dots_are_ignored() {
        let mut cells = vec![0u8; 2];
        set_dot(&mut cells, 2, 1, 999, 999); // 不该 panic
        assert!(cells.iter().all(|&c| c == 0));
    }

    #[test]
    fn braille_cells_handles_empty_and_tiny_areas() {
        assert!(braille_cells(&[], 0, 0).is_empty());
        assert_eq!(braille_cells(&[1.0], 1, 1).len(), 1);
    }
}
