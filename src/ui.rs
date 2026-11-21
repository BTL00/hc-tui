//! Drawing: sidebar (machines → workspaces → tabs), the current tab's panes
//! (predictions on top of the focused one), status line, overlays.
//! Colors lean on gruvbox, like herdr's default theme.

use crate::app::{App, Confirm, Mode, Pane, Prompt, Row};
use crate::keys::HELP;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

const SIDEBAR_WIDTH: u16 = 32;
const BG: Color = Color::Rgb(0x28, 0x28, 0x28);
const BG2: Color = Color::Rgb(0x3c, 0x38, 0x36);
const FG: Color = Color::Rgb(0xeb, 0xdb, 0xb2);
const DIM: Color = Color::Rgb(0x92, 0x83, 0x74);
const YELLOW: Color = Color::Rgb(0xfa, 0xbd, 0x2f);
const RED: Color = Color::Rgb(0xfb, 0x49, 0x34);
const GREEN: Color = Color::Rgb(0xb8, 0xbb, 0x26);
const BLUE: Color = Color::Rgb(0x83, 0xa5, 0x98);
const AQUA: Color = Color::Rgb(0x8e, 0xc0, 0x7c);
const ORANGE: Color = Color::Rgb(0xfe, 0x80, 0x19);
/// Row background for an agent waiting for you (dark gruvbox yellow).
const WAIT_BG: Color = Color::Rgb(0x4a, 0x3b, 0x12);

/// (sidebar, tab area, status line).
pub fn areas(app: &App, area: Rect) -> (Option<Rect>, Rect, Rect) {
    let [body, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    if app.sidebar && body.width > SIDEBAR_WIDTH + 20 {
        let [side, pane] =
            Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(1)]).areas(body);
        (Some(side), pane, status)
    } else {
        (None, body, status)
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let (side, tab, status) = areas(app, f.area());
    if let Some(side) = side {
        draw_sidebar(f, app, side);
    }
    draw_tab(f, app, tab);
    draw_status(f, app, status);
    match &app.mode {
        Mode::Help => draw_help(f, f.area()),
        Mode::Picker { .. } => draw_picker(f, app, f.area()),
        Mode::Prompt(kind, text) => draw_prompt(f, kind, text, f.area()),
        Mode::Confirm(what) => draw_confirm(f, app, what, f.area()),
        _ => {}
    }
}

fn rank(status: &str) -> u8 {
    match status {
        "blocked" => 4,
        "working" => 3,
        "done" => 2,
        "idle" => 1,
        _ => 0,
    }
}

pub fn status_style(status: &str, live: bool) -> (&'static str, Color) {
    if !live {
        return ("✗", DIM);
    }
    match status {
        "working" => ("●", YELLOW),
        "blocked" => ("▲", RED),
        "idle" => ("○", GREEN),
        "done" => ("✓", BLUE),
        _ => ("·", DIM),
    }
}

fn truncate(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw > width {
            out.push('…');
            return out;
        }
        out.push(c);
        w += cw;
    }
    out
}

/// Right-hand tag for a terminal that stopped and waits for you.
fn wait_tag(status: &str) -> Span<'static> {
    let (text, color) = match status {
        "blocked" => ("needs you", RED),
        "error" => ("error", RED),
        _ => ("your turn", YELLOW),
    };
    Span::styled(text, Style::new().fg(color).add_modifier(Modifier::BOLD))
}

/// Right-hand tag naming the agent (`claude`, `codex`...), if hc detected one.
fn app_tag(term: Option<&crate::app::Term>) -> Span<'static> {
    let name = term.map(|x| x.app.replace("-code", "")).unwrap_or_default();
    Span::styled(name, Style::new().fg(DIM))
}

