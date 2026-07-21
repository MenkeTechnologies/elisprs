//! `elisp --dap` — a line-level Debug Adapter Protocol server (stdio) for Emacs
//! Lisp. Control model ported from zshrs's DAP (`zshrs/src/extensions/dap.rs`):
//! `DebugAction` / `PauseSnapshot` / `BreakpointState`, breakpoint + step-mode
//! pause/resume, and the full request set (initialize, launch, setBreakpoints,
//! configurationDone, threads, stackTrace, scopes, variables, continue, next,
//! stepIn, stepOut, pause, evaluate, source, disconnect, setExceptionBreakpoints).
//!
//! Granularity is **per statement** (each form in a `progn` / function body /
//! `let` body / the top level), which is line-level for the usual one-form-per-
//! line elisp. The whole program compiles to a single `fusevm::Chunk` with a
//! `DBG_LINE` marker before every statement (`compiler::emit_line_marker`,
//! emitted only in debug mode); the marker's handler in `host::ext_dispatch`
//! calls [`check_line`], which pauses when a breakpoint matches, step mode is on,
//! or a `pause` was requested. Because markers are emitted into function bodies
//! too, stepping and breakpoints work **inside** functions, not just at the top
//! level.
//!
//! Single-threaded by necessity: the elisp object heap is a `thread_local!`
//! (`host::with_host`), so a separate reader thread (as in zshrs) could not read
//! the paused executor's variables. Instead the executor thread runs the program
//! and, when a marker pauses, services DAP requests inline from [`Dap::pause_loop`]
//! until the client resumes — so `stackTrace` / `scopes` / `variables` /
//! `evaluate` all read the live, correct host. (Consequence vs. zshrs: an async
//! `pause` is honored at the next statement marker rather than mid-statement.)
//!
//! Step semantics follow zshrs v1: `next`/`stepIn` stop at the next statement;
//! `stepOut` resumes to the next breakpoint. Frame-depth-aware step-over/out is
//! future work.
//!
//! DAP messages are `Content-Length`-framed JSON on stdio. The debuggee's stdout
//! (`princ`/`prin1`) is redirected through a pipe and streamed as `output`
//! events, so program output never corrupts the JSON-RPC channel; DAP JSON is
//! written to the *saved* original stdout.

use crate::host::{self, with_host, Obj};
use serde_json::{json, Value as J};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::FromRawFd;
use std::sync::{Arc, Mutex};

/// What the paused executor was told to do (ported from zshrs `DebugAction`).
enum Resume {
    /// Resume normal execution (also `stepOut` in v1 — no frame-depth tracking).
    Continue,
    /// Step to the next statement (`next` / `stepIn`).
    Step,
    /// Client disconnected — stop pausing (the run finishes without more stops).
    Quit,
}

/// The DAP session state. Single-threaded, held in a `thread_local` so
/// [`check_line`] — reached from the VM's `DBG_LINE` handler — can pause into it.
struct Dap {
    reader: BufReader<std::io::Stdin>,
    /// DAP output sink: the *saved* original stdout, shared with the stdout-
    /// capture thread (the mutex makes each framed write atomic).
    out: Arc<Mutex<File>>,
    seq: i64,
    /// Canonical source path → breakpoint line set (ported: `line_breakpoints`).
    breakpoints: HashMap<String, HashSet<u32>>,
    /// Canonical path of the launched program (matches breakpoint keys).
    program: Option<String>,
    config_done: bool,
    started: bool,
    /// Pause at every statement marker (set by `next` / `stepIn` / stopOnEntry).
    step_mode: bool,
    /// Client asked to pause asap (honored at the next marker).
    pause_request: bool,
    /// Current statement line while paused (for `stackTrace`).
    cur_line: u32,
    disconnected: bool,
    capture: Capture,
}

thread_local! {
    /// The live session, installed by [`run_stdio`] for the duration of the run.
    /// Off (`None`) outside `--dap`, so [`check_line`] is a cheap `None` check.
    static DAP: std::cell::RefCell<Option<Dap>> = const { std::cell::RefCell::new(None) };
}

fn with_dap<R>(f: impl FnOnce(&mut Dap) -> R) -> Option<R> {
    DAP.with(|d| d.borrow_mut().as_mut().map(f))
}

