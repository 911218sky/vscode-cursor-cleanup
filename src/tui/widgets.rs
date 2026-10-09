use crate::cleanup::TargetStat;
use crate::fsutil::fmt_size;
use crate::i18n::{self, t, Msg};
use crate::paths::Editor;
use crate::tui::theme;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Row, Table, Wrap};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HIGHLIGHT_COLS: usize = 2; // "▌ "

/// Truncate `s` to at most `max` display columns, appending `…` when cut.
pub fn truncate_to_width(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.width() <= max {
        return s.to_string();
    }
    if max == 1 {
        return "…".to_string();
    }
    let keep = max - 1;
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > keep {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

fn label_cols(area_width: u16, icon: &str) -> usize {
    let chrome = 2usize // left/right borders
        + HIGHLIGHT_COLS
        + icon.width().saturating_add(1); // icon + space
    (area_width as usize).saturating_sub(chrome)
}

fn panel(title: impl Into<String>, frame: u64) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme::border_style(frame))
        .title(Span::styled(title.into(), theme::accent_style(frame, 0)))
}

fn menu_block(title: &str, frame: u64, selected: usize) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme::border_style(frame))
        .title(Span::styled(
            format!(" {title} "),
            theme::accent_style(frame, selected + 1),
        ))
}

pub fn draw_banner(f: &mut Frame, area: Rect, _subtitle: &str, frame: u64, animate: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme::border_style(frame))
        .title(Span::styled(
            format!(" cursor-cleanup · v{VERSION} "),
            theme::accent_style(frame, 0),
        ))
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let marker = if animate {
        theme::spinner(frame)
    } else {
        "·"
    };
    let hint = Line::from(vec![
        Span::styled(format!("{marker} "), theme::accent_style(frame, 0)),
        Span::styled(
            t(Msg::BannerHint).to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let p = Paragraph::new(hint).alignment(Alignment::Left);
    f.render_widget(p, inner);
}

pub fn draw_scanning(f: &mut Frame, area: Rect, editor: Editor, frame: u64) {
    let block = panel(" · Scanning ", frame);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("{} ", theme::spinner(frame)),
                theme::accent_style(frame, 0),
            ),
            Span::styled(
                format!("Scanning {} user data", editor.display_name()),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                theme::dots(frame).to_string(),
                theme::accent_style(frame, 3),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Analyzing cache · logs · extensions · workspace storage",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let p = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(p, inner);
}

pub fn menu_list<'a>(
    items: Vec<ListItem<'a>>,
    selected: usize,
    title: &str,
    frame: u64,
) -> List<'a> {
    List::new(items)
        .block(menu_block(title, frame, selected))
        .highlight_style(theme::select_style())
        .highlight_symbol("▌ ")
}

pub fn list_items(
    labels: &[String],
    selected: usize,
    icons: &[&str],
    frame: u64,
    area_width: u16,
) -> Vec<ListItem<'static>> {
    labels
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let icon = icons.get(i).copied().unwrap_or("·");
            let style = if i == selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let icon_style = if i == selected {
                theme::accent_style(frame, i)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let text = truncate_to_width(s, label_cols(area_width, icon));
            ListItem::new(Line::from(vec![
                Span::styled(format!("{icon} "), icon_style),
                Span::styled(text, style),
            ]))
        })
        .collect()
}

pub fn checkbox_items(
    labels: &[String],
    checked: &[bool],
    selected: usize,
    frame: u64,
    area_width: u16,
) -> Vec<ListItem<'static>> {
    labels
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mark = if i < checked.len() && checked[i] {
                "[x]"
            } else {
                "[ ]"
            };
            let style = if i == selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let mark_color = if i < checked.len() && checked[i] {
                theme::accent(frame, i)
            } else {
                Color::DarkGray
            };
            let text = truncate_to_width(s, label_cols(area_width, mark));
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), Style::default().fg(mark_color)),
                Span::styled(text, style),
            ]))
        })
        .collect()
}

pub fn draw_scan_table(
    f: &mut Frame,
    area: Rect,
    editor: Editor,
    root: &str,
    total: u64,
    stats: &[TargetStat],
    frame: u64,
) {
    let max_bytes = stats.iter().map(|s| s.bytes).max().unwrap_or(1).max(1);

    let header = Row::new(vec!["#", "Item", "Size", "Bar", "Risk"])
        .style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let rows: Vec<Row> = stats
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let risk = i18n::risk_tag(s.target.risk);
            let color = theme::risk_color(s.target.risk);
            let bar_w = 10usize;
            let fill = ((s.bytes as f64 / max_bytes as f64) * bar_w as f64).round() as usize;
            let bar: String = (0..bar_w)
                .map(|j| if j < fill { '━' } else { '─' })
                .collect();

            Row::new(vec![
                format!("{:02}", i + 1),
                i18n::target_title(s.target.kind).to_string(),
                fmt_size(s.bytes),
                bar,
                risk.to_string(),
            ])
            .style(Style::default().fg(color))
        })
        .collect();

    let cleanable: u64 = stats.iter().map(|s| s.bytes).sum();
    // Single bottom title: cleanable + path — avoid overlaying two widgets on the border.
    let clean_txt = fmt_size(cleanable);
    let prefix = format!(" · {} {}  ·  ", t(Msg::Cleanable), clean_txt);
    let path_cols = (area.width as usize).saturating_sub(prefix.width().saturating_add(2));
    let path = truncate_to_width(root, path_cols.max(4));
    let bottom = Line::from(vec![
        Span::styled(" · ", theme::accent_style(frame, 2)),
        Span::styled(
            t(Msg::Cleanable).to_string(),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(" "),
        Span::styled(clean_txt, theme::accent_style(frame, 0)),
        Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
        Span::styled(path, theme::data_style()),
        Span::raw(" "),
    ]);
    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Min(16),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .column_spacing(2)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(theme::border_style(frame))
            .title(Span::styled(
                format!(" {} — ", editor.display_name()),
                Style::default().fg(Color::Gray),
            ))
            .title(Span::styled(
                format!("{} ", fmt_size(total)),
                theme::data_style().add_modifier(Modifier::BOLD),
            ))
            .title_bottom(bottom),
    );
    f.render_widget(table, area);
}

