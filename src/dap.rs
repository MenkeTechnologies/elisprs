//! `elisp --dap` — a Debug Adapter Protocol server (stdio) for Emacs Lisp.
//!
//! The debuggee runs in-process: each top-level form is compiled to a
//! `fusevm::Chunk` and executed on fusevm, sharing the thread-local ElispHost so
//! `defvar`/`setq` globals persist across forms. Breakpoints and stepping work
//! at top-level-form granularity (the unit elisp loads and evaluates).
//!
//! DAP messages are Content-Length-framed JSON on stdio. The debuggee's stdout
//! (`princ`/`prin1`) is redirected through a pipe and streamed as `output`
//! events, so program output never corrupts the JSON-RPC channel. `message`
//! writes to stderr and passes through untouched.

use crate::host::{with_host, Obj};
use serde_json::{json, Value as J};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::FromRawFd;
use std::sync::{Arc, Mutex};

pub fn run_stdio() -> i32 {
    match Dap::new() {
        Ok(mut d) => {
            d.serve();
            0
        }
        Err(e) => {
            eprintln!("elisp --dap: {e}");
            1
        }
    }
}

/// A framed-JSON writer shared between the main thread and the output-capture
/// thread; the mutex makes each message write atomic.
type Out = Arc<Mutex<File>>;

fn send(out: &Out, msg: &J) {
    let s = msg.to_string();
    if let Ok(mut f) = out.lock() {
        let _ = write!(f, "Content-Length: {}\r\n\r\n{}", s.len(), s);
        let _ = f.flush();
    }
}

struct Dap {
    reader: BufReader<std::io::Stdin>,
    out: Out,
    seq: i64,
    breakpoints: std::collections::HashSet<u32>,
    program: Option<String>,
    /// Top-level form source start lines (1-based), parallel to `forms`.
    lines: Vec<u32>,
    /// Current top-level form line while paused (for stackTrace).
    cur_line: u32,
    _capture: Capture,
}

impl Dap {
    fn new() -> Result<Self, String> {
        let capture = Capture::start()?;
        Ok(Dap {
            reader: BufReader::new(std::io::stdin()),
            out: capture.out.clone(),
            seq: 0,
            breakpoints: std::collections::HashSet::new(),
            program: None,
            lines: Vec::new(),
            cur_line: 0,
            _capture: capture,
        })
    }

    fn next_seq(&mut self) -> i64 {
        self.seq += 1;
        self.seq
    }

    fn event(&mut self, event: &str, body: J) {
        let seq = self.next_seq();
        send(
            &self.out,
            &json!({"seq": seq, "type": "event", "event": event, "body": body}),
        );
    }
    fn respond(&mut self, req: &J, body: J) {
        let seq = self.next_seq();
        send(
            &self.out,
            &json!({
                "seq": seq, "type": "response",
                "request_seq": req["seq"], "success": true,
                "command": req["command"], "body": body
            }),
        );
    }

    // ── main protocol loop ──
    fn serve(&mut self) {
        while let Some(msg) = read_msg(&mut self.reader) {
            if msg["type"] != "request" {
                continue;
            }
            let cmd = msg["command"].as_str().unwrap_or("").to_string();
            match cmd.as_str() {
                "initialize" => {
                    self.respond(
                        &msg,
                        json!({
                            "supportsConfigurationDoneRequest": true,
                            "supportsEvaluateForHovers": true,
                            "supportsTerminateRequest": true
                        }),
                    );
                    self.event("initialized", json!({}));
                }
                "setBreakpoints" => {
                    let bps = msg["arguments"]["breakpoints"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    self.breakpoints.clear();
                    let mut verified = Vec::new();
                    for bp in &bps {
                        if let Some(line) = bp["line"].as_u64() {
                            self.breakpoints.insert(line as u32);
                            verified.push(json!({"verified": true, "line": line}));
                        }
                    }
                    self.respond(&msg, json!({ "breakpoints": verified }));
                }
                "launch" => {
                    self.program = msg["arguments"]["program"].as_str().map(String::from);
                    self.respond(&msg, json!({}));
                }
                "configurationDone" => {
                    self.respond(&msg, json!({}));
                    self.run_program();
                }
                "threads" => {
                    self.respond(&msg, json!({"threads": [{"id": 1, "name": "main"}]}));
                }
                "disconnect" | "terminate" => {
                    self.respond(&msg, json!({}));
                    return;
                }
                _ => self.respond(&msg, json!({})),
            }
        }
    }

    // ── execution ──
    fn run_program(&mut self) {
        let Some(path) = self.program.clone() else {
            self.event(
                "output",
                json!({"category": "stderr", "output": "no program specified\n"}),
            );
            self.terminate();
            return;
        };
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                self.event(
                    "output",
                    json!({"category": "stderr", "output": format!("cannot read {path}: {e}\n")}),
                );
                self.terminate();
                return;
            }
        };
        crate::host::reset_host();
        self.lines = toplevel_lines(&src);
        let forms = match with_host(|h| crate::reader::read_all(h, &src)) {
            Ok(f) => f,
            Err(e) => {
                self.event(
                    "output",
                    json!({"category": "stderr", "output": format!("read error: {e}\n")}),
                );
                self.terminate();
                return;
            }
        };

