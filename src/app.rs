//! State and actions. Workspaces, tabs and splits live in the Herdr app's
//! layout document on each machine's core (`layout.rs`), so hc-tui, the Herdr
//! desktop app and the iPhone app all show the same arrangement.

use crate::hc::{self, Conn, Incoming};
use crate::keys::{self, Action, Dir, Key};
use crate::layout;
use crate::predict::Predictor;
use anyhow::Result;
use ratatui::layout::Rect;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub fn nz(n: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(n.max(1)).unwrap_or(std::num::NonZeroU16::MIN)
}

/// `t7` on machine `vps` is `vps/t7`; on this machine just `t7`.
pub fn qualify(on: &Option<String>, id: &str) -> String {
    match on {
        Some(m) => format!("{m}/{id}"),
        None => id.to_string(),
    }
}

fn unqualify<'a>(on: &Option<String>, address: &'a str) -> &'a str {
    match on {
        Some(m) => address
            .strip_prefix(m.as_str())
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(address),
        None => address,
    }
}

#[derive(Clone, Debug)]
pub struct Term {
    pub address: String,
    pub on: Option<String>,
    pub name: Option<String>,
    pub title: String,
    pub cwd: String,
    pub status: String,
    pub app: String,
    pub live: bool,
}

impl Term {
    fn from_json(v: &Value) -> Term {
        let address = v["address"].as_str().unwrap_or("").to_string();
        let on = address.split_once('/').map(|(m, _)| m.to_string());
        Term {
            on,
            name: v["name"].as_str().map(str::to_string),
            title: v["title"].as_str().unwrap_or("").to_string(),
            cwd: v["cwd"].as_str().unwrap_or("").to_string(),
            status: v["status"]["state"].as_str().unwrap_or("").to_string(),
            app: v["status"]["app"].as_str().unwrap_or("").to_string(),
            live: v["state"].as_str() != Some("finished"),
            address,
        }
    }