/// `prefix` + `name` (truncated) + padding + `tag` flush right, in `width` columns.
fn fitted<'a>(
    mut spans: Vec<Span<'a>>,
    name: &str,
    name_style: Style,
    tag: Span<'a>,
    width: usize,
) -> Line<'a> {
    use unicode_width::UnicodeWidthStr;
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let tag_w = tag.content.width();
    let gap = usize::from(tag_w > 0);
    let room = width.saturating_sub(used + tag_w + gap);
    let mut name = truncate(name, room);
    if name.width() > room {
        name = truncate(&name, room.saturating_sub(1));
    }
    let pad = width.saturating_sub(used + name.width() + tag_w);
    spans.push(Span::styled(name, name_style));
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(tag);
    Line::from(spans)
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let navigating = app.mode == Mode::Navigate;
    let inner_w = area.width.saturating_sub(2) as usize;
    let current = app.current.clone();
    let focused = app.focused();
    let mut selected = None;
    let items: Vec<ListItem> = app
        .rows
        .iter()
        .enumerate()
        .map(|(row_i, row)| match row {
            Row::Machine(name, state) => {
                let color = match state.as_str() {
                    "connected" | "local" => AQUA,
                    "connecting" => YELLOW,
                    _ => RED,
                };
                let up = matches!(state.as_str(), "connected" | "local");
                let note = if state == "local" {
                    Span::styled("this pc", Style::new().fg(DIM))
                } else {
                    Span::styled(state.clone(), Style::new().fg(color))
                };
                // 💻 is two columns wide; `fitted` measures it that way.
                ListItem::new(fitted(
                    vec![Span::raw("💻 ")],
                    name,
                    Style::new()
                        .fg(if up { FG } else { DIM })
                        .add_modifier(Modifier::BOLD),
                    note,
                    inner_w,
                ))
            }
            Row::Space(si) => {
                let s = &app.spaces[*si];
                let is_current = current
                    .as_ref()
                    .is_some_and(|c| c.on == s.on && c.space == s.id);
                let style = Style::new()
                    .fg(if is_current { ORANGE } else { DIM })
                    .add_modifier(Modifier::BOLD);
                let mut waiting: Vec<&String> = s
                    .tabs
                    .iter()
                    .flat_map(|t| t.terminals.iter())
                    .filter(|a| app.waiting(a))
                    .collect();
                waiting.dedup();
                let tag = if waiting.is_empty() {
                    Span::raw("")
                } else {
                    Span::styled(
                        format!("{} waiting", waiting.len()),
                        Style::new().fg(YELLOW).add_modifier(Modifier::BOLD),
                    )
                };
                ListItem::new(fitted(
                    vec![Span::raw(" ")],
                    &s.label(),
                    style,
                    tag,
                    inner_w,
                ))
            }
            Row::Tab(si, ti) => {
                let s = &app.spaces[*si];
                let t = &s.tabs[*ti];
                let is_current = current
                    .as_ref()
                    .is_some_and(|c| c.on == s.on && c.space == s.id && c.tab == t.id);
                if navigating && row_i == app.nav_cursor {
                    selected = Some(row_i);
                }
                // The tab shows its busiest terminal's state.
                let worst = t
                    .terminals
                    .iter()
                    .filter_map(|a| app.term(a))
                    .max_by_key(|x| (x.live, rank(&x.status)));
                let (glyph, color) = worst
                    .map(|x| status_style(&x.status, x.live))
                    .unwrap_or(("·", DIM));
                let split = t.terminals.len() > 1;
                let shown = if is_current { focused.clone() } else { None };
                let name = if !t.name.is_empty() {
                    t.name.clone()
                } else if split {
                    format!("tab {}", ti + 1)
                } else {
                    shown
                        .as_deref()
                        .or(t.terminals.first().map(String::as_str))
                        .and_then(|a| app.term(a))
                        .map(|x| x.display())
                        .unwrap_or_else(|| format!("tab {}", ti + 1))
                };
                // Waiting panes of a split tab get their own rows below.
                let waiting = t
                    .terminals
                    .iter()
                    .filter(|a| app.waiting(a))
                    .filter_map(|a| app.term(a))
                    .max_by_key(|x| rank(&x.status));
                let mut name_style = Style::new().fg(FG);
                if is_current {
                    name_style = name_style.add_modifier(Modifier::BOLD);
                }
                let tag = match (waiting, split) {
                    (Some(w), false) => wait_tag(&w.status),
                    (Some(_), true) => {
                        Span::styled("▾", Style::new().fg(YELLOW).add_modifier(Modifier::BOLD))
                    }
                    (None, _) => app_tag(worst),
                };
                let line = fitted(
                    vec![
                        Span::styled(
                            if is_current { " ▌" } else { "  " },
                            Style::new().fg(YELLOW),
                        ),
                        Span::styled(format!("{glyph} "), Style::new().fg(color)),
                    ],
                    &name,
                    name_style,
                    tag,
                    inner_w,
                );
                let item = ListItem::new(line);
                if waiting.is_some() {
                    item.style(Style::new().bg(WAIT_BG))
                } else {
                    item
                }
            }
            Row::Pane(si, ti, pi) => {
                let s = &app.spaces[*si];
                let t = &s.tabs[*ti];
                let address = &t.terminals[*pi];
                let is_current = current
                    .as_ref()
                    .is_some_and(|c| c.on == s.on && c.space == s.id && c.tab == t.id);
                let is_focused = is_current && focused.as_ref() == Some(address);
                if navigating && row_i == app.nav_cursor {
                    selected = Some(row_i);
                }
                let term = app.term(address);
                let (glyph, color) = term
                    .map(|x| status_style(&x.status, x.live))
                    .unwrap_or(("·", DIM));
                let name = term.map(|x| x.display()).unwrap_or_else(|| address.clone());
                let last = *pi + 1 == t.terminals.len();
                let waiting = app.waiting(address);
                let name_style = if is_focused {
                    Style::new().fg(FG).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(if is_current || waiting { FG } else { DIM })
                };
                let tag = match term {
                    Some(x) if waiting => wait_tag(&x.status),
                    _ => app_tag(term),
                };
                let line = fitted(
                    vec![
                        Span::styled(
                            if last { "   └ " } else { "   ├ " },
                            Style::new().fg(if is_focused { YELLOW } else { BG2 }),
                        ),
                        Span::styled(format!("{glyph} "), Style::new().fg(color)),
                    ],
                    &name,
                    name_style,
                    tag,
                    inner_w,
                );
                let item = ListItem::new(line);
                if waiting {
                    item.style(Style::new().bg(WAIT_BG))
                } else {
                    item
                }
            }
            Row::LooseHeader => ListItem::new(Line::from(Span::styled(
                " unplaced · enter adds a tab",
                Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
            ))),
            Row::Loose(i) => {
                if navigating && row_i == app.nav_cursor {
                    selected = Some(row_i);
                }
                let t = &app.terms[*i];
                let (glyph, color) = status_style(&t.status, t.live);
                let waiting = app.waiting(&t.address);
                let line = fitted(
                    vec![
                        Span::raw("  "),
                        Span::styled(format!("{glyph} "), Style::new().fg(color)),
                    ],
                    &t.display(),
                    Style::new().fg(if waiting { FG } else { DIM }),
                    if waiting {
                        wait_tag(&t.status)
                    } else {
                        app_tag(Some(t))
                    },
                    inner_w,
                );
                let item = ListItem::new(line);
                if waiting {
                    item.style(Style::new().bg(WAIT_BG))
                } else {
                    item
                }
            }
        })
        .collect();
    let title = if navigating {
        " workspaces · j/k ←→ enter esc "
    } else {
        " hc-tui "
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::new().fg(if navigating { YELLOW } else { BG2 }))
                .title(Span::styled(
                    title,
                    Style::new().fg(if navigating { YELLOW } else { DIM }),
                ))
                .style(Style::new().bg(BG)),
        )
        .highlight_style(Style::new().bg(BG2).add_modifier(Modifier::BOLD));
    let mut state = ListState::default().with_selected(selected);
    f.render_stateful_widget(list, area, &mut state);
    let mut notes: Vec<String> = app
        .doc_errors()
        .iter()
        .map(|e| format!("layout unavailable: {e}"))
        .collect();
    if app.spaces.is_empty() && app.loose.is_empty() {
        notes.push("no workspaces yet · ctrl+b c".into());
    }
    if !notes.is_empty() {
        let h = (notes.len() as u16 * 2).min(area.height.saturating_sub(2));
        let rect = Rect {
            x: area.x + 1,
            y: area.y + area.height.saturating_sub(h + 1),
            width: area.width.saturating_sub(3),
            height: h,
        };
        f.render_widget(
            Paragraph::new(notes.join("\n"))
                .style(Style::new().fg(DIM).bg(BG))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            rect,
        );
    }
}