        let mut stepping = false; // stop at the very first form? no — only at breakpoints
        for (i, form) in forms.iter().enumerate() {
            let line = self.lines.get(i).copied().unwrap_or(0);
            self.cur_line = line;
            if stepping || (line != 0 && self.breakpoints.contains(&line)) {
                let reason = if stepping { "step" } else { "breakpoint" };
                self.event(
                    "stopped",
                    json!({"reason": reason, "threadId": 1, "allThreadsStopped": true}),
                );
                match self.pause_loop() {
                    Resume::Continue => stepping = false,
                    Resume::Next => stepping = true,
                    Resume::Stop => {
                        self.terminate();
                        return;
                    }
                }
            }
            // compile + run this form on fusevm
            let chunk = match with_host(|h| crate::compiler::compile_top(h, form)) {
                Ok(c) => c,
                Err(e) => {
                    self.event(
                        "output",
                        json!({"category": "stderr", "output": format!("compile error: {e}\n")}),
                    );
                    continue;
                }
            };
            match crate::host::run_chunk(chunk) {
                Ok(v) => {
                    let printed = with_host(|h| h.print(&v, true));
                    self.event(
                        "output",
                        json!({"category": "stdout", "output": format!("{printed}\n")}),
                    );
                }
                Err(e) => {
                    self.event(
                        "output",
                        json!({"category": "stderr", "output": format!("error: {e}\n")}),
                    );
                }
            }
        }
        self.terminate();
    }

    /// Read and service requests while paused; return how to resume.
    fn pause_loop(&mut self) -> Resume {
        while let Some(msg) = read_msg(&mut self.reader) {
            if msg["type"] != "request" {
                continue;
            }
            let cmd = msg["command"].as_str().unwrap_or("").to_string();
            match cmd.as_str() {
                "continue" => {
                    self.respond(&msg, json!({"allThreadsContinued": true}));
                    return Resume::Continue;
                }
                "next" | "stepIn" | "stepOut" => {
                    self.respond(&msg, json!({}));
                    return Resume::Next;
                }
                "threads" => self.respond(&msg, json!({"threads": [{"id": 1, "name": "main"}]})),
                "stackTrace" => {
                    let frame = json!({
                        "id": 1, "name": "toplevel", "line": self.cur_line, "column": 1,
                        "source": {"path": self.program.clone().unwrap_or_default()}
                    });
                    self.respond(&msg, json!({"stackFrames": [frame], "totalFrames": 1}));
                }
                "scopes" => {
                    self.respond(
                        &msg,
                        json!({"scopes": [{
                            "name": "Globals", "variablesReference": 1000, "expensive": false
                        }]}),
                    );
                }
                "variables" => {
                    let vars = global_variables();
                    self.respond(&msg, json!({ "variables": vars }));
                }
                "evaluate" => {
                    let expr = msg["arguments"]["expression"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let result = eval_in_host(&expr);
                    self.respond(&msg, json!({"result": result, "variablesReference": 0}));
                }
                "setBreakpoints" => {
                    let bps = msg["arguments"]["breakpoints"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    self.breakpoints.clear();
                    let mut verified = Vec::new();
                    for bp in &bps {
                        if let Some(line) = bp["line"].as_u64() {
                            self.breakpoints.insert(line as u32);
                            verified.push(json!({"verified": true, "line": line}));
                        }
                    }
                    self.respond(&msg, json!({ "breakpoints": verified }));
                }
                "disconnect" | "terminate" => {
                    self.respond(&msg, json!({}));
                    return Resume::Stop;
                }
                _ => self.respond(&msg, json!({})),
            }
        }
        Resume::Stop
    }

    fn terminate(&mut self) {
        self.event("terminated", json!({}));
        self.event("exited", json!({"exitCode": 0}));
    }
}

enum Resume {
    Continue,
    Next,
    Stop,
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
                        "variablesReference": 0
                    }));
                }
            }
        }
        out
    })
}

