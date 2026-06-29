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

#[test]
fn regexp_search_and_match_data() {
    // string-match returns the char index of the match and records match data.
    assert_eq!(eval("(string-match \"world\" \"hello world\")"), "6");
    assert_eq!(eval("(string-match \"zzz\" \"hello\")"), "nil");
    // Capture groups: match-string / match-beginning / match-end read group N.
    assert_eq!(
        eval(
            "(progn (string-match \"\\\\([a-z]+\\\\)-\\\\([0-9]+\\\\)\" \"  abc-123 \") \
             (list (match-string 1 \"  abc-123 \") (match-string 2 \"  abc-123 \") \
             (match-beginning 1) (match-end 2)))"
        ),
        "(\"abc\" \"123\" 2 9)"
    );
    // elisp counts characters, not bytes: positions are correct across UTF-8.
    assert_eq!(
        eval("(progn (string-match \"é\" \"aébé\") (list (match-beginning 0) (match-end 0)))"),
        "(1 2)"
    );
    // The optional START argument bounds where searching begins.
    assert_eq!(eval("(string-match \"a\" \"banana\" 2)"), "3");
    // Backreferences in the pattern have no analogue in a non-backtracking engine.
    reset_host();
    assert!(eval_str("(string-match \"\\\\(a\\\\)\\\\1\" \"aa\")").is_err());
}

#[test]
fn regexp_replace_and_quote() {
    // Template replacement expands \& (whole match) and \N (group N) backrefs.
    assert_eq!(
        eval("(replace-regexp-in-string \"\\\\([a-z]+\\\\)=\\\\([0-9]+\\\\)\" \"\\\\2:\\\\1\" \"x=1 yy=22\")"),
        "\"1:x 22:yy\""
    );
    // LITERAL non-nil inserts REP verbatim.
    assert_eq!(
        eval("(replace-regexp-in-string \"[0-9]+\" \"#\" \"a1b22c333\" nil t)"),
        "\"a#b#c#\""
    );
    // regexp-quote escapes the regexp metacharacters so the text matches literally.
    assert_eq!(eval("(regexp-quote \"a.b*c\")"), "\"a\\\\.b\\\\*c\"");
}

#[test]
fn regexp_save_match_data() {
    // save-match-data shields the caller's match state from an inner search.
    assert_eq!(
        eval("(progn (string-match \"b\" \"abc\") (save-match-data (string-match \"x\" \"xyz\")) (match-beginning 0))"),
        "1"
    );
    // match-data round-trips through set-match-data.
    assert_eq!(
        eval(
            "(progn (string-match \"\\\\([a-z]+\\\\)-\\\\([0-9]+\\\\)\" \"abc-123\") (match-data))"
        ),
        "(0 7 0 3 4 7)"
    );
    // string-match-p leaves existing match data untouched.
    assert_eq!(
        eval("(progn (string-match \"b\" \"abc\") (string-match-p \"x\" \"xyz\") (match-beginning 0))"),
        "1"
    );
}

#[test]
fn pcase_non_backquote_subset() {
    // Literal dispatch with a wildcard fallthrough.
    assert_eq!(eval("(pcase 2 (1 'one) (2 'two) (_ 'other))"), "two");
    assert_eq!(eval("(pcase 99 (1 'a) (2 'b))"), "nil"); // no clause matches → nil
                                                         // A bare symbol binds the value; nil/t/keywords stay literals.
    assert_eq!(eval("(pcase 42 (x (list 'got x)))"), "(got 42)");
    assert_eq!(
        eval("(list (pcase :k (:k 'kw)) (pcase nil (nil 'n)) (pcase t (t 'tt)))"),
        "(kw n tt)"
    );
    // 'X / (quote X) match literally.
    assert_eq!(eval("(pcase 'foo ('bar 1) ('foo 2) (_ 3))"), "2");
    // pred: function symbol, and (FN ARGS...) appending the value last.
    assert_eq!(
        eval("(pcase 7 ((pred stringp) 'str) ((pred integerp) 'int))"),
        "int"
    );
    assert_eq!(eval("(pcase 5 ((pred (< 10)) 'big) (_ 'small))"), "small");
    // and binds + guards over the binding; or matches any branch.
    assert_eq!(
        eval("(pcase 8 ((and n (guard (> n 5))) (list 'big n)) (_ 'small))"),
        "(big 8)"
    );
    assert_eq!(eval("(pcase 3 ((or 1 2 3) 'low) (_ 'high))"), "low");
}

#[test]
fn emacs_parity_value_fixes() {
    // eq is object identity: distinct float objects are not eq, fixnums are.
    assert_eq!(eval("(eq 1.0 1.0)"), "nil");
    assert_eq!(eval("(eq 1 1)"), "t");
    assert_eq!(eval("(eql 1.0 1.0)"), "t"); // eql compares floats by value
    assert_eq!(eval("(eql 1 1.0)"), "nil");
    assert_eq!(eval("(equal 1.0 1.0)"), "t");
    // round half to even (banker's rounding).
    assert_eq!(eval("(round 2.5)"), "2");
    assert_eq!(eval("(round 0.5)"), "0");
    assert_eq!(eval("(round -2.5)"), "-2");
    assert_eq!(eval("(round 3.5)"), "4");
    // mod: result takes the divisor's sign; float operands keep float result.
    assert_eq!(eval("(mod 13.5 4)"), "1.5");
    assert_eq!(eval("(mod -1 3)"), "2");
    assert_eq!(eval("(mod 1 -3)"), "-2");
    // split-string honors OMIT-NULLS (3rd arg); default-separators omits implicitly.
    assert_eq!(
        eval("(split-string \"a,b,,c\" \",\" t)"),
        "(\"a\" \"b\" \"c\")"
    );
    assert_eq!(
        eval("(split-string \"a,b,,c\" \",\")"),
        "(\"a\" \"b\" \"\" \"c\")"
    );
    // dotimes/dolist return the RESULT form (with the var bound).
    assert_eq!(eval("(dotimes (i 3 i) i)"), "3");
    assert_eq!(
        eval("(let ((s nil)) (dolist (x '(1 2 3) s) (push x s)))"),
        "(3 2 1)"
    );
    // capitalize upcases every word.
    assert_eq!(eval("(capitalize \"hello world\")"), "\"Hello World\"");
    assert_eq!(eval("(capitalize \"foo-bar baz\")"), "\"Foo-Bar Baz\"");
}

#[test]
fn emacs_parity_seq_substring_nth_butlast() {
    // seq-empty-p works on any sequence, not just lists.
    assert_eq!(eval("(seq-empty-p \"\")"), "t");
    assert_eq!(eval("(seq-empty-p [])"), "t");
    assert_eq!(eval("(seq-empty-p '(1))"), "nil");
    // string-blank-p returns the match position (0), not t.
    assert_eq!(eval("(string-blank-p \"  \")"), "0");
    assert_eq!(eval("(string-blank-p \"x\")"), "nil");
    // butlast with negative/zero N keeps the whole list (no trailing nil).
    assert_eq!(eval("(butlast (list 1 2 3) -1)"), "(1 2 3)");
    assert_eq!(eval("(butlast (list 1 2 3))"), "(1 2)");
    assert_eq!(eval("(butlast (list 1 2 3) 5)"), "nil");
    // substring bounds-checks rather than clamping.
    assert_eq!(eval("(substring \"abc\" 1 3)"), "\"bc\"");
    assert!(eval_str("(substring \"abc\" 0 10)").is_err());
    // nth walks the cons spine: improper lists work, vectors signal listp.
    assert_eq!(eval("(nth 0 '(a . 1))"), "a");
    assert_eq!(eval("(nth -1 '(a b c))"), "a");
    assert_eq!(eval("(nth 5 '(a b c))"), "nil");
    assert!(eval_str("(nth 0 [1 2 3])").is_err());
    // last stops at an improper tail instead of erroring.
    assert_eq!(eval("(last '(1 2 . 3))"), "(2 . 3)");
    assert_eq!(eval("(last '(1 2 3))"), "(3)");
    assert_eq!(eval("(last '(1 2 3) 2)"), "(2 3)");
    assert_eq!(eval("(last nil)"), "nil");
}

#[test]
fn emacs_parity_printer_condcase_setqdefault() {
    // Printer abbreviates the two-element quote/function forms (R4-G).
    assert_eq!(eval("(prin1-to-string '(quote a))"), "\"'a\"");
    assert_eq!(eval("(prin1-to-string '(function f))"), "\"#'f\"");
    assert_eq!(eval("(prin1-to-string '(a (quote b) c))"), "\"(a 'b c)\"");
    // …but not three-element lists headed by quote.
    assert_eq!(eval("(prin1-to-string '(quote a b))"), "\"(quote a b)\"");
    // condition-case :success handler runs on normal return with VAR bound (R4-E).
    assert_eq!(eval("(condition-case x 5 (:success (1+ x)))"), "6");
    assert_eq!(
        eval("(condition-case x (+ 2 3) (:success (* x x)) (error 0))"),
        "25"
    );
    assert_eq!(eval("(condition-case x 7 (error 0))"), "7");
    // setq-default behaves as a global set in this no-buffer-local model (R2-F).
    assert_eq!(eval("(setq-default x 5)"), "5");
    assert_eq!(eval("(progn (setq-default a 1 b 2) (list a b))"), "(1 2)");
}

#[test]
fn emacs_parity_print_length_level() {
    // print-length truncates lists and vectors with `...` (R3-D).
    assert_eq!(
        eval("(let ((print-length 3)) (prin1-to-string '(1 2 3 4 5)))"),
        "\"(1 2 3 ...)\""
    );
    assert_eq!(
        eval("(let ((print-length 3)) (prin1-to-string [1 2 3 4 5]))"),
        "\"[1 2 3 ...]\""
    );
    assert_eq!(
        eval("(let ((print-length 2)) (prin1-to-string '(1 2 3 . 4)))"),
        "\"(1 2 ...)\""
    );
    // print-level replaces over-deep nesting with `...`.
    assert_eq!(
        eval("(let ((print-level 2)) (prin1-to-string '(1 (2 (3 (4))))))"),
        "\"(1 (2 ...))\""
    );
    assert_eq!(
        eval("(let ((print-level 1)) (prin1-to-string '(1 (2))))"),
        "\"(1 ...)\""
    );
    // unset (nil) means no limit.
    assert_eq!(eval("(prin1-to-string '(1 2 3 4 5))"), "\"(1 2 3 4 5)\"");
}

#[test]
fn emacs_parity_symbol_read_print_escapes() {
    // Printer escapes so a symbol reads back unchanged (R3-C); empty => `##`.
    assert_eq!(eval("(prin1-to-string (intern \"\"))"), "\"##\"");
    assert_eq!(eval("(prin1-to-string (intern \"a b\"))"), "\"a\\\\ b\"");
    assert_eq!(eval("(prin1-to-string (intern \"a#b\"))"), "\"a\\\\#b\"");
    assert_eq!(eval("(prin1-to-string (intern \"123\"))"), "\"\\\\123\"");
    assert_eq!(eval("(prin1-to-string (intern \".foo\"))"), "\"\\\\.foo\"");
    // …but ordinary symbols and mid-symbol dots/`+` are untouched.
    assert_eq!(eval("(prin1-to-string (intern \"a.b\"))"), "\"a.b\"");
    assert_eq!(eval("(prin1-to-string (intern \"1+\"))"), "\"1+\"");
    // princ stays raw (no escaping).
    assert_eq!(eval("(symbol-name (intern \"a b\"))"), "\"a b\"");
    // Reader honors `\` escapes (R3-B): builds one symbol, never a number.
    assert_eq!(eval("(symbol-name 'foo\\ bar)"), "\"foo bar\"");
    assert_eq!(eval("(symbol-name '\\,)"), "\",\"");
    assert_eq!(eval("(symbol-name '\\123)"), "\"123\"");
    assert_eq!(eval("(eq 'foo\\ bar (intern \"foo bar\"))"), "t");
    // Round-trip: read . prin1 = identity for awkward names.
    assert_eq!(
        eval("(eq (read (prin1-to-string (intern \"x(y\"))) (intern \"x(y\"))"),
        "t"
    );
}

#[test]
fn emacs_parity_error_data_elements() {
    // wrong-type-argument DATA is (PREDICATE VALUE) as separate elements (R2-B).
    assert_eq!(
        eval("(condition-case e (car 5) (error e))"),
        "(wrong-type-argument listp 5)"
    );
    assert_eq!(eval("(condition-case e (car 5) (error (caddr e)))"), "5");
    assert_eq!(eval("(condition-case e (car 5) (error (cadr e)))"), "listp");
    // The value re-reads even with awkward content (strings with spaces).
    assert_eq!(
        eval("(condition-case e (car \"a b\") (error e))"),
        "(wrong-type-argument listp \"a b\")"
    );
    // args-out-of-range DATA is (ARRAY START END).
    assert_eq!(
        eval("(condition-case e (substring \"abc\" 0 10) (error e))"),
        "(args-out-of-range \"abc\" 0 10)"
    );
    // Plain `error` still carries a message string, and explicit `signal` data
    // is preserved unchanged.
    assert_eq!(
        eval("(condition-case e (error \"plain %d\" 7) (error e))"),
        "(error \"plain 7\")"
    );
    assert_eq!(
        eval("(condition-case e (signal 'arith-error '(1 2)) (error e))"),
        "(arith-error 1 2)"
    );
}