fn vt_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn draw_tab(f: &mut Frame, app: &App, area: Rect) {
    if app.placed.is_empty() {
        let msg = if app.current.is_some() {
            "\n  this tab shows nothing"
        } else {
            "\n  no tab open"
        };
        let help = format!(
            "{msg}\n\n  ctrl+b w  pick a tab\n  ctrl+b c  new tab\n  ctrl+b g  goto any terminal\n  ctrl+b ?  help"
        );
        f.render_widget(Paragraph::new(help).style(Style::new().fg(DIM)), area);
        return;
    }
    let focused = app.focused();
    let focus_rect = focused
        .as_ref()
        .and_then(|a| app.placed.iter().find(|(p, _)| p == a))
        .map(|(_, r)| *r);
    for (rect, vertical) in &app.dividers {
        // Dividers next to the focused pane light up.
        let touches = focus_rect.is_some_and(|fr| {
            if *vertical {
                (rect.x + 1 == fr.x || rect.x == fr.x + fr.width)
                    && rect.y < fr.y + fr.height
                    && fr.y < rect.y + rect.height
            } else {
                (rect.y + 1 == fr.y || rect.y == fr.y + fr.height)
                    && rect.x < fr.x + fr.width
                    && fr.x < rect.x + rect.width
            }
        });
        let style = Style::new().fg(if touches { YELLOW } else { BG2 });
        let buf = f.buffer_mut();
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                if let Some(c) = buf.cell_mut((x, y)) {
                    c.set_symbol(if *vertical { "│" } else { "─" })
                        .set_style(style);
                }
            }
        }
    }
    for (address, rect) in &app.placed {
        let is_focused = focused.as_deref() == Some(address.as_str());
        match app.panes.get(address) {
            Some(pane) => draw_pane(f, app, address, pane, *rect, is_focused),
            None => f.render_widget(
                Paragraph::new(format!(" attaching {address}…")).style(Style::new().fg(DIM)),
                *rect,
            ),
        }
    }
}