/// Evaluate an expression string in the live host (for the `evaluate` request).
fn eval_in_host(expr: &str) -> String {
    let forms = match with_host(|h| crate::reader::read_all(h, expr)) {
        Ok(f) => f,
        Err(e) => return format!("read error: {e}"),
    };
    let mut last = String::from("nil");
    for form in &forms {
        match with_host(|h| crate::compiler::compile_top(h, form)) {
            Ok(chunk) => match crate::host::run_chunk(chunk) {
                Ok(v) => last = with_host(|h| h.print(&v, true)),
                Err(e) => return format!("error: {e}"),
            },
            Err(e) => return format!("error: {e}"),
        }
    }
    last
}

// ── source line mapping ──────────────────────────────────────────────────────

/// Start line (1-based) of each top-level form — i.e. each `(` that takes
/// paren depth from 0 to 1, skipping strings, `;` comments, and `?c` literals.
fn toplevel_lines(src: &str) -> Vec<u32> {
    let chars: Vec<char> = src.chars().collect();
    let mut lines = Vec::new();
    let mut line = 1u32;
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => line += 1,
            ';' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '"' => {
                i += 1;
                while i < chars.len() {
                    match chars[i] {
                        '\\' => i += 2,
                        '"' => break,
                        '\n' => line += 1,
                        _ => {}
                    }
                    if chars.get(i) == Some(&'\\') {
                        continue;
                    }
                    i += 1;
                }
            }
            '?' => {
                i += if chars.get(i + 1) == Some(&'\\') {
                    3
                } else {
                    2
                };
                continue;
            }
            '(' => {
                if depth == 0 {
                    lines.push(line);
                }
                depth += 1;
            }
            ')' => depth = (depth - 1).max(0),
            _ => {}
        }
        i += 1;
    }
    lines
}

// ── framed JSON I/O ──────────────────────────────────────────────────────────

fn read_msg(r: &mut impl BufRead) -> Option<J> {
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None; // EOF
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
    r.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

// ── stdout capture (pipe + dup2) ─────────────────────────────────────────────

/// Redirects the process's fd 1 into a pipe so the debuggee's stdout becomes
/// `output` events; DAP JSON is written to the *saved* original stdout.
struct Capture {
    out: Out,
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

            let out: Out = Arc::new(Mutex::new(File::from_raw_fd(saved)));
            let out_thread = out.clone();
            let reader_thread = std::thread::spawn(move || {
                let mut f = File::from_raw_fd(read_fd);
                let mut buf = [0u8; 4096];
                loop {
                    match f.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                            let mut seq_msg = json!({
                                "type": "event", "event": "output",
                                "body": {"category": "stdout", "output": text}
                            });
                            // seq is best-effort on the capture path.
                            seq_msg["seq"] = json!(0);
                            send(&out_thread, &seq_msg);
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

impl Drop for Capture {
    fn drop(&mut self) {
        // Restore the real stdout; the pipe EOFs and the reader thread exits.
        unsafe {
            libc::dup2(self.saved_stdout, 1);
        }
        if let Some(t) = self.reader_thread.take() {
            let _ = t.join();
        }
    }
}