#[test]
fn map_el_generic_api() {
    // map-elt dispatches on type; lists default to an `equal` key test.
    assert_eq!(eval("(map-elt '((a . 1) (b . 2)) 'b)"), "2");
    assert_eq!(eval("(map-elt (list (cons \"a\" 1)) \"a\")"), "1");
    assert_eq!(eval("(map-elt '((a . 1)) 'z 'def)"), "def");
    assert_eq!(eval("(map-elt \"abc\" 1)"), "98");
    assert_eq!(eval("(map-elt [10 20 30] 1)"), "20");
    // queries
    assert_eq!(eval("(map-keys '((a . 1) (b . 2)))"), "(a b)");
    assert_eq!(eval("(map-values '((a . 1) (b . 2)))"), "(1 2)");
    assert_eq!(eval("(map-length [1 2 3])"), "3");
    assert_eq!(eval("(map-empty-p nil)"), "t");
    assert_eq!(eval("(map-contains-key '((a . 1)) 'a)"), "t");
    assert_eq!(eval("(map-nested-elt '((a . ((b . 42)))) '(a b))"), "42");
    // conversion / merge preserve first-seen order
    assert_eq!(
        eval("(map-merge 'list '((a . 1)) '((b . 2)))"),
        "((a . 1) (b . 2))"
    );
    assert_eq!(
        eval("(map-merge-with 'list #'+ '((a . 1) (b . 5)) '((b . 2) (c . 3)))"),
        "((a . 1) (b . 7) (c . 3))"
    );
    // setf (map-elt …) grows an alist at the head and updates in place.
    assert_eq!(
        eval("(let ((m (list))) (setf (map-elt m 'x) 5) m)"),
        "((x . 5))"
    );
    assert_eq!(
        eval("(let ((m '((a . 1)))) (setf (map-elt m 'a) 9) m)"),
        "((a . 9))"
    );
    assert_eq!(eval("(map-delete '((a . 1) (b . 2)) 'a)"), "((b . 2))");
}

#[test]
fn seq_take_drop_while_and_iteration() {
    // seq-take-while / seq-drop-while preserve the input sequence's type.
    assert_eq!(eval("(seq-take-while #'cl-oddp '(1 3 2 5))"), "(1 3)");
    assert_eq!(eval("(seq-take-while #'cl-oddp [1 3 2 5])"), "[1 3]");
    assert_eq!(eval("(seq-drop-while #'cl-oddp '(1 3 2 5))"), "(2 5)");
    assert_eq!(
        eval("(seq-drop-while (lambda (c) (= c ?a)) \"aab\")"),
        "\"b\""
    );
    assert_eq!(eval("(seq-take-while #'cl-oddp '(2 4))"), "nil");
    // seq-contains returns the matching element (testfn gets (ELT E)).
    assert_eq!(eval("(seq-contains '(1 2 3) 2)"), "2");
    assert_eq!(eval("(seq-contains '(1 2 3) 9)"), "nil");
    assert_eq!(
        eval("(seq-contains '(1 2 3) 2 (lambda (a b) (= a (* 2 b))))"),
        "1"
    );
    // seq-setq destructures into existing places; seq-doseq iterates any seq.
    assert_eq!(
        eval("(let (a b) (seq-setq (a &rest b) '(1 2 3)) (list a b))"),
        "(1 (2 3))"
    );
    assert_eq!(
        eval("(let (acc) (seq-doseq (x [1 2 3]) (push x acc)) acc)"),
        "(3 2 1)"
    );
    assert_eq!(
        eval("(let (acc) (seq-doseq (c \"ab\") (push c acc)) acc)"),
        "(98 97)"
    );
}

#[test]
fn seq_take_drop_min_max_nonlist_sequences() {
    // seq-take / seq-drop work on any sequence and preserve its type.
    assert_eq!(eval("(seq-take [1 2 3 4] 2)"), "[1 2]");
    assert_eq!(eval("(seq-take \"abcd\" 2)"), "\"ab\"");
    assert_eq!(eval("(seq-take [1 2] 5)"), "[1 2]");
    assert_eq!(eval("(seq-drop [1 2 3 4] 2)"), "[3 4]");
    assert_eq!(eval("(seq-drop \"abcd\" 2)"), "\"cd\"");
    // seq-mapn accepts mixed sequence types, returns a list.
    assert_eq!(eval("(seq-mapn #'+ '(1 2) [10 20])"), "(11 22)");
    // seq-min / seq-max over vectors and strings.
    assert_eq!(eval("(seq-min [3 1 2])"), "1");
    assert_eq!(eval("(seq-max \"abc\")"), "99");
    // seq-position / seq-first / seq-rest / seq-sort / seq-group-by on vectors.
    assert_eq!(eval("(seq-position [10 20 30] 20)"), "1");
    assert_eq!(eval("(seq-first [10 20])"), "10");
    assert_eq!(eval("(seq-first nil)"), "nil");
    assert_eq!(eval("(seq-rest [10 20 30])"), "[20 30]"); // preserves type
    assert_eq!(eval("(seq-sort #'< [3 1 2])"), "[1 2 3]"); // preserves type
    assert_eq!(eval("(seq-sort #'< \"cab\")"), "\"abc\"");
    assert_eq!(
        eval("(seq-group-by #'cl-evenp [1 2 3 4])"),
        "((nil 1 3) (t 2 4))"
    );
}

#[test]
fn cl_seq_filters_preserve_type() {
    // cl-remove-if / -if-not / cl-delete-if / cl-remove-duplicates keep SEQ's type.
    assert_eq!(eval("(cl-remove-if #'cl-evenp [1 2 3 4])"), "[1 3]");
    assert_eq!(eval("(cl-remove-if-not #'cl-evenp [1 2 3 4])"), "[2 4]");
    assert_eq!(eval("(cl-remove-if #'cl-evenp '(1 2 3 4))"), "(1 3)");
    assert_eq!(
        eval("(cl-remove-if (lambda (c) (= c ?a)) \"banana\")"),
        "\"bnn\""
    );
    assert_eq!(eval("(cl-delete-if #'cl-evenp [1 2 3 4])"), "[1 3]");
    assert_eq!(eval("(cl-delete-if-not #'cl-evenp '(1 2 3 4))"), "(2 4)");
    assert_eq!(eval("(cl-remove-duplicates [1 2 1 3])"), "[2 1 3]");
    assert_eq!(
        eval("(cl-remove-duplicates [1 2 1 3] :from-end t)"),
        "[1 2 3]"
    );
    // :count still honored, type preserved.
    assert_eq!(
        eval("(cl-remove-if #'cl-evenp [1 2 3 4 5 6] :count 1)"),
        "[1 3 4 5 6]"
    );
}

#[test]
fn emacs_parity_math_and_introspection() {
    // expt: integer power, but float for negative or fractional exponents.
    assert_eq!(eval("(expt 2 10)"), "1024");
    assert_eq!(eval("(expt 2 -1)"), "0.5");
    assert_eq!(eval("(expt 2.0 0.5)"), "1.4142135623730951");
    assert_eq!(eval("(sqrt 16)"), "4.0");
    // string-to-number: floats, scientific notation, optional base.
    assert_eq!(eval("(string-to-number \"1.5e3\")"), "1500.0");
    assert_eq!(eval("(string-to-number \"ff\" 16)"), "255");
    assert_eq!(eval("(string-to-number \"-3.14\")"), "-3.14");
    assert_eq!(eval("(string-to-number \"x\")"), "0");
    // type-of names the primitive type.
    assert_eq!(eval("(type-of 5)"), "integer");
    assert_eq!(eval("(type-of 1.5)"), "float");
    assert_eq!(eval("(type-of \"s\")"), "string");
    assert_eq!(eval("(type-of '(1))"), "cons");
    assert_eq!(eval("(type-of [1])"), "vector");
    // functionp: subrs / non-macro closures / function-bound symbols only.
    assert_eq!(eval("(functionp 'car)"), "t");
    assert_eq!(eval("(functionp 5)"), "nil");
    assert_eq!(eval("(functionp 'when)"), "nil"); // macros are not functions
                                                  // misc predicates / float ops.
    assert_eq!(eval("(char-or-string-p ?a)"), "t");
    assert_eq!(eval("(char-equal ?A ?A)"), "t");
    assert_eq!(eval("(isnan (/ 0.0 0.0))"), "t");
    assert_eq!(eval("(fround 2.5)"), "2.0");
    assert_eq!(eval("(ffloor 2.7)"), "2.0");
    // fceiling / ftruncate round toward +inf / zero but keep a float result.
    assert_eq!(eval("(fceiling 3.2)"), "4.0");
    assert_eq!(eval("(fceiling -3.2)"), "-3.0");
    assert_eq!(eval("(ftruncate 3.9)"), "3.0");
    assert_eq!(eval("(ftruncate -3.9)"), "-3.0");
}

#[test]
fn emacs_parity_rounding_coercion_printing() {
    // floor/ceiling/round/truncate take an optional DIVISOR (exact integer division).
    assert_eq!(eval("(floor 7 2)"), "3");
    assert_eq!(eval("(floor -7 2)"), "-4");
    assert_eq!(eval("(floor 7 -2)"), "-4");
    assert_eq!(eval("(ceiling 7 2)"), "4");
    assert_eq!(eval("(truncate 7 2)"), "3");
    assert_eq!(eval("(round 7 2)"), "4");
    assert_eq!(eval("(round 5 2)"), "2"); // ties to even
                                          // last / butlast take an optional N.
    assert_eq!(eval("(last (list 1 2 3) 2)"), "(2 3)");
    assert_eq!(eval("(butlast (list 1 2 3) 2)"), "(1)");
    assert_eq!(eval("(butlast (list 1 2 3))"), "(1 2)");
    // reverse on any sequence; downcase/upcase on a string or a character.
    assert_eq!(eval("(reverse \"abc\")"), "\"cba\"");
    assert_eq!(eval("(reverse [1 2 3])"), "[3 2 1]");
    assert_eq!(eval("(downcase ?A)"), "97");
    assert_eq!(eval("(upcase ?a)"), "65");
    assert_eq!(eval("(downcase \"ABC\")"), "\"abc\"");
    // append: the final argument is the tail as-is (dotted when not a list).
    assert_eq!(eval("(append '(1 2) '(3 4) 5)"), "(1 2 3 4 . 5)");
    assert_eq!(eval("(append '(1 2) 3)"), "(1 2 . 3)");
    assert_eq!(eval("(append nil 3)"), "3");
    assert_eq!(eval("(append '(1 2) '(3 4))"), "(1 2 3 4)");
    // Non-finite floats print in Emacs read syntax.
    assert_eq!(eval("(/ 1.0 0)"), "1.0e+INF");
    assert_eq!(eval("(/ -1.0 0)"), "-1.0e+INF");
    assert_eq!(eval("(/ 0.0 0.0)"), "0.0e+NaN");
}

#[test]
fn emacs_parity_compiler_fixes() {
    // A (lambda ...) form in operator (head) position is applied directly.
    assert_eq!(eval("((lambda (x) x) 5)"), "5");
    assert_eq!(eval("((lambda (a b) (+ a b)) 3 4)"), "7");
    assert_eq!(eval("((lambda (x &optional y) (list x y)) 1)"), "(1 nil)");
    assert_eq!(eval("((lambda (&rest xs) xs) 1 2 3)"), "(1 2 3)");
    // 1+ / 1- preserve float contagion (lowered to Add/Sub, not int Inc/Dec).
    assert_eq!(eval("(1+ 1.0)"), "2.0");
    assert_eq!(eval("(1- 1.0)"), "0.0");
    assert_eq!(eval("(1+ -1.5)"), "-0.5");
    // ...while the integer fast path still works (loop counters).
    assert_eq!(eval("(1+ 41)"), "42");
    assert_eq!(eval("(let ((i 0)) (while (< i 5) (setq i (1+ i))) i)"), "5");
}

#[test]
fn emacs_parity_reader_cluster() {
    // Radix-prefixed integer literals (#x / #o / #b, uppercase, #NNr, signed).
    assert_eq!(eval("#x1f"), "31");
    assert_eq!(eval("#b101"), "5");
    assert_eq!(eval("#o17"), "15");
    assert_eq!(eval("#xFF"), "255");
    assert_eq!(eval("#x-10"), "-16");
    assert_eq!(eval("#16rFF"), "255");
    assert_eq!(eval("(+ #xff 1)"), "256");
    // Character modifier syntax: control / meta / shift, nestable.
    assert_eq!(eval("?\\C-a"), "1");
    assert_eq!(eval("?\\C-A"), "1");
    assert_eq!(eval("?\\C-?"), "127");
    assert_eq!(eval("?\\^a"), "1");
    assert_eq!(eval("?\\M-a"), "134217825");
    assert_eq!(eval("?\\C-\\M-a"), "134217729");
    assert_eq!(eval("?\\S-a"), "33554529");
    // Plain char literals still work.
    assert_eq!(eval("?A"), "65");
    // Non-finite float read syntax round-trips through print.
    assert_eq!(eval("1.0e+INF"), "1.0e+INF");
    assert_eq!(eval("-1.0e+INF"), "-1.0e+INF");
    assert_eq!(eval("0.0e+NaN"), "0.0e+NaN");
}

