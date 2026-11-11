//! Embedding entry point: run an Emacs Lisp program from a host and capture its
//! output.

use crate::host::{self, StdinSource};

/// Unwind payload `kill-emacs` raises inside an embedded run in place of
/// `process::exit`: it skips `unwind-protect`/`condition-case` and is caught by
/// [`run_program`].
pub(crate) struct ExitNow(pub i32);

/// Run `src` as a complete program with `input` as its stdin and return
/// everything it wrote to stdout.
///
/// The forms are evaluated in order as `elisp -e`/`eval_str` does. Everything
/// printed to `standard-output` `t` (`princ`, `prin1`, `print`, `terpri`, ...)
/// and every `read-from-minibuffer`/`read-string` prompt is captured; those two
/// read their lines from `input`, as batch Emacs reads stdin. `message` still
/// goes to the process stderr, as in batch Emacs.
///
/// Each call runs on its own thread with [`crate::INTERP_STACK_BYTES`] of stack
/// and a fresh interpreter, so concurrent calls never share state or output.
/// A read error, an uncaught signal (its `error-message-string` is the message),
/// `kill-emacs` with a nonzero status, and a panic in the interpreter return
/// `Err`. `(kill-emacs)`/`(kill-emacs 0)` returns `Ok` with the output so far.
pub fn run_program(src: &str, input: &str) -> Result<String, String> {
    let src = src.to_owned();
    let stdin = StdinSource::Fed(std::io::Cursor::new(input.as_bytes().to_vec()));
    crate::with_interpreter_stack(move || {
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            host::with_host(|h| {
                h.stdin = stdin;
                h.capture = Some(String::new());
            });
            crate::eval_str(&src).map_err(|e| crate::format_error(&e))
        }));
        let output = host::with_host(|h| h.capture.take()).unwrap_or_default();
        match run {
            Ok(Ok(_)) => Ok(output),
            Ok(Err(e)) => Err(e),
            Err(panic) => match panic.downcast::<ExitNow>() {
                Ok(exit) if exit.0 == 0 => Ok(output),
                Ok(exit) => Err(format!("kill-emacs: exit status {}", exit.0)),
                Err(panic) => Err(panic_message(panic.as_ref())),
            },
        }
    })
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    let msg = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic");
    format!("interpreter panicked: {msg}")
}
