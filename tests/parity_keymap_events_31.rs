//! Keymap traversal (`map-keymap`, `copy-keymap`, `keymap-prompt`), event symbols
//! and `event-convert-list`, and key descriptions (`key-description`,
//! `single-key-description`).
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
fn keymaps_are_traversed_copied_and_described() {
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (define-key m "b" 'bar) (let (r) (map-keymap (lambda (k v) (push (cons k v) r)) m) r))"#,
        r#"=((97 . foo) (98 . bar))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap)) (p (make-sparse-keymap))) (define-key m "a" 'foo) (define-key p "b" 'bar) (set-keymap-parent m p) (let (r) (map-keymap (lambda (k v) (push (cons k v) r)) m) r))"#,
        r#"=((98 . bar) (97 . foo))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap)) (p (make-sparse-keymap))) (define-key m "a" 'foo) (define-key p "b" 'bar) (set-keymap-parent m p) (let (r) (map-keymap-internal (lambda (k v) (push (cons k v) r)) m) r))"#,
        r#"=((97 . foo))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap)) (p (make-sparse-keymap))) (define-key m "a" 'foo) (define-key p "b" 'bar) (set-keymap-parent m p) (map-keymap-internal #'ignore m))"#,
        r#"=(keymap (98 . bar))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "b" 'foo) (define-key m "a" 'bar) (let (r) (map-keymap (lambda (k v) (push (cons k v) r)) m t) r))"#,
        r#"=((98 . foo) (97 . bar))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "b" 'foo) (define-key m "a" 'bar) (let (r) (map-keymap-sorted (lambda (k v) (push (cons k v) r)) m) r))"#,
        r#"=((98 . foo) (97 . bar))"#,
    );
    check(
        r#"(map-keymap #'ignore nil)"#,
        r#"!(wrong-type-argument keymapp nil)"#,
    );
    check(
        r#"(map-keymap #'ignore 1)"#,
        r#"!(wrong-type-argument keymapp 1)"#,
    );
    check(
        r#"(map-keymap #'ignore '(foo))"#,
        r#"!(wrong-type-argument keymapp (foo))"#,
    );
    check(
        r#"(map-keymap 'nofn '(keymap (97 . foo)))"#,
        r#"!(void-function nofn)"#,
    );
    check(r#"(map-keymap #'ignore '(keymap (97 . foo)))"#, r#"=nil"#);
    check(
        r#"(let (r) (map-keymap (lambda (k v) (push (list k v) r)) '(keymap "menu" (97 . foo) (t . dflt) [x y z] (keymap (98 . sub)))) r)"#,
        r#"=((98 sub) (2 z) (1 y) (0 x) (t dflt) (97 foo))"#,
    );
    check(
        r#"(let (r) (map-keymap (lambda (k v) (push (list k v) r)) '(keymap (97 . t) (98))) r)"#,
        r#"=((98 nil) (97 nil))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "\C-xa" 'foo) (let (r) (map-keymap (lambda (k v) (push (cons k v) r)) m) r))"#,
        r#"=((24 keymap (97 . foo)))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m [f1] 'foo) (define-key m [?\M-a] 'bar) (let (r) (map-keymap (lambda (k v) (push (cons k v) r)) m) r))"#,
        r#"=((f1 . foo) (27 keymap (97 . bar)))"#,
    );
    check(
        r#"(copy-keymap '(keymap (97 . foo) (keymap (98 . bar))))"#,
        r#"=(keymap (97 . foo) (keymap (98 . bar)))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (eq m (copy-keymap m)))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" (make-sparse-keymap)) (define-key m "ab" 'x) (let ((c (copy-keymap m))) (define-key c "ab" 'y) (list (lookup-key m "ab") (lookup-key c "ab"))))"#,
        r#"=(x y)"#,
    );
    check(
        r#"(keymap-canonicalize '(keymap (97 . foo) (98 . bar) (97 . baz)))"#,
        r#"=(keymap (97 . foo) (98 . bar))"#,
    );
    check(r#"(key-description [?\C-x ?a])"#, r#"="C-x a""#);
    check(r#"(key-description "\C-xa")"#, r#"="C-x a""#);
    check(
        r#"(key-description [f1 (control f2)])"#,
        r#"="<f1> C-<f2>""#,
    );
    check(r#"(single-key-description ?\C-a)"#, r#"="C-a""#);
    check(r#"(single-key-description 'f1 t)"#, r#"="f1""#);
    check(r#"(keymap-prompt '(keymap "Hi" (97 . foo)))"#, r#"="Hi""#);
    check(
        r#"(keymap-prompt (make-sparse-keymap "Menu"))"#,
        r#"="Menu""#,
    );
    check(r#"(keymap-parent (make-sparse-keymap))"#, r#"=nil"#);
    check(r#"(kbd "C-x a")"#, r#"="a""#);
    check(r#"(kbd "<f1> a")"#, r#"=[f1 97]"#);
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m [remap kill-line] 'foo) (command-remapping 'kill-line nil m))"#,
        r#"=foo"#,
    );
    check(
        r#"(keymap-lookup (let ((m (make-sparse-keymap))) (keymap-set m "C-c a" 'foo) m) "C-c a")"#,
        r#"=foo"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (keymap-set m "C-c a" 'foo) m)"#,
        r#"=(keymap (3 keymap (97 . foo)))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (define-key m "a" nil t) m)"#,
        r#"=(keymap)"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (define-key m "a" nil) m)"#,
        r#"=(keymap (97))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "ab" 'foo) (define-key m "a" 'bar))"#,
        r#"=bar"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (define-key m "a" 'foo) (lookup-key m "ab"))"#,
        r#"=1"#,
    );
    check(r#"(lookup-key '(keymap) [])"#, r#"=(keymap)"#);
    check(r#"(lookup-key '(keymap (97 . foo)) "a b")"#, r#"=1"#);
    check(r#"(lookup-key '(keymap (97 . foo)) [t])"#, r#"=nil"#);
    check(r#"(lookup-key '(keymap (t . foo)) "a" t)"#, r#"=foo"#);
    check(r#"(listify-key-sequence "\C-xa")"#, r#"=(24 97)"#);
    check(r#"(listify-key-sequence [?\M-a])"#, r#"=(134217825)"#);
    check(r#"(event-modifiers ?\C-a)"#, r#"=(control)"#);
    check(r#"(event-basic-type ?\M-a)"#, r#"=97"#);
    check(r#"(event-convert-list '(control ?a))"#, r#"=1"#);
    check(r#"(event-convert-list '(meta control f1))"#, r#"=C-M-f1"#);
    check(r#"(event-apply-modifier ?a 'control 26 "C-")"#, r#"=1"#);
    check(r#"(text-char-description ?\C-a)"#, r#"="^A""#);
    check(
        r#"(make-composed-keymap (list (make-sparse-keymap)))"#,
        r#"=(keymap (keymap))"#,
    );
    check(r#"(define-keymap "a" 'foo)"#, r#"=(keymap (97 . foo))"#);
    check(
        r#"(let ((m (define-keymap "C-a" 'foo "b" 'bar))) (let (r) (map-keymap (lambda (k v) (push k r)) m) r))"#,
        r#"=(1 98)"#,
    );
    check(r#"(key-description "\M-x")"#, r#"="M-x""#);
    check(r#"(key-description "\C-x\C-f")"#, r#"="C-x C-f""#);
    check(r#"(key-description [?\M-x ?\C-a])"#, r#"="M-x C-a""#);
    check(r#"(key-description [27 27])"#, r#"="ESC ESC""#);
    check(r#"(key-description [27])"#, r#"="ESC""#);
    check(r#"(key-description "abc" "\C-x")"#, r#"="C-x a b c""#);
    check(r#"(key-description [1 2] [3])"#, r#"="C-c C-a C-b""#);
    check(
        r#"(key-description 1)"#,
        r#"!(wrong-type-argument sequencep 1)"#,
    );
    check(r#"(key-description '(?a ?b))"#, r#"="a b""#);
    check(r#"(single-key-description ?\C-x)"#, r#"="C-x""#);
    check(r#"(single-key-description ?\M-\C-x)"#, r#"="C-M-x""#);
    check(r#"(single-key-description ?\C-\M-\S-x)"#, r#"="C-M-S-x""#);
    check(
        r#"(single-key-description 'C-M-return)"#,
        r#"="C-M-<return>""#,
    );
    check(r#"(single-key-description '(97 . 100))"#, r#"="a..d""#);
    check(r#"(single-key-description "abc")"#, r#"="abc""#);
    check(
        r#"(single-key-description 1.5)"#,
        r#"!(error "KEY must be an integer, cons, symbol, or string")"#,
    );
    check(r#"(single-key-description '(control ?a))"#, r#"="C-a""#);
    check(r#"(single-key-description '(control f1))"#, r#"="C-<f1>""#);
    check(
        r#"(single-key-description 'down-mouse-1)"#,
        r#"="<down-mouse-1>""#,
    );
    check(r#"(single-key-description 'M-)"#, r#"="<M->""#);
    check(r#"(single-key-description 'a-b)"#, r#"="<a-b>""#);
    check(r#"(single-key-description ?\^@)"#, r#"="C-@""#);
    check(r#"(single-key-description ?é)"#, r#"="é""#);
    check(r#"(single-key-description 4194304)"#, r#"="A-C-@""#);
    check(r#"(single-key-description (+ 4194304 ?a))"#, r#"="A-a""#);
    check(
        r#"(single-key-description (logior ?\H-\s-\A-a))"#,
        r#"="A-H-s-a""#,
    );
    check(
        r#"(event-convert-list '(control shift ?a))"#,
        r#"=33554433"#,
    );
    check(r#"(event-convert-list '(shift ?a))"#, r#"=65"#);
    check(r#"(event-convert-list '(meta ?\C-a))"#, r#"=134217729"#);
    check(r#"(event-convert-list '(control ?@))"#, r#"=0"#);
    check(r#"(event-convert-list '(control ?\s))"#, r#"=67108896"#);
    check(r#"(event-convert-list '(control ?1))"#, r#"=67108913"#);
    check(
        r#"(event-convert-list '(down mouse-1))"#,
        r#"=down-mouse-1"#,
    );
    check(
        r#"(event-convert-list '(double drag mouse-1))"#,
        r#"=double-drag-mouse-1"#,
    );
    check(r#"(event-convert-list '(a))"#, r#"=97"#);
    check(r#"(event-convert-list nil)"#, r#"=nil"#);
    check(r#"(event-convert-list '(control meta))"#, r#"=C-meta"#);
    check(
        r#"(event-convert-list '(f1 f2))"#,
        r#"!(error "Two bases given in one event")"#,
    );
    check(r#"(event-convert-list 1)"#, r#"=nil"#);
    check(r#"(event-modifiers 'C-M-f1)"#, r#"=(meta control)"#);
    check(r#"(event-modifiers 'down-mouse-1)"#, r#"=(down)"#);
    check(r#"(event-modifiers 'mouse-1)"#, r#"=(click)"#);
    check(r#"(event-modifiers 'wheel-up)"#, r#"=(click)"#);
    check(r#"(event-modifiers ?\M-\C-a)"#, r#"=(control meta)"#);
    check(r#"(event-modifiers ?A)"#, r#"=(shift)"#);
    check(r#"(event-modifiers '(mouse-1 foo))"#, r#"=(click)"#);
    check(r#"(event-modifiers "a")"#, r#"=nil"#);
    check(
        r#"(let ((s (make-symbol "S-C-f2"))) (event-modifiers s))"#,
        r#"=(control shift)"#,
    );
    check(
        r#"(internal-event-symbol-parse-modifiers 'C-M-f1)"#,
        r#"=(f1 meta control)"#,
    );
    check(r#"(internal-event-symbol-parse-modifiers 'f1)"#, r#"=(f1)"#);
    check(
        r#"(internal-event-symbol-parse-modifiers 'double-mouse-2)"#,
        r#"=(mouse-2 double)"#,
    );
    check(
        r#"(internal-event-symbol-parse-modifiers 'up-)"#,
        r#"=(## up)"#,
    );
    check(
        r#"(internal-event-symbol-parse-modifiers 1)"#,
        r#"!(wrong-type-argument symbolp 1)"#,
    );
    check(r#"(event-basic-type 'f1)"#, r#"=f1"#);
    check(
        r#"(progn (event-modifiers 'M-f1) (event-basic-type 'M-f1))"#,
        r#"=f1"#,
    );
    check(r#"(event-basic-type ?\M-A)"#, r#"=97"#);
    check(r#"(event-basic-type ?\C-a)"#, r#"=97"#);
    check(r#"(listify-key-sequence [1 2])"#, r#"=(1 2)"#);
    check(r#"(take-while #'cl-evenp '(2 4 5 6))"#, r#"=(2 4)"#);
    check(r#"(drop-while #'cl-evenp '(2 4 5 6))"#, r#"=(5 6)"#);
    check(r#"(all #'cl-evenp '(2 4))"#, r#"=t"#);
    check(r#"(any #'cl-oddp '(2 4 5))"#, r#"=(5)"#);
    check(r#"(member-if #'cl-oddp '(2 4 5 6))"#, r#"=(5 6)"#);
    check(
        r#"(delete-consecutive-dups (list 1 1 2 2 1))"#,
        r#"=(1 2 1)"#,
    );
    check(
        r#"(delete-consecutive-dups (list 1 1 2 2 1) t)"#,
        r#"=(1 2)"#,
    );
    check(r#"(drop 2 '(1 2 3 4))"#, r#"=(3 4)"#);
    check(r#"(ensure-proper-list '(1 . 2))"#, r#"=((1 . 2))"#);
    check(r#"(list-of-strings-p '("a" "b"))"#, r#"=t"#);
    check(r#"(integer-or-null-p 3.0)"#, r#"=nil"#);
    check(r#"(log10 1000)"#, r#"=3.0"#);
    check(
        r#"(error-type-p 'wrong-type-argument)"#,
        r#"=(wrong-type-argument error)"#,
    );
    check(
        r#"(error-has-type-p '(wrong-type-argument x) 'error)"#,
        r#"=(error)"#,
    );
    check(r#"(error-slot-value '(a b c) 2)"#, r#"=c"#);
    check(r#"(static-if t 1 2)"#, r#"=1"#);
    check(r#"(static-when nil 1)"#, r#"=nil"#);
    check(r#"(static-unless nil 1 2)"#, r#"=2"#);
    check(
        r#"(copy-keymap '(keymap (97 . foo) "str" [x y (keymap (1 . z))] (98 menu-item "Name" (keymap (3 . q)) :enable t) (99 "old" (keymap (4 . w)))))"#,
        r#"=(keymap (97 . foo) "str" [x y (keymap (1 . z))] (98 menu-item "Name" (keymap (3 . q)) :enable t) (99 "old" (keymap (4 . w))))"#,
    );
    check(
        r#"(copy-keymap 'foo)"#,
        r#"!(wrong-type-argument keymapp foo)"#,
    );
    check(
        r#"(keymap-prompt '(keymap (keymap "inner") "outer"))"#,
        r#"="inner""#,
    );
    check(
        r#"(map-keymap #'ignore '(keymap (keymap (1 . 2)) (3 . 4) keymap (5 . 6)))"#,
        r#"=nil"#,
    );
    check(
        r#"(let (r) (map-keymap (lambda (k v) (push (list k v) r)) '(keymap (keymap (1 . 2)) (3 . 4) keymap (5 . 6))) r)"#,
        r#"=((5 6) (3 4) (1 2))"#,
    );
    check(
        r#"(let (r) (map-keymap (lambda (k v) (push (list k v) r)) '(keymap (3 . 4) . zzz)) r)"#,
        r#"=((3 4))"#,
    );
    check(
        r#"(let ((m (make-sparse-keymap))) (fset 'my-km '(keymap (1 . a))) (let (r) (map-keymap (lambda (k v) (push (list k v) r)) '(keymap (2 . b) . my-km)) r))"#,
        r#"=((1 a) (2 b))"#,
    );
}
