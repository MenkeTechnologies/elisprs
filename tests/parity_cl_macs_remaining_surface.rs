//! The cl-macs.el / cl-extra.el surface that was void, and the gensym naming
//! rules.
//!
//! Expectations are GNU Emacs 31.1's (`emacs -Q --batch`, `(require 'cl-lib)`).

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn gensym_names_follow_subr_el_and_cl_macs_el() {
    // The preloaded Lisp's own expansion does not consume the counter.
    assert_eq!(eval("(symbol-name (gensym))"), "\"g0\"");
    assert_eq!(eval("(symbol-name (gensym 'foo))"), "\"foo0\"");
    assert_eq!(eval("(symbol-name (cl-gensym 5))"), "\"G5\"");
    assert_eq!(eval("(symbol-name (cl-gensym \"X\"))"), "\"X0\"");
    assert_eq!(eval("(symbol-name (cl-gensym 'foo))"), "\"G0\"");
    assert_eq!(
        eval("(list (symbol-name (cl-gentemp)) (intern-soft \"T0\") (symbol-name (cl-gentemp \"Z\")))"),
        "(\"T0\" T0 \"Z0\")"
    );
}

#[test]
fn macro_writing_helpers() {
    assert_eq!(
        eval("(macroexpand '(cl-with-gensyms (a) (list a)))"),
        "(let ((a (gensym (symbol-name 'a)))) (list a))"
    );
    // The expansion consumes g0 for its own name; the expanded code's
    // `(gensym)' then yields g1.
    assert_eq!(
        eval("(cl-once-only ((x '(+ 1 2))) `(list ,x ,x))"),
        "(let ((g1 (+ 1 2))) (list g1 g1))"
    );
    assert_eq!(
        eval(
            "(progn (defmacro my-dbl (x) (cl-once-only (x) `(+ ,x ,x))) \
              (let ((n 0)) (list (my-dbl (setq n (1+ n))) n)))"
        ),
        "(2 1)"
    );
}

#[test]
fn tagbody_and_prog() {
    assert_eq!(
        eval("(let ((n 0)) (cl-tagbody (setq n (1+ n)) top (when (< n 5) (setq n (1+ n)) (go top))) n)"),
        "5"
    );
    assert_eq!(
        eval("(cl-prog ((x 1) (y 2)) (setq x (+ x y)) (cl-return x))"),
        "3"
    );
    assert_eq!(
        eval("(cl-prog* ((x 1) (y x)) (cl-return (list x y)))"),
        "(1 1)"
    );
    assert_eq!(
        eval("(condition-case e (macroexpand-all '(cl-tagbody a (go b))) (error e))"),
        "(error \"Unknown cl-tagbody go label ‘b’\")"
    );
}

#[test]
fn compiler_macros_and_declarations() {
    assert_eq!(
        eval("(progn (cl-define-compiler-macro my-sq2 (x) `(* ,x ,x)) (defun my-sq2 (x) (* x x)) \
              (list (my-sq2 3) (cl-compiler-macroexpand '(my-sq2 4)) (get 'my-sq2 'compiler-macro)))"),
        "(9 (* 4 4) my-sq2--cmacro)"
    );
    assert_eq!(
        eval("(progn (cl-defsubst my-sq (x) (* x x)) (my-sq 7))"),
        "49"
    );
    assert_eq!(eval("(cl-load-time-value (+ 1 2))"), "3");
    assert_eq!(eval("(cl-declare (special x))"), "nil");
}

#[test]
fn symbols_iteration() {
    assert_eq!(
        eval("(let (r) (cl-do-symbols (s (let ((o (obarray-make))) (intern \"aa\" o) (intern \"bb\" o) o)) \
              (push (symbol-name s) r)) (sort r #'string<))"),
        "(\"aa\" \"bb\")"
    );
}

#[test]
fn random_states_reproduce_cl_extra_sequences() {
    assert_eq!(
        eval(
            "(let ((s (cl-make-random-state 42))) (list (cl-random 100 s) (cl-random 100 s) \
              (cl-random 1000000 s) (cl-random 100000000000 s)))"
        ),
        "(51 82 444917 4049331109)"
    );
    assert_eq!(
        eval("(let ((s (cl-make-random-state 42))) (cl-random 1.0 s))"),
        "0.4981578588485718"
    );
    assert_eq!(
        eval("(list (cl-random-state-p (cl-make-random-state 3)) (cl-random-state-p 3))"),
        "(t nil)"
    );
}