#[test]
fn emacs_parity_format_fields_and_misc_fns() {
    // %N$ argument fields, combinable with flags/width.
    assert_eq!(eval("(format \"%2$s %1$s\" \"a\" \"b\")"), "\"b a\"");
    assert_eq!(eval("(format \"%1$s %1$s\" \"x\" \"y\")"), "\"x x\"");
    assert_eq!(eval("(format \"%2$05d\" 1 42)"), "\"00042\"");
    // Sequential directives still advance normally.
    assert_eq!(eval("(format \"%s %s\" \"a\" \"b\")"), "\"a b\"");
    // logb / read / compare-strings.
    assert_eq!(eval("(logb 8)"), "3");
    assert_eq!(eval("(logb 1)"), "0");
    assert_eq!(eval("(read \"(1 2 3)\")"), "(1 2 3)");
    assert_eq!(eval("(read \"42\")"), "42");
    assert_eq!(
        eval("(compare-strings \"abc\" nil nil \"abd\" nil nil)"),
        "-3"
    );
    assert_eq!(
        eval("(compare-strings \"abc\" nil nil \"abc\" nil nil)"),
        "t"
    );
    assert_eq!(
        eval("(compare-strings \"ABC\" nil nil \"abc\" nil nil t)"),
        "t"
    );
    // error-message-string / seq-mapn.
    assert_eq!(eval("(error-message-string '(error \"hi\"))"), "\"hi\"");
    assert_eq!(eval("(seq-mapn '+ '(1 2) '(3 4))"), "(4 6)");
    assert_eq!(eval("(seq-mapn '+ '(1 2 3) '(10 20))"), "(11 22)");
}

#[test]
fn emacs_parity_sweep_fixes() {
    // vconcat / string-to-vector build vectors from any sequences.
    assert_eq!(eval("(vconcat [1 2] [3 4])"), "[1 2 3 4]");
    assert_eq!(eval("(vconcat \"ab\" [3] (list 4))"), "[97 98 3 4]");
    assert_eq!(eval("(string-to-vector \"ab\")"), "[97 98]");
    // fixnum constants.
    assert_eq!(eval("most-positive-fixnum"), "2305843009213693951");
    assert_eq!(eval("most-negative-fixnum"), "-2305843009213693952");
    // abs keeps type and normalizes -0.0.
    assert_eq!(eval("(abs -5)"), "5");
    assert_eq!(eval("(abs -3.5)"), "3.5");
    assert_eq!(eval("(abs -0.0)"), "0.0");
    // string-prefix/suffix honor IGNORE-CASE; assoc takes a TESTFN.
    assert_eq!(eval("(string-prefix-p \"AB\" \"abc\" t)"), "t");
    assert_eq!(eval("(string-suffix-p \"C\" \"abc\" t)"), "t");
    assert_eq!(
        eval("(assoc \"a\" (list (cons \"a\" 1)) (function string=))"),
        "(\"a\" . 1)"
    );
    // string-pad with PADDING + START; string-equal-ignore-case; upcase-initials.
    assert_eq!(eval("(string-pad \"x\" 3 ?- t)"), "\"--x\"");
    assert_eq!(eval("(string-pad \"x\" 3)"), "\"x  \"");
    assert_eq!(eval("(string-equal-ignore-case \"AB\" \"ab\")"), "t");
    assert_eq!(eval("(upcase-initials \"foo bar\")"), "\"Foo Bar\"");
    // logcount.
    assert_eq!(eval("(logcount 7)"), "3");
    assert_eq!(eval("(logcount -2)"), "1");
}

#[test]
fn emacs_parity_introspection_and_seq() {
    // Function-cell introspection and predicates.
    assert_eq!(eval("(subrp (symbol-function 'car))"), "t");
    assert_eq!(eval("(intern-soft \"nonexistent-xyz-123\")"), "nil");
    assert_eq!(eval("(fixnump 5)"), "t");
    assert_eq!(eval("(fixnump 1.0)"), "nil");
    assert_eq!(eval("(bignump 5)"), "nil");
    assert_eq!(eval("(char-uppercase-p ?A)"), "t");
    assert_eq!(eval("(char-uppercase-p ?a)"), "nil");
    assert_eq!(eval("(string-distance \"kitten\" \"sitting\")"), "3");
    // macrop / special-form-p match Emacs's classification.
    assert_eq!(eval("(special-form-p 'if)"), "t");
    assert_eq!(eval("(special-form-p 'when)"), "nil");
    assert_eq!(eval("(macrop 'when)"), "t");
    assert_eq!(eval("(macrop 'lambda)"), "t");
    assert_eq!(eval("(macrop 'if)"), "nil");
    // alist/seq additions.
    assert_eq!(
        eval("(alist-get 9 (list (cons 1 \"a\")) \"def\")"),
        "\"def\""
    );
    assert_eq!(eval("(seq-concatenate 'list (list 1) (list 2))"), "(1 2)");
    assert_eq!(eval("(seq-concatenate 'vector (list 1) [2])"), "[1 2]");
    assert_eq!(eval("(seq-concatenate 'string (list 104 105))"), "\"hi\"");
    assert_eq!(eval("(copy-alist (list (cons 1 2)))"), "((1 . 2))");
    assert_eq!(eval("(substring-no-properties \"abc\" 1)"), "\"bc\"");
    // string-trim with regexp args.
    assert_eq!(eval("(string-trim \"xxhixx\" \"x+\" \"x+\")"), "\"hi\"");
}

#[test]
fn emacs_parity_format_signs_and_misc() {
    // format + / space sign flags on signed conversions.
    assert_eq!(eval("(format \"%+d\" 5)"), "\"+5\"");
    assert_eq!(eval("(format \"%+d\" -5)"), "\"-5\"");
    assert_eq!(eval("(format \"% d\" 5)"), "\" 5\"");
    assert_eq!(eval("(format \"%+.2f\" 3.14159)"), "\"+3.14\"");
    assert_eq!(eval("(format \"%+05d\" 42)"), "\"+0042\"");
    // %e is C-style: 6-digit default mantissa, signed >=2-digit exponent.
    assert_eq!(eval("(format \"%e\" 31415.9)"), "\"3.141590e+04\"");
    assert_eq!(eval("(format \"%e\" 0.001)"), "\"1.000000e-03\"");
    assert_eq!(eval("(format \"%.2e\" 12345.0)"), "\"1.23e+04\"");
    // hash-table-test / nbutlast.
    assert_eq!(
        eval("(hash-table-test (make-hash-table :test 'equal))"),
        "equal"
    );
    assert_eq!(eval("(hash-table-test (make-hash-table))"), "eql");
    assert_eq!(eval("(nbutlast (list 1 2 3))"), "(1 2)");
}

#[test]
fn emacs_parity_search_and_assoc() {
    // string-search honors the optional START char index.
    assert_eq!(eval("(string-search \"lo\" \"hello world\" 5)"), "nil");
    assert_eq!(eval("(string-search \"lo\" \"hello world\")"), "3");
    assert_eq!(eval("(string-search \"o\" \"foo\" 2)"), "2");
    // memql uses eql (matches floats by value).
    assert_eq!(eval("(memql 2 (list 1 2 3))"), "(2 3)");
    assert_eq!(eval("(memql 2.0 (list 1.0 2.0))"), "(2.0)");
    // assoc-string: string keys, optional case folding, cons-or-string elements.
    assert_eq!(eval("(assoc-string \"a\" (list \"a\" \"b\"))"), "\"a\"");
    assert_eq!(eval("(assoc-string \"B\" (list \"a\" \"b\") t)"), "\"b\"");
    assert_eq!(
        eval("(assoc-string \"k\" (list (cons \"k\" 1)))"),
        "(\"k\" . 1)"
    );
    assert_eq!(eval("(assoc-string \"z\" (list \"a\"))"), "nil");
}

#[test]
fn emacs_parity_format_radix() {
    // %x/%X/%o print sign + magnitude (not two's complement) for negatives.
    assert_eq!(eval("(format \"%x\" -1)"), "\"-1\"");
    assert_eq!(eval("(format \"%x\" -255)"), "\"-ff\"");
    assert_eq!(eval("(format \"%X\" -255)"), "\"-FF\"");
    assert_eq!(eval("(format \"%o\" -8)"), "\"-10\"");
    // The # flag adds a 0x / 0X / 0 prefix (suppressed for zero).
    assert_eq!(eval("(format \"%#x\" 255)"), "\"0xff\"");
    assert_eq!(eval("(format \"%#X\" 255)"), "\"0XFF\"");
    assert_eq!(eval("(format \"%#o\" 8)"), "\"010\"");
    assert_eq!(eval("(format \"%#x\" 0)"), "\"0\"");
    // Zero-fill goes after the sign and the 0x prefix.
    assert_eq!(eval("(format \"%08x\" 255)"), "\"000000ff\"");
    assert_eq!(eval("(format \"%#010x\" 255)"), "\"0x000000ff\"");
    assert_eq!(eval("(format \"%05d\" -42)"), "\"-0042\"");
}

#[test]
fn emacs_parity_case_fold_and_macros() {
    // case-fold-search defaults to t: string matching folds case by default.
    assert_eq!(eval("(string-match \"ABC\" \"abc\")"), "0");
    assert_eq!(
        eval("(let ((case-fold-search nil)) (string-match \"ABC\" \"abc\"))"),
        "nil"
    );
    assert_eq!(
        eval("(replace-regexp-in-string \"a\" \"X\" \"AaA\")"),
        "\"XXX\""
    );
    // cl-incf / cl-decf work on generalized places (not just symbols).
    assert_eq!(eval("(let ((l (list 1 2))) (cl-incf (car l)) l)"), "(2 2)");
    assert_eq!(eval("(let ((x 5)) (incf x 3) x)"), "8");
    // when-let* / if-let* with multiple sequential bindings, short-circuiting.
    assert_eq!(eval("(when-let* ((a 1) (b 2)) (+ a b))"), "3");
    assert_eq!(eval("(when-let* ((a 1) (b nil)) (+ a 1))"), "nil");
    assert_eq!(eval("(if-let* ((a nil)) a 'else)"), "else");
    assert_eq!(eval("(if-let* ((a 5)) a 'else)"), "5");
    // named-let — a self-recursive local loop.
    assert_eq!(
        eval("(named-let loop ((i 0)) (if (< i 3) (loop (1+ i)) i))"),
        "3"
    );
}

#[test]
fn emacs_parity_replace_function_rep() {
    // A function REP is called on each match's text; its result is the replacement.
    assert_eq!(
        eval("(replace-regexp-in-string \"[0-9]+\" (lambda (m) (number-to-string (* 2 (string-to-number m)))) \"a5b10\")"),
        "\"a10b20\""
    );
    assert_eq!(
        eval("(replace-regexp-in-string \"[a-z]+\" #'upcase \"ab cd\")"),
        "\"AB CD\""
    );
    // String REP (template) path still works unchanged.
    assert_eq!(
        eval("(replace-regexp-in-string \"[0-9]+\" \"#\" \"a1b22\")"),
        "\"a#b#\""
    );
    assert_eq!(
        eval("(replace-regexp-in-string \"\\\\([a-z]\\\\)=\\\\([0-9]\\\\)\" \"\\\\2:\\\\1\" \"x=1\")"),
        "\"1:x\""
    );
}

#[test]
fn emacs_parity_mutation_and_seq() {
    // nconc destructively concatenates (the first list's tail is spliced).
    assert_eq!(eval("(nconc (list 1 2) (list 3 4))"), "(1 2 3 4)");
    assert_eq!(
        eval("(let ((a (list 1 2)) (b (list 3))) (nconc a b) a)"),
        "(1 2 3)"
    );
    assert_eq!(eval("(nconc nil (list 1) nil (list 2))"), "(1 2)");
    // plist-put mutates in place when appending a new key.
    assert_eq!(
        eval("(let ((p (list :a 1))) (plist-put p :b 2) p)"),
        "(:a 1 :b 2)"
    );
    // delete-dups is destructive.
    assert_eq!(
        eval("(let ((l (list 1 2 2 3))) (delete-dups l) l)"),
        "(1 2 3)"
    );
    // number-sequence with a negative step counts down.
    assert_eq!(eval("(number-sequence 5 1 -1)"), "(5 4 3 2 1)");
    assert_eq!(eval("(number-sequence 0 10 3)"), "(0 3 6 9)");
    // fillarray fills a vector in place; rassq-delete-all drops matching cdrs.
    assert_eq!(
        eval("(let ((v (make-vector 3 0))) (fillarray v 7) v)"),
        "[7 7 7]"
    );
    assert_eq!(
        eval("(rassq-delete-all 2 (list (cons 'a 2) (cons 'b 3)))"),
        "((b . 3))"
    );
}