    pub fn display(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        if !self.title.is_empty() {
            return self.title.clone();
        }
        self.address.clone()
    }
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: String,
    pub name: String,
    pub root: Value,
    /// Qualified addresses in reading order.
    pub terminals: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Space {
    pub on: Option<String>,
    pub id: String,
    pub name: String,
    pub folder: String,
    pub tabs: Vec<Tab>,
}

impl Space {
    pub fn label(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        let base = self
            .folder
            .trim_end_matches(['\\', '/'])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("");
        if base.is_empty() {
            "workspace".into()
        } else {
            base.to_string()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabRef {
    pub on: Option<String>,
    pub space: String,
    pub tab: String,
}

#[derive(Clone, Debug)]
pub enum Row {
    Machine(String, String),
    Space(usize),
    Tab(usize, usize),
    LooseHeader,
    Loose(usize),
}

pub struct Pane {
    pub channel: u32,
    pub parser: vt100::Parser,
    pub size: (u16, u16),
    pub ended: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    RenameTab,
    NewWorkspace,
    RenameWorkspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Confirm {
    ClosePane(String),
    CloseTab(TabRef),
    CloseWorkspace(Option<String>, String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Prefix,
    Navigate,
    Resize,
    Picker {
        query: String,
        editing: bool,
        filter: Option<char>,
    },
    Help,
    Prompt(Prompt, String),
    Confirm(Confirm),
}

pub struct App {
    pub conn: Conn,
    pub local: String,
    pub terms: Vec<Term>,
    pub machines: Vec<(String, String)>,
    pub spaces: Vec<Space>,
    pub loose: Vec<usize>,
    pub rows: Vec<Row>,
    pub current: Option<TabRef>,
    focus: HashMap<String, String>,
    pub zoomed: Option<String>,
    pub panes: HashMap<String, Pane>,
    pub placed: Vec<(String, Rect)>,
    pub dividers: Vec<(Rect, bool)>,
    tab_area: Rect,
    pub mode: Mode,
    pub sidebar: bool,
    pub predict: Predictor,
    pub message: Option<(String, Instant)>,
    pub quit: bool,
    pub nav_cursor: usize,
    pub picker_cursor: usize,
    refresh_due: Option<Instant>,
    doc_errors: Vec<String>,
}

impl App {
    pub fn new(conn: Conn, predict: bool) -> App {
        let local = conn.hello["core"]["name"]
            .as_str()
            .unwrap_or("local")
            .to_string();
        App {
            conn,
            local,
            terms: Vec::new(),
            machines: Vec::new(),
            spaces: Vec::new(),
            loose: Vec::new(),
            rows: Vec::new(),
            current: None,
            focus: HashMap::new(),
            zoomed: None,
            panes: HashMap::new(),
            placed: Vec::new(),
            dividers: Vec::new(),
            tab_area: Rect::new(0, 0, 80, 24),
            mode: Mode::Normal,
            sidebar: true,
            predict: Predictor::new(predict),
            message: None,
            quit: false,
            nav_cursor: 0,
            picker_cursor: 0,
            refresh_due: None,
            doc_errors: Vec::new(),
        }
    }

    pub fn say(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    fn report<T>(&mut self, r: Result<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.say(format!("{e:#}"));
                None
            }
        }
    }

    pub fn term(&self, address: &str) -> Option<&Term> {
        self.terms.iter().find(|t| t.address == address)
    }

    pub fn machine_name(&self, on: &Option<String>) -> String {
        on.clone().unwrap_or_else(|| self.local.clone())
    }

    // ----- model ----------------------------------------------------------

    pub fn tick(&mut self) {
        if self.refresh_due.is_some_and(|t| Instant::now() >= t) {
            self.refresh_due = None;
            self.refresh();
        }
        if self
            .message
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(6))
        {
            self.message = None;
        }
    }

    fn schedule_refresh(&mut self) {
        let soon = Instant::now() + Duration::from_millis(120);
        if self.refresh_due.is_none_or(|t| t > soon) {
            self.refresh_due = Some(soon);
        }
    }

    pub fn refresh(&mut self) {
        if !self.refresh_model() {
            return;
        }
        if self.current.as_ref().is_none_or(|c| self.tab(c).is_none()) {
            self.current = self.spaces.iter().find_map(|s| {
                s.tabs.first().map(|t| TabRef {
                    on: s.on.clone(),
                    space: s.id.clone(),
                    tab: t.id.clone(),
                })
            });
        }
        self.relayout();
    }

    /// Terminals, machines and layouts from the core; no attaching.
    pub fn refresh_model(&mut self) -> bool {
        let Some(list) = self.report(self.conn.call("terminal.list", json!({}))) else {
            return false;
        };
        self.terms = list["terminals"]
            .as_array()
            .map(|a| a.iter().map(Term::from_json).collect())
            .unwrap_or_default();
        self.machines = list["machines"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|m| {
                        (
                            m["name"].as_str().unwrap_or("").to_string(),
                            m["state"].as_str().unwrap_or("").to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut machines: Vec<Option<String>> = vec![None];
        machines.extend(
            self.machines
                .iter()
                .filter(|(_, s)| s == "connected")
                .map(|(n, _)| Some(n.clone())),
        );
        let mut spaces = Vec::new();
        let mut placed_ok: Vec<Option<String>> = Vec::new();
        self.doc_errors.clear();
        for on in machines {
            match self
                .conn
                .call("doc.get", json!({"name": layout::DOC, "on": on}))
            {
                Ok(r) => {
                    placed_ok.push(on.clone());
                    for s in r["value"]["spaces"].as_array().into_iter().flatten() {
                        spaces.push(Space {
                            on: on.clone(),
                            id: s["id"].as_str().unwrap_or("").to_string(),
                            name: s["name"].as_str().unwrap_or("").to_string(),
                            folder: s["folder"].as_str().unwrap_or("").to_string(),
                            tabs: s["tabs"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .map(|t| {
                                    let mut ids = Vec::new();
                                    layout::terminals(&t["root"], &mut ids);
                                    Tab {
                                        id: t["id"].as_str().unwrap_or("").to_string(),
                                        name: t["name"].as_str().unwrap_or("").to_string(),
                                        root: t["root"].clone(),
                                        terminals: ids.iter().map(|i| qualify(&on, i)).collect(),
                                    }
                                })
                                .collect(),
                        });
                    }
                }
                Err(e) => self
                    .doc_errors
                    .push(format!("{}: {e:#}", self.machine_name(&on))),
            }
        }
        self.spaces = spaces;
        self.loose = (0..self.terms.len())
            .filter(|&i| {
                let t = &self.terms[i];
                t.live
                    && placed_ok.contains(&t.on)
                    && !self
                        .spaces
                        .iter()
                        .any(|s| s.tabs.iter().any(|tab| tab.terminals.contains(&t.address)))
            })
            .collect();
        self.rebuild_rows();
        true
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        let many = !self.machines.is_empty();
        let mut machines: Vec<Option<String>> = vec![None];
        machines.extend(self.machines.iter().map(|(n, _)| Some(n.clone())));
        for on in machines {
            if many {
                let state = match &on {
                    None => "local".to_string(),
                    Some(n) => self
                        .machines
                        .iter()
                        .find(|(m, _)| m == n)
                        .map(|(_, s)| s.clone())
                        .unwrap_or_default(),
                };
                rows.push(Row::Machine(self.machine_name(&on), state));
            }
            for (si, s) in self.spaces.iter().enumerate().filter(|(_, s)| s.on == on) {
                rows.push(Row::Space(si));
                for ti in 0..s.tabs.len() {
                    rows.push(Row::Tab(si, ti));
                }
            }
        }
        if !self.loose.is_empty() {
            rows.push(Row::LooseHeader);
            rows.extend(self.loose.iter().map(|&i| Row::Loose(i)));
        }
        self.rows = rows;
    }

    pub fn tab(&self, r: &TabRef) -> Option<(&Space, &Tab)> {
        let s = self
            .spaces
            .iter()
            .find(|s| s.on == r.on && s.id == r.space)?;
        let t = s.tabs.iter().find(|t| t.id == r.tab)?;
        Some((s, t))
    }

    pub fn current_tab(&self) -> Option<(&Space, &Tab)> {
        self.current.as_ref().and_then(|c| self.tab(c))
    }

    fn tab_key(r: &TabRef) -> String {
        format!("{}:{}", r.on.as_deref().unwrap_or(""), r.tab)
    }

    pub fn focused(&self) -> Option<String> {
        let c = self.current.as_ref()?;
        let f = self.focus.get(&Self::tab_key(c))?;
        self.placed.iter().any(|(a, _)| a == f).then(|| f.clone())
    }

    fn set_focus(&mut self, address: &str) {
        if let Some(c) = &self.current {
            self.focus.insert(Self::tab_key(c), address.to_string());
        }
        self.predict.clear();
    }

    // ----- panes ----------------------------------------------------------

    pub fn set_area(&mut self, area: Rect) {
        if area != self.tab_area {
            self.tab_area = area;
            self.relayout();
        }
    }

    /// Lays out the current tab, attaching what it shows and letting go of the rest.
    pub fn relayout(&mut self) {
        let (placed, dividers) = match self.current_tab() {
            Some((space, tab)) => {
                let zoomed_local = self
                    .zoomed
                    .as_deref()
                    .map(|z| unqualify(&space.on, z).to_string());
                let p = layout::place(&tab.root, self.tab_area, zoomed_local.as_deref());
                let on = space.on.clone();
                (
                    p.panes
                        .into_iter()
                        .map(|(id, r)| (qualify(&on, &id), r))
                        .collect::<Vec<_>>(),
                    p.dividers,
                )
            }
            None => (Vec::new(), Vec::new()),
        };
        self.placed = placed;
        self.dividers = dividers;
        let wanted: Vec<String> = self.placed.iter().map(|(a, _)| a.clone()).collect();
        let gone: Vec<String> = self
            .panes
            .keys()
            .filter(|a| !wanted.contains(a))
            .cloned()
            .collect();
        for a in gone {
            if let Some(p) = self.panes.remove(&a)
                && p.ended.is_none()
            {
                self.conn.send(p.channel, hc::CLOSE, &[]);
            }
        }
        for (address, rect) in self.placed.clone() {
            let size = (rect.width.max(1), rect.height.max(1));
            match self.panes.get_mut(&address) {
                Some(p) => {
                    if p.size != size {
                        p.size = size;
                        p.parser.set_size(nz(size.1), nz(size.0));
                        if p.ended.is_none() {
                            let mut b = size.0.to_le_bytes().to_vec();
                            b.extend(size.1.to_le_bytes());
                            self.conn.send(p.channel, hc::RESIZE, &b);
                        }
                    }
                }
                None => self.attach(&address, size),
            }
        }
        if self.focused().is_none()
            && let Some((a, _)) = self.placed.first().cloned()
        {
            self.set_focus(&a);
        }
        if let Some(f) = self.focused()
            && let Some(p) = self.panes.get(&f)
            && p.ended.is_none()
        {
            self.conn.send(p.channel, hc::FOCUS, &[1]);
        }
    }

    fn attach(&mut self, address: &str, (cols, rows): (u16, u16)) {
        let mut pane = Pane {
            channel: 0,
            parser: vt100::Parser::new(nz(rows), nz(cols), 0),
            size: (cols, rows),
            ended: None,
        };
        let params = json!({"terminal": address, "cols": cols, "rows": rows, "history": 0});
        match self.conn.call("terminal.attach", params) {
            Ok(r) => pane.channel = r["channel"].as_u64().unwrap_or(0) as u32,
            Err(e) => pane.ended = Some(format!("{e:#}")),
        }
        self.panes.insert(address.to_string(), pane);
    }

    pub fn incoming(&mut self, msg: Incoming) {
        match msg {
            Incoming::Stream { channel, data } => {
                self.conn
                    .send(channel, hc::CREDIT, &(data.len() as u32).to_le_bytes());
                let focused = self.focused();
                if let Some((address, p)) = self
                    .panes
                    .iter_mut()
                    .find(|(_, p)| p.channel == channel && p.ended.is_none())
                {
                    p.parser.process(&data);
                    if focused.as_deref() == Some(address.as_str()) {
                        self.predict.reconcile(p.parser.screen());
                    }
                }
            }
            Incoming::Closed { channel, reason } => {
                if let Some(p) = self
                    .panes
                    .values_mut()
                    .find(|p| p.channel == channel && p.ended.is_none())
                {
                    p.ended = Some(
                        reason
                            .as_ref()
                            .and_then(|r| r["reason"].as_str().map(str::to_string))
                            .unwrap_or_else(|| "ended".into()),
                    );
                }
                self.schedule_refresh();
            }
            Incoming::Changed => self.schedule_refresh(),
            Incoming::Lost(why) => self.say(format!("{why}; restart hc-tui once the core is back")),
        }
    }

    pub fn shutdown(&mut self) {
        for (_, p) in self.panes.drain() {
            if p.ended.is_none() {
                self.conn.send(p.channel, hc::CLOSE, &[]);
            }
        }
    }

    // ----- input ----------------------------------------------------------

    pub fn input(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            if self.mode == Mode::Normal {
                match bytes.iter().position(|&b| b == 0x02) {
                    Some(at) => {
                        self.forward(&bytes[..at]);
                        self.mode = Mode::Prefix;
                        bytes = &bytes[at + 1..];
                    }
                    None => {
                        self.forward(bytes);
                        return;
                    }
                }
                continue;
            }
            let (key, used) = keys::parse_key(bytes);
            bytes = &bytes[used.max(1)..];
            self.key(key);
        }
    }

    fn forward(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let Some(f) = self.focused() else { return };
        if let Some(p) = self.panes.get(&f)
            && p.ended.is_none()
        {
            self.predict.typed(bytes, p.parser.screen());
            self.conn.send(p.channel, hc::DATA, bytes);
        }
    }

    fn key(&mut self, key: Key) {
        match self.mode.clone() {
            Mode::Normal => {}
            Mode::Prefix => {
                self.mode = Mode::Normal;
                match keys::prefix_action(key) {
                    Some(action) => self.action(action),
                    None => self.say("unbound key after prefix (ctrl+b ? for help)"),
                }
            }
            Mode::Navigate => self.navigate_key(key),
            Mode::Resize => self.resize_key(key),
            Mode::Picker { .. } => self.picker_key(key),
            Mode::Help => self.mode = Mode::Normal,
            Mode::Prompt(kind, text) => self.prompt_key(kind, text, key),
            Mode::Confirm(what) => {
                self.mode = Mode::Normal;
                if matches!(key, Key::Char('y' | 'Y') | Key::Enter) {
                    self.confirmed(what);
                }
            }
        }
    }

    fn action(&mut self, action: Action) {
        use Action::*;
        match action {
            NewTab => self.new_tab(),
            SplitVertical => self.split("row"),
            SplitHorizontal => self.split("column"),
            Focus(d) => {
                if let Some(a) = self.neighbor(d) {
                    self.set_focus(&a);
                    self.relayout();
                }
            }
            Swap(d) => self.swap(d),
            NavigateWorkspace => {
                self.sidebar = true;
                self.nav_cursor = self.current_row().unwrap_or(0);
                self.mode = Mode::Navigate;
            }
            Detach => self.quit = true,
            Zoom => {
                self.zoomed = if self.zoomed.is_some() {
                    None
                } else {
                    self.focused()
                };
                self.relayout();
            }
            ClosePane => {
                if let Some(f) = self.focused() {
                    self.mode = Mode::Confirm(Confirm::ClosePane(f));
                }
            }
            ResizeMode => {
                if self.placed.len() > 1 {
                    self.mode = Mode::Resize;
                } else {
                    self.say("nothing to resize: the tab has one pane");
                }
            }
            CopyMode => self.say("copy mode is not in hc-tui yet"),
            NextTab => self.step_tab(1),
            PreviousTab => self.step_tab(-1),
            SwitchTab(n) => {
                if let Some(c) = self.current.clone()
                    && let Some(s) = self.spaces.iter().find(|s| s.on == c.on && s.id == c.space)
                    && let Some(t) = s.tabs.get(n)
                {
                    let r = TabRef {
                        on: c.on,
                        space: c.space,
                        tab: t.id.clone(),
                    };
                    self.activate(r);
                }
            }
            RenameTab => {
                let name = self
                    .current_tab()
                    .map(|(_, t)| t.name.clone())
                    .unwrap_or_default();
                self.mode = Mode::Prompt(Prompt::RenameTab, name);
            }
            CloseTab => {
                if let Some(c) = self.current.clone() {
                    self.mode = Mode::Confirm(Confirm::CloseTab(c));
                }
            }
            NewWorkspace => self.mode = Mode::Prompt(Prompt::NewWorkspace, String::new()),
            RenameWorkspace => {
                let name = self
                    .current_tab()
                    .map(|(s, _)| s.name.clone())
                    .unwrap_or_default();
                self.mode = Mode::Prompt(Prompt::RenameWorkspace, name);
            }
            CloseWorkspace => {
                if let Some(c) = self.current.clone() {
                    self.mode = Mode::Confirm(Confirm::CloseWorkspace(c.on, c.space));
                }
            }
            Goto => {
                self.picker_cursor = self
                    .focused()
                    .and_then(|f| self.terms.iter().position(|t| t.address == f))
                    .unwrap_or(0);
                self.mode = Mode::Picker {
                    query: String::new(),
                    editing: false,
                    filter: None,
                };
            }
            ToggleSidebar => self.sidebar = !self.sidebar,
            Help => self.mode = Mode::Help,
            TogglePredict => {
                self.predict.enabled = !self.predict.enabled;
                self.predict.clear();
                self.say(if self.predict.enabled {
                    "predictive echo on"
                } else {
                    "predictive echo off"
                });
            }
            SendPrefix => self.forward(&[0x02]),
            Cancel => {}
        }
    }

    fn activate(&mut self, r: TabRef) {
        if self.current.as_ref() != Some(&r) {
            self.zoomed = None;
            self.current = Some(r);
            self.predict.clear();
            self.relayout();
        }
    }

    fn current_row(&self) -> Option<usize> {
        let c = self.current.as_ref()?;
        self.rows.iter().position(|row| match row {
            Row::Tab(si, ti) => {
                let s = &self.spaces[*si];
                s.on == c.on && s.id == c.space && s.tabs[*ti].id == c.tab
            }
            _ => false,
        })
    }

    fn step_tab(&mut self, delta: isize) {
        let Some(c) = self.current.clone() else {
            return;
        };
        let Some(s) = self.spaces.iter().find(|s| s.on == c.on && s.id == c.space) else {
            return;
        };
        let n = s.tabs.len();
        if n < 2 {
            return;
        }
        let i = s.tabs.iter().position(|t| t.id == c.tab).unwrap_or(0) as isize;
        let t = s.tabs[(i + delta).rem_euclid(n as isize) as usize]
            .id
            .clone();
        self.activate(TabRef {
            on: c.on,
            space: c.space,
            tab: t,
        });
    }

    /// The pane next to the focused one in direction `d`.
    fn neighbor(&self, d: Dir) -> Option<String> {
        let f = self.focused()?;
        let (_, fr) = self.placed.iter().find(|(a, _)| *a == f)?;
        let (fcx, fcy) = (
            fr.x as i32 + fr.width as i32 / 2,
            fr.y as i32 + fr.height as i32 / 2,
        );
        self.placed
            .iter()
            .filter(|(a, _)| *a != f)
            .filter(|(_, r)| {
                let overlap_y = r.y < fr.y + fr.height && fr.y < r.y + r.height;
                let overlap_x = r.x < fr.x + fr.width && fr.x < r.x + r.width;
                match d {
                    Dir::Left => r.x + r.width <= fr.x && overlap_y,
                    Dir::Right => r.x >= fr.x + fr.width && overlap_y,
                    Dir::Up => r.y + r.height <= fr.y && overlap_x,
                    Dir::Down => r.y >= fr.y + fr.height && overlap_x,
                }
            })
            .min_by_key(|(_, r)| {
                let (cx, cy) = (
                    r.x as i32 + r.width as i32 / 2,
                    r.y as i32 + r.height as i32 / 2,
                );
                (cx - fcx).abs() + (cy - fcy).abs()
            })
            .map(|(a, _)| a.clone())
    }

    // ----- layout edits ---------------------------------------------------

    /// Reads the machine's layout fresh, applies `f`, writes it back (retrying
    /// on a version conflict, as the Herdr app does), then refreshes.
    fn edit_doc(&mut self, on: &Option<String>, mut f: impl FnMut(&mut Value) -> bool) -> bool {
        for _ in 0..3 {
            let read = self
                .conn
                .call("doc.get", json!({"name": layout::DOC, "on": on}));
            let Some(r) = self.report(read) else {
                return false;
            };
            let version = r["version"].as_u64().unwrap_or(0);
            let mut value = if r["value"].is_null() {
                layout::empty_doc()
            } else {
                r["value"].clone()
            };
            if !f(&mut value) {
                return false;
            }
            match self.conn.call(
                "doc.set",
                json!({"name": layout::DOC, "on": on, "value": value, "version": version}),
            ) {
                Ok(_) => {
                    self.refresh();
                    return true;
                }
                Err(e) if format!("{e:#}").contains("conflict") => continue,
                Err(e) => {
                    self.say(format!("{e:#}"));
                    return false;
                }
            }
        }
        self.say("the layout kept changing under us; try again");
        false
    }

    /// Starts a shell on `on` in `cwd`; returns its id on that machine.
    fn spawn_shell(&mut self, on: &Option<String>, cwd: Option<String>) -> Option<String> {
        let mut params = json!({
            "argv": [],
            "no_parent": true,
            "outlive_parent": true,
            "env": {"TERM": "xterm-256color", "COLORTERM": "truecolor", "TERM_PROGRAM": "hc-tui"},
        });
        if let Some(cwd) = cwd.filter(|c| !c.is_empty()) {
            params["cwd"] = Value::String(cwd);
        }
        if let Some(m) = on {
            params["on"] = Value::String(m.clone());
        }
        let r = self.conn.call("terminal.run", params);
        let result = self.report(r)?;
        let address = result["address"]
            .as_str()
            .or_else(|| result["terminal"]["address"].as_str())?;
        Some(unqualify(on, address).to_string())
    }

    fn focused_cwd(&self) -> Option<String> {
        let f = self.focused()?;
        self.term(&f).map(|t| t.cwd.clone())
    }

    fn new_tab(&mut self) {
        let Some(c) = self.current.clone() else {
            // No layout yet: a first workspace.
            self.new_workspace("main".into());
            return;
        };
        let folder = self.current_tab().map(|(s, _)| s.folder.clone());
        let cwd = self.focused_cwd().or(folder);
        let Some(id) = self.spawn_shell(&c.on, cwd) else {
            return;
        };
        let new_tab = layout::tab("", &id);
        let tab_id = new_tab["id"].as_str().unwrap_or("").to_string();
        let space = c.space.clone();
        if self.edit_doc(&c.on, |doc| {
            let Some(s) = layout::find_space(doc, &space) else {
                return false;
            };
            if !s["tabs"].is_array() {
                s["tabs"] = json!([]);
            }
            s["tabs"].as_array_mut().unwrap().push(new_tab.clone());
            true
        }) {
            self.activate(TabRef {
                on: c.on.clone(),
                space: c.space,
                tab: tab_id,
            });
            self.set_focus(&qualify(&c.on, &id));
            self.relayout();
        }
    }

    fn new_workspace(&mut self, name: String) {
        let on = self.current.as_ref().and_then(|c| c.on.clone());
        let cwd = self.focused_cwd().or_else(|| {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .ok()
        });
        let Some(id) = self.spawn_shell(&on, cwd.clone()) else {
            return;
        };
        let tab = layout::tab("", &id);
        let tab_id = tab["id"].as_str().unwrap_or("").to_string();
        let mut sp = layout::space(&name, cwd.as_deref().unwrap_or(""), tab);
        if on.is_some() {
            // The folder is a path on that machine; we only know the shell's.
            sp["folder"] = Value::String(String::new());
        }
        let space_id = sp["id"].as_str().unwrap_or("").to_string();
        if self.edit_doc(&on, |doc| {
            layout::spaces_mut(doc).push(sp.clone());
            true
        }) {
            self.activate(TabRef {
                on: on.clone(),
                space: space_id,
                tab: tab_id,
            });
            self.set_focus(&qualify(&on, &id));
            self.relayout();
        }
    }

    fn split(&mut self, axis: &str) {
        let (Some(c), Some(f)) = (self.current.clone(), self.focused()) else {
            self.new_tab();
            return;
        };
        let target = unqualify(&c.on, &f).to_string();
        let cwd = self.focused_cwd();
        let Some(id) = self.spawn_shell(&c.on, cwd) else {
            return;
        };
        let (space, tab) = (c.space.clone(), c.tab.clone());
        self.zoomed = None;
        if self.edit_doc(&c.on, |doc| {
            layout::find_tab(doc, &space, &tab)
                .is_some_and(|t| layout::split(&mut t["root"], &target, &id, axis))
        }) {
            self.set_focus(&qualify(&c.on, &id));
            self.relayout();
        }
    }

    fn swap(&mut self, d: Dir) {
        let (Some(c), Some(f), Some(n)) = (self.current.clone(), self.focused(), self.neighbor(d))
        else {
            return;
        };
        let (a, b) = (
            unqualify(&c.on, &f).to_string(),
            unqualify(&c.on, &n).to_string(),
        );
        let (space, tab) = (c.space.clone(), c.tab.clone());
        self.edit_doc(&c.on, |doc| match layout::find_tab(doc, &space, &tab) {
            Some(t) => {
                layout::swap(&mut t["root"], &a, &b);
                true
            }
            None => false,
        });
    }

    fn resize_key(&mut self, key: Key) {
        let (axis, delta) = match key {
            Key::Char('h') | Key::Left => ("row", -1.0),
            Key::Char('l') | Key::Right => ("row", 1.0),
            Key::Char('k') | Key::Up => ("column", -1.0),
            Key::Char('j') | Key::Down => ("column", 1.0),
            _ => {
                self.mode = Mode::Normal;
                return;
            }
        };
        let (Some(c), Some(f)) = (self.current.clone(), self.focused()) else {
            return;
        };
        let target = unqualify(&c.on, &f).to_string();
        let (space, tab) = (c.space.clone(), c.tab.clone());
        self.edit_doc(&c.on, |doc| {
            layout::find_tab(doc, &space, &tab)
                .is_some_and(|t| layout::resize(&mut t["root"], &target, axis, delta))
        });
    }

    /// Removes a terminal's block from its tab; drops the tab (and the space)
    /// when nothing is left.
    fn unplace(&mut self, on: &Option<String>, space: &str, tab: &str, local_id: &str) {
        self.edit_doc(on, |doc| {
            let Some(s) = layout::find_space(doc, space) else {
                return false;
            };
            let Some(tabs) = s["tabs"].as_array_mut() else {
                return false;
            };
            let Some(ti) = tabs.iter().position(|t| t["id"].as_str() == Some(tab)) else {
                return false;
            };
            if !layout::remove(&mut tabs[ti]["root"], local_id) {
                return false;
            }
            let mut left = Vec::new();
            layout::terminals(&tabs[ti]["root"], &mut left);
            if left.is_empty() {
                tabs.remove(ti);
            }
            if tabs.is_empty() {
                let spaces = layout::spaces_mut(doc);
                spaces.retain(|x| x["id"].as_str() != Some(space));
            }
            true
        });
    }

    fn adopt(&mut self, term_index: usize) {
        let t = self.terms[term_index].clone();
        let on = t.on.clone();
        let id = unqualify(&on, &t.address).to_string();
        let new_tab = layout::tab("", &id);
        let tab_id = new_tab["id"].as_str().unwrap_or("").to_string();
        // Into the current space when it is on the same machine, else that machine's first, else a new one.
        let space = self
            .current
            .as_ref()
            .filter(|c| c.on == on)
            .map(|c| c.space.clone())
            .or_else(|| {
                self.spaces
                    .iter()
                    .find(|s| s.on == on)
                    .map(|s| s.id.clone())
            });
        let fresh = layout::space("hc", "", new_tab.clone());
        let space_id = space
            .clone()
            .unwrap_or_else(|| fresh["id"].as_str().unwrap_or("").to_string());
        if self.edit_doc(&on, |doc| {
            match &space {
                Some(sid) => match layout::find_space(doc, sid) {
                    Some(s) => {
                        if !s["tabs"].is_array() {
                            s["tabs"] = json!([]);
                        }
                        s["tabs"].as_array_mut().unwrap().push(new_tab.clone());
                    }
                    None => return false,
                },
                None => layout::spaces_mut(doc).push(fresh.clone()),
            }
            true
        }) {
            self.activate(TabRef {
                on,
                space: space_id,
                tab: tab_id,
            });
            self.set_focus(&t.address);
            self.relayout();
        }
    }

    /// `--attach`: a terminal by address or name.
    pub fn goto(&mut self, which: &str) {
        let address = self
            .terms
            .iter()
            .find(|t| t.address == which || t.name.as_deref() == Some(which))
            .map(|t| t.address.clone());
        match address {
            Some(a) => self.reveal(&a),
            None => self.say(format!("no terminal {which}")),
        }
    }

    /// Goes to wherever `address` is shown, or adopts it into a tab.
    fn reveal(&mut self, address: &str) {
        let found = self.spaces.iter().find_map(|s| {
            s.tabs
                .iter()
                .find(|t| t.terminals.iter().any(|a| a == address))
                .map(|t| TabRef {
                    on: s.on.clone(),
                    space: s.id.clone(),
                    tab: t.id.clone(),
                })
        });
        match found {
            Some(r) => {
                self.activate(r);
                self.set_focus(address);
                self.relayout();
            }
            None => {
                if let Some(i) = self.terms.iter().position(|t| t.address == address) {
                    self.adopt(i);
                }
            }
        }
    }

    fn navigate_key(&mut self, key: Key) {
        let selectable = |r: &Row| matches!(r, Row::Tab(..) | Row::Loose(_));
        let step = |app: &App, from: usize, down: bool| -> usize {
            let mut i = from;
            loop {
                if down {
                    if i + 1 >= app.rows.len() {
                        return from;
                    }
                    i += 1;
                } else {
                    if i == 0 {
                        return from;
                    }
                    i -= 1;
                }
                if selectable(&app.rows[i]) {
                    return i;
                }
            }
        };
        if !self.rows.get(self.nav_cursor).is_some_and(selectable) {
            self.nav_cursor = step(self, self.nav_cursor, true);
        }
        match key {
            Key::Char('j') | Key::Down => self.nav_cursor = step(self, self.nav_cursor, true),
            Key::Char('k') | Key::Up => self.nav_cursor = step(self, self.nav_cursor, false),
            Key::Left | Key::Right | Key::Char('h') | Key::Char('l') => {
                // First tab of the previous/next workspace, no wrapping.
                let forward = matches!(key, Key::Right | Key::Char('l'));
                let space_of = |r: &Row| match r {
                    Row::Tab(si, _) => Some(*si),
                    _ => None,
                };
                let here = self.rows.get(self.nav_cursor).and_then(space_of);
                let target = if forward {
                    (self.nav_cursor + 1..self.rows.len())
                        .find(|&i| selectable(&self.rows[i]) && space_of(&self.rows[i]) != here)
                } else {
                    (0..self.nav_cursor)
                        .rev()
                        .find(|&i| selectable(&self.rows[i]) && space_of(&self.rows[i]) != here)
                        .map(|end| {
                            let prev = space_of(&self.rows[end]);
                            (0..=end)
                                .find(|&i| {
                                    selectable(&self.rows[i]) && space_of(&self.rows[i]) == prev
                                })
                                .unwrap_or(end)
                        })
                };
                if let Some(t) = target {
                    self.nav_cursor = t;
                }
            }
            Key::Enter => {
                self.mode = Mode::Normal;
                match self.rows.get(self.nav_cursor).cloned() {
                    Some(Row::Tab(si, ti)) => {
                        let s = &self.spaces[si];
                        let r = TabRef {
                            on: s.on.clone(),
                            space: s.id.clone(),
                            tab: s.tabs[ti].id.clone(),
                        };
                        self.activate(r);
                    }
                    Some(Row::Loose(i)) => self.adopt(i),
                    _ => {}
                }
            }
            Key::Esc | Key::Ctrl('[') | Key::Char('q') => self.mode = Mode::Normal,
            _ => {}
        }
    }

    pub fn picker_matches(&self) -> Vec<usize> {
        let Mode::Picker { query, filter, .. } = &self.mode else {
            return Vec::new();
        };
        let q = query.to_lowercase();
        let mut out: Vec<usize> = (0..self.terms.len())
            .filter(|&i| {
                let t = &self.terms[i];
                let status_ok = match filter {
                    Some('b') => t.status == "blocked",
                    Some('w') => t.status == "working",
                    Some('i') => t.status == "idle",
                    Some('d') => t.status == "done",
                    _ => true,
                };
                let place = self.place_of(&t.address).unwrap_or_default();
                let text = format!(
                    "{} {} {} {} {}",
                    t.display(),
                    t.app,
                    place,
                    t.cwd,
                    t.address
                )
                .to_lowercase();
                t.live && status_ok && (q.is_empty() || text.contains(&q))
            })
            .collect();
        out.sort_by_key(|&i| {
            self.place_of(&self.terms[i].address)
                .unwrap_or_else(|| "~".into())
        });
        out
    }

    /// "machine · workspace › tab" for a terminal, if a layout shows it.
    pub fn place_of(&self, address: &str) -> Option<String> {
        self.spaces.iter().find_map(|s| {
            s.tabs
                .iter()
                .position(|t| t.terminals.iter().any(|a| a == address))
                .map(|ti| {
                    let tab = &s.tabs[ti];
                    let tab_name = if tab.name.is_empty() {
                        format!("tab {}", ti + 1)
                    } else {
                        tab.name.clone()
                    };
                    format!(
                        "{} · {} › {}",
                        self.machine_name(&s.on),
                        s.label(),
                        tab_name
                    )
                })
        })
    }

    fn picker_key(&mut self, key: Key) {
        let Mode::Picker {
            mut query,
            editing,
            mut filter,
        } = self.mode.clone()
        else {
            return;
        };
        let matches = self.picker_matches();
        let pos = matches
            .iter()
            .position(|&i| i == self.picker_cursor)
            .unwrap_or(0);
        let go = |app: &mut App, p: usize| {
            if let Some(&i) = matches.get(p) {
                app.picker_cursor = i;
            }
        };
        match key {
            Key::Esc | Key::Ctrl('[') => {
                self.mode = if editing {
                    Mode::Picker {
                        query,
                        editing: false,
                        filter,
                    }
                } else {
                    Mode::Normal
                };
                return;
            }
            Key::Enter => {
                self.mode = Mode::Normal;
                if let Some(&i) = matches.get(pos) {
                    let a = self.terms[i].address.clone();
                    self.reveal(&a);
                }
                return;
            }
            Key::Down => go(self, (pos + 1).min(matches.len().saturating_sub(1))),
            Key::Up => go(self, pos.saturating_sub(1)),
            Key::Backspace if editing => {
                query.pop();
            }
            Key::Ctrl('u') if editing => query.clear(),
            Key::Char(c) if editing => query.push(c),
            Key::Char('/') => {
                self.mode = Mode::Picker {
                    query,
                    editing: true,
                    filter,
                };
                return;
            }
            Key::Char('j') => go(self, (pos + 1).min(matches.len().saturating_sub(1))),
            Key::Char('k') => go(self, pos.saturating_sub(1)),
            Key::Char(c @ ('b' | 'w' | 'i' | 'd')) => filter = Some(c),
            Key::Char('a') => filter = None,
            Key::Left | Key::Right => {
                let place = |i: usize| {
                    self.place_of(&self.terms[i].address)
                        .map(|p| p.rsplit_once(" › ").map(|x| x.0.to_string()).unwrap_or(p))
                };
                if let Some(&cur) = matches.get(pos) {
                    let here = place(cur);
                    let target = if key == Key::Right {
                        matches[pos..].iter().copied().find(|&i| place(i) != here)
                    } else {
                        matches[..pos]
                            .iter()
                            .rev()
                            .copied()
                            .find(|&i| place(i) != here)
                            .map(|end| {
                                let prev = place(end);
                                matches
                                    .iter()
                                    .copied()
                                    .find(|&i| place(i) == prev)
                                    .unwrap_or(end)
                            })
                    };
                    if let Some(t) = target {
                        self.picker_cursor = t;
                    }
                }
            }
            _ => {}
        }
        self.mode = Mode::Picker {
            query,
            editing,
            filter,
        };
        let fresh = self.picker_matches();
        if !fresh.contains(&self.picker_cursor)
            && let Some(&first) = fresh.first()
        {
            self.picker_cursor = first;
        }
    }

    fn prompt_key(&mut self, kind: Prompt, mut text: String, key: Key) {
        match key {
            Key::Esc | Key::Ctrl('[') | Key::Ctrl('c') => self.mode = Mode::Normal,
            Key::Enter => {
                self.mode = Mode::Normal;
                self.prompt_done(kind, text.trim().to_string());
            }
            Key::Backspace | Key::Ctrl('h') => {
                text.pop();
                self.mode = Mode::Prompt(kind, text);
            }
            Key::Ctrl('u') => self.mode = Mode::Prompt(kind, String::new()),
            Key::Ctrl('w') => {
                let cut = text.trim_end().rfind(' ').map(|i| i + 1).unwrap_or(0);
                text.truncate(cut);
                self.mode = Mode::Prompt(kind, text);
            }
            Key::Char(c) => {
                text.push(c);
                self.mode = Mode::Prompt(kind, text);
            }
            _ => self.mode = Mode::Prompt(kind, text),
        }
    }

    fn prompt_done(&mut self, kind: Prompt, text: String) {
        match kind {
            Prompt::RenameTab => {
                let Some(c) = self.current.clone() else {
                    return;
                };
                self.edit_doc(&c.on, |doc| match layout::find_tab(doc, &c.space, &c.tab) {
                    Some(t) => {
                        t["name"] = Value::String(text.clone());
                        true
                    }
                    None => false,
                });
            }
            Prompt::NewWorkspace => {
                if !text.is_empty() {
                    self.new_workspace(text);
                }
            }
            Prompt::RenameWorkspace => {
                let Some(c) = self.current.clone() else {
                    return;
                };
                self.edit_doc(&c.on, |doc| match layout::find_space(doc, &c.space) {
                    Some(s) => {
                        s["name"] = Value::String(text.clone());
                        true
                    }
                    None => false,
                });
            }
        }
    }

    fn kill(&mut self, address: &str) {
        let r = self
            .conn
            .call("terminal.kill", json!({"terminal": address}));
        if let Err(e) = r {
            // Already gone is fine: we only want it out of the layout.
            if !format!("{e:#}").contains("not_found") {
                self.say(format!("{e:#}"));
            }
        }
    }

    fn confirmed(&mut self, what: Confirm) {
        match what {
            Confirm::ClosePane(address) => {
                let Some(c) = self.current.clone() else {
                    return;
                };
                self.kill(&address);
                let id = unqualify(&c.on, &address).to_string();
                self.zoomed = None;
                self.unplace(&c.on, &c.space, &c.tab, &id);
            }
            Confirm::CloseTab(r) => {
                let terms = self
                    .tab(&r)
                    .map(|(_, t)| t.terminals.clone())
                    .unwrap_or_default();
                for a in &terms {
                    self.kill(a);
                }
                let (space, tab) = (r.space.clone(), r.tab.clone());
                self.edit_doc(&r.on, |doc| {
                    let Some(s) = layout::find_space(doc, &space) else {
                        return false;
                    };
                    if let Some(tabs) = s["tabs"].as_array_mut() {
                        tabs.retain(|t| t["id"].as_str() != Some(tab.as_str()));
                    }
                    true
                });
            }
            Confirm::CloseWorkspace(on, space) => {
                let terms: Vec<String> = self
                    .spaces
                    .iter()
                    .filter(|s| s.on == on && s.id == space)
                    .flat_map(|s| s.tabs.iter().flat_map(|t| t.terminals.clone()))
                    .collect();
                for a in &terms {
                    self.kill(a);
                }
                self.edit_doc(&on, |doc| {
                    layout::spaces_mut(doc).retain(|s| s["id"].as_str() != Some(space.as_str()));
                    true
                });
            }
        }
    }

    pub fn doc_errors(&self) -> &[String] {
        &self.doc_errors
    }
}