#[test]
fn float_limits_and_misc() {
    assert_eq!(eval("cl-most-positive-float"), "nil");
    assert_eq!(
        eval("(progn (cl-float-limits) (list cl-float-epsilon cl-most-positive-float \
              cl-least-positive-normalized-float cl-float-negative-epsilon))"),
        "(2.220446049250313e-16 1.7976931348623157e+308 2.2250738585072014e-308 1.1102230246251565e-16)"
    );
    assert_eq!(eval("(cl-floatp-safe 1.0)"), "t");
    assert_eq!(
        eval(
            "(let ((h (make-hash-table))) (puthash 1 nil h) \
              (list (hash-table-contains-p 1 h) (hash-table-contains-p 2 h)))"
        ),
        "(t nil)"
    );
    assert_eq!(
        eval("(with-output-to-string (princ \"a\") (cl-fresh-line) (cl-fresh-line) (princ \"b\"))"),
        "\"a\nb\""
    );
}

#[test]
fn with_accessors_binds_places_over_one_evaluation() {
    // cl-macs.el: each NAME is a symbol macro for (ACCESSOR INSTANCE), so setf,
    // setq and cl-incf write through; INSTANCE is evaluated once (cl-once-only).
    assert_eq!(
        eval(
            "(progn (cl-defstruct pt x y) \
              (let ((p (make-pt :x 1 :y 2))) \
                (cl-with-accessors ((a pt-x) (b pt-y)) p \
                  (setf a 10) (setq b (+ a b)) (list a b p))))"
        ),
        "(10 12 #s(pt 10 12))"
    );
    assert_eq!(
        eval(
            "(let ((c (list 1 2)) (n 0)) \
              (cl-with-accessors ((h car) (tl cdr)) (progn (cl-incf n) c) \
                (cl-incf h) (list h tl c n)))"
        ),
        "(2 (2) (2 2) 1)"
    );
    // An empty body is nil; no bindings is just the body.
    assert_eq!(eval("(cl-with-accessors ((h car)) '(7))"), "nil");
    assert_eq!(eval("(cl-with-accessors () '(7) 3 4)"), "4");
    assert_eq!(
        eval("(condition-case e (macroexpand '(cl-with-accessors ((1 car)) x y)) (error e))"),
        "(error \"Malformed ‘cl-with-accessors’ binding: (1 car)\")"
    );
}

#[test]
fn struct_introspection_follows_the_struct_type() {
    let defs = "(cl-defstruct a1 x (y 3 :read-only t)) \
                (cl-defstruct (a2 (:type list)) x) \
                (cl-defstruct (a3 (:type vector) :named) x) \
                (cl-defstruct (a4 (:include a1)) z) ";
    // cl-struct-sequence-type was void.
    assert_eq!(
        eval(&format!(
            "(progn {defs}(mapcar #'cl-struct-sequence-type '(a1 a2 a3 a4)))"
        )),
        "(nil list vector nil)"
    );
    // Only a record gets the bare (cl-tag-slot) entry; a :named typed struct
    // carries the tag as a real slot; an unnamed one has none.
    assert_eq!(
        eval(&format!(
            "(progn {defs}(mapcar #'cl-struct-slot-info '(a1 a2 a3 a4)))"
        )),
        "(((cl-tag-slot) (x nil) (y 3 :read-only t)) ((x nil)) ((cl-tag-slot nil) (x nil)) \
         ((cl-tag-slot) (x nil) (y 3 :read-only t) (z nil)))"
    );
    assert_eq!(
        eval(&format!(
            "(progn {defs}(list (cl-struct-slot-offset 'a2 'x) (cl-struct-slot-offset 'a3 'x) \
             (cl-struct-slot-offset 'a4 'z)))"
        )),
        "(0 1 3)"
    );
    assert_eq!(
        eval(&format!(
            "(progn {defs}(condition-case e (cl-struct-slot-offset 'a1 'q) (error e)))"
        )),
        "(cl-struct-unknown-slot a1 q)"
    );
    assert_eq!(
        eval("(condition-case e (cl-struct-slot-info 'nosuch) (error e))"),
        "(error \"nosuch is not a struct name\")"
    );
}