#[test]
fn emacs_parity_cl_seq_extras() {
    // cl-reduce honors :initial-value; empty + no init calls the function nullary.
    assert_eq!(eval("(cl-reduce #'+ (list 1 2 3) :initial-value 10)"), "16");
    assert_eq!(eval("(cl-reduce #'+ (list 1 2 3 4))"), "10");
    assert_eq!(eval("(cl-reduce #'+ nil)"), "0");
    // cl-mapcar walks N sequences in parallel.
    assert_eq!(eval("(cl-mapcar #'+ (list 1 2) (list 3 4))"), "(4 6)");
    assert_eq!(eval("(cl-mapcar #'+ (list 1 2 3) (list 10 20))"), "(11 22)");
    // cl-remove-duplicates keeps the LAST occurrence of each element (Emacs default).
    assert_eq!(eval("(cl-remove-duplicates (list 1 2 1 3 2))"), "(1 3 2)");
    // seq-group-by lists groups in first-encounter order of the key.
    assert_eq!(
        eval("(seq-group-by #'cl-evenp (list 1 2 3 4))"),
        "((nil 1 3) (t 2 4))"
    );
}

#[test]
fn emacs_parity_cl_control_and_length() {
    // length= / length< / length> on sequences.
    assert_eq!(eval("(length= (list 1 2) 2)"), "t");
    assert_eq!(eval("(length< (list 1 2) 3)"), "t");
    assert_eq!(eval("(length> (list 1 2 3) 2)"), "t");
    assert_eq!(eval("(length= \"abc\" 3)"), "t");
    // cl-getf with a default.
    assert_eq!(eval("(cl-getf (list :a 1) :b 99)"), "99");
    assert_eq!(eval("(cl-getf (list :a 1 :b 2) :b 99)"), "2");
    // cl-typecase dispatches on the value's type.
    assert_eq!(eval("(cl-typecase 5 (string 's) (integer 'i))"), "i");
    assert_eq!(eval("(cl-typecase \"x\" (string 's) (integer 'i))"), "s");
    assert_eq!(
        eval("(cl-typecase 1.5 (integer 'i) (float 'f) (t 'o))"),
        "f"
    );
    // cl-destructuring-bind: positional, &rest, &optional.
    assert_eq!(
        eval("(cl-destructuring-bind (a b c) (list 1 2 3) (+ a b c))"),
        "6"
    );
    assert_eq!(
        eval("(cl-destructuring-bind (a &rest r) (list 1 2 3 4) (list a r))"),
        "(1 (2 3 4))"
    );
    assert_eq!(
        eval("(cl-destructuring-bind (a &optional b) (list 1) (list a b))"),
        "(1 nil)"
    );
    // string-clean-whitespace collapses runs and trims.
    assert_eq!(eval("(string-clean-whitespace \"  a   b  \")"), "\"a b\"");
}

#[test]
fn emacs_parity_cl_loop_subset() {
    // Numeric for with to / below / downto / by.
    assert_eq!(eval("(cl-loop for i from 1 to 5 collect i)"), "(1 2 3 4 5)");
    assert_eq!(
        eval("(cl-loop for i from 0 below 5 collect i)"),
        "(0 1 2 3 4)"
    );
    assert_eq!(
        eval("(cl-loop for i from 10 downto 7 collect i)"),
        "(10 9 8 7)"
    );
    assert_eq!(
        eval("(cl-loop for i from 1 to 10 by 2 collect i)"),
        "(1 3 5 7 9)"
    );
    // for ... in / on.
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3) collect (* x x))"),
        "(1 4 9)"
    );
    assert_eq!(
        eval("(cl-loop for c on (list 1 2 3) collect (car c))"),
        "(1 2 3)"
    );
    // Accumulation: sum / count / append / maximize / minimize.
    assert_eq!(eval("(cl-loop for x in (list 1 2 3) sum x)"), "6");
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3 4) count (cl-evenp x))"),
        "2"
    );
    assert_eq!(
        eval("(cl-loop for x in (list (list 1) (list 2 3)) append x)"),
        "(1 2 3)"
    );
    assert_eq!(eval("(cl-loop for i from 1 to 4 maximize (* i i))"), "16");
    // repeat / until / do / finally return.
    assert_eq!(eval("(cl-loop repeat 3 collect 9)"), "(9 9 9)");
    assert_eq!(
        eval("(cl-loop for i from 1 to 100 until (> i 3) collect i)"),
        "(1 2 3)"
    );
    assert_eq!(
        eval("(let ((s 0)) (cl-loop for i from 1 to 5 do (setq s (+ s i))) s)"),
        "15"
    );
    assert_eq!(
        eval("(cl-loop for i from 1 to 5 do (ignore i) finally return 42)"),
        "42"
    );
}

#[test]
fn emacs_parity_cl_loop_conditionals() {
    // when / unless conditionalize the following accumulation clause.
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3 4 5) when (cl-evenp x) collect x)"),
        "(2 4)"
    );
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3 4) unless (cl-evenp x) collect x)"),
        "(1 3)"
    );
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3 4 5) when (> x 2) sum x)"),
        "12"
    );
    // if ... else.
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3) if (cl-oddp x) collect x else collect (- x))"),
        "(1 -2 3)"
    );
    // with bindings; collect into a named var.
    assert_eq!(
        eval("(cl-loop with total = 0 for x in (list 1 2 3) do (setq total (+ total x)) finally return total)"),
        "6"
    );
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3) collect x into ys finally return (cons 'done ys))"),
        "(done 1 2 3)"
    );
    // always / never / thereis.
    assert_eq!(
        eval("(cl-loop for x in (list 2 4 6) always (cl-evenp x))"),
        "t"
    );
    assert_eq!(
        eval("(cl-loop for x in (list 2 4 5) always (cl-evenp x))"),
        "nil"
    );
    assert_eq!(
        eval("(cl-loop for x in (list 1 3 5) never (cl-evenp x))"),
        "t"
    );
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3) thereis (and (cl-evenp x) x))"),
        "2"
    );
}

#[test]
fn scope_restored_after_nonlocal_exit_from_nested_let() {
    // A throw/error out of an inner `let` must not leak its lexical scope, or the
    // surrounding bindings get corrupted. (Root cause of a macroexpander-looking
    // bug where ERT `should` around a catch-emitting macro saw "void variable".)
    assert_eq!(
        eval("(let ((a 10)) (catch 'x (let ((b 2)) (throw 'x b))) a)"),
        "10"
    );
    assert_eq!(
        eval("(let ((a 7)) (condition-case nil (let ((b 1)) (error \"boom\")) (error nil)) a)"),
        "7"
    );
    assert_eq!(
        eval("(let ((a 1)) (catch 'x (let ((b 2)) (let ((c 3)) (throw 'x (+ b c))))) a)"),
        "1"
    );
    // A macro that emits a catch/nested-let, wrapped in an ERT `should` inside an
    // ert-deftest (three macro levels) — the exact shape that used to corrupt the
    // ERT runner's locals.
    assert_eq!(
        eval(
            "(progn (defun probe () \
               (let ((a 5)) (catch 'd (let ((b 2)) (when (cl-evenp b) (throw 'd b)))) a)) \
             (probe))"
        ),
        "5"
    );
}

#[test]
fn emacs_parity_sequences_and_introspection() {
    // mapcar / seq-* accept any sequence (vector, string), not just lists.
    assert_eq!(eval("(mapcar #'1+ [1 2 3])"), "(2 3 4)");
    assert_eq!(eval("(mapcar #'1+ \"abc\")"), "(98 99 100)");
    assert_eq!(eval("(seq-reduce #'+ [1 2 3] 0)"), "6");
    assert_eq!(eval("(seq-filter #'cl-evenp [1 2 3 4])"), "(2 4)");
    assert_eq!(eval("(seq-count #'cl-evenp [1 2 3 4])"), "2");
    // boundp / default-value / gensym.
    assert_eq!(eval("(boundp 'most-positive-fixnum)"), "t");
    assert_eq!(eval("(boundp 'totally-undefined-xyz)"), "nil");
    assert_eq!(eval("(default-value 'case-fold-search)"), "t");
    assert_eq!(eval("(symbolp (gensym))"), "t");
    // hash-table print syntax (Emacs 30): omit test=eql and empty data.
    assert_eq!(
        eval("(format \"%S\" (make-hash-table))"),
        "\"#s(hash-table)\""
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 1 2 h) (format \"%S\" h))"),
        "\"#s(hash-table data (1 2))\""
    );
    assert_eq!(
        eval("(let ((h (make-hash-table :test 'equal))) (puthash \"a\" 1 h) (format \"%S\" h))"),
        "\"#s(hash-table test equal data (\\\"a\\\" 1))\""
    );
}

#[test]
fn pcase_backquote_patterns() {
    // Backquote (structural) patterns, supported via the reader's eager expansion.
    assert_eq!(eval("(pcase (list 1 2) (`(,a ,b) (+ a b)))"), "3");
    assert_eq!(
        eval("(pcase (list 1 2 3) (`(,a ,b ,c) (list c b a)))"),
        "(3 2 1)"
    );
    assert_eq!(eval("(pcase (cons 1 2) (`(,a . ,b) (+ a b)))"), "3");
    assert_eq!(
        eval("(pcase (list 1 2 3) (`(,a . ,rest) (list a rest)))"),
        "(1 (2 3))"
    );
    // Nested, and literals inside the template.
    assert_eq!(
        eval("(pcase (list 1 (list 2 3)) (`(,a (,b ,c)) (list a b c)))"),
        "(1 2 3)"
    );
    assert_eq!(eval("(pcase '(foo 5) (`(foo ,n) n) (_ 'no))"), "5");
    // Non-matching subject (atom vs cons pattern) falls through safely.
    assert_eq!(eval("(pcase 5 (`(,a ,b) 'pair) (_ 'atom))"), "atom");
    assert_eq!(eval("(pcase (list 1) (`(,a ,b) 'two) (`(,a) 'one))"), "one");
}

#[test]
fn dotted_backquote_reader() {
    // The reader builds a dotted cons from a `,x in the dotted-cdr position.
    assert_eq!(eval("(let ((a 1) (b 2)) `(,a . ,b))"), "(1 . 2)");
    assert_eq!(eval("(let ((a 1) (b (list 2 3))) `(,a . ,b))"), "(1 2 3)");
    assert_eq!(eval("(let ((a 1)) `(,a . 5))"), "(1 . 5)");
    assert_eq!(eval("(let ((x 9)) `(a b . ,x))"), "(a b . 9)");
}

#[test]
fn emacs_parity_local_functions_and_let_alist() {
    // cl-flet: lexical local functions; #'NAME refs work too.
    assert_eq!(eval("(cl-flet ((sq (n) (* n n))) (sq 6))"), "36");
    assert_eq!(
        eval("(cl-flet ((add (a b) (+ a b)) (neg (x) (- x))) (add (neg 3) 10))"),
        "7"
    );
    assert_eq!(
        eval("(cl-flet ((f (x) (* x 2))) (mapcar #'f (list 1 2 3)))"),
        "(2 4 6)"
    );
    // cl-labels: self- and mutual recursion (closures capture the gensym by ref).
    assert_eq!(
        eval("(cl-labels ((fac (n) (if (= n 0) 1 (* n (fac (1- n)))))) (fac 5))"),
        "120"
    );
    assert_eq!(
        eval("(cl-labels ((evn (n) (if (= n 0) t (od (1- n)))) (od (n) (if (= n 0) nil (evn (1- n))))) (evn 10))"),
        "t"
    );
    // and-let* / let-alist.
    assert_eq!(eval("(and-let* ((x 5) (y 10)) (+ x y))"), "15");
    assert_eq!(eval("(and-let* ((x 5) (y nil)) 99)"), "nil");
    assert_eq!(
        eval("(let-alist (list (cons 'a 1) (cons 'b 2)) (+ .a .b))"),
        "3"
    );
    assert_eq!(
        eval("(let-alist (list (cons 'name \"bob\")) .name)"),
        "\"bob\""
    );
    // fset / fboundp.
    assert_eq!(
        eval("(progn (fset 'myf (lambda (x) (* x 3))) (myf 4))"),
        "12"
    );
    assert_eq!(eval("(fboundp 'car)"), "t");
    assert_eq!(eval("(fboundp 'undefined-xyz)"), "nil");
    // cl-dolist / cl-dotimes.
    assert_eq!(
        eval("(let ((s 0)) (cl-dotimes (i 4) (setq s (+ s i))) s)"),
        "6"
    );
}

#[test]
fn emacs_parity_cl_block_and_subseq() {
    // cl-block / cl-return-from / cl-return.
    assert_eq!(eval("(cl-block foo (cl-return-from foo 42) 99)"), "42");
    assert_eq!(eval("(cl-block nil (cl-return 7) 8)"), "7");
    assert_eq!(eval("(cl-block b (+ 1 (cl-return-from b 10)) 5)"), "10");
    // cl-dolist establishes a nil block, so cl-return escapes it.
    assert_eq!(
        eval("(cl-dolist (x (list 1 2 3 4)) (if (= x 3) (cl-return x)))"),
        "3"
    );
    // cl-pushnew: add unless present (by eql).
    assert_eq!(eval("(let ((l (list 2 3))) (cl-pushnew 1 l) l)"), "(1 2 3)");
    assert_eq!(eval("(let ((l (list 1 2))) (cl-pushnew 1 l) l)"), "(1 2)");
    // cl-find-if-not.
    assert_eq!(eval("(cl-find-if-not #'cl-evenp (list 2 4 5))"), "5");
    assert_eq!(eval("(cl-find-if-not #'cl-evenp (list 2 4 6))"), "nil");
    // cl-subseq / seq-subseq on any sequence, optional & negative end.
    assert_eq!(eval("(cl-subseq \"hello\" 1 3)"), "\"el\"");
    assert_eq!(eval("(cl-subseq [1 2 3 4] 1 3)"), "[2 3]");
    assert_eq!(eval("(cl-subseq \"hello\" 2)"), "\"llo\"");
    assert_eq!(eval("(cl-subseq [1 2 3 4 5] 1 -1)"), "[2 3 4]");
}

