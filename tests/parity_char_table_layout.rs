//! Char-tables laid out as `chartab.c` lays them out: the 64-slot top level and the
//! depth-1..3 sub-char-tables `prin1` shows, the cached ASCII slot and its
//! `#N=` label, `map-char-table`, `optimize-char-table`, full keymaps built on
//! them, and `accessible-keymaps` / `where-is-internal` over keymaps.
//!
//! Every expectation is the output of `emacs -Q --batch` (GNU Emacs 31.1) for
//! the same form in a fresh process: `=` and the printed value, or `!` and the
//! printed error object.

use elisprs::{eval_str, print, reset_host};

fn check(form: &str, expected: &str) {
    reset_host();
    let src = format!(
        "(let ((print-escape-newlines t) (print-circle t)) \
           (condition-case e (eval '{form} t) \
             (:success (concat \"=\" (prin1-to-string e))) \
             (error (concat \"!\" (prin1-to-string e)))))"
    );
    let v = eval_str(&src).expect("eval failed");
    assert_eq!(print(&v, false), expected, "{form}");
}
#[test]
fn a_char_table_prints_its_block_tree() {
    check(
        r#"(make-char-table 'foo)"#,
        r#"=#^[nil nil foo nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil]"#,
    );
    check(
        r#"(make-char-table 'foo 7)"#,
        r#"=#^[7 nil foo 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7 7]"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (set-char-table-range ct '(0 . 65535) 2) ct)"#,
        r#"=#^[nil nil foo 2 2 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil]"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (set-char-table-range ct '(0 . 4194303) 2) ct)"#,
        r#"=#^[nil nil foo 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2]"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (set-char-table-range ct t 3) ct)"#,
        r#"=#^[nil nil foo 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3 3]"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (set-char-table-range ct nil 3) ct)"#,
        r#"=#^[3 nil foo nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil]"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo 1))) (aset ct ?a 2) (char-table-range ct nil))"#,
        r#"=1"#,
    );
    check(
        r#"(progn (put 'bar 'char-table-extra-slots 2) (make-char-table 'bar))"#,
        r#"=#^[nil nil bar nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil]"#,
    );
    check(
        r#"(progn (put 'bar 'char-table-extra-slots 2) (make-char-table 'bar 5))"#,
        r#"=#^[5 nil bar 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5 5]"#,
    );
    check(
        r#"(progn (put 'bar 'char-table-extra-slots 2) (let ((ct (make-char-table 'bar))) (set-char-table-extra-slot ct 1 'x) ct))"#,
        r#"=#^[nil nil bar nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil x]"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (list (aref ct ?a) (char-table-range ct ?a) (char-table-range ct '(1 . 2))))"#,
        r#"=(nil nil nil)"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (set-char-table-range ct '(97 . 100) 'x) (let (r) (map-char-table (lambda (k v) (push (cons k v) r)) ct) r))"#,
        r#"=(((101 . 4194303) . x))"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (aset ct 97 'x) (aset ct 98 'x) (aset ct 100 'y) (let (r) (map-char-table (lambda (k v) (push (cons (if (consp k) (cons (car k) (cdr k)) k) v) r)) ct) r))"#,
        r#"=((100 . y) ((97 . 98) . x))"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo)) (p (make-char-table 'foo))) (aset p 50 'p) (aset ct 49 'c) (set-char-table-parent ct p) (let (r) (map-char-table (lambda (k v) (push (cons (if (consp k) (cons (car k) (cdr k)) k) v) r)) ct) r))"#,
        r#"=((50 . p) (49 . c))"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo 'z))) (let (r) (map-char-table (lambda (k v) (push (cons (if (consp k) (cons (car k) (cdr k)) k) v) r)) ct) r))"#,
        r#"=(((0 . 4194303) . z))"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (set-char-table-range ct nil 'dflt) (aset ct ?a 'x) (let (r) (map-char-table (lambda (k v) (push (cons (if (consp k) (cons (car k) (cdr k)) k) v) r)) ct) r))"#,
        r#"=(((98 . 4194303) . dflt) (97 . x) ((0 . 96) . dflt))"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (aset ct ?a 1) (equal ct (copy-sequence ct)))"#,
        r#"=t"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (aset ct ?a 1) (eq ct (copy-sequence ct)))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (aset ct ?a 1) (let ((c (copy-sequence ct))) (aset c ?b 2) (list (aref ct ?b) (aref c ?b))))"#,
        r#"=(nil 2)"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (optimize-char-table ct) ct)"#,
        r#"=#^[nil nil foo nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil]"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (aset ct ?a 1) (aset ct ?a nil) (optimize-char-table ct) ct)"#,
        r#"=#^[nil nil foo nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil]"#,
    );
    check(r#"(length (make-char-table 'foo))"#, r#"=4194304"#);
    check(
        r#"(aref (make-char-table 'foo) 4194304)"#,
        r#"!(wrong-type-argument characterp 4194304)"#,
    );
    check(
        r#"(aset (make-char-table 'foo) -1 1)"#,
        r#"!(wrong-type-argument characterp -1)"#,
    );
    check(r#"(char-table-subtype (make-char-table 'foo))"#, r#"=foo"#);
    check(r#"(char-table-p (make-char-table 'foo))"#, r#"=t"#);
    check(r#"(vectorp (make-char-table 'foo))"#, r#"=nil"#);
    check(r#"(arrayp (make-char-table 'foo))"#, r#"=t"#);
    check(r#"(sequencep (make-char-table 'foo))"#, r#"=t"#);
    check(r#"(type-of (make-char-table 'foo))"#, r#"=char-table"#);
    check(
        r#"(make-char-table 'foo 1 2)"#,
        r#"!(wrong-number-of-arguments make-char-table 3)"#,
    );
    check(
        r#"(make-char-table 'foo 'a)"#,
        r#"=#^[a nil foo a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a]"#,
    );
    check(
        r#"(make-char-table 1)"#,
        r#"!(wrong-type-argument symbolp 1)"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (aset ct ?\M-a 1))"#,
        r#"!(wrong-type-argument characterp 134217825)"#,
    );
    check(
        r#"(let ((ct (make-char-table 'foo))) (aset ct 4194303 1) (aref ct 4194303))"#,
        r#"=1"#,
    );
    check(
        r#"(make-keymap)"#,
        r#"=(keymap #^[nil nil keymap nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil])"#,
    );
    check(
        r#"(make-keymap "prompt")"#,
        r#"=(keymap #^[nil nil keymap nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] "prompt")"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m "a" 'foo) (lookup-key m "a"))"#,
        r#"=foo"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m "a" 'foo) (define-key m "a" nil) (list (lookup-key m "a") (aref (cadr m) ?a)))"#,
        r#"=(nil t)"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m [f1] 'foo) m)"#,
        r#"=(keymap #^[nil nil keymap nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] (f1 . foo))"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m "\M-a" 'foo) (lookup-key m "\M-a"))"#,
        r#"=foo"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m [?\C-x ?a] 'foo) (lookup-key m [?\C-x ?a]))"#,
        r#"=foo"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m "a" 'foo) (define-key m "b" 'bar) (let (r) (map-keymap (lambda (k v) (push (cons k v) r)) m) r))"#,
        r#"=((98 . bar) (97 . foo))"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m "a" 'foo) (let (r) (map-keymap (lambda (k v) (push (cons k v) r)) (copy-keymap m)) r))"#,
        r#"=((97 . foo))"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m "a" 'foo) (keymap-parent m))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((m (make-keymap)) (p (make-sparse-keymap))) (define-key p "z" 'zz) (set-keymap-parent m p) (lookup-key m "z"))"#,
        r#"=zz"#,
    );
    check(r#"(let ((m (make-keymap))) (keymapp m))"#, r#"=t"#);
    check(
        r#"(let ((m (make-keymap))) (define-key m "a" 'foo) (keymap-canonicalize m))"#,
        r#"=(keymap (97 . foo))"#,
    );
    check(r#"(let ((m (make-keymap))) (keymap-prompt m))"#, r#"=nil"#);
    check(
        r#"(let ((m (make-keymap "P"))) (keymap-prompt m))"#,
        r#"="P""#,
    );
    check(
        r#"(let ((m (make-keymap))) (suppress-keymap m) (lookup-key m "a"))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((m (make-keymap))) (define-key m (kbd "C-a") 'beg) (lookup-key m (kbd "C-a")))"#,
        r#"=beg"#,
    );
}

#[test]
fn accessible_keymaps_and_where_is_internal() {
    check(
        r#"(length (make-hash-table))"#,
        r#"!(wrong-type-argument sequencep #s(hash-table))"#,
    );
    check(r#"(length (make-bool-vector 3 t))"#, r#"=3"#);
    check(r#"(length (record 'a 1))"#, r#"=2"#);
    check(r#"(length (make-char-table 'x))"#, r#"=4194304"#);
    check(r#"(arrayp (make-char-table 'x))"#, r#"=t"#);
    check(r#"(sequencep (make-char-table 'x))"#, r#"=t"#);
    check(r#"(vectorp (make-char-table 'x))"#, r#"=nil"#);
    check(r#"(copy-sequence (make-bool-vector 2 t))"#, r#"=#&2"""#);
    check(r#"(seq-length (make-char-table 'x))"#, r#"=4194304"#);
    check(r#"(elt (make-char-table 'x 5) 3)"#, r#"=5"#);
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "\C-xa" 'foo) (define-key m "\C-xb" 'bar) (accessible-keymaps m))"#,
        r#"=(([] keymap (24 . #1=(keymap (98 . bar) (97 . foo)))) ([24] . #1#))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "\C-xa" 'foo) (define-key m "\C-xb" 'bar) (accessible-keymaps m [24]))"#,
        r#"=(([24] keymap (98 . bar) (97 . foo)))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "\C-xa" 'foo) (define-key m "\M-a" 'bar) (accessible-keymaps m))"#,
        r#"=(([] keymap (27 . #1=(keymap (97 . bar))) (24 . #2=(keymap (97 . foo)))) ([27] . #1#) ([24] . #2#))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (define-key m "\C-xb" 'foo) (define-key m [f1] 'foo) (where-is-internal 'foo m))"#,
        r#"=([f1] [97] [24 98])"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (define-key m "\C-xb" 'foo) (define-key m [f1] 'foo) (where-is-internal 'foo m t))"#,
        r#"=[97]"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m [f1] 'foo) (define-key m "\C-xb" 'foo) (where-is-internal 'foo m t))"#,
        r#"=[24 98]"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m [f1] 'foo) (where-is-internal 'foo m 'non-ascii))"#,
        r#"=[f1]"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (define-key m "a" 'bar) (where-is-internal 'foo m))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap)) (p (make-sparse-keymap))) (define-key p "a" 'foo) (set-keymap-parent m p) (where-is-internal 'foo m))"#,
        r#"=([97])"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap)) (p (make-sparse-keymap))) (define-key p "a" 'foo) (set-keymap-parent m p) (define-key m "a" 'other) (where-is-internal 'foo m))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap)) (n (make-sparse-keymap))) (define-key n "x" 'foo) (where-is-internal 'foo (list m n)))"#,
        r#"=([120])"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "\M-x" 'foo) (where-is-internal 'foo m))"#,
        r#"=([134217848])"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" '(menu-item "x" foo)) (where-is-internal 'foo m))"#,
        r#"=([97])"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" '(menu-item "x" foo)) (where-is-internal 'foo m nil t))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" '("x" . foo)) (where-is-internal 'foo m))"#,
        r#"=([97])"#,
    );
    check(r#"(where-is-internal 'foo nil)"#, r#"=nil"#);
    check(
        r#"(where-is-internal 'foo '(keymap (97 . foo)))"#,
        r#"=([97])"#,
    );
    check(
        r#"(where-is-internal 'foo (list '(keymap (97 . foo)) '(keymap (98 . foo))))"#,
        r#"=([97] [98])"#,
    );
    check(
        r#"(where-is-internal 'foo (make-sparse-keymap))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" (lambda () (interactive))) (where-is-internal (lookup-key m "a") m))"#,
        r#"=([97])"#,
    );
}
