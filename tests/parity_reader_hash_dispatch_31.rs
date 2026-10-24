//! lread.c `read0`'s `#` dispatch: `#s`, `#$`, `#^[…]` / `#^^[…]` and the
//! `read-circle` gate on `#N=` / `#N#`.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch -l FILE`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v =
        eval_str(&format!("(condition-case e {src} (error (list 'ERR e)))")).expect("eval failed");
    print(&v, true)
}

/// `#s` not followed by `(` reports the buffered text; `#s()` is `#s`; a
/// record's type slot is whatever object was written there.
#[test]
fn hash_s_errors_and_record_type_slot() {
    assert_eq!(
        eval("(read-from-string \"#s1\")"),
        "(ERR (invalid-read-syntax \"#s1\"))"
    );
    assert_eq!(
        eval("(read-from-string \"#s\")"),
        "(ERR (invalid-read-syntax \"#s\"))"
    );
    assert_eq!(
        eval("(read-from-string \"#s()\")"),
        "(ERR (invalid-read-syntax \"#s\"))"
    );
    assert_eq!(eval("(read-from-string \"#s(a\")"), "(ERR (end-of-file))");
    assert_eq!(
        eval("(let ((r (car (read-from-string \"#s(1 2)\")))) (list (type-of r) (aref r 0) (aref r 1)))"),
        "(1 1 2)"
    );
    assert_eq!(
        eval("(let ((r (car (read-from-string \"#s(\\\"x\\\" 2)\")))) (list (type-of r) (aref r 1) (recordp r)))"),
        "(\"x\" 2 t)"
    );
}

/// `#$` reads as the value of `load-file-name`.
#[test]
fn hash_dollar_is_load_file_name() {
    assert_eq!(
        eval("(let ((load-file-name \"/a/b.el\")) (read-from-string \"(#$ . 3)\"))"),
        "((\"/a/b.el\" . 3) . 8)"
    );
    assert_eq!(
        eval("(let ((load-file-name nil)) (read-from-string \"#$\"))"),
        "(nil . 2)"
    );
}

/// With `read-circle` nil, `#N=` and `#N#` are invalid syntax naming the
/// buffered text; `#NrDIGITS` is unaffected.
#[test]
fn read_circle_nil_rejects_labels() {
    assert_eq!(
        eval("(let ((read-circle nil)) (read-from-string \"#1=a\"))"),
        "(ERR (invalid-read-syntax \"#1=\"))"
    );
    assert_eq!(
        eval("(let ((read-circle nil)) (read-from-string \"#1#\"))"),
        "(ERR (invalid-read-syntax \"#1#\"))"
    );
    assert_eq!(
        eval("(let ((read-circle nil)) (read-from-string \"#2r101\"))"),
        "(5 . 6)"
    );
    assert_eq!(
        eval("(let* ((s \"#1=a\") (i 0) (read-circle nil) (f (lambda (&optional c) (if c (setq i (1- i)) (prog1 (if (< i (length s)) (aref s i)) (setq i (1+ i))))))) (list (condition-case e (read f) (error e)) i))"),
        "((invalid-read-syntax \"#1=\") 3)"
    );
}

/// `#^[…]` is a char-table and `#^^[…]` a sub-char-table, checked by
/// `char_table_from_rev_list` / `sub_char_table_from_rev_list`; a printed
/// char-table reads back as an `equal` one.
#[test]
fn char_table_literals() {
    for (src, want) in [
        ("#^x", "(ERR (invalid-read-syntax \"#^x\"))"),
        ("#^", "(ERR (invalid-read-syntax \"#^\"))"),
        ("#^^x", "(ERR (invalid-read-syntax \"#^^x\"))"),
        (
            "#^[nil]",
            "(ERR (invalid-read-syntax \"Invalid size char-table\"))",
        ),
        ("#^[", "(ERR (end-of-file))"),
        ("#^^[1]", "(ERR (error \"Invalid size of sub-char-table\"))"),
        (
            "#^^[5 0 1]",
            "(ERR (error \"Invalid depth in sub-char-table\"))",
        ),
        (
            "#^^[1 0]",
            "(ERR (error \"Invalid size in sub-char-table\"))",
        ),
    ] {
        assert_eq!(
            eval(&format!("(read-from-string \"{src}\")")),
            want,
            "{src}"
        );
    }
    assert_eq!(
        eval("(read-from-string (concat \"#^^[1 -1 \" (mapconcat (lambda (_) \"0\") (number-sequence 1 16) \" \") \"]\"))"),
        "(ERR (error \"Invalid minimum character in sub-char-table\"))"
    );
    assert_eq!(
        eval("(read-from-string (concat \"#^[nil nil foo nil \" (mapconcat (lambda (_) \"7\") (number-sequence 1 63) \" \") \"]\"))"),
        "(ERR (invalid-read-syntax \"Invalid size char-table\"))"
    );
    assert_eq!(
        eval("(let ((c (make-char-table 'foo 'd))) (set-char-table-range c '(#x100 . #x2000) 'r) (aset c ?b 'b) (aset c #x10FFFF 'top) (let ((r (car (read-from-string (prin1-to-string c))))) (list (aref r ?a) (aref r ?b) (aref r #x150) (aref r #x10FFFF) (equal r c) (string= (prin1-to-string r) (prin1-to-string c)))))"),
        "(d b r top t t)"
    );
    assert_eq!(
        eval("(let ((print-circle t) (c (make-char-table 'foo 0))) (aset c ?a 1) (let ((r (car (read-from-string (prin1-to-string c))))) (list (aref r ?a) (aref r ?z) (equal r c))))"),
        "(1 0 t)"
    );
    assert_eq!(
        eval("(progn (put 'xx 'char-table-extra-slots 2) (let ((c (make-char-table 'xx 0))) (set-char-table-extra-slot c 1 'e) (let ((r (car (read-from-string (prin1-to-string c))))) (list (char-table-extra-slot r 1) (char-table-subtype r) (char-table-parent r)))))"),
        "(e xx nil)"
    );
    assert_eq!(
        eval("(equal (car (read-from-string (prin1-to-string (syntax-table)))) (syntax-table))"),
        "t"
    );
}