#[test]
fn emacs_parity_cl_defstruct() {
    // Constructor (keyword args + per-slot defaults), accessors, predicate, copier.
    assert_eq!(
        eval("(cl-defstruct point x y) (let ((p (make-point :x 3 :y 4))) (list (point-x p) (point-y p)))"),
        "(3 4)"
    );
    assert_eq!(
        eval("(cl-defstruct dd (x 0) (y 10)) (let ((p (make-dd :x 3))) (list (dd-x p) (dd-y p)))"),
        "(3 10)"
    );
    assert_eq!(eval("(cl-defstruct e3 x y) (e3-p (make-e3))"), "t");
    assert_eq!(eval("(cl-defstruct e4 a b) (e4-p (list 1 2))"), "nil");
    assert_eq!(
        eval("(cl-defstruct cc x) (let* ((a (make-cc :x 1)) (b (copy-cc a))) (list (cc-x b) (eq a b)))"),
        "(1 nil)"
    );
    // setf / cl-incf on a slot (accessor registered for setf across top-level forms).
    assert_eq!(
        eval("(cl-defstruct sf x y) (let ((p (make-sf :x 1 :y 2))) (setf (sf-x p) 99) (cl-incf (sf-y p) 10) (list (sf-x p) (sf-y p)))"),
        "(99 12)"
    );
    // cl-defstruct returns the struct name.
    assert_eq!(eval("(cl-defstruct ret a b)"), "ret");
}

#[test]
fn emacs_parity_cl_keyword_seq_fns() {
    // :test / :key on the matching cl-* functions.
    assert_eq!(eval("(cl-member 2.0 (list 1 2 3) :test #'=)"), "(2 3)");
    assert_eq!(eval("(cl-position 3 (list 1 2 3 4) :test #'=)"), "2");
    assert_eq!(eval("(cl-find 2 (list 1 2 3) :key #'1-)"), "3");
    assert_eq!(eval("(cl-count 2 (list 1 2 2 3 2))"), "3");
    assert_eq!(
        eval("(cl-assoc \"x\" (list (cons \"x\" 1)) :test #'string=)"),
        "(\"x\" . 1)"
    );
    // :count on cl-remove / cl-substitute.
    assert_eq!(eval("(cl-remove 2 (list 1 2 3 2))"), "(1 3)");
    assert_eq!(eval("(cl-remove 2 (list 1 2 3 2) :count 1)"), "(1 3 2)");
    assert_eq!(eval("(cl-substitute 9 2 (list 1 2 3 2))"), "(1 9 3 9)");
    assert_eq!(
        eval("(cl-substitute 9 2 (list 1 2 3 2) :count 1)"),
        "(1 9 3 2)"
    );
    // type preserved (string in, string out).
    assert_eq!(eval("(cl-remove ?a \"banana\")"), "\"bnn\"");
    // defaults still work without keywords.
    assert_eq!(eval("(cl-member 2 (list 1 2 3))"), "(2 3)");
    assert_eq!(
        eval("(cl-assoc 2 (list (cons 1 \"a\") (cons 2 \"b\")))"),
        "(2 . \"b\")"
    );
}

#[test]
fn emacs_parity_error_object_data() {
    // condition-case binds the handler var to the real (SYMBOL . DATA) object,
    // so the data list is preserved (not stringified).
    assert_eq!(
        eval("(condition-case e (signal 'wrong-type-argument (list 'integerp 5)) (wrong-type-argument (list (car e) (cadr e) (caddr e))))"),
        "(wrong-type-argument integerp 5)"
    );
    assert_eq!(
        eval("(condition-case e (signal 'my-err '(1 2 3)) (my-err (caddr e)))"),
        "2"
    );
    // error builds (error "MESSAGE").
    assert_eq!(
        eval("(condition-case e (error \"boom\") (error e))"),
        "(error \"boom\")"
    );
    assert_eq!(
        eval("(condition-case e (error \"x %d\" 5) (error (cdr e)))"),
        "(\"x 5\")"
    );
    // car of a non-cons yields a wrong-type-argument; symbol matches.
    assert_eq!(
        eval("(condition-case e (car 5) (error (car e)))"),
        "wrong-type-argument"
    );
    // ignore-error (singular) and with-suppressed-warnings.
    assert_eq!(eval("(ignore-error wrong-type-argument (car 5))"), "nil");
    assert_eq!(
        eval("(with-suppressed-warnings ((obsolete foo)) (+ 1 2))"),
        "3"
    );
}

#[test]
fn emacs_parity_error_system_and_destructuring() {
    // user-error signals the user-error condition (caught by user-error or error).
    assert_eq!(
        eval("(condition-case nil (user-error \"bad\") (user-error 5))"),
        "5"
    );
    assert_eq!(
        eval("(condition-case e (user-error \"oops\") (error (cadr e)))"),
        "\"oops\""
    );
    // define-error + get/put + error-message-string.
    assert_eq!(
        eval("(define-error 'my-error \"My custom\") (condition-case e (signal 'my-error '(1)) (my-error (cadr e)))"),
        "1"
    );
    assert_eq!(eval("(get 'error 'error-conditions)"), "(error)");
    assert_eq!(eval("(put 'foo-x 'bar 42) (get 'foo-x 'bar)"), "42");
    assert_eq!(
        eval("(error-message-string '(wrong-type-argument integerp 5))"),
        "\"Wrong type argument: integerp, 5\""
    );
    // #'(lambda ...) is a closure (compiler fix), not the literal form.
    assert_eq!(eval("(funcall #'(lambda (x) (* x x)) 5)"), "25");
    assert_eq!(
        eval("(mapcar #'(lambda (x) (1+ x)) (list 1 2 3))"),
        "(2 3 4)"
    );
    assert_eq!(eval("(funcall (cl-function (lambda (x) (* x x))) 5)"), "25");
    // seq-let / pcase-let destructuring; macroexp-progn.
    assert_eq!(
        eval("(seq-let (a b c) [10 20 30] (list c b a))"),
        "(30 20 10)"
    );
    assert_eq!(
        eval("(pcase-let ((`(,a (,b ,c)) (list 1 (list 2 3)))) (+ a b c))"),
        "6"
    );
    assert_eq!(eval("(macroexp-progn (list 1 2))"), "(progn 1 2)");
}

#[test]
fn emacs_parity_cl_letf_and_destructuring() {
    // cl-letf temporarily sets a place, restoring it afterward.
    assert_eq!(
        eval("(defvar gv2 5) (list (cl-letf (((symbol-value 'gv2) 99)) gv2) gv2)"),
        "(99 5)"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (cl-letf (((nth 1 l) 99)) (nth 1 l)))"),
        "99"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (cl-letf (((car l) 9)) l))"),
        "(1 2 3)"
    );
    // letrec (recursive closures) / dlet.
    assert_eq!(
        eval("(letrec ((f (lambda (n) (if (= n 0) 1 (* n (funcall f (1- n))))))) (funcall f 5))"),
        "120"
    );
    assert_eq!(eval("(dlet ((x 5)) x)"), "5");
    // Nested cl-destructuring-bind (multiple levels) and &rest.
    assert_eq!(
        eval("(cl-destructuring-bind (a (b c) d) (list 1 (list 2 3) 4) (list a b c d))"),
        "(1 2 3 4)"
    );
    assert_eq!(
        eval("(cl-destructuring-bind (a (b (c d))) (list 1 (list 2 (list 3 4))) (list a b c d))"),
        "(1 2 3 4)"
    );
    assert_eq!(
        eval("(cl-destructuring-bind (a &rest r) (list 1 2 3) (cons a r))"),
        "(1 2 3)"
    );
    // seq-let with &rest.
    assert_eq!(
        eval("(seq-let (a &rest r) (list 1 2 3) (list a r))"),
        "(1 (2 3))"
    );
}

#[test]
fn emacs_parity_struct_record_printing() {
    // cl-defstruct instances print as Emacs record syntax and type-of names them.
    assert_eq!(
        eval("(cl-defstruct point x y) (format \"%S\" (make-point :x 3 :y 4))"),
        "\"#s(point 3 4)\""
    );
    assert_eq!(
        eval("(cl-defstruct point x y) (type-of (make-point :x 1))"),
        "point"
    );
    assert_eq!(eval("(cl-defstruct point x) (recordp (make-point))"), "t");
    assert_eq!(
        eval("(cl-defstruct point x) (cl-struct-p (make-point))"),
        "t"
    );
    // Plain vectors are unaffected.
    assert_eq!(eval("(recordp [1 2 3])"), "nil");
    assert_eq!(eval("(type-of [1 2 3])"), "vector");
    // Nested structs print recursively.
    assert_eq!(
        eval("(cl-defstruct node val next) (prin1-to-string (make-node :val 1 :next (make-node :val 2)))"),
        "\"#s(node 1 #s(node 2 nil))\""
    );
}

#[test]
fn emacs_parity_eval_and_cl_loop_implicit_from() {
    // eval compiles and runs a form (macroexpanding first).
    assert_eq!(eval("(eval (read \"(+ 1 2)\"))"), "3");
    assert_eq!(eval("(eval '(+ 1 2 3))"), "6");
    assert_eq!(eval("(eval (list '* 6 7))"), "42");
    assert_eq!(eval("(let ((form '(if t 'yes 'no))) (eval form))"), "yes");
    assert_eq!(eval("(funcall 'eval '(+ 10 20))"), "30");
    // eval of a macro form (macroexpanded).
    assert_eq!(eval("(eval '(when t 1 2 3))"), "3");
    // cl-loop `for VAR to/below N` with an implicit `from 0`.
    assert_eq!(eval("(cl-loop for i below 5 collect i)"), "(0 1 2 3 4)");
    assert_eq!(eval("(cl-loop for i to 4 sum i)"), "10");
    assert_eq!(eval("(cl-loop for i below 10 by 3 collect i)"), "(0 3 6 9)");
}

#[test]
fn emacs_parity_macroexpand_and_introspection() {
    // macroexpand / macroexpand-1 / macroexpand-all on real (user/prelude) macros.
    assert_eq!(
        eval("(defmacro my-inc (x) (list '1+ x)) (macroexpand '(my-inc 5))"),
        "(1+ 5)"
    );
    assert_eq!(
        eval("(macroexpand '(push 1 lst))"),
        "(setq lst (cons 1 lst))"
    );
    assert_eq!(eval("(macroexpand-1 '(+ 1 2))"), "(+ 1 2)"); // not a macro -> unchanged
    assert_eq!(
        eval("(defmacro m2 (x) (list 'progn x x)) (macroexpand-all '(m2 (m2 5)))"),
        "(progn (progn 5 5) (progn 5 5))"
    );
    // indirect-function follows to the function object.
    assert_eq!(eval("(indirect-function 'car)"), "#<subr car>");
    // cl-sort (with :key), commandp, plistp.
    assert_eq!(eval("(cl-sort (list 3 1 2) #'<)"), "(1 2 3)");
    assert_eq!(
        eval("(cl-sort (list '(3 a) '(1 b) '(2 c)) #'< :key #'car)"),
        "((1 b) (2 c) (3 a))"
    );
    assert_eq!(eval("(commandp #'car)"), "nil");
    assert_eq!(eval("(plistp (list :a 1))"), "t");
    assert_eq!(eval("(plistp (list :a 1 :b))"), "nil");
    assert_eq!(eval("(plistp 5)"), "nil");
}

#[test]
fn emacs_parity_float_printing() {
    // Emacs-style shortest float printing, with exponential for extreme magnitudes.
    assert_eq!(eval("(number-to-string 1e100)"), "\"1e+100\"");
    assert_eq!(eval("(number-to-string 1e14)"), "\"100000000000000.0\"");
    assert_eq!(eval("(number-to-string 1e15)"), "\"1e+15\"");
    assert_eq!(
        eval("(number-to-string 1234567890123456.0)"),
        "\"1234567890123456.0\""
    );
    assert_eq!(eval("(number-to-string 0.0001)"), "\"0.0001\"");
    assert_eq!(eval("(number-to-string 0.00001)"), "\"1e-05\"");
    assert_eq!(eval("(number-to-string 1.5e-10)"), "\"1.5e-10\"");
    assert_eq!(eval("(number-to-string -2.5e30)"), "\"-2.5e+30\"");
    assert_eq!(eval("(number-to-string 100.0)"), "\"100.0\"");
    assert_eq!(eval("(number-to-string 3.14159)"), "\"3.14159\"");
    assert_eq!(eval("(number-to-string 2.0)"), "\"2.0\"");
    assert_eq!(eval("(prin1-to-string 1e20)"), "\"1e+20\"");
    // pcase (cl-type …) pattern and pcase-exhaustive.
    assert_eq!(eval("(pcase 5 ((cl-type integer) 'int) (_ 'other))"), "int");
    assert_eq!(
        eval("(pcase \"x\" ((cl-type string) 'str) ((cl-type integer) 'int))"),
        "str"
    );
    assert_eq!(eval("(pcase-exhaustive 1 (1 'one) (2 'two))"), "one");
}

