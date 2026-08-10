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

/// A program whose only statement inside a `defun` is on line 2, so a line
/// breakpoint there can only be reached by entering the function body.
const FUNCTION_PROGRAM: &str =
    "(defun add2 (x)\n  (+ x 2))\n(princ (format \"r=%d\\n\" (add2 40)))\n";

/// `add2` reached by every route elisp offers: the direct call, `funcall` and
/// `apply` on the symbol, and a call of the object read out of the function
/// cell. GNU Emacs 30.2 (`advice-add` on `add2`, measured) fires on all four.
const ROUTES_PROGRAM: &str = "(defun add2 (x)\n  (+ x 2))\n\
    (princ (format \"1=%d\\n\" (add2 1)))\n\
    (princ (format \"2=%d\\n\" (funcall 'add2 2)))\n\
    (princ (format \"3=%d\\n\" (apply 'add2 '(3))))\n\
    (princ (format \"4=%d\\n\" (funcall (symbol-function 'add2) 4)))\n";

/// A function object captured before `add2` is redefined. Emacs 30.2 does NOT
/// fire on such a stale object — instrumentation lives on the function cell —
/// so only the second call may stop, and it stops in the *new* body (line 5).
const STALE_PROGRAM: &str = "(defun add2 (x)\n  (+ x 2))\n\
    (setq old (symbol-function 'add2))\n\
    (defun add2 (x)\n  (+ x 3))\n\
    (princ (format \"old=%d\\n\" (funcall old 10)))\n\
    (princ (format \"new=%d\\n\" (add2 10)))\n";

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    seq: i64,
    /// Every `output` event's text, accumulated as messages are read. `read_until`
    /// discards what it skips past, so debuggee output emitted between two stops
    /// would otherwise be invisible to the assertions.
    program_output: String,
    /// How many `stopped` events arrived over the whole session.
    ///
    /// Three tests here assert an *absence* of a stop, which `terminated` plus
    /// stdout cannot express: `run_to_end` and `read_until` discard every message
    /// they skip, so an adapter that announced a bogus stop without actually
    /// pausing produced exactly the same observations as one that stayed silent.
    stops_seen: usize,
}

impl Session {
    /// Standard handshake with a single line breakpoint on line 3.
    fn start(program_path: &str) -> Self {
        Self::start_with_lines(program_path, &[3])
    }

    /// Same handshake, with the breakpoint lines chosen by the caller.
    fn start_with_lines(program_path: &str, lines: &[u32]) -> Self {
        let mut s = Self::spawn();
        s.send("initialize", json!({}));
        let bps: Vec<Value> = lines.iter().map(|l| json!({ "line": l })).collect();
        s.send(
            "setBreakpoints",
            json!({ "source": { "path": program_path }, "breakpoints": bps }),
        );
        s.send("launch", json!({ "program": program_path }));
        s.send("configurationDone", json!({}));
        s
    }

    /// The handshake for a *function* breakpoint session: `initialize` first (so
    /// the test can read the capabilities back), then the named functions, then
    /// launch. No line breakpoints at all, so any stop is the function
    /// breakpoint's doing.
    fn start_with_functions(program_path: &str, names: &[&str]) -> Self {
        let mut s = Self::spawn();
        s.send("initialize", json!({}));
        let caps = s.response("initialize");
        assert_eq!(
            caps["body"]["supportsFunctionBreakpoints"], true,
            "adapter must advertise supportsFunctionBreakpoints, or a conforming \
             client never sends setFunctionBreakpoints at all"
        );
        let bps: Vec<Value> = names.iter().map(|n| json!({ "name": n })).collect();
        s.send("setFunctionBreakpoints", json!({ "breakpoints": bps }));
        let r = s.response("setFunctionBreakpoints");
        let verified = r["body"]["breakpoints"]
            .as_array()
            .expect("breakpoints array");
        assert_eq!(verified.len(), names.len(), "one entry per requested name");
        assert!(
            verified.iter().all(|b| b["verified"] == true),
            "every function breakpoint is verified"
        );
        s.send("launch", json!({ "program": program_path }));
        s.send("configurationDone", json!({}));
        s
    }

