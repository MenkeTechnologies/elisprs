//! End-to-end tests: elisp source is lowered to a fusevm chunk and executed on
//! fusevm (no bespoke interpreter). Each test resets the thread-local host.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn aot_chunk_is_self_contained() {
    use elisprs::{compiler, host, reader};
    // A program that uses the prelude (mapcar), a user defun, and a closure.
    let src = "(defun sq (x) (* x x)) (mapcar #'sq (list 1 2 3 4))";

    // Compile in a host with the prelude loaded, then capture the heap image
    // exactly as `--aot` embeds it.
    reset_host();
    let _ = eval_str(""); // load prelude
    let chunk = host::with_host(|h| {
        let forms = reader::read_all(h, src).unwrap();
        compiler::compile_program(h, &forms).unwrap()
    });
    let image = host::with_host(|h| h.export_heap_image());

    // Simulate AOT load: a FRESH host (builtins only — no prelude, no user heap),
    // rebuild the heap from the image, then run the chunk on fusevm. If the chunk
    // is self-contained, every Value::Obj handle resolves and the result matches.
    reset_host();
    host::with_host(|h| h.import_heap_image(image));
    let result = host::run_chunk(chunk).expect("aot run failed");
    assert_eq!(print(&result, true), "(1 4 9 16)");
}

#[test]
fn arithmetic() {
    assert_eq!(eval("(+ 1 2 3)"), "6");
    assert_eq!(eval("(* 2 (+ 3 4))"), "14");
    assert_eq!(eval("(/ 7.0 2)"), "3.5");
    assert_eq!(eval("(- 5)"), "-5");
    assert_eq!(eval("(1+ 41)"), "42");
}

#[test]
fn cons_cells_are_real_heap_objects() {
    // The whole point of moving off rust_lisp: dotted pairs + identity + mutation.
    assert_eq!(eval("(cons 1 2)"), "(1 . 2)");
    assert_eq!(eval("(quote (a b . c))"), "(a b . c)");
    assert_eq!(
        eval("(progn (setq p (cons 1 2)) (setcar p 9) p)"),
        "(9 . 2)"
    );
    assert_eq!(eval("(car (cons 1 2))"), "1");
    assert_eq!(eval("(cdr (cons 1 2))"), "2");
}

#[test]
fn lists() {
    assert_eq!(eval("(list 1 2 3)"), "(1 2 3)");
    assert_eq!(eval("(reverse (list 1 2 3))"), "(3 2 1)");
    assert_eq!(eval("(length (list 1 2 3 4))"), "4");
    assert_eq!(eval("(append (list 1 2) (list 3 4))"), "(1 2 3 4)");
    assert_eq!(eval("(nth 2 (list 10 20 30 40))"), "30");
}

#[test]
fn special_forms() {
    assert_eq!(eval("(if (eq 1 1) (quote y) (quote n))"), "y");
    assert_eq!(eval("(if nil 1 2)"), "2");
    assert_eq!(eval("(and 1 2 3)"), "3");
    assert_eq!(eval("(or nil nil 7)"), "7");
    assert_eq!(eval("(when t 1 2 3)"), "3");
    assert_eq!(eval("(unless nil 5)"), "5");
    assert_eq!(eval("(progn (setq x 10) (+ x 5))"), "15");
}

#[test]
fn equality_identity_vs_structural() {
    assert_eq!(eval("(eq (quote a) (quote a))"), "t");
    assert_eq!(eval("(equal (list 1 2) (list 1 2))"), "t");
    assert_eq!(eval("(eq (list 1 2) (list 1 2))"), "nil");
}

#[test]
fn predicates() {
    assert_eq!(
        eval(
            "(list (consp (cons 1 2)) (consp nil) (symbolp (quote x)) (stringp \"s\") (null nil))"
        ),
        "(t nil t t t)"
    );
    assert_eq!(eval("(integerp 5)"), "t");
    assert_eq!(eval("(floatp 5.0)"), "t");
}