fn draw_pane(f: &mut Frame, app: &App, address: &str, pane: &Pane, area: Rect, focused: bool) {
    let screen = pane.parser.screen();
    let (rows, cols) = (screen.size().0.get(), screen.size().1.get());
    let buf = f.buffer_mut();
    for r in 0..rows.min(area.height) {
        for c in 0..cols.min(area.width) {
            let Some(cell) = screen.cell(r, c) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let mut m = Modifier::empty();
            if cell.bold() {
                m |= Modifier::BOLD;
            }
            if cell.italic() {
                m |= Modifier::ITALIC;
            }
            if cell.underline() {
                m |= Modifier::UNDERLINED;
            }
            if cell.inverse() {
                m |= Modifier::REVERSED;
            }
            if cell.dim() {
                m |= Modifier::DIM;
            }
            let style = Style::new()
                .fg(vt_color(cell.fgcolor()))
                .bg(vt_color(cell.bgcolor()))
                .add_modifier(m);
            let contents = cell.contents();
            if let Some(out) = buf.cell_mut((area.x + c, area.y + r)) {
                out.set_symbol(if contents.is_empty() { " " } else { contents })
                    .set_style(style);
            }
        }
    }
    if let Some(why) = &pane.ended {
        let name = app
            .term(address)
            .map(|t| t.display())
            .unwrap_or_else(|| address.to_string());
        let note = truncate(&format!(" [{name}: {why}] "), area.width as usize);
        let w = note.chars().count() as u16;
        let rect = Rect {
            x: area.x + area.width.saturating_sub(w),
            y: area.y,
            width: w,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(note).style(Style::new().fg(BG).bg(DIM)),
            rect,
        );
        return;
    }
    if !focused {
        return;
    }
    let buf = f.buffer_mut();
    for p in app.predict.items() {
        if p.row < area.height
            && p.col < area.width
            && let Some(out) = buf.cell_mut((area.x + p.col, area.y + p.row))
        {
            let mut s = [0u8; 4];
            out.set_symbol(p.ch.encode_utf8(&mut s))
                .set_style(Style::new().add_modifier(Modifier::UNDERLINED));
        }
    }
    if matches!(app.mode, Mode::Normal | Mode::Prefix) {
        let (row, col) = app
            .predict
            .cursor()
            .unwrap_or_else(|| screen.cursor_position());
        if !screen.hide_cursor() && row < area.height && col < area.width {
            f.set_cursor_position((area.x + col, area.y + row));
        }
    }
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let mode = match &app.mode {
        Mode::Prefix => Span::styled(
            " PREFIX ",
            Style::new().fg(BG).bg(YELLOW).add_modifier(Modifier::BOLD),
        ),
        Mode::Navigate => Span::styled(
            " NAV ",
            Style::new().fg(BG).bg(AQUA).add_modifier(Modifier::BOLD),
        ),
        Mode::Resize => Span::styled(
            " RESIZE h/j/k/l esc ",
            Style::new().fg(BG).bg(ORANGE).add_modifier(Modifier::BOLD),
        ),
        Mode::Picker { .. } => Span::styled(
            " GOTO ",
            Style::new().fg(BG).bg(BLUE).add_modifier(Modifier::BOLD),
        ),
        _ => Span::styled(" hc ", Style::new().fg(BG).bg(DIM)),
    };
    let mut spans = vec![mode, Span::raw(" ")];
    if let Some((s, t)) = app.current_tab() {
        let ti = s.tabs.iter().position(|x| x.id == t.id).unwrap_or(0);
        spans.push(Span::styled(
            format!(
                "{} › {} ",
                s.label(),
                if t.name.is_empty() {
                    format!("tab {}", ti + 1)
                } else {
                    t.name.clone()
                }
            ),
            Style::new().fg(ORANGE),
        ));
    }
    if let Some(term) = app.focused().and_then(|a| app.term(&a)) {
        spans.push(Span::styled(
            format!("{} ", term.address),
            Style::new().fg(DIM),
        ));
        spans.push(Span::styled(truncate(&term.cwd, 36), Style::new().fg(FG)));
    }
    if app.zoomed.is_some() {
        spans.push(Span::styled(" [zoom]", Style::new().fg(AQUA)));
    }
    if let Some((msg, _)) = &app.message {
        spans.push(Span::styled(format!("  {msg}"), Style::new().fg(YELLOW)));
    }
    let waiting = app.waiting_list().len();
    let badge = if waiting > 0 {
        format!(" {waiting} waiting · ctrl+b a ")
    } else {
        String::new()
    };
    let right = format!(
        " {}ctrl+b ? help ",
        if app.predict.enabled { "echo⚡ " } else { "" }
    );
    let left_w: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (area.width as usize)
        .saturating_sub(left_w + badge.chars().count() + right.chars().count());
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(
        badge,
        Style::new().fg(BG).bg(YELLOW).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(right, Style::new().fg(DIM)));
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::new().bg(BG2)),
        area,
    );
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn popup(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(YELLOW))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(YELLOW).add_modifier(Modifier::BOLD),
        ))
        .style(Style::new().bg(BG).fg(FG))
}

