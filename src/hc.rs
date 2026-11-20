//! Talking to herdr-core through `hc connect`.
//!
//! Two child processes: one framed connection (JSON-RPC on channel 0, attach
//! streams on their own channels) and one plain connection that only carries
//! `events.subscribe`. Going through `hc connect` instead of the core's socket
//! keeps us out of platform details (named pipes, elevation checks, macOS
//! sessions) and works the same for every hc install.
//!
//! Frame layout follows herdr-core's `wire.rs` (Apache-2.0):
//! `[channel u32 LE][kind u8][len u32 LE][payload]`.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const DATA: u8 = 0;
pub const CREDIT: u8 = 1;
pub const CLOSE: u8 = 2;
pub const RESIZE: u8 = 3;
pub const FOCUS: u8 = 4;

const HEADER: usize = 9;
const MAX_PAYLOAD: usize = 16 << 20;
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// What the connection threads report to the app loop.
pub enum Incoming {
    /// Terminal bytes on an attach channel.
    Stream { channel: u32, data: Vec<u8> },
    /// The core ended an attach channel (reason JSON when it gave one).
    Closed { channel: u32, reason: Option<Value> },
    /// Something changed (event, heartbeat, missed): refresh the list.
    Changed,
    /// A connection to the core ended.
    Lost(String),
}

/// Where the `hc` binary is: `HC_TUI_HC`, then `hc` on PATH, then the default
/// Windows install folder.
pub fn hc_binary() -> PathBuf {
    if let Some(p) = std::env::var_os("HC_TUI_HC") {
        return PathBuf::from(p);
    }
    let exe = if cfg!(windows) { "hc.exe" } else { "hc" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        let p = PathBuf::from(local)
            .join("Programs")
            .join("herdr-core")
            .join("hc.exe");
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(exe)
}

fn spawn_connect(hc: &PathBuf) -> Result<Child> {
    Command::new(hc)
        .arg("connect")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "cannot start `{} connect` (is herdr-core installed?)",
                hc.display()
            )
        })
}

pub fn encode(channel: u32, kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&channel.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn read_frame(r: &mut impl Read) -> Option<(u32, u8, Vec<u8>)> {
    let mut header = [0u8; HEADER];
    r.read_exact(&mut header).ok()?;
    let channel = u32::from_le_bytes(header[0..4].try_into().ok()?);
    let len = u32::from_le_bytes(header[5..9].try_into().ok()?) as usize;
    if len > MAX_PAYLOAD {
        return None;
    }
    let mut payload = vec![0; len];
    r.read_exact(&mut payload).ok()?;
    Some((channel, header[4], payload))
}

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Value>>>>;

/// The framed connection: RPC calls plus attach streams.
pub struct Conn {
    writer: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    next_id: AtomicU64,
    child: Mutex<Child>,
    /// What the core said in `hello`.
    pub hello: Value,
}

impl Conn {
    pub fn open(hc: &PathBuf, tx: mpsc::Sender<Incoming>) -> Result<Conn> {
        let mut child = spawn_connect(hc)?;
        let mut stdin = child.stdin.take().context("no stdin")?;
        let stdout = child.stdout.take().context("no stdout")?;
        let mut reader = BufReader::new(stdout);
        let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello", "params": {"framed": true, "generation": 1}});
        stdin.write_all(format!("{hello}\n").as_bytes())?;
        stdin.flush()?;
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .context("no answer from hc connect")?;
        if line.trim().is_empty() {
            bail!(
                "hc connect closed without answering: is the core running? (`hc server start` in a normal, non-admin terminal)"
            );
        }
        let reply: Value = serde_json::from_str(&line).context("hello reply is not JSON")?;
        if let Some(err) = reply.get("error") {
            bail!("hc: {}", describe_error(err));
        }
        let hello = reply["result"].clone();

        let pending: Pending = Arc::default();
        let thread_pending = pending.clone();
        std::thread::spawn(move || {
            while let Some((channel, kind, payload)) = read_frame(&mut reader) {
                match (channel, kind) {
                    (0, DATA) => {
                        let Ok(msg) = serde_json::from_slice::<Value>(&payload) else {
                            continue;
                        };
                        if let Some(id) = msg.get("id").and_then(Value::as_u64)
                            && (msg.get("result").is_some() || msg.get("error").is_some())
                        {
                            if let Some(waiter) = thread_pending.lock().unwrap().remove(&id) {
                                let _ = waiter.send(msg);
                            }
                        } else {
                            let _ = tx.send(Incoming::Changed);
                        }
                    }
                    (ch, DATA) => {
                        let _ = tx.send(Incoming::Stream {
                            channel: ch,
                            data: payload,
                        });
                    }
                    (ch, CLOSE) => {
                        let reason = serde_json::from_slice(&payload).ok();
                        let _ = tx.send(Incoming::Closed {
                            channel: ch,
                            reason,
                        });
                    }
                    _ => {}
                }
            }
            let _ = tx.send(Incoming::Lost("lost the core (control connection)".into()));
        });

        Ok(Conn {
            writer: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            child: Mutex::new(child),
            hello,
        })
    }

    pub fn send(&self, channel: u32, kind: u8, payload: &[u8]) {
        let mut w = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        let _ = w.write_all(&encode(channel, kind, payload));
        let _ = w.flush();
    }

    /// One JSON-RPC call; waits for its reply.
    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.send(0, DATA, req.to_string().as_bytes());
        let reply = rx.recv_timeout(CALL_TIMEOUT).map_err(|_| {
            self.pending.lock().unwrap().remove(&id);
            anyhow!("{method}: no answer from the core")
        })?;
        match reply.get("error") {
            Some(err) => Err(anyhow!("{method}: {}", describe_error(err))),
            None => Ok(reply["result"].clone()),
        }
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
        }
    }
}

pub fn describe_error(err: &Value) -> String {
    let msg = err["message"].as_str().unwrap_or("error");
    match err["data"]["hint"].as_str() {
        Some(hint) if !hint.is_empty() => format!("{msg} ({hint})"),
        _ => msg.to_string(),
    }
}

/// The plain connection carrying `events.subscribe`: every line after the
/// reply is a notification, and each one just means "refresh".
pub struct Events {
    child: Child,
    _stdin: ChildStdin,
}

impl Events {
    pub fn open(hc: &PathBuf, tx: mpsc::Sender<Incoming>) -> Result<Events> {
        let mut child = spawn_connect(hc)?;
        let mut stdin = child.stdin.take().context("no stdin")?;
        let stdout = child.stdout.take().context("no stdout")?;
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "events.subscribe", "params": {}});
        stdin.write_all(format!("{req}\n").as_bytes())?;
        stdin.flush()?;
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if line.contains("\"heartbeat\"") {
                    continue;
                }
                let _ = tx.send(Incoming::Changed);
            }
            let _ = tx.send(Incoming::Lost("lost the core (event stream)".into()));
        });
        Ok(Events {
            child,
            _stdin: stdin,
        })
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
