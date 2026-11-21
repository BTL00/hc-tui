//! hc-tui: an unofficial terminal UI for herdr-core (`hc`), with a herdr-style
//! sidebar, herdr's default keys, every machine `hc` knows, and predictive echo.

mod app;
mod hc;
mod keys;
mod layout;
mod predict;
mod term_io;
mod ui;

use anyhow::Result;
use std::sync::mpsc;
use std::time::Duration;

enum Event {
    Input(Vec<u8>),
    Core(hc::Incoming),
}

const USAGE: &str = "hc-tui - terminal UI for herdr-core (hc)

Usage: hc-tui [--no-predict] [--attach <terminal>] [--list]

  --no-predict        start with predictive echo off (toggle: ctrl+b e)
  --attach <t>        attach to this terminal first (t7, a name, machine/t7)
  --list              print machines, workspaces and tabs, then exit
  -V, --version       print the version

Environment: HC_TUI_HC=<path to hc>, HC_TUI_PREDICT=0
Keys: herdr defaults, prefix ctrl+b. ctrl+b ? shows them all.";

fn main() {
    if let Err(e) = run() {
        eprintln!("hc-tui: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut predict = std::env::var("HC_TUI_PREDICT")
        .map(|v| v != "0")
        .unwrap_or(true);
    let mut first: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--no-predict" => predict = false,
            "--attach" => first = args.next(),
            "--list" => return list(),
            "-V" | "--version" => {
                println!("hc-tui {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other:?}\n\n{USAGE}"),
        }
    }

    let hc_bin = hc::hc_binary();
    let (tx, rx) = mpsc::channel::<Event>();
    let (core_tx, core_rx) = mpsc::channel::<hc::Incoming>();
    let conn = hc::Conn::open(&hc_bin, core_tx.clone())?;
    let _events = hc::Events::open(&hc_bin, core_tx)?;
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for msg in core_rx {
                if tx.send(Event::Core(msg)).is_err() {
                    break;
                }
            }
        });
    }

    let mut terminal = ratatui::init();
    let raw = term_io::RawMode::enter();
    term_io::spawn_reader(tx, Event::Input);

    let mut app = app::App::new(conn, predict);
    let size = terminal.size()?;
    let (_, tab, _) = ui::areas(
        &app,
        ratatui::layout::Rect::new(0, 0, size.width, size.height),
    );
    app.set_area(tab);
    app.refresh();
    if let Some(t) = first {
        app.goto(&t);
    }

    let result = (|| -> Result<()> {
        while !app.quit {
            let size = terminal.size()?;
            let (_, tab, _) = ui::areas(
                &app,
                ratatui::layout::Rect::new(0, 0, size.width, size.height),
            );
            app.set_area(tab);
            terminal.draw(|f| ui::draw(f, &app))?;
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(ev) => handle(&mut app, ev),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            // Take everything already queued before drawing again.
            while let Ok(ev) = rx.try_recv() {
                handle(&mut app, ev);
                if app.quit {
                    break;
                }
            }
            app.tick();
        }
        Ok(())
    })();

    app.shutdown();
    drop(raw);
    ratatui::restore();
    result
}

/// `--list`: the sidebar as text, no UI.
fn list() -> Result<()> {
    let (tx, _rx) = mpsc::channel();
    let conn = hc::Conn::open(&hc::hc_binary(), tx)?;
    let mut app = app::App::new(conn, false);
    app.refresh_model();
    for e in app.doc_errors() {
        println!("! layout unavailable: {e}");
    }
    for row in &app.rows {
        match row {
            app::Row::Machine(name, state) => println!("💻 {name} ({state})"),
            app::Row::Space(si) => println!("  {}", app.spaces[*si].label()),
            app::Row::Tab(si, ti) => {
                let t = &app.spaces[*si].tabs[*ti];
                let terms: Vec<String> = t
                    .terminals
                    .iter()
                    .map(|a| match app.term(a) {
                        Some(x) => format!(
                            "{a} {} [{}]",
                            x.display(),
                            if x.live {
                                x.status.as_str()
                            } else {
                                "finished"
                            }
                        ),
                        None => format!("{a} (missing)"),
                    })
                    .collect();
                let name = if t.name.is_empty() {
                    format!("tab {}", ti + 1)
                } else {
                    t.name.clone()
                };
                println!("    {name}: {}", terms.join(", "));
            }
            app::Row::Pane(..) => {}
            app::Row::LooseHeader => println!("  (unplaced)"),
            app::Row::Loose(i) => {
                let x = &app.terms[*i];
                println!("    {} {} [{}]", x.address, x.display(), x.status);
            }
        }
    }
    Ok(())
}

fn handle(app: &mut app::App, ev: Event) {
    match ev {
        Event::Input(bytes) => app.input(&bytes),
        Event::Core(msg) => app.incoming(msg),
    }
}
