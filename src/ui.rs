//! Drawing: sidebar (machines → workspaces → tabs), the current tab's panes
//! (predictions on top of the focused one), status line, overlays.
//! Colours come from the theme picked in settings (`theme.rs`).

use crate::app::{App, Confirm, CopyItem, Mode, Pane, Prompt, Row};
use crate::keys::HELP;
use crate::theme::{THEMES, Theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

thread_local! {
    static THEME: std::cell::Cell<&'static Theme> = std::cell::Cell::new(&THEMES[0]);
}

/// The theme of the frame being drawn.
fn th() -> &'static Theme {
    THEME.with(|c| c.get())
}

/// (sidebar, tab area, status line).
pub fn areas(app: &App, area: Rect) -> (Option<Rect>, Rect, Rect) {
    let [body, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let width = app.settings.sidebar_width;
    if app.sidebar && body.width > width + 20 {
        let [side, pane] =
            Layout::horizontal([Constraint::Length(width), Constraint::Min(1)]).areas(body);
        // A little air between the sidebar line and the first column.
        let gap = app.settings.pane_gap.min(pane.width.saturating_sub(1));
        let pane = Rect {
            x: pane.x + gap,
            width: pane.width - gap,
            ..pane
        };
        (Some(side), pane, status)
    } else {
        (None, body, status)
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    THEME.with(|c| c.set(crate::theme::by_name(&app.settings.theme)));
    f.render_widget(Block::default().style(Style::new().bg(th().bg)), f.area());
    let (side, tab, status) = areas(app, f.area());
    match side {
        Some(side) => draw_sidebar(f, app, side),
        None => app.sidebar_view.set((Rect::default(), 0)),
    }
    draw_tab(f, app, tab);
    draw_status(f, app, status);
    match &app.mode {
        Mode::Help => draw_help(f, f.area()),
        Mode::Picker { .. } => draw_picker(f, app, f.area()),
        Mode::Prompt(kind, text) => draw_prompt(f, kind, text, f.area()),
        Mode::Confirm(what) => draw_confirm(f, app, what, f.area()),
        Mode::Settings { cursor } => draw_settings(f, app, *cursor, f.area()),
        Mode::Copy { items, cursor } => draw_copy(f, app, items, *cursor, f.area()),
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
        return ("✗", th().dim);
    }
    match status {
        "working" => ("●", th().yellow),
        "blocked" => ("▲", th().red),
        "idle" => ("○", th().green),
        "done" => ("✓", th().blue),
        _ => ("·", th().dim),
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
        "blocked" => ("needs you", th().red),
        "error" => ("error", th().red),
        _ => ("your turn", th().yellow),
    };
    Span::styled(text, Style::new().fg(color).add_modifier(Modifier::BOLD))
}

/// Right-hand tag naming the agent (`claude`, `codex`...), if hc detected one.
fn app_tag(term: Option<&crate::app::Term>) -> Span<'static> {
    let name = term.map(|x| x.app.replace("-code", "")).unwrap_or_default();
    Span::styled(name, Style::new().fg(th().dim))
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
                    "connected" | "local" => th().aqua,
                    "connecting" => th().yellow,
                    _ => th().red,
                };
                let up = matches!(state.as_str(), "connected" | "local");
                let note = if state == "local" {
                    Span::styled("this pc", Style::new().fg(th().dim))
                } else {
                    Span::styled(state.clone(), Style::new().fg(color))
                };
                // 💻 is two columns wide; `fitted` measures it that way.
                ListItem::new(fitted(
                    vec![Span::raw("💻 ")],
                    name,
                    Style::new()
                        .fg(if up { th().fg } else { th().dim })
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
                    .fg(if is_current { th().orange } else { th().dim })
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
                        Style::new().fg(th().yellow).add_modifier(Modifier::BOLD),
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
                    .unwrap_or(("·", th().dim));
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
                let mut name_style = Style::new().fg(th().fg);
                if is_current {
                    name_style = name_style.add_modifier(Modifier::BOLD);
                }
                let tag = match (waiting, split) {
                    (Some(w), false) => wait_tag(&w.status),
                    (Some(_), true) => Span::styled(
                        "▾",
                        Style::new().fg(th().yellow).add_modifier(Modifier::BOLD),
                    ),
                    (None, _) => app_tag(worst),
                };
                let line = fitted(
                    vec![
                        Span::styled(
                            if is_current { " ▌" } else { "  " },
                            Style::new().fg(th().yellow),
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
                    item.style(Style::new().bg(th().wait_bg))
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
                    .unwrap_or(("·", th().dim));
                let name = term.map(|x| x.display()).unwrap_or_else(|| address.clone());
                let last = *pi + 1 == t.terminals.len();
                let waiting = app.waiting(address);
                let name_style = if is_focused {
                    Style::new().fg(th().fg).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(if is_current || waiting {
                        th().fg
                    } else {
                        th().dim
                    })
                };
                let tag = match term {
                    Some(x) if waiting => wait_tag(&x.status),
                    _ => app_tag(term),
                };
                let line = fitted(
                    vec![
                        Span::styled(
                            if last { "   └ " } else { "   ├ " },
                            Style::new().fg(if is_focused { th().yellow } else { th().bg2 }),
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
                    item.style(Style::new().bg(th().wait_bg))
                } else {
                    item
                }
            }
            Row::LooseHeader => ListItem::new(Line::from(Span::styled(
                " unplaced · enter adds a tab",
                Style::new().fg(th().dim).add_modifier(Modifier::ITALIC),
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
                    Style::new().fg(if waiting { th().fg } else { th().dim }),
                    if waiting {
                        wait_tag(&t.status)
                    } else {
                        app_tag(Some(t))
                    },
                    inner_w,
                );
                let item = ListItem::new(line);
                if waiting {
                    item.style(Style::new().bg(th().wait_bg))
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
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(if navigating { th().yellow } else { th().bg2 }))
        .title(Span::styled(
            title,
            Style::new().fg(if navigating { th().yellow } else { th().dim }),
        ))
        .style(Style::new().bg(th().bg));
    let inner = block.inner(area);
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(th().bg2).add_modifier(Modifier::BOLD));
    let mut state = ListState::default().with_selected(selected);
    f.render_stateful_widget(list, area, &mut state);
    app.sidebar_view.set((inner, state.offset()));
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
                .style(Style::new().fg(th().dim).bg(th().bg))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            rect,
        );
    }
}

/// A pane cell colour under the theme: defaults and the 16 basic colours follow it.
fn vt_color(c: vt100::Color, background: bool) -> Color {
    let t = th();
    match c {
        vt100::Color::Default => match t.pane {
            Some((fg, bg)) => {
                if background {
                    bg
                } else {
                    fg
                }
            }
            None => Color::Reset,
        },
        vt100::Color::Idx(i) if i < 16 => t.ansi.map_or(Color::Indexed(i), |a| a[i as usize]),
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
        f.render_widget(Paragraph::new(help).style(Style::new().fg(th().dim)), area);
        return;
    }
    let focused = app.focused();
    let focus_rect = focused
        .as_ref()
        .and_then(|a| app.placed.iter().find(|(p, _)| p == a))
        .map(|(_, r)| *r);
    for d in &app.dividers {
        let (rect, vertical) = (&d.rect, &d.vertical);
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
        let style = Style::new().fg(if touches { th().yellow } else { th().bg2 });
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
    // A divider being dragged: where it will land.
    if let Some((d, at)) = &app.drag {
        let line = if d.vertical {
            Rect {
                x: *at,
                width: 1,
                ..d.rect
            }
        } else {
            Rect {
                y: *at,
                height: 1,
                ..d.rect
            }
        };
        let buf = f.buffer_mut();
        for y in line.y..line.y + line.height {
            for x in line.x..line.x + line.width {
                if let Some(c) = buf.cell_mut((x, y)) {
                    c.set_symbol(if d.vertical { "┃" } else { "━" })
                        .set_style(Style::new().fg(th().yellow));
                }
            }
        }
    }
    for (address, rect) in &app.placed {
        let is_focused = focused.as_deref() == Some(address.as_str());
        match app.panes.get(address) {
            Some(pane) => draw_pane(f, app, address, pane, *rect, is_focused),
            None => f.render_widget(
                Paragraph::new(format!(" attaching {address}…")).style(Style::new().fg(th().dim)),
                *rect,
            ),
        }
    }
}

fn draw_pane(f: &mut Frame, app: &App, address: &str, pane: &Pane, area: Rect, focused: bool) {
    let screen = pane.parser.screen();
    let sel = app.selection.as_ref().filter(|s| s.address == address);
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
            if sel.is_some_and(|s| s.covers(r, c)) {
                m ^= Modifier::REVERSED;
            }
            let style = Style::new()
                .fg(vt_color(cell.fgcolor(), false))
                .bg(vt_color(cell.bgcolor(), true))
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
            Paragraph::new(note).style(Style::new().fg(th().ink).bg(th().dim)),
            rect,
        );
        return;
    }
    let scrolled = screen.scrollback();
    if scrolled > 0 {
        let note = format!(" ↑ {scrolled} lines · type or wheel down ");
        let w = (note.chars().count() as u16).min(area.width);
        let rect = Rect {
            x: area.x + area.width.saturating_sub(w),
            y: area.y,
            width: w,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(note).style(Style::new().fg(th().ink).bg(th().yellow)),
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
            Style::new()
                .fg(th().ink)
                .bg(th().yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Mode::Navigate => Span::styled(
            " NAV ",
            Style::new()
                .fg(th().ink)
                .bg(th().aqua)
                .add_modifier(Modifier::BOLD),
        ),
        Mode::Resize => Span::styled(
            " RESIZE h/j/k/l esc ",
            Style::new()
                .fg(th().ink)
                .bg(th().orange)
                .add_modifier(Modifier::BOLD),
        ),
        Mode::Picker { .. } => Span::styled(
            " GOTO ",
            Style::new()
                .fg(th().ink)
                .bg(th().blue)
                .add_modifier(Modifier::BOLD),
        ),
        _ => Span::styled(" hc ", Style::new().fg(th().ink).bg(th().dim)),
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
            Style::new().fg(th().orange),
        ));
    }
    if let Some(term) = app.focused().and_then(|a| app.term(&a)) {
        spans.push(Span::styled(
            format!("{} ", term.address),
            Style::new().fg(th().dim),
        ));
        spans.push(Span::styled(
            truncate(&term.cwd, 36),
            Style::new().fg(th().fg),
        ));
    }
    if app.zoomed.is_some() {
        spans.push(Span::styled(" [zoom]", Style::new().fg(th().aqua)));
    }
    if let Some((msg, _)) = &app.message {
        spans.push(Span::styled(
            format!("  {msg}"),
            Style::new().fg(th().yellow),
        ));
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
    // A Claude pane in focus gets a clickable copy chip (same as ctrl+b y).
    let claude = app
        .focused()
        .and_then(|a| app.term(&a))
        .is_some_and(|t| t.app == "claude-code");
    let chip = if claude { " copy code " } else { "" };
    let left_w: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let right_w = chip.chars().count() + 1 + badge.chars().count() + right.chars().count();
    let pad = (area.width as usize).saturating_sub(left_w + right_w);
    spans.push(Span::raw(" ".repeat(pad)));
    let chip_x = area.x + (left_w + pad).min(area.width as usize) as u16;
    app.copy_chip.set(if claude {
        Rect::new(chip_x, area.y, chip.chars().count() as u16, 1)
    } else {
        Rect::default()
    });
    spans.push(Span::styled(
        chip,
        Style::new()
            .fg(th().ink)
            .bg(th().aqua)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        badge,
        Style::new()
            .fg(th().ink)
            .bg(th().yellow)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(right, Style::new().fg(th().dim)));
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::new().bg(th().bg2)),
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
        .border_style(Style::new().fg(th().yellow))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(th().yellow).add_modifier(Modifier::BOLD),
        ))
        .style(Style::new().bg(th().bg).fg(th().fg))
}

fn draw_help(f: &mut Frame, area: Rect) {
    let rect = centered(area, 66, HELP.len() as u16 + 2);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!(" {k:<18}"), Style::new().fg(th().aqua)),
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
                Style::new().fg(th().orange).add_modifier(Modifier::BOLD),
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
            Span::styled(t.display(), Style::new().fg(th().fg)),
            Span::styled(
                format!("  {tab}  {} {}", t.app, t.status),
                Style::new().fg(th().dim),
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
    let list =
        List::new(items).highlight_style(Style::new().bg(th().bg2).add_modifier(Modifier::BOLD));
    let mut state = ListState::default().with_selected(selected);
    f.render_stateful_widget(list, list_area, &mut state);
    if let Some(t) = app.terms.get(app.picker_cursor) {
        f.render_widget(
            Paragraph::new(format!(" {} · {}", t.address, t.cwd)).style(Style::new().fg(th().dim)),
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

/// The settings popup: one row per setting, ‹ value › to change.
fn draw_settings(f: &mut Frame, app: &App, cursor: usize, area: Rect) {
    let rows = app.settings_rows();
    let rect = centered(area, 46, rows.len() as u16 + 4);
    app.settings_view.set(rect);
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, (name, value))| {
            let on = i == cursor;
            let mark = if on { "▸ " } else { "  " };
            let style = if on {
                Style::new().fg(th().fg).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(th().fg)
            };
            Line::from(vec![
                Span::styled(format!(" {mark}"), Style::new().fg(th().yellow)),
                Span::styled(format!("{name:<17}"), style),
                Span::styled("‹ ", Style::new().fg(th().dim)),
                Span::styled(
                    value.clone(),
                    Style::new().fg(if on { th().yellow } else { th().aqua }),
                ),
                Span::styled(" ›", Style::new().fg(th().dim)),
            ])
        })
        .chain([
            Line::raw(""),
            Line::from(Span::styled(
                " j/k move · ←/→ or click change · esc close",
                Style::new().fg(th().dim),
            )),
        ])
        .collect();
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(lines).block(popup("settings")), rect);
}

/// The copy picker: Claude's code blocks, newest first, with a preview.
fn draw_copy(f: &mut Frame, app: &App, items: &[CopyItem], cursor: usize, area: Rect) {
    const PREVIEW: usize = 10;
    let w = area.width.saturating_sub(4).min(100);
    let shown = items
        .len()
        .min(area.height.saturating_sub(PREVIEW as u16 + 6) as usize);
    let rect = centered(area, w, shown as u16 + PREVIEW as u16 + 4);
    app.copy_view.set(rect);
    let inner_w = w.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = items
        .iter()
        .take(shown)
        .enumerate()
        .map(|(i, it)| {
            let on = i == cursor;
            let key = if i < 9 {
                format!(" {} ", i + 1)
            } else {
                "   ".into()
            };
            Line::from(vec![
                Span::styled(key, Style::new().fg(th().dim)),
                Span::styled(
                    truncate(&it.label, inner_w.saturating_sub(4)),
                    if on {
                        Style::new()
                            .fg(th().ink)
                            .bg(th().yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(th().fg)
                    },
                ),
            ])
        })
        .collect();
    lines.push(Line::from(Span::styled(
        "─".repeat(inner_w),
        Style::new().fg(th().bg2),
    )));
    if let Some(it) = items.get(cursor) {
        let total = it.text.lines().count();
        for l in it.text.lines().take(PREVIEW) {
            lines.push(Line::from(Span::styled(
                truncate(&format!(" {l}"), inner_w),
                Style::new().fg(th().aqua),
            )));
        }
        if total > PREVIEW {
            lines.push(Line::from(Span::styled(
                format!(" … {} more", total - PREVIEW),
                Style::new().fg(th().dim),
            )));
        }
    }
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines).block(popup("copy · enter or 1-9 copies · click · esc")),
        rect,
    );
}
