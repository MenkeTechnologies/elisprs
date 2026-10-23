//! editfns.c's fields, `char-after`/`char-before` positions and the small C
//! surface the editing libraries call: minibuf.c's `minibufferp` and
//! `minibuffer-prompt-end`, the global keymap tree, charset.c's
//! `char-charset`/`charset-after`, buffer.c's `enable-multibyte-characters`
//! and `get-buffer-create` of a buffer.
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

/// `find_field`: a position between two fields belongs to the one an inserted
/// character would join; ESCAPE-FROM-EDGE takes the other side.
#[test]
fn field_bounds() {
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"abc\" 'field 'f1) \"def\" (propertize \"ghi\" 'field 'f2)) (list (field-beginning 5) (field-end 5) (field-string 2) (field-string-no-properties 8) (field-beginning 4) (field-end 4) (field-beginning 4 t) (field-end 4 t) (field-end 7) (field-beginning 7)))"),
        "(4 7 #(\"abc\" 0 3 (field f1)) \"ghi\" 1 4 1 7 7 4)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"aa\" 'field 'x) (propertize \"bb\" 'field 'boundary) (propertize \"cc\" 'field 'y)) (list (field-beginning 4 t) (field-end 4 t) (field-beginning 3) (field-end 3)))"),
        "(1 7 1 3)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"abc\" 'field 'f1) \"def\") (delete-field 2) (buffer-string))"),
        "\"def\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"prompt> \") (put-text-property 1 (point) 'field 'prompt) (insert \"input\") (list (field-beginning) (field-string)))"),
        "(9 \"input\")"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (list (field-beginning 2) (field-end 2) (field-string)))"),
        "(1 4 \"abc\")"
    );
}

/// NEW-POS is pulled back into OLD-POS's field, unless fields are inhibited.
#[test]
fn constrain_to_field() {
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"abc\" 'field 'f1) \"def\") (list (constrain-to-field 6 2) (constrain-to-field 2 6) (constrain-to-field 3 5 nil) (progn (goto-char 6) (constrain-to-field nil 2) (point)) (let ((inhibit-field-text-motion t)) (constrain-to-field 6 2))))"),
        "(4 4 4 4 6)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert (propertize \"ab\\ncd\" 'field 'f1) \"ef\") (list (constrain-to-field 7 2 nil t) (constrain-to-field 7 5 nil t) (constrain-to-field 7 2)))"),
        "(6 6 6)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (constrain-to-field \"x\" 1))"),
        "(ERR (wrong-type-argument integer-or-marker-p \"x\"))"
    );
}

/// ` *Minibuf-0*` is a minibuffer but never an active one.
#[test]
fn minibuffer_predicates() {
    assert_eq!(
        eval("(list (minibufferp) (minibufferp nil t) (minibufferp \" *Minibuf-1*\") (minibufferp \"nonexist\") (minibufferp (get-buffer-create \" *Minibuf-0*\")) (minibufferp (get-buffer-create \" *Minibuf-0*\") t))"),
        "(nil nil nil nil t nil)"
    );
    assert_eq!(
        eval("(minibufferp 3)"),
        "(ERR (wrong-type-argument bufferp 3))"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (narrow-to-region 2 3) (minibuffer-prompt-end))"),
        "2"
    );
    assert_eq!(
        eval("(with-current-buffer \" *Minibuf-0*\" (minibuffer-prompt-end))"),
        "1"
    );
}

/// subr.el's `esc-map`, `ctl-x-map` and `global-map`, selected by keymap.c's
/// `use-global-map`.
#[test]
fn global_keymap_tree() {
    assert_eq!(
        eval("(list (keymapp global-map) (eq (current-global-map) global-map) (lookup-key global-map \"\\e\") (lookup-key global-map \"\\C-x\") (eq (symbol-function 'ESC-prefix) esc-map) (lookup-key ctl-x-map \"\\t\") (lookup-key esc-map \"\\C-\\\\\") (lookup-key global-map \"\\C-x\\t\") (lookup-key global-map \"\\M-x\"))"),
        "(t t ESC-prefix Control-X-prefix t indent-rigidly indent-region indent-rigidly execute-extended-command)"
    );
    assert_eq!(
        eval("(use-global-map 3)"),
        "(ERR (wrong-type-argument keymapp 3))"
    );
    assert_eq!(
        eval("(let ((m (make-sparse-keymap))) (use-global-map m) (prog1 (eq (current-global-map) m) (use-global-map global-map)))"),
        "t"
    );
}

/// The charsets of an `emacs -Q` session's priority list.
#[test]
fn charsets() {
    assert_eq!(
        eval("(mapcar (lambda (c) (char-charset c)) (list ?a #x7f #x80 #xa0 ?é #x100 ?λ ?中 #x10FFFF #x3FFF80 #x3FFFFF))"),
        "(ascii ascii unicode unicode unicode unicode unicode unicode unicode eight-bit eight-bit)"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"aé\") (list (charset-after 1) (charset-after 2) (charset-after 3) (charset-after 0)))"),
        "(ascii unicode nil nil)"
    );
    assert_eq!(
        eval("(list (char-charset ?a '(unicode ascii)) (char-charset #x80 '(ascii)))"),
        "(unicode nil)"
    );
    assert_eq!(
        eval("(char-charset -1)"),
        "(ERR (wrong-type-argument characterp -1))"
    );
    assert_eq!(
        eval("(list (char-table-p char-script-table) (char-table-subtype char-script-table))"),
        "(t char-script-table)"
    );
}

/// `char-after` and `char-before` take any position `fix_position` does.
#[test]
fn char_after_positions() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (list (char-after (copy-marker 2)) (char-before (copy-marker 2)) (char-after -5) (char-before -5) (char-after (expt 2 70)) (char-after 3) (char-before 4) (char-before 1)))"),
        "(98 97 nil nil nil 99 99 nil)"
    );
    assert_eq!(
        eval("(char-before \"x\")"),
        "(ERR (wrong-type-argument integer-or-marker-p \"x\"))"
    );
    assert_eq!(
        eval("(char-after 1.0)"),
        "(ERR (wrong-type-argument integer-or-marker-p 1.0))"
    );
}

/// `enable-multibyte-characters` cannot be set; `get-buffer-create` hands a
/// buffer object back, killed or not.
#[test]
fn multibyte_flag_and_buffer_objects() {
    assert_eq!(
        eval("(list enable-multibyte-characters (default-value 'enable-multibyte-characters) (special-variable-p 'enable-multibyte-characters))"),
        "(t t t)"
    );
    assert_eq!(
        eval("(setq enable-multibyte-characters nil)"),
        "(ERR (setting-constant enable-multibyte-characters))"
    );
    assert_eq!(
        eval("(let ((enable-multibyte-characters nil)) 1)"),
        "(ERR (setting-constant enable-multibyte-characters))"
    );
    assert_eq!(
        eval("(set-default 'enable-multibyte-characters nil)"),
        "(ERR (setting-constant enable-multibyte-characters))"
    );
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"gbc\"))) (prog1 (eq (get-buffer-create b) b) (kill-buffer b)))"),
        "t"
    );
    assert_eq!(
        eval("(let ((b (generate-new-buffer \"gbc\"))) (kill-buffer b) (list (eq (get-buffer-create b) b) (buffer-live-p b)))"),
        "(t nil)"
    );
    assert_eq!(
        eval("(get-buffer-create 3)"),
        "(ERR (wrong-type-argument stringp 3))"
    );
}