pub fn run_stdio() -> i32 {
    let capture = match Capture::start() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("elisp --dap: {e}");
            return 1;
        }
    };
    let dap = Dap {
        reader: BufReader::new(std::io::stdin()),
        out: capture.out.clone(),
        seq: 0,
        breakpoints: HashMap::new(),
        program: None,
        config_done: false,
        started: false,
        step_mode: false,
        pause_request: false,
        cur_line: 0,
        disconnected: false,
        capture,
    };
    DAP.with(|d| *d.borrow_mut() = Some(dap));

    // Serve top-level requests until the client says `configurationDone` (with a
    // program launched), then run the program in-process. `check_line` pauses
    // into `Dap::pause_loop` from within the run; nothing here holds the `DAP`
    // borrow across `run_program`, so that re-borrow is sound.
    // EOF (`read_msg` → None) ends the loop; the inner breaks handle disconnect
    // and the run-then-terminate path.
    while let Some(msg) = with_dap(|d| d.read_msg()).flatten() {
        let run_now = with_dap(|d| d.handle_toplevel(&msg)).unwrap_or(false);
        if with_dap(|d| d.disconnected).unwrap_or(true) {
            break;
        }
        if run_now {
            run_program();
            // Drain the stdout-capture pipe (restore fd 1 → the reader thread
            // hits EOF and flushes every pending `output` event) BEFORE emitting
            // `terminated`, so program output always precedes the terminal event.
            with_dap(|d| d.capture.finish());
            with_dap(|d| d.terminate());
            break;
        }
    }

    // Drop the session (restores the real stdout via `Capture`'s Drop).
    DAP.with(|d| *d.borrow_mut() = None);
    0
}

/// Read the launched program, compile the whole thing to one debug-instrumented
/// chunk, and run it. `check_line` fires at each statement marker during the run.
fn run_program() {
    let Some(path) = with_dap(|d| d.program.clone()).flatten() else {
        with_dap(|d| {
            d.event(
                "output",
                json!({"category": "stderr", "output": "no program specified\n"}),
            )
        });
        return;
    };
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            with_dap(|d| {
                d.event(
                    "output",
                    json!({"category": "stderr", "output": format!("cannot read {path}: {e}\n")}),
                )
            });
            return;
        }
    };

    host::reset_host();
    host::set_debug_mode(true);
    // The reader records a source line for every list form; the compiler emits a
    // `DBG_LINE` marker per statement from those.
    let compiled = with_host(|h| {
        let forms = crate::reader::read_all(h, &src)?;
        crate::compiler::compile_program(h, &forms)
    });
    let result = match compiled {
        Ok(chunk) => host::run_chunk(chunk),
        Err(e) => Err(e),
    };
    host::set_debug_mode(false);

    if let Err(e) = result {
        with_dap(|d| {
            d.event(
                "output",
                json!({"category": "stderr", "output": format!("error: {e}\n")}),
            )
        });
    }
}

/// Called from the VM's `DBG_LINE` handler at every statement marker. Pauses when
/// a breakpoint matches, step mode is on, or a `pause` was requested; otherwise
/// an `Option`-check-cheap no-op. Ported from zshrs `check_line`.
pub fn check_line(line: u32) {
    with_dap(|d| d.on_line(line));
}

impl Dap {
    fn next_seq(&mut self) -> i64 {
        self.seq += 1;
        self.seq
    }

    fn write(&mut self, msg: &J) {
        let s = msg.to_string();
        if let Ok(mut f) = self.out.lock() {
            let _ = write!(f, "Content-Length: {}\r\n\r\n{}", s.len(), s);
            let _ = f.flush();
        }
    }

    fn event(&mut self, event: &str, body: J) {
        let seq = self.next_seq();
        self.write(&json!({"seq": seq, "type": "event", "event": event, "body": body}));
    }

    fn respond(&mut self, req: &J, body: J) {
        let seq = self.next_seq();
        self.write(&json!({
            "seq": seq, "type": "response",
            "request_seq": req["seq"], "success": true,
            "command": req["command"], "body": body,
        }));
    }

    /// Both `launch` and `configurationDone` must have arrived before the program
    /// runs (their order varies by client). Returns true exactly once.
    fn maybe_start(&mut self) -> bool {
        if self.program.is_some() && self.config_done && !self.started {
            self.started = true;
            true
        } else {
            false
        }
    }

