//! lread.c `skip_lazy_string`: `#@NNN` skips a lazily-loaded docstring or
//! bytecode string. Reading from a string or a buffer, the count is not used
//! (`skip_dyn_bytes` skips through the next `\037`); `#@00` reads as nil and
//! skips everything after it.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

/// Evaluate SRC in a fresh host; a signalled error comes back as `(ERR ERROR)`.
fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// What follows the skipped text is read in its place, in any position an
/// object can occupy.
#[test]
fn lazy_string_skipping() {
    assert_eq!(
        eval("(read-from-string \"#@4 abc\\037def ghi\")"),
        "(def . 11)"
    );
    assert_eq!(
        eval("(read-from-string \"(a #@3 xy\\037 b)\")"),
        "((a b) . 13)"
    );
    assert_eq!(eval("(read-from-string \"(a #@3 xy\\037)\")"), "((a) . 11)");
    assert_eq!(eval("(read-from-string \"'#@2 z\\037q\")"), "('q . 8)");
    assert_eq!(
        eval("(read-from-string \"(a . #@2 q\\037b)\")"),
        "((a . b) . 13)"
    );
    assert_eq!(eval("(read-from-string \"#@ \\037x\")"), "(x . 5)");
    assert_eq!(eval("(read-from-string \"#@0\\037x\")"), "(x . 5)");
    assert_eq!(eval("(read-from-string \"#@1x\\037y\")"), "(y . 6)");
    assert_eq!(
        eval("(with-temp-buffer (insert \"#@5 hello\\037(1 2) rest\") (goto-char 1) (list (read (current-buffer)) (point)))"),
        "((1 2) 16)"
    );
}

/// Without a `\037` the skip runs to the end; `#@00` is nil and ends the
/// input; an overflowing count is invalid syntax.
#[test]
fn lazy_string_end_and_errors() {
    assert_eq!(eval("(car (read-from-string \"#@00 foo bar\"))"), "nil");
    assert_eq!(eval("(cdr (read-from-string \"#@00 foo bar\"))"), "12");
    assert_eq!(
        eval("(read-from-string \"(a #@00 b)\")"),
        "(ERR (end-of-file))"
    );
    assert_eq!(
        eval("(read-from-string \"#@99999999999999999999999 x\")"),
        "(ERR (invalid-read-syntax \"#@\"))"
    );
}
