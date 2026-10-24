//! `read` from a function stream: lread.c's `read0` reads through
//! `source_function_get` and gives characters back through
//! `source_function_unget`, so the stream function sees every `readchar`
//! and `unreadchar` of the reader, in order.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// Read TEXT through a function that logs each call: a character it
/// delivered, `(un C)` for one given back, nil past the end. The result is
/// `(OBJECT-OR-ERROR POSITION LOG)`.
fn trace(text: &str) -> String {
    trace_with(text, false)
}

/// [`trace`], printed with `print-circle` bound to CIRCLE.
fn trace_with(text: &str, circle: bool) -> String {
    let lit: String = text
        .chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            '\n' => vec!['\\', 'n'],
            c => vec![c],
        })
        .collect();
    let circle = if circle { "t" } else { "nil" };
    reset_host();
    let v = eval_str(&format!(
        "(let ((print-circle {circle})) (prin1-to-string (let* ((s \"{lit}\") (i 0) (log nil) (f (lambda (&optional c) (if c (progn (push (list 'un c) log) (setq i (1- i))) (let ((ch (if (< i (length s)) (aref s i) nil))) (setq i (1+ i)) (push ch log) ch))))) (list (condition-case e (read f) (error e)) i (nreverse log)))))"
    ))
    .expect("eval failed");
    print(&v, false)
}

/// Every atom inside a list is ended by a character that is given back and
/// read again; a closing `)` after a dotted tail is given back twice; a
/// string, a `#&` bool-vector and a list close without a give-back.
#[test]
fn reads_and_unreads_match_read0() {
    let cases = [
        ("(a b)", "((a b) 5 (40 97 32 (un 32) 32 98 41 (un 41) 41))"),
        ("(a b) x", "((a b) 5 (40 97 32 (un 32) 32 98 41 (un 41) 41))"),
        ("abc def", "(abc 3 (97 98 99 32 (un 32)))"),
        ("\"ab\" x", "(\"ab\" 4 (34 97 98 34))"),
        ("[1 (2 . 3) ?a 'b] ", "([1 (2 . 3) 97 'b] 17 (91 49 32 (un 32) 32 40 50 32 (un 32) 32 46 32 (un 32) 32 51 41 (un 41) 41 (un 41) 41 32 63 97 32 (un 32) 32 39 98 93 (un 93) 93))"),
        ("#x1f;c", "(31 4 (35 120 49 102 59 (un 59)))"),
        ("(a ;c\n b . c)", "((a b . c) 13 (40 97 32 (un 32) 32 59 99 10 32 98 32 (un 32) 32 46 32 (un 32) 32 99 41 (un 41) 41 (un 41) 41))"),
        ("1.5e3)", "(1500.0 5 (49 46 53 101 51 41 (un 41)))"),
        ("  ", "((end-of-file) 3 (32 32 nil))"),
        ("#s(a 1) ", "(#s(a 1) 7 (35 115 40 97 32 (un 32) 32 49 41 (un 41) 41))"),
        ("`(a ,b ,@c) #'f", "(`(a ,b ,@c) 11 (96 40 97 32 (un 32) 32 44 98 (un 98) 98 32 (un 32) 32 44 64 99 41 (un 41) 41))"),
        ("a\\ b c", "(a\\ b 4 (97 92 32 98 32 (un 32)))"),
        (")", "((invalid-read-syntax \")\") 1 (41))"),
        ("]", "((invalid-read-syntax \"]\") 1 (93))"),
        ("(a . b c)", "((invalid-read-syntax \"expected )\") 8 (40 97 32 (un 32) 32 46 32 (un 32) 32 98 32 (un 32) 32 99 (un 99) 99))"),
        ("( . a)", "((invalid-read-syntax \".\") 3 (40 32 46 32 (un 32)))"),
        ("?ab", "((invalid-read-syntax \"?\") 2 (63 97 98 (un 98)))"),
        ("?\\C-a)", "(1 5 (63 92 67 45 97 41 (un 41)))"),
        ("?\\^?x", "((invalid-read-syntax \"?\") 4 (63 92 94 63 120 (un 120)))"),
        ("?\\M-\\C-b", "(134217730 9 (63 92 77 45 92 67 45 98 nil))"),
        ("\"a\\x41;b\" x", "(\"aA;b\" 9 (34 97 92 120 52 49 59 (un 59) 59 98 34))"),
        ("\"\\101\\1012\"", "(\"AA2\" 11 (34 92 49 48 49 92 49 48 49 50 34))"),
        ("?\\x41 ", "(65 5 (63 92 120 52 49 32 (un 32) 32 (un 32)))"),
        ("?\\s-a ", "(8388705 5 (63 92 115 45 97 32 (un 32)))"),
        ("?\\sx", "((invalid-read-syntax \"?\") 3 (63 92 115 120 (un 120) 120 (un 120)))"),
        ("#s1", "((invalid-read-syntax \"#s1\") 2 (35 115 49 (un 49)))"),
        ("#^x", "((invalid-read-syntax \"#^x\") 2 (35 94 120 (un 120)))"),
        ("#&3\"a\"z", "(#&3\"\" 6 (35 38 51 34 97 34))"),
        ("#&x", "((invalid-read-syntax \"#&\") 3 (35 38 120))"),
        ("#!shebang", "((end-of-file) 11 (35 33 115 104 101 98 97 110 103 nil nil))"),
        ("#b101 ", "(5 5 (35 98 49 48 49 32 (un 32)))"),
        ("#o78", "((invalid-read-syntax \"integer, radix 8\") 5 (35 111 55 56 nil))"),
        ("#x-ff)", "(-255 5 (35 120 45 102 102 41 (un 41)))"),
        ("#36rZZ ", "(1295 6 (35 51 54 114 90 90 32 (un 32)))"),
        ("#1rz", "((invalid-read-syntax \"integer, radix 1\") 3 (35 49 114))"),
        ("#1#", "((invalid-read-syntax \"#1#\") 3 (35 49 35))"),
        ("#:foo bar", "(foo 5 (35 58 102 111 111 32 (un 32)))"),
        ("#: x", "(## 2 (35 58 32 (un 32)))"),
        ("#_x", "(x 4 (35 95 120 nil))"),
        ("##", "(## 2 (35 35))"),
        ("(1 2", "((end-of-file) 6 (40 49 32 (un 32) 32 50 nil nil))"),
        ("\"abc", "((end-of-file) 5 (34 97 98 99 nil))"),
        (",@x", "((\\,@ x) 4 (44 64 120 nil))"),
        (",x", "((\\, x) 3 (44 120 (un 120) 120 nil))"),
        (".5", "(0.5 3 (46 53 (un 53) 53 nil))"),
        ("-.5e2;", "(-50.0 5 (45 46 53 101 50 59 (un 59)))"),
        ("a\\(b", "(a\\(b 5 (97 92 40 98 nil))"),
        ("#y", "((invalid-read-syntax \"#y\") 2 (35 121))"),
        ("#2r102", "((invalid-read-syntax \"integer, radix 2\") 7 (35 50 114 49 48 50 nil))"),
        ("`,'#'f", "(`,'#'f 7 (96 44 39 (un 39) 39 35 39 102 nil))"),
    ];
    for (text, want) in cases {
        assert_eq!(trace(text), want, "reading {text:?}");
    }
    assert_eq!(
        trace_with("#1=(a . #1#)", true),
        "(#1=(a . #1#) 12 (35 49 61 40 97 32 (un 32) 32 46 32 (un 32) 32 35 49 35 41 (un 41) 41))"
    );
}