    /// Spawn `elisp --dap` with a watchdog; no protocol traffic yet.
    fn spawn() -> Self {
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
        Session {
            child,
            stdin,
            stdout,
            seq: 0,
            program_output: String::new(),
            stops_seen: 0,
        }
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
        let msg: Value = serde_json::from_slice(&buf).ok()?;
        if msg["type"] == "event" && msg["event"] == "output" {
            self.program_output
                .push_str(msg["body"]["output"].as_str().unwrap_or(""));
        }
        if msg["type"] == "event" && msg["event"] == "stopped" {
            self.stops_seen += 1;
        }
        Some(msg)
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

    /// `evaluate` in the paused host — the only view of a *lexical* binding
    /// (`variables` lists symbol value cells, which a closure parameter is not).
    fn evaluate(&mut self, expr: &str) -> String {
        self.send("evaluate", json!({ "expression": expr }));
        let r = self.response("evaluate");
        r["body"]["result"].as_str().unwrap_or("").to_string()
    }

    /// Drive to completion. Returns the debuggee's stdout for the WHOLE session
    /// (see [`Session::program_output`]) and whether `terminated` arrived.
    fn run_to_end(&mut self) -> (String, bool) {
        for _ in 0..50 {
            match self.read_msg() {
                Some(m) if m["type"] == "event" && m["event"] == "terminated" => {
                    return (self.program_output.clone(), true);
                }
                Some(_) => continue,
                None => break,
            }
        }
        (self.program_output.clone(), false)
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

/// Debugging has to reach *inside* a user function, not just the top level.
///
/// The whole program compiles to one debug-instrumented chunk, so a `defun`'s
/// body carries statement markers too — and a compiled call funnels through
/// `host::call_function` → `run_closure` → `run_chunk`, which keeps those
/// markers live. This pins that: line 2 exists only inside `add2`'s body, so
/// the stop proves the function was defined, called, and entered under `--dap`,
/// and `evaluate` proves the parameter is bound in the paused frame.
#[test]
fn dap_line_breakpoint_inside_a_function_body() {
    let path = std::env::temp_dir().join(format!("elisprs_dap_fn_{}.el", std::process::id()));
    std::fs::write(&path, FUNCTION_PROGRAM).expect("write program");
    let path_str = path.to_string_lossy().into_owned();

    let mut s = Session::start_with_lines(&path_str, &[2]);

    let stop = s.stopped();
    assert_eq!(
        stop["body"]["reason"], "breakpoint",
        "stops on the breakpoint inside the function body"
    );
    assert_eq!(s.stack_line(), 2, "the stop is on the function's body line");
    assert_eq!(
        s.evaluate("x"),
        "40",
        "the closure parameter is bound in the paused frame"
    );

    s.send("continue", json!({ "threadId": 1 }));
    let (out, terminated) = s.run_to_end();
    assert!(out.contains("r=42"), "the call returned 42, got {out:?}");
    assert!(terminated, "session terminated after continue");

    let _ = std::fs::remove_file(&path);
}

/// Write `src` to a uniquely named temp `.el` and hand back its path.
fn temp_program(tag: &str, src: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("elisprs_dap_{tag}_{}.el", std::process::id()));
    std::fs::write(&p, src).expect("write program");
    p
}

/// `setFunctionBreakpoints` with no line breakpoint at all: entering the named
/// function is the only thing that can produce a stop.
///
/// Entering arms step mode rather than pausing on the call, so the stop lands on
/// the function's first real statement (line 2). The cause is carried through the
/// arming, so the reason is the protocol's `"function breakpoint"` and not the
/// `"step"` that awkrs's `set_step_mode(true)` alone would produce.
#[test]
fn dap_function_breakpoint_stops_inside_the_named_function() {
    let path = temp_program("fbp", FUNCTION_PROGRAM);
    let path_str = path.to_string_lossy().into_owned();

    let mut s = Session::start_with_functions(&path_str, &["add2"]);

    let stop = s.stopped();
    assert_eq!(
        stop["body"]["reason"], "function breakpoint",
        "the stop is attributed to the function breakpoint, not to stepping"
    );
    assert_eq!(s.stack_line(), 2, "the stop is the function's body line");
    assert_eq!(s.evaluate("x"), "40", "the argument is bound at the stop");

    s.send("continue", json!({ "threadId": 1 }));
    let (out, terminated) = s.run_to_end();
    assert!(
        out.contains("r=42"),
        "the call still returned 42, got {out:?}"
    );
    assert!(terminated, "session terminated after continue");

    let _ = std::fs::remove_file(&path);
}

/// The `"function breakpoint"` attribution belongs to the stop it labels and to
/// no other. Stepping on from that stop is an ordinary `"step"` — it lands on
/// line 4, the caller's next statement, because line 3's own marker fired before
/// the call — and the next call re-arms and is labelled again.
#[test]
fn dap_function_breakpoint_reason_does_not_stick_to_later_stops() {
    let path = temp_program("reason", ROUTES_PROGRAM);
    let path_str = path.to_string_lossy().into_owned();

    let mut s = Session::start_with_functions(&path_str, &["add2"]);

    assert_eq!(s.stopped()["body"]["reason"], "function breakpoint");
    assert_eq!(s.stack_line(), 2);

    s.send("next", json!({ "threadId": 1 }));
    let stepped = s.stopped();
    assert_eq!(
        stepped["body"]["reason"], "step",
        "the attribution was consumed by the stop it labelled"
    );
    assert_eq!(
        s.stack_line(),
        4,
        "the step lands on the caller's next line"
    );

    // Still armed: the `funcall` on line 4 re-enters add2 and is labelled again.
    s.send("continue", json!({ "threadId": 1 }));
    assert_eq!(s.stopped()["body"]["reason"], "function breakpoint");
    assert_eq!(s.stack_line(), 2);

    let _ = std::fs::remove_file(&path);
}

/// The breakpoint tracks the *function cell*, not the syntax of the call.
///
/// `funcall`/`apply` resolve the designator to the function object before the
/// callee is entered, so a name-only match would silently miss three of these
/// four routes. Each argument is distinct, so the `evaluate` sequence also pins
/// which route produced which stop.
#[test]
fn dap_function_breakpoint_fires_on_every_call_route() {
    let path = temp_program("routes", ROUTES_PROGRAM);
    let path_str = path.to_string_lossy().into_owned();

    let mut s = Session::start_with_functions(&path_str, &["add2"]);

    for expected in ["1", "2", "3", "4"] {
        let stop = s.stopped();
        assert_eq!(stop["body"]["reason"], "function breakpoint");
        assert_eq!(s.stack_line(), 2, "every route stops in the body");
        assert_eq!(
            s.evaluate("x"),
            expected,
            "route with argument {expected} must stop"
        );
        s.send("continue", json!({ "threadId": 1 }));
    }

    let (out, terminated) = s.run_to_end();
    for line in ["1=3", "2=4", "3=5", "4=6"] {
        assert!(out.contains(line), "output {line} missing from {out:?}");
    }
    assert!(terminated, "session terminated after the last continue");

    let _ = std::fs::remove_file(&path);
}

/// A function object captured before the symbol was redefined is not the cell's
/// object any more, so it must not stop — the same answer Emacs 30.2 gives.
/// Only the direct call to the redefined `add2` stops, in the new body.
#[test]
fn dap_function_breakpoint_ignores_a_stale_function_object() {
    let path = temp_program("stale", STALE_PROGRAM);
    let path_str = path.to_string_lossy().into_owned();

    let mut s = Session::start_with_functions(&path_str, &["add2"]);

    let stop = s.stopped();
    assert_eq!(stop["body"]["reason"], "function breakpoint");
    assert_eq!(
        s.stack_line(),
        5,
        "the only stop is in the redefined body, not the captured one"
    );

    s.send("continue", json!({ "threadId": 1 }));
    let (out, terminated) = s.run_to_end();
    assert!(
        out.contains("old=12") && out.contains("new=13"),
        "both calls still ran, got {out:?}"
    );
    assert!(terminated, "no second stop: the stale object was ignored");
    assert_eq!(
        s.stops_seen, 1,
        "only the redefined body stops; the stale function object must not"
    );

    let _ = std::fs::remove_file(&path);
}

/// A breakpoint on a name nothing calls never stops, and the program is
/// unaffected.
#[test]
fn dap_function_breakpoint_on_an_uncalled_name_never_stops() {
    let path = temp_program("nofn", ROUTES_PROGRAM);
    let path_str = path.to_string_lossy().into_owned();

    let mut s = Session::start_with_functions(&path_str, &["nosuchfn"]);

    let (out, terminated) = s.run_to_end();
    assert!(terminated, "the program ran to completion with no stop");
    assert_eq!(
        s.stops_seen, 0,
        "a breakpoint on an uncalled name must produce no stopped event at all"
    );
    for line in ["1=3", "2=4", "3=5", "4=6"] {
        assert!(out.contains(line), "output {line} missing from {out:?}");
    }

    let _ = std::fs::remove_file(&path);
}

/// The set can be replaced while the executor is paused: armed from a line-
/// breakpoint stop, then cleared with an empty list at the next stop.
#[test]
fn dap_function_breakpoints_can_be_set_and_cleared_while_paused() {
    let path = temp_program("paused", ROUTES_PROGRAM);
    let path_str = path.to_string_lossy().into_owned();

    // Line 3 is `(princ ... (add2 1))`, so the marker fires before the call.
    let mut s = Session::start_with_lines(&path_str, &[3]);
    let stop = s.stopped();
    assert_eq!(stop["body"]["reason"], "breakpoint");

    s.send(
        "setFunctionBreakpoints",
        json!({ "breakpoints": [{ "name": "add2" }] }),
    );
    let r = s.response("setFunctionBreakpoints");
    assert_eq!(r["body"]["breakpoints"].as_array().map(Vec::len), Some(1));

    s.send("continue", json!({ "threadId": 1 }));
    let stop = s.stopped();
    assert_eq!(stop["body"]["reason"], "function breakpoint");
    assert_eq!(
        s.stack_line(),
        2,
        "the newly armed breakpoint stopped in add2"
    );

    // Empty list clears the whole set, so the three remaining routes run free.
    s.send("setFunctionBreakpoints", json!({ "breakpoints": [] }));
    let r = s.response("setFunctionBreakpoints");
    assert_eq!(r["body"]["breakpoints"].as_array().map(Vec::len), Some(0));

    s.send("continue", json!({ "threadId": 1 }));
    let (out, terminated) = s.run_to_end();
    assert!(terminated, "no further stops after clearing");
    assert_eq!(
        s.stops_seen, 2,
        "the line breakpoint and the one function breakpoint, nothing after the clear"
    );
    for line in ["1=3", "2=4", "3=5", "4=6"] {
        assert!(out.contains(line), "output {line} missing from {out:?}");
    }

    let _ = std::fs::remove_file(&path);
}