#[test]
fn vectors() {
    assert_eq!(eval("(aref (vector 10 20 30) 1)"), "20");
    assert_eq!(
        eval("(progn (setq v (make-vector 3 0)) (aset v 1 9) v)"),
        "[0 9 0]"
    );
    assert_eq!(eval("(length (vector 1 2 3))"), "3");
}

#[test]
fn strings_and_format() {
    assert_eq!(
        eval("(format \"%s=%d hex=%x\" (quote n) 255 255)"),
        "\"n=255 hex=ff\""
    );
    assert_eq!(eval("(concat \"foo\" \"bar\")"), "\"foobar\"");
}

#[test]
fn functions_and_recursion() {
    assert_eq!(
        eval("(progn (defun fact (n) (if (<= n 1) 1 (* n (fact (1- n))))) (fact 6))"),
        "720"
    );
    assert_eq!(
        eval("(progn (defun add3 (a b c) (+ a b c)) (add3 10 20 30))"),
        "60"
    );
    assert_eq!(eval("(funcall (lambda (x) (1+ x)) 41)"), "42");
}

#[test]
fn let_binding() {
    assert_eq!(eval("(let ((x 10) (y 20)) (+ x y))"), "30");
    assert_eq!(eval("(let* ((x 2) (y (* x 3))) (+ x y))"), "8");
    assert_eq!(eval("(let ((x 1)) (let ((x 2)) x))"), "2"); // shadowing
}

#[test]
fn iteration_and_cond() {
    assert_eq!(
        eval("(let ((acc 0) (i 1)) (while (<= i 5) (setq acc (+ acc i)) (setq i (1+ i))) acc)"),
        "15"
    );
    assert_eq!(
        eval("(cond ((eq 1 2) (quote a)) ((eq 1 1) (quote b)) (t (quote c)))"),
        "b"
    );
    assert_eq!(eval("(cond (nil 1) (42))"), "42"); // clause with no body returns the test
}

#[test]
fn higher_order_reentrancy() {
    // mapcar over a lambda, and a user-defined recursive higher-order function —
    // both re-enter elisp from a closure body running on a nested fusevm VM.
    assert_eq!(
        eval("(mapcar (lambda (n) (* n n)) (list 1 2 3 4))"),
        "(1 4 9 16)"
    );
    assert_eq!(
        eval("(progn (defun my-map (f xs) (if (null xs) nil (cons (funcall f (car xs)) (my-map f (cdr xs))))) (my-map (lambda (n) (1+ n)) (list 10 20 30)))"),
        "(11 21 31)"
    );
}

#[test]
fn backquote_and_user_macros() {
    assert_eq!(eval("(let ((x 5)) `(a ,x c))"), "(a 5 c)");
    assert_eq!(eval("(let ((xs (list 2 3))) `(1 ,@xs 4))"), "(1 2 3 4)");
    // defmacro must be a prior top-level form (as in real elisp loading).
    assert_eq!(
        eval("(defmacro my-when (c &rest body) `(if ,c (progn ,@body) nil)) (my-when t 1 2 3)"),
        "3"
    );
    // a macro that generates a defun
    assert_eq!(
        eval("(defmacro defk (n k) `(defun ,n () ,k)) (defk answer 42) (answer)"),
        "42"
    );
}

#[test]
fn prelude_derived_surface() {
    assert_eq!(eval("(cadr (list 1 2 3))"), "2");
    assert_eq!(eval("(nthcdr 2 (list 10 20 30 40))"), "(30 40)");
    assert_eq!(
        eval("(seq-filter (lambda (x) (> x 2)) (list 1 2 3 4 5))"),
        "(3 4 5)"
    );
    assert_eq!(
        eval("(seq-reduce (lambda (a b) (+ a b)) (list 1 2 3 4) 0)"),
        "10"
    );
    assert_eq!(
        eval("(mapconcat (lambda (x) (number-to-string x)) (list 1 2 3) \"-\")"),
        "\"1-2-3\""
    );
    assert_eq!(
        eval("(let ((s 0)) (dolist (x (list 1 2 3 4)) (setq s (+ s x))) s)"),
        "10"
    );
    assert_eq!(
        eval("(let ((s 0)) (dotimes (i 5) (setq s (+ s i))) s)"),
        "10"
    );
    assert_eq!(eval("(let ((l (list 1 2))) (push 0 l) l)"), "(0 1 2)");
    assert_eq!(eval("(max 3 7 2 9 1)"), "9");
    assert_eq!(eval("(abs -5)"), "5");
    assert_eq!(eval("(delete-dups (list 1 2 1 3 2 4))"), "(1 2 3 4)");
}