    /// Canonicalize so IDE (relative) and executor (absolute) paths agree.
    fn canon(p: &str) -> String {
        std::fs::canonicalize(p)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| p.to_string())
    }

    fn set_breakpoints(&mut self, msg: &J) {
        let path = Self::canon(msg["arguments"]["source"]["path"].as_str().unwrap_or(""));
        let mut lines = HashSet::new();
        let mut verified = Vec::new();
        if let Some(arr) = msg["arguments"]["breakpoints"].as_array() {
            for bp in arr {
                if let Some(l) = bp["line"].as_u64() {
                    lines.insert(l as u32);
                    verified.push(json!({"verified": true, "line": l}));
                }
            }
        }
        if !path.is_empty() {
            self.breakpoints.insert(path, lines);
        }
        self.respond(msg, json!({"breakpoints": verified}));
    }

    /// Service a request received while NOT paused. Returns true when the program
    /// should now run (both `launch` and `configurationDone` seen).
    fn handle_toplevel(&mut self, msg: &J) -> bool {
        if msg["type"] != "request" {
            return false;
        }
        match msg["command"].as_str().unwrap_or("") {
            "initialize" => {
                self.respond(
                    msg,
                    json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsEvaluateForHovers": true,
                        "supportsTerminateRequest": true,
                    }),
                );
                self.event("initialized", json!({}));
            }
            "setBreakpoints" => self.set_breakpoints(msg),
            "setExceptionBreakpoints" => self.respond(msg, json!({})),
            "launch" => {
                let raw = msg["arguments"]["program"].as_str().unwrap_or("");
                self.program = Some(Self::canon(raw));
                if msg["arguments"]["stopOnEntry"].as_bool().unwrap_or(false) {
                    self.step_mode = true;
                }
                self.respond(msg, json!({}));
                return self.maybe_start();
            }
            "configurationDone" => {
                self.config_done = true;
                self.respond(msg, json!({}));
                return self.maybe_start();
            }
            "threads" => self.respond(msg, json!({"threads": [{"id": 1, "name": "main"}]})),
            "disconnect" | "terminate" => {
                self.respond(msg, json!({}));
                self.disconnected = true;
            }
            "source" => self.respond(msg, json!({})),
            _ => self.respond(msg, json!({})),
        }
        false
    }

    fn breakpoint_hit(&self, line: u32) -> bool {
        self.program
            .as_ref()
            .and_then(|p| self.breakpoints.get(p))
            .map(|s| s.contains(&line))
            .unwrap_or(false)
    }

    /// A statement marker fired. Decide whether to pause, and if so, emit
    /// `stopped` and service requests until the client resumes. Ported from
    /// zshrs `check_line` + `DapShared::pause`.
    fn on_line(&mut self, line: u32) {
        if self.disconnected {
            return;
        }
        self.cur_line = line;
        let reason = if self.pause_request {
            self.pause_request = false;
            "pause"
        } else if self.step_mode {
            "step"
        } else if self.breakpoint_hit(line) {
            "breakpoint"
        } else {
            return;
        };
        let file = self.program.clone().unwrap_or_default();
        self.event(
            "stopped",
            json!({
                "reason": reason,
                "threadId": 1,
                "allThreadsStopped": true,
                "text": format!("{file}:{line}"),
            }),
        );
        match self.pause_loop() {
            Resume::Continue => self.step_mode = false,
            Resume::Step => self.step_mode = true,
            Resume::Quit => {
                self.disconnected = true;
            }
        }
    }

    /// Read and service requests while paused; return how to resume. Handles the
    /// requests a client sends while stopped: stack/scopes/variables/evaluate and
    /// the flow controls. All host reads happen here on the executor thread, so
    /// they see the live paused state.
    fn pause_loop(&mut self) -> Resume {
        loop {
            let Some(msg) = self.read_msg() else {
                return Resume::Quit; // EOF
            };
            if msg["type"] != "request" {
                continue;
            }
            match msg["command"].as_str().unwrap_or("") {
                "continue" => {
                    self.respond(&msg, json!({"allThreadsContinued": true}));
                    return Resume::Continue;
                }
                "next" | "stepIn" => {
                    self.respond(&msg, json!({"allThreadsContinued": true}));
                    return Resume::Step;
                }
                // v1: step-out resumes to the next breakpoint (no frame depth yet).
                "stepOut" => {
                    self.respond(&msg, json!({"allThreadsContinued": true}));
                    return Resume::Continue;
                }
                "pause" => self.respond(&msg, json!({})), // already paused
                "threads" => self.respond(&msg, json!({"threads": [{"id": 1, "name": "main"}]})),
                "stackTrace" => {
                    let frame = json!({
                        "id": 1, "name": "main", "line": self.cur_line, "column": 1,
                        "source": {"path": self.program.clone().unwrap_or_default()},
                    });
                    self.respond(&msg, json!({"stackFrames": [frame], "totalFrames": 1}));
                }
                "scopes" => {
                    self.respond(
                        &msg,
                        json!({"scopes": [{
                            "name": "Globals", "variablesReference": 1000, "expensive": false,
                        }]}),
                    );
                }
                "variables" => {
                    let vars = global_variables();
                    self.respond(&msg, json!({"variables": vars}));
                }
                "evaluate" => {
                    let expr = msg["arguments"]["expression"].as_str().unwrap_or("");
                    let result = eval_in_host(expr);
                    self.respond(&msg, json!({"result": result, "variablesReference": 0}));
                }
                "setBreakpoints" => self.set_breakpoints(&msg),
                "setExceptionBreakpoints" => self.respond(&msg, json!({})),
                "source" => self.respond(&msg, json!({})),
                "disconnect" | "terminate" => {
                    self.respond(&msg, json!({}));
                    return Resume::Quit;
                }
                _ => self.respond(&msg, json!({})),
            }
        }
    }

    fn terminate(&mut self) {
        self.event("terminated", json!({}));
        self.event("exited", json!({"exitCode": 0}));
    }

    /// Read one `Content-Length`-framed JSON message; `None` at EOF.
    fn read_msg(&mut self) -> Option<J> {
        let mut content_len = 0usize;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line).ok()? == 0 {
                return None;
            }
            let t = line.trim_end();
            if t.is_empty() {
                break;
            }
            if let Some(v) = t.strip_prefix("Content-Length:") {
                content_len = v.trim().parse().ok()?;
            }
        }
        let mut buf = vec![0u8; content_len];
        self.reader.read_exact(&mut buf).ok()?;
        serde_json::from_slice(&buf).ok()
    }
}