fn draw_help(f: &mut Frame, area: Rect) {
    let rect = centered(area, 66, HELP.len() as u16 + 2);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!(" {k:<18}"), Style::new().fg(AQUA)),
                Span::raw(*d),
            ])
        })
        .collect();
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines).block(popup("keys · any key closes")),
        rect,
    );
}

fn draw_picker(f: &mut Frame, app: &App, area: Rect) {
    let Mode::Picker {
        query,
        editing,
        filter,
    } = &app.mode
    else {
        return;
    };
    let rect = centered(
        area,
        90.min(area.width.saturating_sub(4)),
        area.height.saturating_sub(4).max(8),
    );
    f.render_widget(Clear, rect);
    let matches = app.picker_matches();
    let mut items = Vec::new();
    let mut selected = None;
    let mut last: Option<String> = None;
    for &i in &matches {
        let t = &app.terms[i];
        let place = app.place_of(&t.address);
        let section = place
            .as_ref()
            .map(|p| {
                p.rsplit_once(" › ")
                    .map(|x| x.0.to_string())
                    .unwrap_or(p.clone())
            })
            .unwrap_or_else(|| "unplaced".into());
        if last.as_ref() != Some(&section) {
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" {section}"),
                Style::new().fg(ORANGE).add_modifier(Modifier::BOLD),
            ))));
            last = Some(section);
        }
        if i == app.picker_cursor {
            selected = Some(items.len());
        }
        let (glyph, color) = status_style(&t.status, t.live);
        let tab = place
            .as_ref()
            .and_then(|p| p.rsplit_once(" › ").map(|x| x.1.to_string()))
            .unwrap_or_default();
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("   {glyph} "), Style::new().fg(color)),
            Span::styled(t.display(), Style::new().fg(FG)),
            Span::styled(
                format!("  {tab}  {} {}", t.app, t.status),
                Style::new().fg(DIM),
            ),
        ])));
    }
    let filter_txt = match filter {
        Some('b') => "blocked",
        Some('w') => "working",
        Some('i') => "idle",
        Some('d') => "done",
        _ => "all",
    };
    let search = if *editing {
        format!("/{query}▏")
    } else if query.is_empty() {
        String::new()
    } else {
        format!("/{query}")
    };
    let title = format!("goto {search} · {filter_txt} · / search  b w i d a  ←→  enter esc");
    let block = popup(&title);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let [list_area, info] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let list = List::new(items).highlight_style(Style::new().bg(BG2).add_modifier(Modifier::BOLD));
    let mut state = ListState::default().with_selected(selected);
    f.render_stateful_widget(list, list_area, &mut state);
    if let Some(t) = app.terms.get(app.picker_cursor) {
        f.render_widget(
            Paragraph::new(format!(" {} · {}", t.address, t.cwd)).style(Style::new().fg(DIM)),
            info,
        );
    }
}