#[test]
fn lexical_and_dynamic_binding() {
    // Lexical closures capture their defining environment (modern-elisp default).
    assert_eq!(
        eval("(defun make-adder (n) (lambda (x) (+ x n))) (funcall (make-adder 10) 5)"),
        "15"
    );
    // Each closure captures its own binding.
    assert_eq!(
        eval("(list (funcall (let ((i 1)) (lambda () i))) (funcall (let ((i 2)) (lambda () i))))"),
        "(1 2)"
    );
    // Capture is by reference: setq through the closure persists.
    assert_eq!(
        eval("(let ((c 0)) (defun bump () (setq c (1+ c))) (bump) (bump) (bump) c)"),
        "3"
    );
    // A defvar'd (special) variable is dynamically scoped: the callee sees the let.
    assert_eq!(
        eval("(defvar *dyn* 1) (defun rd () *dyn*) (let ((*dyn* 42)) (rd))"),
        "42"
    );
}

#[test]
fn hash_tables() {
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'equal))) (puthash \"a\" 1 h) (gethash \"a\" h))"),
        "1"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'k 42 h) (puthash 'k 99 h) (gethash 'k h))"),
        "99"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table)) (s 0)) (puthash 1 10 h) (puthash 2 20 h) (maphash (lambda (k v) (setq s (+ s v))) h) s)"),
        "30"
    );
    assert_eq!(
        eval("(gethash 'missing (make-hash-table) 'default)"),
        "default"
    );
}

#[test]
fn string_ops() {
    assert_eq!(eval("(substring \"hello world\" 0 5)"), "\"hello\"");
    assert_eq!(eval("(substring \"hello\" -3)"), "\"llo\"");
    assert_eq!(eval("(split-string \"a b  c\")"), "(\"a\" \"b\" \"c\")");
    assert_eq!(
        eval("(split-string \"a,b,c\" \",\")"),
        "(\"a\" \"b\" \"c\")"
    );
    assert_eq!(eval("(string-prefix-p \"foo\" \"foobar\")"), "t");
    assert_eq!(
        eval("(string-join (list \"a\" \"b\" \"c\") \"-\")"),
        "\"a-b-c\""
    );
    assert_eq!(eval("(make-string 3 65)"), "\"AAA\"");
    assert_eq!(eval("(string-search \"lo\" \"hello\")"), "3");
}

#[test]
fn nonlocal_exits() {
    assert_eq!(eval("(catch 'done (throw 'done 42))"), "42");
    assert_eq!(eval("(catch 'x (+ 1 (throw 'x 99)) 999)"), "99"); // throw aborts mid-expr
    assert_eq!(
        eval("(catch 'outer (catch 'inner (throw 'outer 7)) 8)"),
        "7"
    );
    assert_eq!(
        eval("(condition-case nil (/ 1 0) (error 'caught))"),
        "caught"
    );
    assert_eq!(eval("(condition-case nil (+ 1 2) (error 'no))"), "3");
    assert_eq!(eval("(ignore-errors (/ 1 0))"), "nil");
    assert_eq!(eval("(ignore-errors 123)"), "123");
    // unwind-protect cleanup runs even when the body throws
    assert_eq!(
        eval("(let ((log nil)) (catch 'x (unwind-protect (throw 'x 1) (setq log 'cleaned))) log)"),
        "cleaned"
    );
    // catch+throw as a loop break
    assert_eq!(
        eval("(catch 'x (dotimes (i 100) (if (= i 5) (throw 'x i))))"),
        "5"
    );
}