#[test]
fn emacs_parity_pcase_app_and_setf_places() {
    // pcase (app FN PAT): match PAT against (FN value); FN may be a lambda.
    assert_eq!(eval("(pcase 5 ((app 1+ 6) 'yes))"), "yes");
    assert_eq!(eval("(pcase (list 1 2) ((app car 1) 'one))"), "one");
    assert_eq!(
        eval("(pcase 10 ((and x (app (lambda (n) (* n 2)) y)) (list x y)))"),
        "(10 20)"
    );
    // pred with a lambda now works too (same apply rule).
    assert_eq!(eval("(pcase 3 ((pred (lambda (n) (> n 1))) 'big))"), "big");
    // setf on alist-get: setcdr existing, prepend new.
    assert_eq!(
        eval("(let ((a (list (cons 1 2)))) (setf (alist-get 1 a) 99) a)"),
        "((1 . 99))"
    );
    assert_eq!(
        eval("(let ((a nil)) (setf (alist-get 'x a) 5) a)"),
        "((x . 5))"
    );
    assert_eq!(
        eval("(let ((a (list (cons 1 2)))) (setf (alist-get 9 a) 7) a)"),
        "((9 . 7) (1 . 2))"
    );
    // setf on plist-get: set existing cell, else prepend (K V) (Emacs order).
    assert_eq!(
        eval("(let ((p (list :a 1))) (setf (plist-get p :a) 9) p)"),
        "(:a 9)"
    );
    assert_eq!(
        eval("(let ((p (list :a 1))) (setf (plist-get p :b) 2) p)"),
        "(:b 2 :a 1)"
    );
    // cl-typep.
    assert_eq!(eval("(cl-typep 5 'integer)"), "t");
    assert_eq!(eval("(cl-typep \"x\" 'string)"), "t");
    assert_eq!(eval("(cl-typep 5 'string)"), "nil");
}

#[test]
fn emacs_parity_cl_count_if_and_string_fill() {
    assert_eq!(eval("(cl-count-if #'cl-oddp '(1 2 3 4 5))"), "3");
    assert_eq!(eval("(cl-count-if-not #'cl-oddp '(1 2 3 4 5))"), "2");
    assert_eq!(eval("(cl-position-if #'cl-evenp '(1 2 3 4))"), "1");
    assert_eq!(eval("(cl-position-if-not #'cl-evenp '(1 2 3 4))"), "0");
    assert_eq!(eval("(string-fill \"a b c d\" 3)"), "\"a b\nc d\"");
    assert_eq!(
        eval("(string-fill \"one two three\" 100)"),
        "\"one two three\""
    );
    // string-limit: first/last N chars, whole string when short enough.
    assert_eq!(eval("(string-limit \"abcdef\" 3)"), "\"abc\"");
    assert_eq!(eval("(string-limit \"abcdef\" 3 t)"), "\"def\"");
    assert_eq!(eval("(string-limit \"abcdef\" 10)"), "\"abcdef\"");
    assert_eq!(eval("(string-limit \"abcdef\" 0)"), "\"\"");
}

#[test]
fn cl_letf_function_cell_and_hash_empty() {
    // cl-letf / cl-letf* can temporarily rebind a function cell and restore it.
    assert_eq!(
        eval("(cl-letf (((symbol-function 'foo) (lambda () 1))) (foo))"),
        "1"
    );
    assert_eq!(
        eval("(cl-letf* (((symbol-function 'foo) (lambda () 42))) (foo))"),
        "42"
    );
    assert_eq!(
        eval("(progn (defun bar () 1) (cl-letf (((symbol-function 'bar) (lambda () 2))) (bar)) (bar))"),
        "1"
    );
    // hash-table-empty-p
    assert_eq!(eval("(hash-table-empty-p (make-hash-table))"), "t");
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'k 1 h) (hash-table-empty-p h))"),
        "nil"
    );
}

#[test]
fn plist_optional_predicate() {
    // plist-get/member/put accept the Emacs-29 optional PREDICATE (default eq).
    assert_eq!(eval("(plist-get '(\"a\" 1 \"b\" 2) \"b\" #'equal)"), "2");
    assert_eq!(eval("(plist-member '(\"a\" 1) \"a\" #'equal)"), "(\"a\" 1)");
    assert_eq!(eval("(plist-put '(\"a\" 1) \"a\" 9 #'equal)"), "(\"a\" 9)");
    // default behavior unchanged
    assert_eq!(eval("(plist-get '(:a 1 :b 2) :b)"), "2");
    assert_eq!(eval("(plist-put (list :a 1) :b 2)"), "(:a 1 :b 2)");
}

#[test]
fn cl_loop_vconcat_concat_and_string_replace_empty() {
    // cl-loop vconcat/concat accumulation clauses (+ `into`).
    assert_eq!(
        eval("(cl-loop for i below 3 vconcat (vector i))"),
        "[0 1 2]"
    );
    assert_eq!(
        eval("(cl-loop for s in '(\"a\" \"b\" \"c\") concat s)"),
        "\"abc\""
    );
    assert_eq!(
        eval("(cl-loop for s in '(\"a\" \"b\") concat s into r finally return r)"),
        "\"ab\""
    );
    assert_eq!(
        eval("(cl-loop for i to 2 vconcat (vector i i))"),
        "[0 0 1 1 2 2]"
    );
    // string-replace signals on an empty FROMSTRING (matches Emacs).
    assert!(eval_str("(string-replace \"\" \"x\" \"ab\")").is_err());
    assert_eq!(
        eval("(string-replace \"a\" \"X\" \"banana\")"),
        "\"bXnXnX\""
    );
}

#[test]
fn time_functions_utc() {
    // Deterministic UTC formatting (ZONE = t) against known epochs.
    assert_eq!(
        eval("(format-time-string \"%Y-%m-%d %H:%M:%S\" 0 t)"),
        "\"1970-01-01 00:00:00\""
    );
    assert_eq!(
        eval("(format-time-string \"%A %B %e, %Y\" 0 t)"),
        "\"Thursday January  1, 1970\""
    );
    assert_eq!(
        eval("(format-time-string \"%F %T %z\" 1700000000 t)"),
        "\"2023-11-14 22:13:20 +0000\""
    );
    assert_eq!(
        eval("(format-time-string \"%I:%M %p\" 0 t)"),
        "\"12:00 AM\""
    );
    assert_eq!(
        eval("(format-time-string \"%j (%a)\" 0 t)"),
        "\"001 (Thu)\""
    );
    assert_eq!(
        eval("(current-time-string 0 t)"),
        "\"Thu Jan  1 00:00:00 1970\""
    );
    assert_eq!(eval("(decode-time 0 t)"), "(0 0 0 1 1 1970 4 nil 0)");
    assert_eq!(eval("(float-time 5)"), "5.0");
    // fixed numeric ZONE offset (seconds east of UTC)
    assert_eq!(
        eval("(format-time-string \"%H:%M %z\" 0 3600)"),
        "\"01:00 +0100\""
    );
    // current-time and float-time agree on "now" (within a couple seconds).
    assert_eq!(
        eval("(< (abs (- (float-time (current-time)) (float-time))) 5)"),
        "t"
    );
    // encode-time: inverse of decode, both calling conventions, UTC.
    assert_eq!(
        eval("(encode-time (list 0 0 0 1 1 1970 nil nil t))"),
        "(0 0)"
    );
    assert_eq!(
        eval("(float-time (encode-time (list 0 0 0 1 1 2024 nil nil t)))"),
        "1704067200.0"
    );
    assert_eq!(
        eval("(format-time-string \"%F %T\" (encode-time 0 0 12 1 1 2000 t) t)"),
        "\"2000-01-01 12:00:00\""
    );
    // fixed-offset components: +0100 stated time is one hour earlier in UTC.
    assert_eq!(
        eval("(format-time-string \"%F\" (encode-time (list 0 0 0 1 1 1970 nil nil 3600)) t)"),
        "\"1969-12-31\""
    );
}

#[test]
fn safe_length_and_struct_copier() {
    // safe-length counts cons cells, stopping at an improper tail.
    assert_eq!(eval("(safe-length '(1 2 . 3))"), "2");
    assert_eq!(eval("(safe-length '(1 2 3))"), "3");
    assert_eq!(eval("(safe-length nil)"), "0");
    assert_eq!(eval("(safe-length 5)"), "0");
    // cl-defstruct (:copier NAME) and (:copier nil).
    assert_eq!(
        eval("(progn (cl-defstruct (q (:constructor mq) (:copier cq)) v) (q-v (cq (mq :v 7))))"),
        "7"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (r (:copier nil)) v) (fboundp 'copy-r))"),
        "nil"
    );
    assert_eq!(
        eval("(progn (cl-defstruct s2 v) (s2-v (copy-s2 (make-s2 :v 4))))"),
        "4"
    );
}

#[test]
fn cl_defstruct_include_and_predicate() {
    // :include inherits parent slots (prepended) and accessors line up.
    assert_eq!(
        eval("(progn (cl-defstruct animal name) (cl-defstruct (dog (:include animal)) breed) (animal-name (make-dog :name \"Rex\" :breed \"Lab\")))"),
        "\"Rex\""
    );
    // Subtype satisfies the parent predicate; a parent is not the subtype.
    assert_eq!(
        eval("(progn (cl-defstruct an3 name) (cl-defstruct (cat (:include an3)) c) (list (an3-p (make-cat)) (cat-p (make-an3))))"),
        "(t nil)"
    );
    // Multi-level inheritance: predicates and inherited slots both work.
    assert_eq!(
        eval("(progn (cl-defstruct an5 n) (cl-defstruct (b5 (:include an5)) x) (cl-defstruct (c5 (:include b5)) y) (list (an5-p (make-c5)) (b5-p (make-c5)) (c5-n (make-c5 :n 1 :x 2 :y 3))))"),
        "(t t 1)"
    );
    // :predicate renames the predicate.
    assert_eq!(
        eval("(progn (cl-defstruct (p3 (:predicate is-p3)) x) (is-p3 (make-p3)))"),
        "t"
    );
}

#[test]
fn cl_defstruct_boa_constructor() {
    // BOA (positional) constructor: arg order = ARGLIST, not slot order.
    assert_eq!(
        eval("(progn (cl-defstruct (v5 (:constructor nv5 (y x))) x y) (list (v5-x (nv5 100 200)) (v5-y (nv5 100 200))))"),
        "(200 100)"
    );
    // &optional, and slots absent from ARGLIST take their default.
    assert_eq!(
        eval("(progn (cl-defstruct (v6 (:constructor nv6 (a))) (x 9) a) (list (v6-x (nv6 5)) (v6-a (nv6 5))))"),
        "(9 5)"
    );
    // &rest, plus a BOA and a keyword constructor coexisting.
    assert_eq!(
        eval("(progn (cl-defstruct (v7 (:constructor nv7 (&rest xs)) (:constructor mk-v7)) xs) (list (v7-xs (nv7 1 2 3)) (v7-xs (mk-v7 :xs '(9)))))"),
        "((1 2 3) (9))"
    );
    // (:constructor nil) suppresses the default; another constructor still works.
    assert_eq!(
        eval("(progn (cl-defstruct (v9 (:constructor nil) (:constructor nv9 (a))) a) (list (fboundp 'make-v9) (v9-a (nv9 7))))"),
        "(nil 7)"
    );
}

#[test]
fn cl_defun_and_destructuring_keys() {
    // cl-defun with &key (defaults), &optional defaults, &rest.
    assert_eq!(
        eval("(progn (cl-defun f2 (a &key (b 10)) (list a b)) (f2 1 :b 2))"),
        "(1 2)"
    );
    assert_eq!(
        eval("(progn (cl-defun f3 (a &key (b 10)) (list a b)) (f3 1))"),
        "(1 10)"
    );
    assert_eq!(eval("(progn (cl-defun f4 (&optional (x 5)) x) (f4))"), "5");
    assert_eq!(
        eval("(progn (cl-defun f7 (a &optional (b (* a 2))) (list a b)) (f7 5))"),
        "(5 10)"
    );
    assert_eq!(
        eval("(progn (cl-defun f5 (a &rest r) (list a r)) (f5 1 2 3))"),
        "(1 (2 3))"
    );
    // cl-destructuring-bind with real &optional/&key defaults.
    assert_eq!(
        eval("(cl-destructuring-bind (a &optional (b 9)) '(1) (list a b))"),
        "(1 9)"
    );
    assert_eq!(
        eval("(cl-destructuring-bind (a &key (b 9) c) '(1 :c 3) (list a b c))"),
        "(1 9 3)"
    );
    // multiple values are lists.
    assert_eq!(
        eval("(cl-multiple-value-bind (a b) (list 1 2) (+ a b))"),
        "3"
    );
    assert_eq!(eval("(cl-values 1 2 3)"), "(1 2 3)");
}

