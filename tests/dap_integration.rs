//! End-to-end DAP integration test for `elisp --dap`.
//!
//! Spawns the built `elisp --dap` over stdio pipes and drives a real Debug
//! Adapter Protocol session — initialize → setBreakpoints → launch →
//! configurationDone — then asserts the executor stops at the breakpoint line,
//! reports the right frame line and variables, single-steps to the next line,
//! and terminates on `continue`. Headless and dependency-free (only the built
//! binary + serde_json), so it runs in CI with no Emacs and no external tools.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

/// A four-statement program, one form per line, so a line breakpoint maps to a
/// single statement. Line 3 binds `c`; line 4 prints it.
const PROGRAM: &str = "(setq a 1)\n(setq b 2)\n(setq c (+ a b))\n(princ (format \"c=%d\\n\" c))\n";

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    seq: i64,
}

impl Session {
    fn start(program_path: &str) -> Self {
        let bin = env!("CARGO_BIN_EXE_elisp");
        let mut child = Command::new(bin)
            .arg("--dap")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn elisp --dap");
        // Watchdog: a wedged debugger must fail the test, not hang CI.
        let id = child.id();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(20));
            // Best-effort kill by pid; the reads below will then hit EOF.
            let _ = Command::new("kill").arg("-9").arg(id.to_string()).status();
        });
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut s = Session {
            child,
            stdin,
            stdout,
            seq: 0,
        };
        // Standard handshake.
        s.send("initialize", json!({}));
        s.send(
            "setBreakpoints",
            json!({ "source": { "path": program_path }, "breakpoints": [{ "line": 3 }] }),
        );
        s.send("launch", json!({ "program": program_path }));
        s.send("configurationDone", json!({}));
        s
    }

    fn send(&mut self, command: &str, arguments: Value) {
        self.seq += 1;
        let msg = json!({
            "seq": self.seq, "type": "request", "command": command, "arguments": arguments,
        });
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).expect("write req");
        self.stdin.flush().expect("flush req");
    }

    /// Read one `Content-Length`-framed JSON message, or `None` at EOF.
    fn read_msg(&mut self) -> Option<Value> {
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            if self.stdout.read_line(&mut line).ok()? == 0 {
                return None;
            }
            let t = line.trim_end();
            if t.is_empty() {
                break;
            }
            if let Some(v) = t.strip_prefix("Content-Length:") {
                len = v.trim().parse().ok()?;
            }
        }
        let mut buf = vec![0u8; len];
        self.stdout.read_exact(&mut buf).ok()?;
        serde_json::from_slice(&buf).ok()
    }

    /// Read messages until one satisfies `pred`, returning it. Panics at EOF so a
    /// crashed/wedged adapter fails the test with a clear message.
    fn read_until(&mut self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..200 {
            match self.read_msg() {
                Some(m) if pred(&m) => return m,
                Some(_) => continue,
                None => break,
            }
        }
        panic!("did not receive {what} before EOF");
    }

    fn stopped(&mut self) -> Value {
        self.read_until("stopped event", |m| {
            m["type"] == "event" && m["event"] == "stopped"
        })
    }

    fn response(&mut self, command: &str) -> Value {
        self.read_until(&format!("{command} response"), |m| {
            m["type"] == "response" && m["command"] == command
        })
    }

    fn stack_line(&mut self) -> u64 {
        self.send("stackTrace", json!({ "threadId": 1 }));
        let r = self.response("stackTrace");
        r["body"]["stackFrames"][0]["line"].as_u64().unwrap()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn dap_breakpoint_step_and_terminate() {
    let path = std::env::temp_dir().join(format!("elisprs_dap_it_{}.el", std::process::id()));
    std::fs::write(&path, PROGRAM).expect("write program");
    let path_str = path.to_string_lossy().into_owned();

    let mut s = Session::start(&path_str);

    // 1) Stops at the breakpoint on line 3.
    let stop = s.stopped();
    assert_eq!(
        stop["body"]["reason"], "breakpoint",
        "first stop is the breakpoint"
    );
    assert_eq!(s.stack_line(), 3, "breakpoint frame is line 3");

    // Variables reflect the paused state: a and b are bound, c is not yet.
    s.send("variables", json!({ "variablesReference": 1000 }));
    let vars = s.response("variables");
    let mut a = None;
    let mut b = None;
    let mut c = None;
    for v in vars["body"]["variables"].as_array().unwrap() {
        match v["name"].as_str().unwrap_or("") {
            "a" => a = v["value"].as_str().map(String::from),
            "b" => b = v["value"].as_str().map(String::from),
            "c" => c = v["value"].as_str().map(String::from),
            _ => {}
        }
    }
    assert_eq!(a.as_deref(), Some("1"), "a is bound to 1 at the breakpoint");
    assert_eq!(b.as_deref(), Some("2"), "b is bound to 2 at the breakpoint");
    assert!(c.is_none(), "c is not yet bound before line 3 runs");

    // 2) Single-step advances to the next statement (line 4).
    s.send("next", json!({ "threadId": 1 }));
    let stop2 = s.stopped();
    assert_eq!(stop2["body"]["reason"], "step", "second stop is a step");
    assert_eq!(s.stack_line(), 4, "step lands on line 4");

    // 3) Continue runs to completion: the program prints, then terminates.
    s.send("continue", json!({ "threadId": 1 }));
    let mut saw_output = false;
    let mut saw_terminated = false;
    for _ in 0..50 {
        match s.read_msg() {
            Some(m) if m["type"] == "event" && m["event"] == "output" => {
                if m["body"]["output"].as_str().unwrap_or("").contains("c=3") {
                    saw_output = true;
                }
            }
            Some(m) if m["type"] == "event" && m["event"] == "terminated" => {
                saw_terminated = true;
                break;
            }
            Some(_) => continue,
            None => break,
        }
    }
    assert!(
        saw_output,
        "program stdout (c=3) streamed as an output event"
    );
    assert!(saw_terminated, "session terminated after continue");

    let _ = std::fs::remove_file(&path);
}