pub fn draw_progress(
    f: &mut Frame,
    area: Rect,
    log: &[String],
    step: usize,
    total_steps: usize,
    frame: u64,
) {
    let ratio = if total_steps == 0 {
        1.0
    } else {
        step as f64 / total_steps as f64
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme::border_style(frame))
        .title(Span::styled(
            format!(
                " {} {} ({}/{}) ",
                theme::spinner(frame),
                t(Msg::AboutToClean),
                step.min(total_steps),
                total_steps
            ),
            theme::accent_style(frame, 0),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let bar_line = Line::from(vec![
        Span::styled(
            theme::progress_bar(ratio, inner.width.saturating_sub(8) as usize, frame),
            theme::accent_style(frame, 1),
        ),
        Span::raw(" "),
        Span::styled(
            theme::pct_label(ratio),
            Style::default().fg(Color::White),
        ),
    ]);

    let log_text = log.join("\n");
    let body = Paragraph::new(vec![bar_line, Line::from(""), Line::from(log_text)])
        .wrap(Wrap { trim: true });

    f.render_widget(body, inner);
}

pub fn draw_confirm(f: &mut Frame, area: Rect, prompt: &str, _frame: u64) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme::focus_border_style())
        .title(Span::styled(
            " Confirm ",
            theme::accent_style(0, 0),
        ));
    let p = Paragraph::new(prompt.to_string())
        .wrap(Wrap { trim: true })
        .block(block);
    f.render_widget(p, area);
}

pub fn draw_input_field(
    f: &mut Frame,
    area: Rect,
    hint: &str,
    value: &str,
    cursor: usize,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme::border_style(0))
        .title(Span::styled(
            format!(" {} ", t(Msg::BackupNameAsk)),
            theme::accent_style(0, 0),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let hint_p = Paragraph::new(hint).style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint_p, Rect { height: 2, ..inner });

    let input_area = Rect {
        y: inner.y + 2,
        height: 3,
        ..inner
    };

    let before = value.chars().take(cursor).collect::<String>();
    let after = value.chars().skip(cursor).collect::<String>();
    let line = Line::from(vec![
        Span::styled("· ", theme::accent_style(0, 1)),
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled("▌", theme::accent_style(0, 2)),
        Span::styled(after, Style::default().fg(Color::White)),
    ]);

    let p = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(theme::focus_border_style()),
    );
    f.render_widget(p, input_area);
}

pub fn status_line(msg: &str, kind: StatusKind, frame: u64) -> Paragraph<'static> {
    let (icon, color) = match kind {
        StatusKind::Ok => ("✔", theme::ok_color()),
        StatusKind::Warn => ("!", theme::warn_color()),
        StatusKind::Dim => ("·", Color::DarkGray),
    };
    let icon_style = if matches!(kind, StatusKind::Dim) {
        Style::default().fg(theme::accent(frame, 0))
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    };

    Paragraph::new(Line::from(vec![
        Span::styled(format!("{icon} "), icon_style),
        Span::styled(msg.to_string(), Style::default().fg(color)),
    ]))
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(theme::border_style(frame)),
    )
}

pub enum StatusKind {
    Ok,
    Warn,
    Dim,
}

/// Map a mouse click inside a list widget area to an item index.
/// Uses the same Block layout as `menu_list` (plain border + title).
pub fn mouse_to_index(
    area: Rect,
    col: u16,
    row: u16,
    count: usize,
    scroll: usize,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .title(" ");
    let inner = block.inner(area);
    if col < inner.x
        || col >= inner.x + inner.width
        || row < inner.y
        || row >= inner.y + inner.height
    {
        return None;
    }
    let rel = (row - inner.y) as usize + scroll;
    if rel < count {
        Some(rel)
    } else {
        None
    }
}

/// Icons for common menu screens (same order as menu items).
pub fn icons_for_screen(screen: MenuScreen, count: usize) -> Vec<&'static str> {
    let defaults: &[&str] = match screen {
        MenuScreen::Language => &["繁", "简", "En"],
        MenuScreen::MainMenu => &["—", "·", "↺", "×"],
        MenuScreen::Plan => &["·", "—", "═", "…", "←"],
        MenuScreen::CustomClean => &["✓", "→", "←"],
        MenuScreen::BackupMode => &["·", "—", "←"],
        MenuScreen::RestoreManage => &["↺", "×", "←"],
        MenuScreen::Confirm => &["✓", "×"],
        MenuScreen::Generic => &[],
    };
    (0..count)
        .map(|i| defaults.get(i).copied().unwrap_or("·"))
        .collect()
}

pub enum MenuScreen {
    Language,
    MainMenu,
    Plan,
    CustomClean,
    BackupMode,
    RestoreManage,
    Confirm,
    Generic,
}