fn draw_prompt(f: &mut Frame, kind: &Prompt, text: &str, area: Rect) {
    let title = match kind {
        Prompt::RenameTab => "rename tab",
        Prompt::NewWorkspace => "new workspace name",
        Prompt::RenameWorkspace => "rename workspace",
    };
    let rect = centered(area, 56, 3);
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(format!("{text}▏")).block(popup(title)), rect);
}

fn draw_confirm(f: &mut Frame, app: &App, what: &Confirm, area: Rect) {
    let text = match what {
        Confirm::ClosePane(address) => {
            let name = app
                .term(address)
                .map(|t| t.display())
                .unwrap_or(address.clone());
            format!("Close pane {name}? Its program ends.  y / n")
        }
        Confirm::CloseTab(r) => {
            let n = app.tab(r).map(|(_, t)| t.terminals.len()).unwrap_or(0);
            format!("Close this tab and its {n} terminal(s)?  y / n")
        }
        Confirm::CloseWorkspace(on, id) => {
            let s = app.spaces.iter().find(|s| &s.on == on && &s.id == id);
            let n: usize = s
                .map(|s| s.tabs.iter().map(|t| t.terminals.len()).sum())
                .unwrap_or(0);
            format!(
                "Close workspace {} and its {n} terminal(s)?  y / n",
                s.map(|s| s.label()).unwrap_or_default()
            )
        }
    };
    let rect = centered(area, (text.chars().count() as u16 + 4).max(30), 3);
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(text).block(popup("confirm")), rect);
}