#[test]
fn assoc_delete_all_and_completion() {
    // assoc-delete-all removes every matching entry (default `equal` test).
    assert_eq!(
        eval("(assoc-delete-all \"a\" (list (cons \"a\" 1) (cons \"b\" 2)))"),
        "((\"b\" . 2))"
    );
    assert_eq!(
        eval("(assoc-delete-all 2 (list (cons 1 'x) (cons 2 'y) (cons 2 'z)))"),
        "((1 . x))"
    );
    // completion API over lists and alists.
    assert_eq!(eval("(try-completion \"foo\" '(\"foo\"))"), "t");
    assert_eq!(
        eval("(try-completion \"fo\" '(\"foo\" \"foobar\"))"),
        "\"foo\""
    );
    assert_eq!(eval("(try-completion \"x\" '(\"foo\"))"), "nil");
    assert_eq!(
        eval("(try-completion \"fo\" '((\"foo\" . 1) (\"fox\" . 2)))"),
        "\"fo\""
    );
    assert_eq!(
        eval("(all-completions \"fo\" '(\"foo\" \"foobar\" \"baz\"))"),
        "(\"foo\" \"foobar\")"
    );
    assert_eq!(eval("(test-completion \"foo\" '(\"foo\" \"bar\"))"), "t");
    assert_eq!(eval("(test-completion \"fo\" '(\"foo\"))"), "nil");
    // predicate filters candidates.
    assert_eq!(
        eval("(all-completions \"fo\" '(\"foo\" \"fox\") (lambda (e) (string= e \"foo\")))"),
        "(\"foo\")"
    );
}

#[test]
fn with_output_to_string_captures() {
    // princ / prin1 / terpri output is captured into the returned string.
    assert_eq!(
        eval("(with-output-to-string (princ \"hi\") (princ 42))"),
        "\"hi42\""
    );
    assert_eq!(
        eval("(with-output-to-string (prin1 '(1 2)) (terpri) (princ \"x\"))"),
        "\"(1 2)\nx\""
    );
    assert_eq!(eval("(with-output-to-string)"), "\"\"");
    // Nested captures are independent (inner output doesn't reach the outer).
    assert_eq!(
        eval("(with-output-to-string (princ \"a\") (with-output-to-string (princ \"in\")) (princ \"b\"))"),
        "\"ab\""
    );
    // An error inside still pops the capture (no leak): the next capture is clean.
    assert_eq!(
        eval("(progn (ignore-errors (with-output-to-string (princ \"x\") (error \"boom\"))) (with-output-to-string (princ \"ok\")))"),
        "\"ok\""
    );
}

#[test]
fn emacs_parity_cl_math_family() {
    // Two-value division returns (QUOTIENT REMAINDER).
    assert_eq!(eval("(cl-floor 7 2)"), "(3 1)");
    assert_eq!(eval("(cl-ceiling 7 2)"), "(4 -1)");
    assert_eq!(eval("(cl-truncate -7 2)"), "(-3 -1)");
    assert_eq!(eval("(cl-round 5 2)"), "(2 1)");
    assert_eq!(eval("(cl-mod 7 3)"), "1");
    assert_eq!(eval("(cl-rem -7 3)"), "-1");
    assert_eq!(eval("(cl-gcd 12 18 8)"), "2");
    assert_eq!(eval("(cl-gcd)"), "0");
    assert_eq!(eval("(cl-lcm 4 6 10)"), "60");
    assert_eq!(eval("(cl-lcm)"), "1");
    assert_eq!(eval("(cl-lcm 0 5)"), "0");
    assert_eq!(eval("(cl-isqrt 17)"), "4");
    // cl-oddp must hold for negative odds.
    assert_eq!(eval("(cl-oddp -3)"), "t");
    assert_eq!(eval("(cl-evenp -4)"), "t");
}

#[test]
fn emacs_parity_cl_set_and_seq_ops() {
    assert_eq!(eval("(cl-subst 9 2 '(1 (2 3) 2))"), "(1 (9 3) 9)");
    assert_eq!(eval("(cl-delete-duplicates (list 1 2 1 3))"), "(2 1 3)");
    assert_eq!(eval("(cl-maplist #'car '(1 2 3))"), "(1 2 3)");
    // :from-end folds right; :key maps before folding.
    assert_eq!(eval("(cl-reduce #'- '(1 2 3) :from-end t)"), "2");
    assert_eq!(
        eval("(cl-reduce #'cons '(1 2 3) :from-end t :initial-value 9)"),
        "(1 2 3 . 9)"
    );
    assert_eq!(eval("(cl-reduce #'+ '(1 2 3) :key #'1+)"), "9");
    assert_eq!(eval("(cl-merge 'list '(1 3) '(2 4) #'<)"), "(1 2 3 4)");
    assert_eq!(eval("(cl-merge 'vector '(1 3) '(2 4) #'<)"), "[1 2 3 4]");
    assert_eq!(eval("(cl-set-difference '(1 2 3 4 5) '(2 4))"), "(1 3 5)");
    assert_eq!(eval("(cl-union '(1 2) '(2 3))"), "(3 1 2)");
    assert_eq!(eval("(cl-intersection '(1 2 3) '(2 3 4))"), "(3 2)");
    assert_eq!(eval("(cl-adjoin 1 '(2 3))"), "(1 2 3)");
    assert_eq!(eval("(cl-adjoin 2 '(2 3))"), "(2 3)");
    assert_eq!(eval("(cl-endp nil)"), "t");
}

#[test]
fn emacs_parity_split_regexp_and_cl_bounds() {
    // split-string treats SEPARATORS as a regexp.
    assert_eq!(
        eval("(split-string \"a1b2c\" \"[0-9]\")"),
        "(\"a\" \"b\" \"c\")"
    );
    assert_eq!(
        eval("(split-string \"aXXbXc\" \"X+\")"),
        "(\"a\" \"b\" \"c\")"
    );
    // cl bounding keywords.
    assert_eq!(
        eval("(cl-remove-if #'cl-oddp '(1 2 3 4) :count 1)"),
        "(2 3 4)"
    );
    assert_eq!(eval("(cl-position 3 '(1 2 3 4 3) :start 3)"), "4");
    assert_eq!(eval("(cl-count 2 '(1 2 2 3 2) :start 2)"), "2");
    assert_eq!(eval("(cl-count 2 '(1 2 2 3 2) :end 2)"), "1");
    assert_eq!(
        eval("(cl-count-if #'cl-oddp '(1 2 3 4 5) :start 1 :end 4)"),
        "1"
    );
    // format-message curve-quotes the format string.
    assert_eq!(
        eval("(format-message \"use `%s'\" \"x\")"),
        "\"use \u{2018}x\u{2019}\""
    );
    // string-version-lessp compares numeric runs numerically.
    assert_eq!(eval("(string-version-lessp \"foo2\" \"foo10\")"), "t");
    assert_eq!(eval("(string-version-lessp \"foo10\" \"foo2\")"), "nil");
    assert_eq!(eval("(string-version-lessp \"1.2.3\" \"1.10.0\")"), "t");
}

#[test]
fn emacs_parity_cl_macros_do_typecase_loop_destructure() {
    assert_eq!(eval("(cl-the integer 5)"), "5");
    assert_eq!(eval("(cl-etypecase \"x\" (integer 'i) (string 's))"), "s");
    assert_eq!(eval("(cl-ecase 2 (1 'a) (2 'b))"), "b");
    // cl-do steps in parallel from the previous iteration's values.
    assert_eq!(
        eval("(cl-do ((i 0 (1+ i)) (s 0 (+ s i))) ((= i 4) s))"),
        "6"
    );
    assert_eq!(
        eval("(cl-do ((i 0 (1+ i)) (j 10 (1- j))) ((= i 3) (list i j)))"),
        "(3 7)"
    );
    // cl-loop destructuring `for PATTERN in', including dotted patterns.
    assert_eq!(
        eval("(cl-loop for (a b) in '((1 2) (3 4)) collect (+ a b))"),
        "(3 7)"
    );
    assert_eq!(
        eval("(cl-loop for (k . v) in '((a . 1) (b . 2)) collect v)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(cl-destructuring-bind (a . b) '(1 2 3) (list a b))"),
        "(1 (2 3))"
    );
}

#[test]
fn emacs_parity_type_and_width_fns() {
    // cl-type-of refines integers to fixnum and nil to null.
    assert_eq!(eval("(cl-type-of 5)"), "fixnum");
    assert_eq!(eval("(cl-type-of nil)"), "null");
    assert_eq!(eval("(cl-type-of (list 1))"), "cons");
    assert_eq!(eval("(number-or-marker-p 5)"), "t");
    assert_eq!(eval("(integer-or-marker-p 5)"), "t");
    assert_eq!(
        eval("(subst-char-in-string ?a ?X \"banana\")"),
        "\"bXnXnX\""
    );
    assert_eq!(eval("(string-bytes \"h\u{e9}llo\")"), "6");
    // Display width: CJK ideographs count as 2 columns.
    assert_eq!(eval("(string-width \"abc\")"), "3");
    assert_eq!(eval("(string-width \"\u{65e5}\u{672c}\u{8a9e}\")"), "6");
    assert_eq!(eval("(char-width ?\u{65e5})"), "2");
    assert_eq!(eval("(truncate-string-to-width \"hello\" 3)"), "\"hel\"");
    assert_eq!(
        eval("(truncate-string-to-width \"\u{65e5}\u{672c}\u{8a9e}\" 4)"),
        "\"\u{65e5}\u{672c}\""
    );
}

#[test]
fn emacs_parity_format_g() {
    // %g switches to exponent form when the decimal exponent >= precision (default 6)
    // or < -4, and trims trailing zeros unless the # flag is set.
    assert_eq!(eval("(format \"%g\" 1000000.0)"), "\"1e+06\"");
    assert_eq!(eval("(format \"%g\" 100000.0)"), "\"100000\"");
    assert_eq!(eval("(format \"%g\" 0.00001)"), "\"1e-05\"");
    assert_eq!(eval("(format \"%.3g\" 3.14159)"), "\"3.14\"");
    assert_eq!(eval("(format \"%g\" 1234567.0)"), "\"1.23457e+06\"");
    assert_eq!(eval("(format \"%g\" 0.0)"), "\"0\"");
    assert_eq!(eval("(format \"%g\" 10.0)"), "\"10\"");
    assert_eq!(eval("(format \"%#g\" 1.5)"), "\"1.50000\"");
    assert_eq!(eval("(format \"%g\" -0.00001)"), "\"-1e-05\"");
    assert_eq!(eval("(format \"%10.3g\" 3.14159)"), "\"      3.14\"");
}

#[test]
fn emacs_parity_sort_keyword_and_defstruct_options() {
    // (sort SEQ) with no predicate uses the default value< ordering (was a panic).
    assert_eq!(eval("(sort (list 3 1 2))"), "(1 2 3)");
    assert_eq!(eval("(sort (vector 3 1 2))"), "[1 2 3]");
    assert_eq!(
        eval("(sort (list \"c\" \"a\" \"b\"))"),
        "(\"a\" \"b\" \"c\")"
    );
    assert_eq!(eval("(sort (list 'z 'a 'm))"), "(a m z)");
    // Emacs-30 keyword form.
    assert_eq!(eval("(sort (list 3 1 2) :key #'- :lessp #'<)"), "(3 2 1)");
    assert_eq!(
        eval("(sort (list \"bb\" \"a\" \"ccc\") :key #'length)"),
        "(\"a\" \"bb\" \"ccc\")"
    );
    assert_eq!(eval("(sort (list 3 1 2) :reverse t)"), "(3 2 1)");
    // cl-defstruct (:constructor NAME) / (:conc-name P).
    assert_eq!(
        eval("(progn (cl-defstruct (pt3 (:constructor mk)) a) (pt3-a (mk :a 5)))"),
        "5"
    );
    assert_eq!(
        eval("(progn (cl-defstruct (pt4 (:conc-name p4/)) x) (p4/x (make-pt4 :x 7)))"),
        "7"
    );
}

#[test]
fn emacs_parity_seq_more() {
    assert_eq!(eval("(seq-sort-by #'- #'< '(1 3 2))"), "(3 2 1)");
    assert_eq!(eval("(seq-mapcat #'list '(1 2) 'list)"), "(1 2)");
    assert_eq!(
        eval("(seq-mapcat (lambda (x) (list x x)) '(1 2))"),
        "(1 1 2 2)"
    );
    assert_eq!(eval("(seq-remove-at-position '(a b c) 1)"), "(a c)");
    assert_eq!(eval("(seq-remove-at-position [a b c] 0)"), "[b c]");
    assert_eq!(eval("(seq-split '(1 2 3 4 5) 2)"), "((1 2) (3 4) (5))");
    assert_eq!(eval("(seq-positions '(1 2 1 3) 1)"), "(0 2)");
    // seq-partition preserves the element type (vector in -> vector chunks).
    assert_eq!(eval("(seq-partition [1 2 3 4 5] 2)"), "([1 2] [3 4] [5])");
}

#[test]
fn emacs_parity_float_math() {
    assert_eq!(eval("(log 100 10)"), "2.0");
    assert_eq!(eval("(log 8 2)"), "3.0");
    assert_eq!(eval("(exp 0)"), "1.0");
    assert_eq!(eval("(sin 0)"), "0.0");
    assert_eq!(eval("(cos 0)"), "1.0");
    assert_eq!(eval("(atan 1)"), "0.7853981633974483");
    assert_eq!(eval("(atan 1 1)"), "0.7853981633974483");
    assert_eq!(eval("(ldexp 1.5 3)"), "12.0");
    assert_eq!(eval("(frexp 8.0)"), "(0.5 . 4)");
    assert_eq!(eval("(copysign 3.0 -1.0)"), "-3.0");
    assert_eq!(eval("(cl-parse-integer \"42\")"), "42");
    assert_eq!(eval("(cl-parse-integer \"ff\" :radix 16)"), "255");
}