/// A nil stream is `standard-input`; anything but a character from the
/// function is the end of input, which `unreadchar` does not give back.
#[test]
fn standard_input_and_end_of_input() {
    assert_eq!(
        eval("(let* ((s \"(x y) z\") (i 0) (n 0) (f (lambda (&optional c) (setq n (1+ n)) (if c (setq i (1- i)) (prog1 (if (< i (length s)) (aref s i)) (setq i (1+ i))))))) (let ((standard-input f)) (list (read) i n)))"),
        "((x y) 5 9)"
    );
    assert_eq!(
        eval("(let* ((s \"foo\") (i 0) (f (lambda (&optional c) (if c (setq i (1- i)) (prog1 (if (< i (length s)) (aref s i) 'eof) (setq i (1+ i))))))) (list (read f) i))"),
        "(foo 4)"
    );
    assert_eq!(
        eval("(let* ((s \"(a\") (i 0) (f (lambda (&optional c) (if c (setq i (1- i)) (prog1 (if (< i (length s)) (aref s i) -1) (setq i (1+ i))))))) (list (condition-case e (read f) (error e)) i))"),
        "((end-of-file) 4)"
    );
    assert_eq!(
        eval("(let* ((s \"ab\") (i 0) (f (lambda (&optional c) (if c (error \"no unread %c\" c) (prog1 (if (< i (length s)) (aref s i)) (setq i (1+ i))))))) (read f))"),
        "ab"
    );
    assert_eq!(
        eval("(read (lambda (&optional c) (error \"boom\")))"),
        "(ERR (error \"boom\"))"
    );
}