// ── host inspection ──────────────────────────────────────────────────────────

/// Snapshot every symbol that has a value cell, as DAP `variables`.
fn global_variables() -> Vec<J> {
    with_host(|h| {
        let mut out = Vec::new();
        for obj in &h.arena {
            if let Obj::Symbol(s) = obj {
                if let Some(v) = &s.value {
                    out.push(json!({
                        "name": s.name,
                        "value": h.print(v, true),
                        "variablesReference": 0,
                    }));
                }
            }
        }
        out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        out
    })
}

/// Evaluate an expression string in the live host (the `evaluate` request).
/// Debug mode is forced off around the compile+run so the evaluated forms carry
/// no statement markers — otherwise `check_line` would recurse while paused.
fn eval_in_host(expr: &str) -> String {
    let prev = host::debug_mode();
    host::set_debug_mode(false);
    let result = (|| {
        let forms = with_host(|h| crate::reader::read_all(h, expr))
            .map_err(|e| format!("read error: {e}"))?;
        let mut last = String::from("nil");
        for form in &forms {
            let chunk = with_host(|h| crate::compiler::compile_top(h, form))
                .map_err(|e| format!("error: {e}"))?;
            match host::run_chunk(chunk) {
                Ok(v) => last = with_host(|h| h.print(&v, true)),
                Err(e) => return Err(format!("error: {e}")),
            }
        }
        Ok(last)
    })();
    host::set_debug_mode(prev);
    result.unwrap_or_else(|e| e)
}

// ── stdout capture (pipe + dup2) ─────────────────────────────────────────────

/// Redirects the process's fd 1 into a pipe so the debuggee's stdout becomes
/// `output` events; DAP JSON is written to the *saved* original stdout.
struct Capture {
    out: Arc<Mutex<File>>,
    saved_stdout: i32,
    reader_thread: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    fn start() -> Result<Self, String> {
        unsafe {
            let saved = libc::dup(1);
            if saved < 0 {
                return Err("dup(stdout) failed".into());
            }
            let mut fds = [0i32; 2];
            if libc::pipe(fds.as_mut_ptr()) != 0 {
                return Err("pipe() failed".into());
            }
            let (read_fd, write_fd) = (fds[0], fds[1]);
            if libc::dup2(write_fd, 1) < 0 {
                return Err("dup2() failed".into());
            }
            libc::close(write_fd);

            let out: Arc<Mutex<File>> = Arc::new(Mutex::new(File::from_raw_fd(saved)));
            let out_thread = out.clone();
            let reader_thread = std::thread::spawn(move || {
                let mut f = File::from_raw_fd(read_fd);
                let mut buf = [0u8; 4096];
                loop {
                    match f.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                            let msg = json!({
                                "seq": 0, "type": "event", "event": "output",
                                "body": {"category": "stdout", "output": text},
                            });
                            let s = msg.to_string();
                            if let Ok(mut w) = out_thread.lock() {
                                let _ = write!(w, "Content-Length: {}\r\n\r\n{}", s.len(), s);
                                let _ = w.flush();
                            }
                        }
                    }
                }
            });

            Ok(Capture {
                out,
                saved_stdout: saved,
                reader_thread: Some(reader_thread),
            })
        }
    }
}

impl Capture {
    /// Restore the real stdout and join the reader thread, flushing every pending
    /// `output` event. Idempotent — a second call (from `Drop`) is a harmless
    /// re-`dup2` with the thread already joined.
    fn finish(&mut self) {
        unsafe {
            libc::dup2(self.saved_stdout, 1);
        }
        if let Some(t) = self.reader_thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.finish();
    }
}