#[test]
fn emacs_parity_cl_list_and_plist_ops() {
    // :from-end keeps the first occurrence; default keeps the last.
    assert_eq!(
        eval("(cl-remove-duplicates '(1 2 1 3) :from-end t)"),
        "(1 2 3)"
    );
    assert_eq!(eval("(cl-remove-duplicates '(1 2 1 3))"), "(2 1 3)");
    assert_eq!(eval("(cl-pairlis '(a b) '(1 2))"), "((a . 1) (b . 2))");
    assert_eq!(eval("(lax-plist-get '(\"a\" 1 \"b\" 2) \"b\")"), "2");
    // cl-tailp / cl-ldiff key off eq identity of the tail.
    assert_eq!(eval("(cl-tailp '(3) (cddr '(1 2 3)))"), "nil");
    assert_eq!(eval("(let ((l (list 1 2 3))) (cl-tailp (cddr l) l))"), "t");
    assert_eq!(eval("(cl-ldiff '(1 2 3 4) '(3 4))"), "(1 2 3 4)");
    assert_eq!(
        eval("(let ((l (list 1 2 3 4))) (cl-ldiff l (cddr l)))"),
        "(1 2)"
    );
}

#[test]
fn emacs_parity_char_equal_assert_format_spec() {
    // char-equal folds case when case-fold-search (default t) is non-nil.
    assert_eq!(eval("(char-equal ?a ?A)"), "t");
    assert_eq!(eval("(char-equal ?a ?b)"), "nil");
    assert_eq!(
        eval("(let ((case-fold-search nil)) (char-equal ?a ?A))"),
        "nil"
    );
    // cl-assert returns nil on success, signals cl-assertion-failed (a subtype of error).
    assert_eq!(eval("(cl-assert (= 1 1))"), "nil");
    assert_eq!(
        eval("(condition-case e (cl-assert (= 1 2)) (error (car e)))"),
        "cl-assertion-failed"
    );
    // cl-check-type returns nil when the type matches, else signals wrong-type-argument.
    assert_eq!(eval("(cl-check-type 5 integer)"), "nil");
    assert_eq!(
        eval("(condition-case e (cl-check-type 5 string) (wrong-type-argument (car e)))"),
        "wrong-type-argument"
    );
    // format-spec.
    assert_eq!(
        eval("(format-spec \"%a-%b\" '((?a . \"X\") (?b . \"Y\")))"),
        "\"X-Y\""
    );
    assert_eq!(eval("(format-spec \"100%%\" nil)"), "\"100%\"");
}

#[test]
fn emacs_parity_cl_defmethod_dispatch() {
    // Single type dispatch.
    assert_eq!(
        eval("(progn (cl-defgeneric area (s)) (cl-defmethod area ((s integer)) (* s s)) (area 4))"),
        "16"
    );
    // Disjoint-type dispatch.
    assert_eq!(eval("(progn (cl-defmethod foo ((x string)) 'str) (cl-defmethod foo ((x integer)) 'int) (list (foo \"a\") (foo 5)))"), "(str int)");
    // Specificity: integer beats number, eql beats integer.
    assert_eq!(eval("(progn (cl-defmethod bar ((x number)) 'num) (cl-defmethod bar ((x integer)) 'int) (list (bar 5) (bar 1.5)))"), "(int num)");
    assert_eq!(eval("(progn (cl-defmethod baz ((x (eql 0))) 'zero) (cl-defmethod baz ((x integer)) 'other) (list (baz 0) (baz 7)))"), "(zero other)");
    // Multi-arg dispatch and unspecialized args.
    assert_eq!(
        eval("(progn (cl-defmethod add2 ((a integer) (b integer)) (+ a b)) (add2 3 4))"),
        "7"
    );
    assert_eq!(eval("(progn (cl-defmethod dft (x) 'fallback) (cl-defmethod dft ((x string)) 'str) (list (dft 5) (dft \"a\")))"), "(fallback str)");
    // No applicable method signals cl-no-applicable-method; redefinition replaces.
    assert_eq!(eval("(condition-case e (progn (cl-defgeneric none (x)) (none 5)) (cl-no-applicable-method 'no))"), "no");
    assert_eq!(eval("(progn (cl-defmethod redef ((x integer)) 'v1) (cl-defmethod redef ((x integer)) 'v2) (redef 3))"), "v2");
}

#[test]
fn emacs_parity_cl_method_combination() {
    // :before must not clobber the primary (different combination, same specs).
    assert_eq!(eval("(progn (cl-defmethod q ((x integer)) (list 'prim x)) (cl-defmethod q :before ((x integer)) (ignore 'b)) (q 5))"), "(prim 5)");
    // cl-call-next-method chains primaries by specificity.
    assert_eq!(eval("(progn (cl-defmethod cnm ((x integer)) (cons 'int (cl-call-next-method))) (cl-defmethod cnm ((x number)) (list 'num x)) (cnm 3))"), "(int num 3)");
    assert_eq!(eval("(progn (cl-defmethod m3 ((x integer)) (cons 1 (cl-call-next-method))) (cl-defmethod m3 ((x number)) (cons 2 (cl-call-next-method))) (cl-defmethod m3 ((x t)) '(3)) (m3 5))"), "(1 2 3)");
    // cl-next-method-p.
    assert_eq!(eval("(progn (cl-defmethod nm ((x integer)) (if (cl-next-method-p) 'has 'no)) (cl-defmethod nm ((x number)) 'base) (nm 5))"), "has");
    // :before / :after run around the primary in order.
    assert_eq!(eval("(let ((log nil)) (cl-defmethod ord ((x integer)) (push 'prim log)) (cl-defmethod ord :before ((x integer)) (push 'before log)) (cl-defmethod ord :after ((x integer)) (push 'after log)) (ord 1) (reverse log))"), "(before prim after)");
    // :around wraps the core.
    assert_eq!(eval("(progn (cl-defmethod ar ((x integer)) 'core) (cl-defmethod ar :around ((x integer)) (list 'around (cl-call-next-method))) (ar 1))"), "(around core)");
}

#[test]
fn emacs_parity_cl_coerce_gensym_digit() {
    assert_eq!(eval("(cl-coerce '(1 2) 'vector)"), "[1 2]");
    assert_eq!(eval("(cl-coerce \"abc\" 'list)"), "(97 98 99)");
    assert_eq!(eval("(cl-coerce 5 'float)"), "5.0");
    assert_eq!(eval("(cl-coerce '(?a ?b) 'string)"), "\"ab\"");
    assert_eq!(eval("(cl-digit-char-p ?7)"), "7");
    assert_eq!(eval("(cl-digit-char-p ?a 16)"), "10");
    assert_eq!(eval("(cl-digit-char-p ?a)"), "nil");
}

#[test]
fn emacs_parity_read_from_string_and_seq_fixes() {
    // read-from-string returns (OBJECT . END-INDEX), honoring START.
    assert_eq!(eval("(read-from-string \"(a b)\")"), "((a b) . 5)");
    assert_eq!(eval("(read-from-string \"42 foo\")"), "(42 . 2)");
    assert_eq!(eval("(read-from-string \"(1 2) (3 4)\" 5)"), "((3 4) . 11)");
    assert_eq!(eval("(pp-to-string '(1 2))"), "\"(1 2)\n\"");
    // seq-contains-p works on strings/vectors; remove preserves the sequence type.
    assert_eq!(eval("(seq-contains-p \"abc\" ?b)"), "t");
    assert_eq!(eval("(remove 3 [1 2 3])"), "[1 2]");
    assert_eq!(eval("(multibyte-string-p \"abc\")"), "nil");
    assert_eq!(eval("(multibyte-string-p \"h\u{e9}llo\")"), "t");
    // cl-substitute-if / cl-mapcan.
    assert_eq!(
        eval("(cl-substitute-if 0 #'cl-evenp [1 2 3 4])"),
        "[1 0 3 0]"
    );
    assert_eq!(eval("(cl-mapcan #'list '(1 2) '(3 4))"), "(1 3 2 4)");
}

#[test]
fn emacs_parity_cl_loop_more_clauses() {
    // across iterates strings/vectors; being the elements/hash-keys/hash-values of.
    assert_eq!(
        eval("(cl-loop for x across \"abc\" collect x)"),
        "(97 98 99)"
    );
    assert_eq!(
        eval("(cl-loop for x being the elements of (list 1 2) collect x)"),
        "(1 2)"
    );
    assert_eq!(eval("(cl-loop for k being the hash-keys of (let ((h (make-hash-table))) (puthash 'a 1 h) h) collect k)"), "(a)");
    // when ... return; named NAME accepted.
    assert_eq!(
        eval("(cl-loop for i from 1 to 5 when (= i 3) return 'found)"),
        "found"
    );
    assert_eq!(
        eval("(cl-loop named outer for i from 1 to 3 collect i)"),
        "(1 2 3)"
    );
    // for V = INIT then STEP, including interaction with until/while.
    assert_eq!(
        eval("(cl-loop repeat 3 for x = 1 then (1+ x) collect x)"),
        "(1 2 3)"
    );
    assert_eq!(
        eval("(cl-loop for x = 5 then (1- x) until (= x 0) collect x)"),
        "(5 4 3 2 1)"
    );
    assert_eq!(
        eval("(cl-loop for x = 2 then (* x x) while (< x 100) collect x)"),
        "(2 4 16)"
    );
}

#[test]
fn emacs_parity_pcase_let_star_dolist_seq() {
    assert_eq!(
        eval("(pcase-let* ((`(,a ,b) '(1 2)) (`(,c) '(3))) (+ a b c))"),
        "6"
    );
    // pcase-let* bindings are sequential (later sees earlier).
    assert_eq!(
        eval("(pcase-let* ((`(,a ,b) '(1 2)) (`(,c) (list (+ a b)))) c)"),
        "3"
    );
    // seq pattern on list/vector/string.
    assert_eq!(eval("(pcase '(1 2) ((seq a b) (+ a b)))"), "3");
    assert_eq!(
        eval("(pcase [1 2 3] ((seq a b c) (list a b c)))"),
        "(1 2 3)"
    );
    assert_eq!(eval("(pcase \"ab\" ((seq x y) (list x y)))"), "(97 98)");
    // pcase-dolist destructures each element.
    assert_eq!(eval("(let ((r nil)) (pcase-dolist (`(,k ,v) '((a 1) (b 2))) (push (cons k v) r)) (reverse r))"), "((a . 1) (b . 2))");
}

#[test]
fn emacs_parity_backquote_vector_and_pcase() {
    // Backquoted vector templates evaluate (including ,@ splicing).
    assert_eq!(eval("(let ((a 1) (b 2)) `[,a ,b])"), "[1 2]");
    assert_eq!(eval("(let ((a 1)) `[,a 99 ,(+ a 1)])"), "[1 99 2]");
    assert_eq!(eval("(let ((c '(3 4))) `[1 ,@c 5])"), "[1 3 4 5]");
    // Vector patterns in pcase, with exact-length matching.
    assert_eq!(eval("(pcase [1 2] (`[,a ,b] (+ a b)))"), "3");
    assert_eq!(
        eval("(pcase [1 2 3] (`[,a ,b ,c] (list c b a)))"),
        "(3 2 1)"
    );
    assert_eq!(eval("(pcase [1 2 3] (`[,a ,b] (list a b)) (_ 'no))"), "no");
    assert_eq!(eval("(pcase 5 (`[,a ,b] 'vec) (_ 'not-vec))"), "not-vec");
}

#[test]
fn emacs_parity_rx_macro() {
    assert_eq!(eval("(rx (+ digit))"), "\"[[:digit:]]+\"");
    assert_eq!(eval("(rx bol \"x\" eol)"), "\"^x$\"");
    assert_eq!(eval("(rx (any \"a-z\"))"), "\"[a-z]\"");
    assert_eq!(
        eval("(rx (or \"cat\" \"dog\"))"),
        "\"\\\\(?:cat\\\\|dog\\\\)\""
    );
    assert_eq!(eval("(rx (or \"a\" \"b\" \"c\"))"), "\"[abc]\"");
    assert_eq!(eval("(rx (group (+ alpha)))"), "\"\\\\([[:alpha:]]+\\\\)\"");
    assert_eq!(eval("(rx (= 3 digit))"), "\"[[:digit:]]\\\\{3\\\\}\"");
    assert_eq!(eval("(rx (not (any \"0-9\")))"), "\"[^0-9]\"");
    assert_eq!(eval("(rx (** 2 4 \"a\"))"), "\"a\\\\{2,4\\\\}\"");
    // rx as a pcase pattern.
    assert_eq!(
        eval("(pcase \"hello\" ((rx \"he\" (+ alpha)) 'match))"),
        "match"
    );
    assert_eq!(
        eval("(pcase \"123\" ((rx bos (+ digit) eos) 'allnum) (_ 'no))"),
        "allnum"
    );
    assert_eq!(
        eval("(pcase \"a1b\" ((rx bos (+ digit) eos) 'allnum) (_ 'no))"),
        "no"
    );
}
