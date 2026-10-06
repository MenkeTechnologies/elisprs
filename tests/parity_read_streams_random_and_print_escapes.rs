//! `random`, `read` from a buffer or marker, `format-time-string`'s `%z`
//! family, and the string escapes of `prin1`.
//!
//! - `Frandom` (fns.c): a non-positive integer LIMIT is `(args-out-of-range
//!   LIMIT)`, a bignum LIMIT bounds the answer, and any non-integer LIMIT
//!   (a float, a symbol, a string that reseeds) answers a random fixnum where
//!   elisprs signalled `integerp`.
//! - `Fread` (lread.c) reads a buffer from its point and leaves point after the
//!   object, and reads a marker from its position and advances it; elisprs
//!   accepted only a string, and `read` took no optional STREAM.
//! - nstrftime's `%:z`, `%::z`, `%:::z` were printed literally, and `%z`
//!   ignored the `-`/`_` flags and a field width (`do_tz_offset`).
//! - print.c's string loop: `print-escape-multibyte` writes `\xXXXX` with a
//!   `\ ` separator before a hex digit, and `octalout` widens an escape to
//!   three digits when an octal digit follows.
//!
//! Every expectation is `emacs -Q --batch --eval '(prin1 FORM)'` on GNU
//! Emacs 31.1.

use elisprs::{eval_str, print, reset_host};

fn check(src: &str, want: &str) {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    assert_eq!(print(&v, true), want, "{src}");
}

#[test]
fn random_follows_frandom() {
    check(
        r##"(condition-case e (random 0) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range 0))"##,
    );
    check(
        r##"(condition-case e (random -5) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range -5))"##,
    );
    check(
        r##"(condition-case e (random (- (expt 2 70))) (error (list 'signal e)))"##,
        r##"(signal (args-out-of-range -1180591620717411303424))"##,
    );
    check(
        r##"(let ((r (random (expt 2 70)))) (list (integerp r) (<= 0 r) (< r (expt 2 70))))"##,
        r##"(t t t)"##,
    );
    check(
        r##"(mapcar (lambda (x) (fixnump (random x))) (list 1.5 "seed" 'sym [1]))"##,
        r##"(t t t t)"##,
    );
    check(
        r##"(let ((a (progn (random "s") (list (random) (random)))) (b (progn (random "s") (list (random) (random))))) (equal a b))"##,
        r##"t"##,
    );
}

#[test]
fn read_takes_a_buffer_or_marker_stream() {
    check(
        r##"(with-temp-buffer (insert "(a b) c 12 \"s\"") (goto-char 1) (list (read (current-buffer)) (point) (read (current-buffer)) (point) (read (current-buffer)) (point) (read (current-buffer)) (point)))"##,
        r##"((a b) 6 c 8 12 11 "s" 15)"##,
    );
    check(
        r##"(with-temp-buffer (insert "  ; comment\n foo bar") (goto-char 1) (list (read (current-buffer)) (point)))"##,
        r##"(foo 17)"##,
    );
    check(
        r##"(with-temp-buffer (insert "x y z") (let ((m (copy-marker 3))) (list (read m) (marker-position m) (point))))"##,
        r##"(y 4 6)"##,
    );
    check(
        r##"(with-temp-buffer (insert "abc def") (narrow-to-region 1 3) (goto-char 1) (list (read (current-buffer)) (point)))"##,
        r##"(ab 3)"##,
    );
    check(
        r##"(with-temp-buffer (insert "(a") (goto-char 1) (condition-case nil (read (current-buffer)) (end-of-file (point))))"##,
        r##"3"##,
    );
    check(
        r##"(let ((b (generate-new-buffer "rb"))) (with-current-buffer b (insert "q r") (goto-char 1)) (prog1 (list (read b) (with-current-buffer b (point)) (eq b (current-buffer))) (kill-buffer b)))"##,
        r##"(q 2 nil)"##,
    );
    check(
        r##"(with-temp-buffer (insert "'a `(b ,c) #'d") (goto-char 1) (list (read (current-buffer)) (read (current-buffer)) (read (current-buffer)) (point)))"##,
        r##"('a `(b ,c) #'d 15)"##,
    );
    check(
        r##"(with-temp-buffer (insert "abc(") (goto-char 1) (list (read (current-buffer)) (point)))"##,
        r##"(abc 4)"##,
    );
    check(r##"(func-arity 'read)"##, r##"(0 . 1)"##);
}

#[test]
fn percent_z_colons_and_padding_follow_nstrftime() {
    check(
        r##"(format-time-string "%z|%:z|%::z|%:::z" 0 t)"##,
        r##""+0000|+00:00|+00:00:00|+00""##,
    );
    check(
        r##"(format-time-string "%z|%:z|%::z|%:::z" 0 -19800)"##,
        r##""-0530|-05:30|-05:30:00|-05:30""##,
    );
    check(
        r##"(format-time-string "%z|%:z|%::z|%:::z" 0 3661)"##,
        r##""+0101|+01:01|+01:01:01|+01:01:01""##,
    );
    check(
        r##"(format-time-string "%:::z|%:::z" 0 5400)"##,
        r##""+01:30|+01:30""##,
    );
    check(
        r##"(format-time-string "%::::z|%:a|%:" 0 t)"##,
        r##""%::::z|%:a|%:""##,
    );
    check(
        r##"(format-time-string "%-:z|%10:z|%_z|%8z|%-z" 0 3600)"##,
        r##""+1:00|+000001:00| +100|+0000100|+100""##,
    );
    check(
        r##"(format-time-string "%8z|%_8:z" 0 -3600)"##,
        r##""-0000100|   -1:00""##,
    );
}

#[test]
fn string_escapes_follow_print_c() {
    check(
        r##"(let ((print-escape-control-characters t)) (prin1-to-string (string 1 ?5 1 ?9 8 ?0 127 ?1 31 ?a)))"##,
        r##""\"\\0015\\19\\0100\\1771\\37a\"""##,
    );
    check(
        r##"(let ((print-escape-multibyte t)) (prin1-to-string "éz中F😀"))"##,
        r##""\"\\x00e9z\\x4e2d\\ F\\x1f600\"""##,
    );
    check(
        r##"(let ((print-escape-multibyte t)) (prin1-to-string "é\"a"))"##,
        r##""\"\\x00e9\\\"a\"""##,
    );
    check(
        r##"(let ((print-escape-multibyte t)) (format "%S" "éb"))"##,
        r##""\"\\x00e9\\ b\"""##,
    );
    check(
        r##"(let ((print-escape-multibyte t)) (prin1-to-string (propertize "éa" 'face 'bold)))"##,
        r##""#(\"\\x00e9\\ a\" 0 2 (face bold))""##,
    );
    check(
        r##"(let ((print-escape-multibyte t) (print-escape-control-characters t)) (prin1-to-string (string 233 1 ?a)))"##,
        r##""\"\\x00e9\\1a\"""##,
    );
    check(
        r##"(let ((print-escape-multibyte t)) (list (prin1-to-string 'é) (prin1-to-string ?é) (format "%s" "é")))"##,
        r##"("é" "233" "é")"##,
    );
}

/// `emacs -Q --batch --eval 1` writes nothing to stderr. The prelude's
/// `command-line-args` / `command-line-args-left` were `defvar`ed with a
/// docstring before `put` was defined, so storing the docstring failed and
/// every run printed `prelude form failed: void-function: put` twice.
#[test]
fn startup_writes_nothing_to_stderr() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_elisp"))
        .args(["-e", "1"])
        .output()
        .expect("run elisp");
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}
