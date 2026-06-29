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
    // Backreferences in the pattern are handled by fancy-regex's backtracking.
    assert_eq!(eval("(string-match \"\\\\(a\\\\)\\\\1\" \"aa\")"), "0");
    // \1 must match the SAME text group 1 captured, not just the same class.
    assert_eq!(eval("(string-match \"\\\\(.\\\\)\\\\1\" \"abccba\")"), "2");
    assert_eq!(
        eval("(replace-regexp-in-string \"\\\\([a-z]\\\\)\\\\1\" \"X\" \"aabbcd\")"),
        "\"XXcd\""
    );
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
fn condition_case_multi_condition_handlers() {
    // A handler may list several conditions; any match selects it.
    assert_eq!(
        eval("(condition-case e (signal 'arith-error '(1)) ((arith-error error) 'caught))"),
        "caught"
    );
    assert_eq!(
        eval("(condition-case e (error \"x\") ((quit error) 'c))"),
        "c"
    );
    // the matching clause is chosen among several.
    assert_eq!(
        eval("(condition-case e (signal 'arith-error nil) ((file-error wrong-type-argument) 'no) (arith-error 'yes))"),
        "yes"
    );
    // single-condition handlers still work.
    assert_eq!(
        eval("(condition-case e (/ 1 0) (arith-error 'div0))"),
        "div0"
    );
    // error-condition inheritance: a define-error subtype is caught by a parent.
    assert_eq!(
        eval("(progn (define-error 'my-err \"My\" 'arith-error) (condition-case e (signal 'my-err nil) (arith-error 'parent)))"),
        "parent"
    );
    assert_eq!(
        eval("(progn (define-error 'my-err2 \"My\" 'arith-error) (condition-case e (signal 'my-err2 nil) (error 'root)))"),
        "root"
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
fn cl_loop_hash_using_values() {
    // `using (hash-values V)` binds the value alongside the key.
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 1 10 h) (puthash 2 20 h) (cl-loop for k being the hash-keys of h using (hash-values v) sum v))"),
        "30"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 'a 1 h) (cl-loop for k being the hash-keys of h using (hash-values v) collect (cons k v)))"),
        "((a . 1))"
    );
    // plain hash-keys / hash-values still work.
    assert_eq!(
        eval("(let ((h (make-hash-table))) (puthash 1 9 h) (cl-loop for v being the hash-values of h collect v))"),
        "(9)"
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
fn cl_whole_and_flet_star() {
    // &whole binds the entire list, then normal destructuring proceeds.
    assert_eq!(
        eval("(cl-destructuring-bind (&whole all a b) '(1 2) (list all a b))"),
        "((1 2) 1 2)"
    );
    assert_eq!(
        eval("(cl-destructuring-bind (&whole w a &rest r) '(1 2 3) (list w a r))"),
        "((1 2 3) 1 (2 3))"
    );
    // cl-flet* is sequential: a later local fn can call an earlier one.
    assert_eq!(
        eval("(cl-flet* ((f (x) (* x 2)) (g (y) (f y))) (g 3))"),
        "6"
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
fn print_escape_newlines_and_reader_control_escapes() {
    // With print-escape-newlines, prin1 of "a<LF>b" yields the 6 chars
    // ?\" ?a ?\\ ?n ?b ?\" — i.e. the newline becomes a 2-char \n escape.
    assert_eq!(
        eval("(let* ((print-escape-newlines t) (s (prin1-to-string \"a\nb\"))) (list (length s) (aref s 2) (aref s 3)))"),
        "(6 92 110)"
    );
    // Default (nil): the newline is printed literally (5 chars, LF at index 2).
    assert_eq!(
        eval("(let* ((s (prin1-to-string \"a\nb\"))) (list (length s) (aref s 2)))"),
        "(5 10)"
    );
    // reader control escapes: \f \a \b \v \d resolve to their control codes.
    assert_eq!(
        eval("(list (aref \"\\f\" 0) (aref \"\\a\" 0) (aref \"\\b\" 0) (aref \"\\v\" 0) (aref \"\\d\" 0))"),
        "(12 7 8 11 127)"
    );
}

#[test]
fn quote_strings_and_charclass_backslash() {
    // combine-and-quote-strings: quote args with spaces/quotes/backslashes.
    assert_eq!(
        eval("(combine-and-quote-strings (list \"a\" \"b c\" \"d\"))"),
        "\"a \\\"b c\\\" d\""
    );
    // round-trip through split-string-and-unquote (incl. an embedded backslash).
    assert_eq!(
        eval("(split-string-and-unquote (combine-and-quote-strings (list \"p\\\\q\" \"r s\")))"),
        "(\"p\\\\q\" \"r s\")"
    );
    assert_eq!(
        eval("(split-string-and-unquote \"a \\\"b c\\\" d\")"),
        "(\"a\" \"b c\" \"d\")"
    );
    // regexp char class treats backslash as a literal member (Emacs semantics).
    assert_eq!(eval("(string-match \"[\\\\\\\"]\" \"a\\\\b\")"), "1");
    assert_eq!(
        eval("(replace-regexp-in-string \"[\\\\\\\"]\" \"X\" \"a\\\\b\\\"c\")"),
        "\"aXbXc\""
    );
}

#[test]
fn json_encode_basic() {
    assert_eq!(eval("(json-encode 5)"), "\"5\"");
    assert_eq!(eval("(json-encode t)"), "\"true\"");
    assert_eq!(eval("(json-encode nil)"), "\"null\"");
    assert_eq!(eval("(json-encode '(1 2 3))"), "\"[1,2,3]\"");
    assert_eq!(eval("(json-encode [1 2 3])"), "\"[1,2,3]\"");
    assert_eq!(
        eval("(json-encode '((a . 1) (b . 2)))"),
        "\"{\\\"a\\\":1,\\\"b\\\":2}\""
    );
    assert_eq!(
        eval("(json-encode '(:a 1 :b 2))"),
        "\"{\\\"a\\\":1,\\\"b\\\":2}\""
    );
    // nested + string escaping.
    assert_eq!(
        eval("(json-encode '((name . \"Bob\") (tags . [1 2])))"),
        "\"{\\\"name\\\":\\\"Bob\\\",\\\"tags\\\":[1,2]}\""
    );
    // ngettext / char-displayable-p
    assert_eq!(eval("(ngettext \"cat\" \"cats\" 1)"), "\"cat\"");
    assert_eq!(eval("(ngettext \"cat\" \"cats\" 2)"), "\"cats\"");
    assert_eq!(eval("(char-displayable-p ?a)"), "t");
}

#[test]
fn json_read_from_string() {
    // Scalars, true/false/null defaults.
    assert_eq!(eval("(json-read-from-string \"5\")"), "5");
    assert_eq!(eval("(json-read-from-string \"-1.5e3\")"), "-1500.0");
    assert_eq!(eval("(json-read-from-string \"true\")"), "t");
    assert_eq!(eval("(json-read-from-string \"false\")"), ":json-false");
    assert_eq!(eval("(json-read-from-string \"null\")"), "nil");
    // arrays → vectors, objects → alists with symbol keys.
    assert_eq!(eval("(json-read-from-string \"[1, 2, 3]\")"), "[1 2 3]");
    assert_eq!(
        eval("(json-read-from-string \"{\\\"a\\\": 1, \\\"b\\\": [2, 3]}\")"),
        "((a . 1) (b . [2 3]))"
    );
    assert_eq!(eval("(json-read-from-string \"[]\")"), "[]");
    assert_eq!(eval("(json-read-from-string \"{}\")"), "nil");
    // nested + a full encode∘decode round-trip.
    assert_eq!(
        eval("(json-encode (json-read-from-string \"{\\\"a\\\":1,\\\"b\\\":[2,3]}\"))"),
        "\"{\\\"a\\\":1,\\\"b\\\":[2,3]}\""
    );
}

#[test]
fn buffer_edit_motion_extras() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (char-before))"),
        "99"
    );
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abcdef\") (goto-char 3) (delete-char -2) (buffer-string))"
        ),
        "\"cdef\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\") (insert-char ?z 3) (buffer-string))"),
        "\"xzzz\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\nb\nc\nd\") (count-lines (point-min) (point-max)))"),
        "4"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\nb\n\") (count-lines (point-min) (point-max)))"),
        "2"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\nb\nc\") (goto-char 4) (line-number-at-pos))"),
        "2"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello\") (goto-char 3) (current-column))"),
        "2"
    );
    // backward search.
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo bar foo\") (goto-char (point-max)) (search-backward \"foo\") (point))"),
        "9"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a1b2c3\") (goto-char (point-max)) (re-search-backward \"[0-9]\") (match-string 0))"),
        "\"3\""
    );
    // char-set skip + word motion.
    assert_eq!(eval("(with-temp-buffer (insert \"abc123\") (goto-char 1) (skip-chars-forward \"a-z\") (point))"), "4");
    assert_eq!(
        eval("(with-temp-buffer (insert \"one two\") (goto-char 1) (forward-word) (point))"),
        "4"
    );
    assert_eq!(eval("(with-temp-buffer (insert \"one two\") (goto-char (point-max)) (backward-word) (point))"), "5");
}

#[test]
fn file_write_read_roundtrip() {
    // target/ is writable and gitignored; one progn does write → read → cleanup.
    assert_eq!(
        eval(
            "(let ((f \"target/elp-fs-test.txt\")) \
               (write-region \"hello\\nworld\" nil f) \
               (prog1 (with-temp-buffer (insert-file-contents f) (buffer-string)) \
                 (delete-file f)))"
        ),
        "\"hello\nworld\""
    );
    // append, then buffer-region write (chars 2..5 of \"abcdef\" = \"bcd\").
    assert_eq!(
        eval(
            "(let ((f \"target/elp-fs-test2.txt\")) \
               (write-region \"X\" nil f) (write-region \"Y\" nil f t) \
               (with-temp-buffer (insert \"abcdef\") (write-region 2 5 f)) \
               (prog1 (with-temp-buffer (insert-file-contents f) (buffer-string)) \
                 (delete-file f)))"
        ),
        "\"bcd\""
    );
}

#[test]
fn buffer_replace_match_and_save_excursion() {
    // replace-match on the whole match and on a subexpression.
    assert_eq!(
        eval("(with-temp-buffer (insert \"foobar\") (goto-char 1) (re-search-forward \"o+\") (replace-match \"X\") (buffer-string))"),
        "\"fXbar\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"Hello\") (goto-char 1) (re-search-forward \"\\\\(l+\\\\)\") (replace-match \"L\" nil nil nil 1) (buffer-string))"),
        "\"HeLo\""
    );
    // template \N group reordering.
    assert_eq!(
        eval("(with-temp-buffer (insert \"cat dog\") (goto-char 1) (re-search-forward \"\\\\(\\\\w+\\\\) \\\\(\\\\w+\\\\)\") (replace-match \"\\\\2 \\\\1\") (buffer-string))"),
        "\"dog cat\""
    );
    // search/replace-all loop.
    assert_eq!(
        eval("(with-temp-buffer (insert \"aXbXc\") (goto-char 1) (while (re-search-forward \"X\" nil t) (replace-match \"-\")) (buffer-string))"),
        "\"a-b-c\""
    );
    // case adaptation (FIXEDCASE nil): uppercase match upcases replacement.
    assert_eq!(
        eval("(with-temp-buffer (insert \"FOO bar\") (goto-char 1) (re-search-forward \"foo\") (replace-match \"x\") (buffer-string))"),
        "\"X bar\""
    );
    // save-excursion restores point.
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) (save-excursion (goto-char 3)) (point))"),
        "1"
    );
}

#[test]
fn buffer_motion_and_search() {
    // line motion + positions.
    assert_eq!(
        eval(
            "(with-temp-buffer (insert \"abc\nzdef\nghi\") (goto-char 1) (forward-line 1) (point))"
        ),
        "5"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\nzdef\") (goto-char 1) (end-of-line) (point))"),
        "4"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"ab\ncd\nef\") (goto-char 1) (list (forward-line 5) (point)))"),
        "(2 9)"
    );
    // literal + regexp search move point and set match data.
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello world\") (goto-char 1) (search-forward \"world\") (point))"),
        "12"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"foo123bar\") (goto-char 1) (re-search-forward \"[0-9]+\") (match-string 0))"),
        "\"123\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"w orld\") (goto-char 1) (re-search-forward \"\\\\(o\\\\)rld\") (match-string 1))"),
        "\"o\""
    );
    // looking-at is anchored at point.
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) (looking-at \"ab\"))"),
        "t"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (looking-at \"ab\"))"),
        "nil"
    );
    // a line-iteration loop with eobp / char-after.
    assert_eq!(
        eval("(with-temp-buffer (insert \"a\nb\nc\") (goto-char 1) (let (ls) (while (not (eobp)) (push (char-after) ls) (forward-line 1)) (nreverse ls)))"),
        "(97 98 99)"
    );
}

#[test]
fn temp_buffer_text_editing() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello\") (buffer-string))"),
        "\"hello\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"ab\") (insert \"cd\") (buffer-string))"),
        "\"abcd\""
    );
    assert_eq!(eval("(with-temp-buffer (insert \"abc\") (point))"), "4");
    // goto-char + insert at a position.
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) (insert \"X\") (buffer-string))"),
        "\"Xabc\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello\") (buffer-substring 2 4))"),
        "\"el\""
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello\") (goto-char 2) (char-after))"),
        "101"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello\") (delete-region 2 4) (buffer-string))"),
        "\"hlo\""
    );
    // insert accepts chars and strings; point-max reflects size.
    assert_eq!(
        eval("(with-temp-buffer (insert ?A ?B \"cd\") (buffer-string))"),
        "\"ABcd\""
    );
    assert_eq!(eval("(with-temp-buffer (insert \"hi\") (point-max))"), "3");
    // insert-file-contents reads a real file (cwd = crate root).
    assert_eq!(
        eval("(with-temp-buffer (insert-file-contents \"Cargo.toml\") (buffer-substring 1 8))"),
        "\"[packag\""
    );
}

#[test]
fn regexp_line_anchors() {
    // ^ and $ are line-anchored (multiline) in elisp regexps...
    assert_eq!(eval("(string-match \"^b$\" \"a\nb\nc\")"), "2");
    assert_eq!(eval("(string-match \"^abc$\" \"abc\")"), "0");
    assert_eq!(
        eval("(replace-regexp-in-string \"^\" \"> \" \"a\nb\")"),
        "\"> a\n> b\""
    );
    // ...while \\` and \\' stay anchored to the absolute string start/end.
    assert_eq!(eval("(string-match \"\\\\`a\" \"a\nb\")"), "0");
    assert_eq!(eval("(string-match \"\\\\`b\" \"a\nb\")"), "nil");
    assert_eq!(eval("(string-match \"c\\\\'\" \"a\nc\")"), "2");
    // buffer search across lines with ^...$.
    assert_eq!(
        eval("(with-temp-buffer (insert \"a 1\nb 2\nc 3\n\") (goto-char 1) (let (r) (while (re-search-forward \"^\\\\([a-z]\\\\) [0-9]$\" nil t) (push (match-string 1) r)) (nreverse r)))"),
        "(\"a\" \"b\" \"c\")"
    );
}

#[test]
fn subprocess_calls() {
    // shell-command-to-string returns stdout (with trailing newline from echo).
    assert_eq!(eval("(shell-command-to-string \"echo hi\")"), "\"hi\n\"");
    assert_eq!(
        eval("(string-trim (shell-command-to-string \"echo  x \"))"),
        "\"x\""
    );
    // call-process returns the exit code; with DESTINATION t it inserts stdout.
    assert_eq!(eval("(call-process \"true\" nil nil nil)"), "0");
    assert_eq!(eval("(call-process \"false\" nil nil nil)"), "1");
    assert_eq!(
        eval("(with-temp-buffer (call-process \"echo\" nil t nil \"x\" \"y\") (buffer-string))"),
        "\"x y\n\""
    );
    // process-lines splits stdout into lines; errors on non-zero exit.
    assert_eq!(
        eval("(process-lines \"printf\" \"a\nb\nc\n\")"),
        "(\"a\" \"b\" \"c\")"
    );
    assert!(eval_str("(process-lines \"false\")").is_err());
}

#[test]
fn filesystem_queries() {
    // cargo runs tests with cwd = crate root, so these repo files are stable.
    assert_eq!(eval("(file-exists-p \"Cargo.toml\")"), "t");
    assert_eq!(eval("(file-exists-p \"does-not-exist.xyz\")"), "nil");
    assert_eq!(eval("(file-directory-p \"src\")"), "t");
    assert_eq!(eval("(file-directory-p \"Cargo.toml\")"), "nil");
    assert_eq!(eval("(file-regular-p \"Cargo.toml\")"), "t");
    assert_eq!(eval("(file-readable-p \"Cargo.toml\")"), "t");
    assert_eq!(eval("(file-symlink-p \"Cargo.toml\")"), "nil");
    // directory-files: "." ".." come first; MATCH filters; src has lib.rs.
    assert_eq!(
        eval("(seq-take (directory-files \".\") 2)"),
        "(\".\" \"..\")"
    );
    assert_eq!(
        eval("(directory-files \"src\" nil \"^lib\\\\.rs$\")"),
        "(\"lib.rs\")"
    );
    assert_eq!(
        eval("(and (member \"Cargo.toml\" (directory-files \".\")) t)"),
        "t"
    );
    // missing directory signals file-missing.
    assert_eq!(
        eval("(condition-case e (directory-files \"no-such-dir\") (file-missing 'caught))"),
        "caught"
    );
}

#[test]
fn expand_file_name_and_env() {
    // expand-file-name against an explicit DIR; collapses . .. //.
    assert_eq!(eval("(expand-file-name \"b\" \"/a\")"), "\"/a/b\"");
    assert_eq!(eval("(expand-file-name \"b/\" \"/a/\")"), "\"/a/b/\"");
    assert_eq!(eval("(expand-file-name \"../c\" \"/a/b\")"), "\"/a/c\"");
    assert_eq!(eval("(expand-file-name \"/abs\" \"/a\")"), "\"/abs\"");
    assert_eq!(eval("(expand-file-name \"a//b\" \"/x\")"), "\"/x/a/b\"");
    assert_eq!(eval("(expand-file-name \"x/..\" \"/a\")"), "\"/a\"");
    // ~ expansion uses $HOME.
    assert_eq!(
        eval("(progn (setenv \"HOME\" \"/home/u\") (expand-file-name \"~/x\"))"),
        "\"/home/u/x\""
    );
    // file-relative-name (common-prefix); getenv/setenv round-trip.
    assert_eq!(eval("(file-relative-name \"/a/b/c\" \"/a\")"), "\"b/c\"");
    assert_eq!(
        eval("(progn (setenv \"ELP_T\" \"v\") (getenv \"ELP_T\"))"),
        "\"v\""
    );
}

#[test]
fn file_name_path_functions() {
    assert_eq!(eval("(file-name-directory \"/a/b/c.txt\")"), "\"/a/b/\"");
    assert_eq!(eval("(file-name-directory \"rel.txt\")"), "nil");
    assert_eq!(eval("(file-name-nondirectory \"/a/b/c.txt\")"), "\"c.txt\"");
    assert_eq!(eval("(file-name-extension \"/a/b/c.txt\")"), "\"txt\"");
    assert_eq!(eval("(file-name-extension \".bashrc\")"), "nil"); // hidden: no ext
    assert_eq!(eval("(file-name-extension \"/a.b/c\")"), "nil"); // dot in dir part
    assert_eq!(
        eval("(file-name-sans-extension \"/a/b/c.txt\")"),
        "\"/a/b/c\""
    );
    assert_eq!(eval("(file-name-base \"/a/b/c.txt\")"), "\"c\"");
    assert_eq!(eval("(file-name-as-directory \"/a/b\")"), "\"/a/b/\"");
    assert_eq!(eval("(directory-file-name \"/a/b/\")"), "\"/a/b\"");
    assert_eq!(eval("(directory-file-name \"/\")"), "\"/\"");
    assert_eq!(eval("(file-name-concat \"a\" \"b\" \"c\")"), "\"a/b/c\"");
    assert_eq!(eval("(file-name-absolute-p \"/x\")"), "t");
    assert_eq!(eval("(file-name-absolute-p \"x\")"), "nil");
    assert_eq!(
        eval("(file-name-split \"/a/b/\")"),
        "(\"\" \"a\" \"b\" \"\")"
    );
}

#[test]
fn cl_typep_struct_and_constants() {
    // cl-typep / cl-defmethod dispatch use the hyphenated TYPE-p for structs.
    assert_eq!(
        eval("(progn (cl-defstruct pt x) (cl-typep (make-pt) 'pt))"),
        "t"
    );
    assert_eq!(
        eval("(progn (cl-defstruct pt2 x) (cl-typep 5 'pt2))"),
        "nil"
    );
    assert_eq!(eval("(cl-typep 5 'integer)"), "t"); // builtin still uses TYPEp
                                                    // float-pi / float-e / pi constants.
    assert_eq!(eval("float-pi"), "3.141592653589793");
    assert_eq!(eval("(= pi float-pi)"), "t");
    assert_eq!(eval("(< (abs (- float-e 2.71828)) 0.001)"), "t");
}

#[test]
fn add_to_list_append_and_compare() {
    // default prepend; no dup; APPEND adds at the end; custom COMPARE-FN.
    assert_eq!(
        eval("(progn (defvar tl (list 2 3)) (add-to-list 'tl 1) (add-to-list 'tl 9 t) tl)"),
        "(1 2 3 9)"
    );
    assert_eq!(
        eval("(progn (defvar t2 (list 1 2)) (add-to-list 't2 1) t2)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(progn (defvar t3 (list \"b\")) (add-to-list 't3 \"a\" nil #'string=) t3)"),
        "(\"a\" \"b\")"
    );
    // equal-including-properties = equal (no text-property model).
    assert_eq!(eval("(equal-including-properties \"a\" \"a\")"), "t");
}

#[test]
fn string_split_memoization_defalias() {
    // string-split is split-string.
    assert_eq!(
        eval("(string-split \"a,b,c\" \",\")"),
        "(\"a\" \"b\" \"c\")"
    );
    assert_eq!(eval("(string-split \"  a b  c \")"), "(\"a\" \"b\" \"c\")");
    // with-memoization caches in PLACE; the second body is never run.
    assert_eq!(
        eval("(let ((c nil)) (list (with-memoization c (+ 1 2)) (with-memoization c (error \"x\")) c))"),
        "(3 3 3)"
    );
    // defalias makes a symbol call through to another function/symbol.
    assert_eq!(eval("(progn (defalias 'my-inc '1+) (my-inc 5))"), "6");
    assert_eq!(
        eval("(progn (defalias 'my-d (lambda (x) (* x 3))) (my-d 4))"),
        "12"
    );
}

#[test]
fn feature_system_and_introspection() {
    // provide/featurep/require over the bundled features.
    assert_eq!(eval("(provide 'my-feat)"), "my-feat");
    assert_eq!(eval("(progn (provide 'zz) (featurep 'zz))"), "t");
    assert_eq!(eval("(featurep 'cl-lib)"), "t"); // bundled
    assert_eq!(eval("(require 'cl-lib)"), "cl-lib");
    assert_eq!(eval("(require 'no-such-xyz nil t)"), "nil");
    assert_eq!(
        eval("(condition-case e (require 'no-such-xyz) (file-missing 'caught))"),
        "caught"
    );
    // bound-and-true-p, special-variable-p, make-local-variable (no-op).
    assert_eq!(eval("(bound-and-true-p case-fold-search)"), "t");
    assert_eq!(eval("(bound-and-true-p totally-unbound-xyz)"), "nil");
    assert_eq!(eval("(special-variable-p 'case-fold-search)"), "t");
    assert_eq!(eval("(special-variable-p 'car)"), "nil");
    assert_eq!(eval("(make-local-variable 'x)"), "x");
    // func-arity / subr-arity → (MIN . MAX), MAX is `many` for variadic.
    assert_eq!(eval("(func-arity 'cons)"), "(2 . 2)");
    assert_eq!(eval("(func-arity '+)"), "(0 . many)");
    assert_eq!(eval("(func-arity (lambda (a b &optional c) a))"), "(2 . 3)");
    assert_eq!(eval("(func-arity (lambda (a &rest r) a))"), "(1 . many)");
    assert_eq!(
        eval("(progn (defun myf (a b) a) (func-arity 'myf))"),
        "(2 . 2)"
    );
}

#[test]
fn json_native_api() {
    // json-parse-string defaults: hash-table (string keys), :null/:false.
    assert_eq!(
        eval("(json-parse-string \"{\\\"a\\\":1}\")"),
        "#s(hash-table test equal data (\"a\" 1))"
    );
    assert_eq!(eval("(json-parse-string \"null\")"), ":null");
    assert_eq!(eval("(json-parse-string \"false\")"), ":false");
    // keyword args override object/array type and the null/false objects.
    assert_eq!(
        eval("(json-parse-string \"{\\\"a\\\":1}\" :object-type 'alist)"),
        "((a . 1))"
    );
    assert_eq!(
        eval("(json-parse-string \"[1,2]\" :array-type 'list)"),
        "(1 2)"
    );
    assert_eq!(
        eval("(json-parse-string \"null\" :null-object :NULL)"),
        ":NULL"
    );
    // json-serialize uses :null/:false; json-encode (json.el) keeps :json-false.
    assert_eq!(
        eval("(json-serialize (vector t :false :null))"),
        "\"[true,false,null]\""
    );
    assert_eq!(
        eval("(json-serialize '(:a 1 :b 2))"),
        "\"{\\\"a\\\":1,\\\"b\\\":2}\""
    );
    assert_eq!(eval("(json-encode :json-false)"), "\"false\"");
}

#[test]
fn regexp_opt_basic() {
    // Sorted, regexp-quoted alternation; special cases match Emacs.
    assert_eq!(
        eval("(regexp-opt '(\"foo\" \"bar\"))"),
        "\"\\\\(?:bar\\\\|foo\\\\)\""
    );
    assert_eq!(
        eval("(regexp-opt '(\"a.b\" \"c+d\"))"),
        "\"\\\\(?:a\\\\.b\\\\|c\\\\+d\\\\)\""
    );
    assert_eq!(eval("(regexp-opt '(\"x\"))"), "\"x\""); // single char: no group
    assert_eq!(eval("(regexp-opt '(\"abc\"))"), "\"\\\\(?:abc\\\\)\""); // multi-char: grouped
    assert_eq!(eval("(regexp-opt '(\"foo\") t)"), "\"\\\\(foo\\\\)\"");
    assert_eq!(eval("(regexp-opt '())"), "\"\\\\(?:\\\\`a\\\\`\\\\)\"");
    // Functional: the produced regexp matches correctly even with shared prefixes.
    assert_eq!(
        eval("(let ((re (regexp-opt '(\"foo\" \"foobar\")))) (string-match re \"zfoobar\"))"),
        "1"
    );
}

#[test]
fn cl_place_swappers() {
    // cl-rotatef rotates values left (swap for two places).
    assert_eq!(
        eval("(let ((a 1) (b 2)) (cl-rotatef a b) (list a b))"),
        "(2 1)"
    );
    assert_eq!(
        eval("(let ((a 1) (b 2) (c 3)) (cl-rotatef a b c) (list a b c))"),
        "(2 3 1)"
    );
    // …on generalized places too.
    assert_eq!(
        eval("(let ((v (vector 0 1))) (cl-rotatef (aref v 0) (aref v 1)) v)"),
        "[1 0]"
    );
    // cl-psetf assigns in parallel (so a/b swap).
    assert_eq!(
        eval("(let ((a 1) (b 2)) (cl-psetf a b b a) (list a b))"),
        "(2 1)"
    );
    // cl-shiftf shifts left, last place takes NEWVAL, returns the first's old value.
    assert_eq!(
        eval("(let ((a 1) (b 2)) (cl-shiftf a b 3) (list a b))"),
        "(2 3)"
    );
    assert_eq!(eval("(let ((a 1) (b 2)) (cl-shiftf a b 3))"), "1");
    // cl-getf is a setf-able place; cl-locally is progn.
    assert_eq!(
        eval("(let ((p (list :a 1))) (cl-incf (cl-getf p :a)) p)"),
        "(:a 2)"
    );
    assert_eq!(eval("(cl-locally 5)"), "5");
}

#[test]
fn capitalize_char_and_evenp_typecheck() {
    // capitalize on a character upcases it (like upcase); strings unchanged.
    assert_eq!(eval("(capitalize ?a)"), "65");
    assert_eq!(eval("(capitalize ?1)"), "49");
    assert_eq!(eval("(capitalize \"foo-bar baz\")"), "\"Foo-Bar Baz\"");
    // cl-evenp / cl-oddp require an integer (signal on a float, like Emacs).
    assert_eq!(eval("(cl-evenp 4)"), "t");
    assert_eq!(eval("(cl-oddp -3)"), "t");
    assert!(eval_str("(cl-evenp 2.0)").is_err());
    assert_eq!(
        eval("(condition-case e (cl-oddp 3.5) (wrong-type-argument (cadr e)))"),
        "integer-or-marker-p"
    );
}

#[test]
fn iflet_test_clauses_and_pcase_let() {
    // if-let*/when-let* accept a single-element (CONDITION) clause (test, no bind).
    assert_eq!(eval("(when-let* (((= 1 1)) (x 5)) x)"), "5");
    assert_eq!(eval("(when-let* (((= 1 2)) (x 5)) x)"), "nil");
    assert_eq!(eval("(if-let* ((x 5) ((> x 3))) 'big 'small)"), "big");
    assert_eq!(eval("(if-let* ((x 5) ((< x 3))) 'big 'small)"), "small");
    // pcase (let PAT EXPR): bind PAT to EXPR's value (always matches if PAT does).
    assert_eq!(eval("(pcase 5 ((and (pred integerp) (let y 10)) y))"), "10");
    assert_eq!(eval("(pcase 5 ((let `(,a ,b) (list 1 2)) (+ a b)))"), "3");
    assert_eq!(eval("(pcase \"x\" ((and s (let n (length s))) n))"), "1");
}

#[test]
fn cl_position_from_end() {
    // :from-end returns the LAST matching index; default is the first.
    assert_eq!(eval("(cl-position 2 '(1 2 3 2) :from-end t)"), "3");
    assert_eq!(eval("(cl-position 2 '(1 2 3 2))"), "1");
    assert_eq!(eval("(cl-position 9 '(1 2 3))"), "nil");
    assert_eq!(
        eval("(cl-position-if #'cl-evenp '(1 2 3 4) :from-end t)"),
        "3"
    );
    assert_eq!(eval("(cl-position-if #'cl-evenp '(1 2 3 4))"), "1");
    // cl-find / cl-find-if honor :from-end and :key; cl-find-if takes keyword args.
    assert_eq!(eval("(cl-find-if #'cl-evenp '(1 2 3 4) :from-end t)"), "4");
    assert_eq!(eval("(cl-find-if #'cl-evenp '(1 2 3 4))"), "2");
    assert_eq!(eval("(cl-find-if-not #'cl-evenp '(2 4 5 6))"), "5");
    assert_eq!(
        eval("(cl-find 2 (list (cons 2 'a) (cons 2 'b)) :key #'car :from-end t)"),
        "(2 . b)"
    );
}

#[test]
fn cl_search_mismatch_and_list_utils() {
    // cl-search: contiguous subsequence index (lists and strings).
    assert_eq!(eval("(cl-search '(2 3) '(1 2 3 4))"), "1");
    assert_eq!(eval("(cl-search '(9) '(1 2 3))"), "nil");
    assert_eq!(eval("(cl-search '() '(1 2))"), "0");
    assert_eq!(eval("(cl-search \"bc\" \"abcd\")"), "1");
    // cl-mismatch: first differing index, nil if equal.
    assert_eq!(eval("(cl-mismatch '(1 2 3) '(1 2 4))"), "2");
    assert_eq!(eval("(cl-mismatch '(1 2) '(1 2))"), "nil");
    assert_eq!(eval("(cl-mismatch '(1 2) '(1 2 3))"), "2");
    // list utilities + 3-level cl- accessors + predicates.
    assert_eq!(eval("(cl-caddr '(1 2 3 4))"), "3");
    assert_eq!(eval("(cl-revappend '(1 2) '(3 4))"), "(2 1 3 4)");
    assert_eq!(eval("(cl-nth-value 1 (cl-floor 7 2))"), "1");
    assert_eq!(eval("(cl-notany #'cl-evenp '(1 3))"), "t");
    assert_eq!(eval("(cl-set-exclusive-or '(1 2) '(2 3))"), "(1 3)");
}

#[test]
fn replace_regexp_case_adaptation() {
    // FIXEDCASE nil: replacement case adapts to each match's case.
    assert_eq!(
        eval("(replace-regexp-in-string \"x\" \"y\" \"XxX\")"),
        "\"YyY\""
    );
    assert_eq!(
        eval("(replace-regexp-in-string \"foo\" \"bar\" \"FOO Foo foo\")"),
        "\"BAR Bar bar\""
    );
    // capitalized match upcases the first letter of each word, keeping the rest.
    assert_eq!(
        eval("(replace-regexp-in-string \"Xy\" \"foo bar\" \"Xy\")"),
        "\"Foo Bar\""
    );
    assert_eq!(
        eval("(replace-regexp-in-string \"ab\" \"cd\" \"Ab\")"),
        "\"Cd\""
    );
    // a non-letter match (no case) leaves the replacement unchanged.
    assert_eq!(
        eval("(replace-regexp-in-string \"1\" \"ab\" \"1\")"),
        "\"ab\""
    );
    // FIXEDCASE t disables adaptation.
    assert_eq!(
        eval("(replace-regexp-in-string \"foo\" \"bar\" \"FOO Foo foo\" t)"),
        "\"bar bar bar\""
    );
}

#[test]
fn wrong_type_argument_includes_value() {
    // wrong-type-argument error data carries the offending value, like Emacs.
    let caught = |expr: &str| eval(&format!("(condition-case e {expr} (error e))"));
    assert_eq!(caught("(length 5)"), "(wrong-type-argument sequencep 5)");
    assert_eq!(
        caught("(aref '(1 2) 0)"),
        "(wrong-type-argument arrayp (1 2))"
    );
    assert_eq!(caught("(symbol-name 5)"), "(wrong-type-argument symbolp 5)");
    // elt on a non-sequence reports sequencep (not arrayp), with the value.
    assert_eq!(caught("(elt 5 0)"), "(wrong-type-argument sequencep 5)");
    // nreverse on a non-list non-array reports arrayp.
    assert_eq!(caught("(nreverse 5)"), "(wrong-type-argument arrayp 5)");
}

#[test]
fn cl_subsetp_membership() {
    assert_eq!(eval("(cl-subsetp '(1 2) '(1 2 3))"), "t");
    assert_eq!(eval("(cl-subsetp '(1 5) '(1 2 3))"), "nil");
    assert_eq!(eval("(cl-subsetp nil '(1))"), "t");
    assert_eq!(
        eval("(cl-subsetp '((1) (2)) '((1) (2) (3)) :key #'car)"),
        "t"
    );
    assert_eq!(
        eval("(cl-subsetp '(\"a\") '(\"A\") :test #'string-equal)"),
        "nil"
    );
}

#[test]
fn setf_get_and_cl_nth_places() {
    // (setf (get SYM PROP) V) -> put.
    assert_eq!(
        eval("(progn (setf (get 'foo 'bar) 42) (get 'foo 'bar))"),
        "42"
    );
    // cl-first/cl-third/... and cl-rest as setf places.
    assert_eq!(
        eval("(let ((l (list 1 2 3 4))) (setf (cl-first l) 'a (cl-third l) 'c) l)"),
        "(a 2 c 4)"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (setf (cl-rest l) '(9)) l)"),
        "(1 9)"
    );
    // works through cl-incf / cl-rotatef too.
    assert_eq!(
        eval("(let ((l (list 1 2))) (cl-incf (cl-second l)) l)"),
        "(1 3)"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (cl-rotatef (cl-first l) (cl-third l)) l)"),
        "(3 2 1)"
    );
}

#[test]
fn regexp_numbered_groups() {
    // `\(?N:…\)` explicit group numbers (sequential) map to the right match data.
    assert_eq!(
        eval(
            "(progn (string-match \"\\\\(?1:[0-9]+\\\\)-\\\\(?2:[0-9]+\\\\)\" \"12-34\") \
               (list (match-string 1 \"12-34\") (match-string 2 \"12-34\")))"
        ),
        "(\"12\" \"34\")"
    );
    // Shy groups still work alongside numbered ones.
    assert_eq!(
        eval(
            "(progn (string-match \"\\\\(?:x\\\\)\\\\(?1:y\\\\)\" \"xy\") (match-string 1 \"xy\"))"
        ),
        "\"y\""
    );
}

#[test]
fn cl_loop_named_block() {
    // `named NAME` lets `cl-return-from NAME` exit the loop.
    assert_eq!(
        eval("(cl-loop named outer for i from 1 to 3 do (cl-return-from outer i))"),
        "1"
    );
    // A named outer loop can be exited from inside a nested loop.
    assert_eq!(
        eval(
            "(let ((r nil)) (cl-loop named outer for i from 1 to 3 do \
               (cl-loop for j from 1 to 3 do \
                 (when (= j 2) (cl-return-from outer (setq r (cons i j)))))) r)"
        ),
        "(1 . 2)"
    );
    // Unnamed loops still honor `cl-return`.
    assert_eq!(
        eval("(cl-loop for i from 1 to 3 do (when (= i 2) (cl-return 42)))"),
        "42"
    );
    assert_eq!(
        eval("(cl-loop named o for i from 1 to 3 collect i)"),
        "(1 2 3)"
    );
}

#[test]
fn cl_symbol_macrolet_substitution() {
    assert_eq!(eval("(cl-symbol-macrolet ((x 10)) (+ x 1))"), "11");
    assert_eq!(eval("(cl-symbol-macrolet ((x 5) (y 6)) (+ x y))"), "11");
    // setq on a symbol-macro becomes setf of its expansion.
    assert_eq!(
        eval("(let ((x 1)) (cl-symbol-macrolet ((y x)) (setq y 5)) x)"),
        "5"
    );
    assert_eq!(
        eval("(let ((v (vector 0 0))) (cl-symbol-macrolet ((a (aref v 0))) (setf a 9)) v)"),
        "[9 0]"
    );
    // quoted occurrences are not substituted.
    assert_eq!(eval("(cl-symbol-macrolet ((x 2)) (list 'x x))"), "(x 2)");
}

#[test]
fn cl_macrolet_local_macros() {
    assert_eq!(eval("(cl-macrolet ((sq (x) (list '* x x))) (sq 5))"), "25");
    assert_eq!(
        eval("(cl-macrolet ((inc (x) (list '1+ x))) (inc (inc 3)))"),
        "5"
    );
    assert_eq!(
        eval("(cl-macrolet ((swp (a b) (list 'list b a))) (swp 1 2))"),
        "(2 1)"
    );
    assert_eq!(
        eval("(cl-macrolet ((m (&rest xs) (cons '+ xs))) (m 1 2 3))"),
        "6"
    );
    assert_eq!(
        eval("(cl-macrolet ((id (x) x)) (mapcar (lambda (n) (id n)) (list 1 2)))"),
        "(1 2)"
    );
    assert_eq!(
        eval("(cl-macrolet ((sq (x) (list '* x x))) (+ (sq 3) (sq 4)))"),
        "25"
    );
}

#[test]
fn toplevel_progn_splices_macros() {
    // A macro defined and used in the same top-level progn is expanded, because
    // top-level progns are spliced into separate forms.
    assert_eq!(
        eval("(progn (defmacro tw (x) (list 'progn x x)) (let ((n 0)) (tw (setq n (1+ n))) n))"),
        "2"
    );
    assert_eq!(
        eval("(progn (progn (defmacro sq (x) (list '* x x))) (sq 4))"),
        "16"
    );
    // progn still returns its last value and runs effects in order.
    assert_eq!(eval("(progn 1 2 3)"), "3");
    assert_eq!(eval("(progn (setq zz 5) (* zz 2))"), "10");
    assert_eq!(eval("(progn)"), "nil");
}

#[test]
fn reader_trailing_dot_int_and_named_codepoint() {
    // A trailing decimal point with no fraction reads as an integer.
    assert_eq!(eval("(read \"1.\")"), "1");
    assert_eq!(eval("(read \"-3.\")"), "-3");
    assert_eq!(eval("(eq (read \"10.\") 10)"), "t");
    // …but real floats are unaffected.
    assert_eq!(eval("(read \"1.5\")"), "1.5");
    assert_eq!(eval("(read \"1.e3\")"), "1000.0");
    // \N{U+HHHH} codepoint escape in chars and strings.
    assert_eq!(eval("(read \"?\\\\N{U+1F600}\")"), "128512");
    assert_eq!(eval("(read \"?\\\\N{U+41}\")"), "65");
    assert_eq!(eval("(read \"\\\"x\\\\N{U+41}y\\\"\")"), "\"xAy\"");
}

#[test]
fn safe_length_proper_list_p_cycle_safe() {
    assert_eq!(eval("(safe-length (list 1 2 3))"), "3");
    assert_eq!(eval("(safe-length '(1 2 . 3))"), "2");
    assert_eq!(eval("(proper-list-p (list 1 2 3))"), "3");
    assert_eq!(eval("(proper-list-p '(1 2 . 3))"), "nil");
    // Circular lists terminate (Floyd) instead of hanging.
    assert_eq!(
        eval("(safe-length (let ((l (list 1 2))) (setcdr (cdr l) l) l))"),
        "4"
    );
    assert_eq!(
        eval("(proper-list-p (let ((l (list 1 2))) (setcdr (cdr l) l) l))"),
        "nil"
    );
    assert_eq!(eval("(proper-list-p 5)"), "nil");
}

#[test]
fn value_less_total_order() {
    assert_eq!(eval("(value< 1 2)"), "t");
    assert_eq!(eval("(value< 2 1)"), "nil");
    assert_eq!(eval("(value< \"a\" \"b\")"), "t");
    assert_eq!(eval("(value< 'a 'b)"), "t");
    assert_eq!(eval("(value< (list 1 2) (list 1 3))"), "t");
    assert_eq!(eval("(value< (list 1) (list 1 2))"), "t");
    assert_eq!(eval("(value< [1 2] [1 3])"), "t");
    // Cross-type signals (type-mismatch A B).
    assert_eq!(
        eval("(condition-case e (value< 1 \"a\") (error e))"),
        "(type-mismatch 1 \"a\")"
    );
    // It works as a sort predicate.
    assert_eq!(
        eval("(sort (list \"b\" \"a\" \"c\") #'value<)"),
        "(\"a\" \"b\" \"c\")"
    );
}

#[test]
fn hash_table_and_record_read_syntax() {
    // #s(hash-table …) reads into a hash table.
    assert_eq!(
        eval("(gethash 'a (read \"#s(hash-table test eq data (a 1))\"))"),
        "1"
    );
    assert_eq!(
        eval("(gethash \"x\" (read \"#s(hash-table test equal data (\\\"x\\\" 5))\"))"),
        "5"
    );
    assert_eq!(
        eval("(hash-table-count (read \"#s(hash-table data (a 1 b 2 c 3))\"))"),
        "3"
    );
    // #s(NAME …) reads into a record.
    assert_eq!(eval("(type-of (read \"#s(foo 1 2)\"))"), "foo");
    assert_eq!(eval("(recordp (read \"#s(pt 3 4)\"))"), "t");
    assert_eq!(eval("(aref (read \"#s(pt 3 4)\") 1)"), "3");
    // #("str" …) drops text properties → the bare string.
    assert_eq!(eval("(read \"#(\\\"ab\\\" 0 1 (face bold))\")"), "\"ab\"");
}

#[test]
fn cl_deftype_aliases() {
    assert_eq!(
        eval("(progn (cl-deftype small () '(integer 0 9)) (cl-typep 5 'small))"),
        "t"
    );
    assert_eq!(
        eval("(progn (cl-deftype small () '(integer 0 9)) (cl-typep 15 'small))"),
        "nil"
    );
    assert_eq!(
        eval("(progn (cl-deftype my-int () 'integer) (cl-typep 5 'my-int))"),
        "t"
    );
    assert_eq!(
        eval("(progn (cl-deftype rng (lo hi) (list 'integer lo hi)) (cl-typep 5 '(rng 1 10)))"),
        "t"
    );
    assert_eq!(
        eval("(progn (cl-deftype small () '(integer 0 9)) (cl-typecase 5 (small 'yes) (t 'no)))"),
        "yes"
    );
}

#[test]
fn cl_typep_compound_specifiers() {
    assert_eq!(eval("(cl-typep 5 '(integer 1 10))"), "t");
    assert_eq!(eval("(cl-typep 15 '(integer 1 10))"), "nil");
    assert_eq!(eval("(cl-typep 5 '(or string integer))"), "t");
    assert_eq!(
        eval("(cl-typep \"x\" '(and string (satisfies stringp)))"),
        "t"
    );
    assert_eq!(eval("(cl-typep 5 '(member 1 2 5))"), "t");
    assert_eq!(eval("(cl-typep 5 '(not string))"), "t");
    assert_eq!(eval("(cl-typep 5 '(satisfies cl-plusp))"), "t");
    assert_eq!(eval("(cl-typep 5 '(integer 1 *))"), "t");
    assert_eq!(
        eval("(cl-typecase 5 ((integer 1 3) 'lo) ((integer 4 10) 'hi))"),
        "hi"
    );
    // Simple types still work.
    assert_eq!(eval("(cl-typep 5 'integer)"), "t");
}

#[test]
fn make_separator_line_fn() {
    assert_eq!(eval("(length (make-separator-line))"), "80");
    assert_eq!(eval("(length (make-separator-line 10))"), "11");
    assert_eq!(eval("(make-separator-line 0)"), "\"\n\"");
}

#[test]
fn cl_member_assoc_if_family() {
    assert_eq!(eval("(cl-member-if #'cl-evenp (list 1 3 4 5))"), "(4 5)");
    assert_eq!(
        eval("(cl-member-if-not #'cl-evenp (list 2 4 5 6))"),
        "(5 6)"
    );
    assert_eq!(eval("(cl-member-if #'cl-evenp (list 1 3 5))"), "nil");
    assert_eq!(
        eval("(cl-assoc-if #'cl-evenp '((1 . a) (2 . b)))"),
        "(2 . b)"
    );
    assert_eq!(
        eval("(cl-assoc-if-not #'cl-evenp '((2 . a) (3 . b)))"),
        "(3 . b)"
    );
    assert_eq!(eval("(cl-rassoc 2 '((a . 1) (b . 2)))"), "(b . 2)");
    assert_eq!(
        eval("(cl-rassoc \"b\" '((1 . \"a\") (2 . \"b\")) :test #'equal)"),
        "(2 . \"b\")"
    );
    assert_eq!(
        eval("(cl-rassoc-if #'cl-evenp '((a . 1) (b . 4)))"),
        "(b . 4)"
    );
}

#[test]
fn setf_nthcdr_place() {
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (setf (nthcdr 1 l) (list 8 9)) l)"),
        "(1 8 9)"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (setf (nthcdr 0 l) (list 8 9)) l)"),
        "(8 9)"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3 4))) (setf (nthcdr 2 l) nil) l)"),
        "(1 2)"
    );
    // nth place still works.
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (setf (nth 1 l) 9) l)"),
        "(1 9 3)"
    );
}

#[test]
fn destructive_list_ops() {
    // nreverse relinks in place: the original head becomes the tail.
    assert_eq!(eval("(let ((l (list 1 2 3))) (nreverse l) l)"), "(1)");
    assert_eq!(eval("(nreverse (list 1 2 3))"), "(3 2 1)");
    // delq/delete splice interior matches in place.
    assert_eq!(eval("(let ((l (list 1 2 3 2))) (delq 2 l) l)"), "(1 3)");
    assert_eq!(
        eval("(delete \"b\" (list \"a\" \"b\" \"c\"))"),
        "(\"a\" \"c\")"
    );
    // nbutlast truncates in place.
    assert_eq!(
        eval("(let ((l (list 1 2 3 4))) (nbutlast l 1) l)"),
        "(1 2 3)"
    );
    assert_eq!(eval("(nbutlast (list 1 2) 5)"), "nil");
    // reverse stays non-destructive.
    assert_eq!(eval("(let ((l (list 1 2 3))) (reverse l) l)"), "(1 2 3)");
}

#[test]
fn sort_destructive_classic_form() {
    // Classic (sort SEQ PRED) is destructive: the variable sees the sorted order.
    assert_eq!(eval("(let ((l (list 3 1 2))) (sort l #'<) l)"), "(1 2 3)");
    assert_eq!(eval("(let ((v (vector 3 1 2))) (sort v #'<) v)"), "[1 2 3]");
    // The Emacs-30 keyword form is non-destructive unless :in-place t.
    assert_eq!(
        eval("(let ((l (list 3 1 2))) (sort l :lessp #'<) l)"),
        "(3 1 2)"
    );
    assert_eq!(
        eval("(let ((l (list 3 1 2))) (sort l :lessp #'< :in-place t) l)"),
        "(1 2 3)"
    );
    // Both forms return the sorted sequence.
    assert_eq!(eval("(sort (list 3 1 2) #'<)"), "(1 2 3)");
    assert_eq!(eval("(sort (list 3 1 2) :lessp #'<)"), "(1 2 3)");
}

#[test]
fn aref_aset_negative_index_errors() {
    assert_eq!(
        eval("(condition-case e (aref [1 2] -1) (args-out-of-range (cdr e)))"),
        "([1 2] -1)"
    );
    assert_eq!(
        eval("(condition-case e (aref \"abc\" -2) (args-out-of-range 'oor))"),
        "oor"
    );
    assert_eq!(
        eval("(condition-case e (aset (vector 1 2) -1 0) (args-out-of-range 'oor))"),
        "oor"
    );
    // Valid indices unaffected.
    assert_eq!(eval("(aref [10 20 30] 1)"), "20");
    assert_eq!(eval("(let ((v (vector 1 2))) (aset v 0 9) v)"), "[9 2]");
}

#[test]
fn uncaught_throw_becomes_no_catch() {
    // A throw with no matching catch signals (no-catch TAG VALUE).
    assert_eq!(
        eval("(condition-case e (throw 'nope 5) (no-catch (cdr e)))"),
        "(nope 5)"
    );
    assert_eq!(
        eval("(condition-case e (catch 'a (throw 'b 1)) (no-catch (cdr e)))"),
        "(b 1)"
    );
    // no-catch is an error subtype.
    assert_eq!(
        eval("(condition-case e (throw 'z 1) (error 'caught))"),
        "caught"
    );
    // Real catches still work and pass through condition-case to an outer catch.
    assert_eq!(
        eval("(catch 'tag (condition-case e (throw 'tag 5) (error 'caught)))"),
        "5"
    );
    assert_eq!(eval("(catch 'x (catch 'y (throw 'x 9)))"), "9");
    assert_eq!(
        eval("(catch 'done (dolist (x '(1 2 3)) (when (= x 2) (throw 'done x))))"),
        "2"
    );
}

#[test]
fn error_message_assert_propertize() {
    // error-message-string prints data readably (strings quoted).
    assert_eq!(
        eval("(error-message-string '(wrong-type-argument numberp \"x\"))"),
        "\"Wrong type argument: numberp, \\\"x\\\"\""
    );
    assert_eq!(
        eval("(error-message-string '(error \"plain\"))"),
        "\"plain\""
    );
    // cl-assert: custom message via plain error; else cl-assertion-failed.
    assert_eq!(
        eval("(condition-case e (cl-assert (= 1 2) nil \"msg %d\" 5) (error (cadr e)))"),
        "\"msg 5\""
    );
    assert_eq!(
        eval("(condition-case e (cl-assert (= 1 2)) (cl-assertion-failed 'failed))"),
        "failed"
    );
    // propertize returns the bare string (properties dropped).
    assert_eq!(eval("(propertize \"hello\" 'face 'bold)"), "\"hello\"");
    assert_eq!(eval("(concat (propertize \"a\" 'x 1) \"b\")"), "\"ab\"");
}

#[test]
fn string_to_number_base() {
    // Base 10 (and nil) parse floats; other bases are integer-only.
    assert_eq!(eval("(string-to-number \"1.5\" 10)"), "1.5");
    assert_eq!(eval("(string-to-number \"1.5\")"), "1.5");
    assert_eq!(eval("(string-to-number \"-3.5e2\" 10)"), "-350.0");
    assert_eq!(eval("(string-to-number \"1.5\" 16)"), "1");
    assert_eq!(eval("(string-to-number \"ff\" 16)"), "255");
    assert_eq!(eval("(string-to-number \"101\" 2)"), "5");
    assert_eq!(eval("(string-to-number \"10\" 8)"), "8");
}

#[test]
fn buffer_columns_lines_and_words() {
    // current-column expands tabs to the next multiple of 8.
    assert_eq!(
        eval("(with-temp-buffer (insert \"x\\ty\") (goto-char (point-max)) (current-column))"),
        "9"
    );
    // line-{beginning,end}-position honor the optional N.
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\\ndef\") (goto-char (point-min)) (line-end-position 2))"),
        "8"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\\ndef\\nghi\") (goto-char 1) (line-beginning-position 3))"),
        "9"
    );
    // count-words / how-many / newline / open-line.
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc def ghi\") (count-words (point-min) (point-max)))"),
        "3"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"a1b2c3\") (goto-char 1) (how-many \"[0-9]\"))"),
        "3"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"hi\") (newline 2) (length (buffer-string)))"),
        "4"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"ab\") (open-line 1) (point))"),
        "3"
    );
}

#[test]
fn match_string_no_props_and_regexp_opt_depth() {
    assert_eq!(
        eval("(progn (string-match \"\\\\([a-z]+\\\\)\" \"  abc\") (match-string-no-properties 1 \"  abc\"))"),
        "\"abc\""
    );
    assert_eq!(eval("(regexp-opt-depth \"\\\\(a\\\\)\\\\(b\\\\)\")"), "2");
    assert_eq!(eval("(regexp-opt-depth \"\\\\(?:a\\\\)\")"), "0");
    assert_eq!(eval("(regexp-opt-depth \"\\\\(?1:a\\\\)\")"), "0");
    assert_eq!(eval("(regexp-opt-depth \"abc\")"), "0");
    assert_eq!(eval("(regexp-opt-depth \"\\\\(a\\\\(b\\\\)\\\\)\")"), "2");
}

#[test]
fn cl_loop_and_on_by_initially() {
    // `and` parallel binding.
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3) and y in (list 4 5 6) collect (+ x y))"),
        "(5 7 9)"
    );
    assert_eq!(
        eval("(cl-loop for x in (list 1 2 3) and i from 0 collect (cons i x))"),
        "((0 . 1) (1 . 2) (2 . 3))"
    );
    // `on ... by` with destructuring.
    assert_eq!(
        eval("(cl-loop for (k v) on (list 1 2 3 4) by #'cddr collect (cons k v))"),
        "((1 . 2) (3 . 4))"
    );
    assert_eq!(
        eval("(cl-loop for x on (list 1 2 3) by #'cddr collect (car x))"),
        "(1 3)"
    );
    // `initially`.
    assert_eq!(
        eval("(cl-loop initially (setq cltmp 0) for i to 2 do (setq cltmp (+ cltmp i)) finally return cltmp)"),
        "3"
    );
}

#[test]
fn declare_noop_and_seq_let_vector() {
    // declare is a runtime no-op inside a function body.
    assert_eq!(eval("(funcall (lambda (x) (declare (pure t)) x) 5)"), "5");
    assert_eq!(
        eval("(progn (defun decl-fn (x) (declare (side-effect-free t)) (* x x)) (decl-fn 4))"),
        "16"
    );
    assert_eq!(eval("(declare (pure t))"), "nil");
    // seq-let accepts a vector pattern.
    assert_eq!(eval("(seq-let [a b] (vector 1 2) (+ a b))"), "3");
    assert_eq!(
        eval("(seq-let [a &rest r] (list 1 2 3) (list a r))"),
        "(1 (2 3))"
    );
    assert_eq!(eval("(seq-let (a b) (list 1 2) (+ a b))"), "3");
}

#[test]
fn pcase_setq_destructures() {
    assert_eq!(eval("(let (a) (pcase-setq a 5) a)"), "5");
    assert_eq!(
        eval("(let (a b) (pcase-setq `(,a ,b) (list 1 2)) (list a b))"),
        "(1 2)"
    );
    assert_eq!(eval("(let (a b) (pcase-setq a 1 b 2) (list a b))"), "(1 2)");
    assert_eq!(
        eval("(let (a b) (pcase-setq `(,a . ,b) (cons 9 10)) (list a b))"),
        "(9 10)"
    );
    assert_eq!(
        eval("(let (h tl) (pcase-setq `(,h . ,tl) (list 1 2 3)) (list h tl))"),
        "(1 (2 3))"
    );
    assert_eq!(eval("(let (a) (pcase-setq a 5))"), "5");
}

#[test]
fn rx_anychar_backref_to_string() {
    assert_eq!(eval("(rx anychar)"), "\"[^z-a]\"");
    assert_eq!(eval("(rx anything)"), "\"[^z-a]\"");
    assert_eq!(eval("(rx nonl)"), "\".\"");
    assert_eq!(eval("(rx (backref 1))"), "\"\\\\1\"");
    assert_eq!(
        eval("(rx (group (+ digit)) (backref 1))"),
        "\"\\\\([[:digit:]]+\\\\)\\\\1\""
    );
    assert_eq!(eval("(rx (minimal-match (+ \"a\")))"), "\"a+\"");
    // rx-to-string: wraps in a shy group unless already grouped / NO-GROUP.
    assert_eq!(
        eval("(rx-to-string '(or \"cat\" \"dog\"))"),
        "\"\\\\(?:cat\\\\|dog\\\\)\""
    );
    assert_eq!(
        eval("(rx-to-string '(seq \"a\" \"b\"))"),
        "\"\\\\(?:ab\\\\)\""
    );
    assert_eq!(eval("(rx-to-string '(seq \"a\" \"b\") t)"), "\"ab\"");
    assert_eq!(eval("(rx-to-string '(any \"a-z\"))"), "\"[a-z]\"");
}

#[test]
fn cl_map_typed() {
    assert_eq!(eval("(cl-map 'string #'identity [97 98])"), "\"ab\"");
    assert_eq!(eval("(cl-map 'list #'+ (list 1 2) (list 3 4))"), "(4 6)");
    assert_eq!(eval("(cl-map 'vector #'1+ (list 1 2 3))"), "[2 3 4]");
    assert_eq!(eval("(cl-map 'string #'upcase \"abc\")"), "\"ABC\"");
    assert_eq!(eval("(cl-map nil #'identity (list 1 2))"), "nil");
    assert_eq!(eval("(cl-map 'list #'* [1 2 3] [4 5 6])"), "(4 10 18)");
}

#[test]
fn concat_mapconcat_accept_vectors() {
    assert_eq!(eval("(concat [?a ?b])"), "\"ab\"");
    assert_eq!(eval("(concat [?a ?b] \"cd\" (list ?e))"), "\"abcde\"");
    assert_eq!(
        eval("(mapconcat #'identity (vector \"a\" \"b\") \"-\")"),
        "\"a-b\""
    );
    assert_eq!(eval("(mapconcat #'string \"abc\" \"-\")"), "\"a-b-c\"");
    assert_eq!(
        eval("(mapconcat #'number-to-string [1 2 3] \",\")"),
        "\"1,2,3\""
    );
    // Regression: lists still work.
    assert_eq!(eval("(concat (list 65 66))"), "\"AB\"");
    assert_eq!(eval("(mapconcat #'identity (list \"x\" \"y\"))"), "\"xy\"");
}

#[test]
fn eval_compile_macros_and_guards() {
    assert_eq!(eval("(cl-eval-when (eval) 5)"), "5");
    assert_eq!(eval("(cl-eval-when (compile) 5)"), "nil");
    assert_eq!(eval("(cl-eval-when (load eval) (+ 1 2))"), "3");
    assert_eq!(eval("(eval-when-compile (+ 1 2))"), "3");
    assert_eq!(eval("(eval-and-compile (* 2 3))"), "6");
    assert_eq!(eval("(with-no-warnings 42)"), "42");
    assert_eq!(eval("(byte-code-function-p 'car)"), "nil");
    assert_eq!(eval("(macroexp-quote 5)"), "5");
    assert_eq!(eval("(macroexp-quote 'foo)"), "'foo");
    assert_eq!(eval("(progn (defvar-local my-dl2 9) my-dl2)"), "9");
    // (eval) with no args errors instead of crashing.
    assert!(eval_str("(eval)").is_err());
    assert!(eval_str("(macroexpand)").is_err());
}

#[test]
fn time_arithmetic_and_accessors() {
    assert_eq!(eval("(time-add 1 2)"), "3");
    assert_eq!(eval("(time-add (list 0 1) 2)"), "3");
    assert_eq!(eval("(time-subtract 5 2)"), "3");
    assert_eq!(eval("(time-less-p 1 2)"), "t");
    assert_eq!(eval("(time-equal-p 3 3.0)"), "t");
    assert_eq!(eval("(time-to-seconds (list 0 10))"), "10.0");
    assert_eq!(eval("(time-convert 5.7 'integer)"), "5");
    // Fractional results interoperate via float-time.
    assert_eq!(eval("(float-time (time-subtract 5.5 2))"), "3.5");
    // decoded-time accessors.
    assert_eq!(eval("(decoded-time-hour (decode-time 3661 t))"), "1");
    assert_eq!(eval("(decoded-time-year (decode-time 0 t))"), "1970");
    assert_eq!(eval("(current-time-zone 0 t)"), "(0 \"UTC\")");
}

#[test]
fn char_width_and_capitalize_unicode() {
    // Wide chars (CJK/emoji) are width 2; control chars special.
    assert_eq!(eval("(char-width ?中)"), "2");
    assert_eq!(eval("(char-width ?😀)"), "2");
    assert_eq!(eval("(char-width ?a)"), "1");
    assert_eq!(eval("(char-width ?\\n)"), "0");
    assert_eq!(eval("(char-width ?\\t)"), "8");
    assert_eq!(eval("(char-width ?\\C-a)"), "2");
    assert_eq!(eval("(string-width \"aあ😀\")"), "5");
    // capitalize handles non-ASCII cased letters.
    assert_eq!(eval("(capitalize \"ÿ\")"), "\"Ÿ\"");
    assert_eq!(eval("(capitalize \"café crème\")"), "\"Café Crème\"");
    assert_eq!(eval("(capitalize \"hello WORLD\")"), "\"Hello World\"");
}

#[test]
fn seq_split_type_and_set_equal() {
    // seq-partition/seq-split preserve the element-sequence type.
    assert_eq!(eval("(seq-split \"abcdef\" 2)"), "(\"ab\" \"cd\" \"ef\")");
    assert_eq!(eval("(seq-partition \"abcde\" 2)"), "(\"ab\" \"cd\" \"e\")");
    assert_eq!(eval("(seq-partition [1 2 3 4 5] 2)"), "([1 2] [3 4] [5])");
    assert_eq!(eval("(seq-partition (list 1 2 3) 2)"), "((1 2) (3))");
    // seq-set-equal-p
    assert_eq!(eval("(seq-set-equal-p '(1 2 3) '(3 2 1))"), "t");
    assert_eq!(eval("(seq-set-equal-p '(1 2) '(1 2 3))"), "nil");
    assert_eq!(
        eval("(seq-set-equal-p '(\"a\") '(\"A\") #'string-equal-ignore-case)"),
        "t"
    );
    assert_eq!(eval("(seq-into-sequence \"ab\")"), "\"ab\"");
}

#[test]
fn map_extras() {
    assert_eq!(
        eval("(gethash 'a (map-into '((a . 1)) '(hash-table :test eq)))"),
        "1"
    );
    assert_eq!(eval("(map-insert '((a . 1)) 'b 2)"), "((b . 2) (a . 1))");
    assert_eq!(eval("(map-values-apply #'1+ '((a . 1) (b . 2)))"), "(2 3)");
    assert_eq!(eval("(map-keys-apply #'symbol-name '((a . 1)))"), "(\"a\")");
    assert_eq!(
        eval("(let ((m (list (cons 'a 1)))) (map-put! m 'a 9) m)"),
        "((a . 9))"
    );
    assert_eq!(
        eval("(condition-case nil (map-put! (list (cons 'a 1)) 'b 2) (error 'cannot))"),
        "cannot"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (map-put! h 'x 5) (gethash 'x h))"),
        "5"
    );
}

#[test]
fn number_sequence_and_collate() {
    // number-sequence with only FROM yields a singleton.
    assert_eq!(eval("(number-sequence 10)"), "(10)");
    assert_eq!(eval("(number-sequence 1 5)"), "(1 2 3 4 5)");
    assert_eq!(eval("(number-sequence 5 1 -1)"), "(5 4 3 2 1)");
    // Collation (C-locale default = ordinary string order).
    assert_eq!(eval("(string-collate-lessp \"a\" \"b\")"), "t");
    assert_eq!(eval("(string-collate-equalp \"a\" \"a\")"), "t");
    assert_eq!(eval("(text-quoting-style)"), "curve");
}

#[test]
fn base64_url_rot13() {
    assert_eq!(eval("(base64-encode-string \"hello\")"), "\"aGVsbG8=\"");
    assert_eq!(eval("(base64-decode-string \"aGVsbG8=\")"), "\"hello\"");
    assert_eq!(
        eval("(base64url-encode-string \"hello?>\")"),
        "\"aGVsbG8_Pg==\""
    );
    assert_eq!(
        eval("(base64url-encode-string \"hello?>\" t)"),
        "\"aGVsbG8_Pg\""
    );
    assert_eq!(
        eval("(base64-decode-string (base64-encode-string \"round trip\"))"),
        "\"round trip\""
    );
    assert_eq!(eval("(url-hexify-string \"a b&c\")"), "\"a%20b%26c\"");
    assert_eq!(eval("(url-unhex-string \"a%20b%26c\")"), "\"a b&c\"");
    assert_eq!(
        eval("(rot13-string \"Hello, World!\")"),
        "\"Uryyb, Jbeyq!\""
    );
    assert_eq!(eval("(rot13-string (rot13-string \"abc\"))"), "\"abc\"");
}

#[test]
fn hashing_functions() {
    assert_eq!(
        eval("(sha1 \"\")"),
        "\"da39a3ee5e6b4b0d3255bfef95601890afd80709\""
    );
    assert_eq!(
        eval("(sha1 \"hello\")"),
        "\"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\""
    );
    assert_eq!(eval("(md5 \"\")"), "\"d41d8cd98f00b204e9800998ecf8427e\"");
    assert_eq!(
        eval("(md5 \"abc\")"),
        "\"900150983cd24fb0d6963f7d28e17f72\""
    );
    assert_eq!(
        eval("(secure-hash 'sha256 \"abc\")"),
        "\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
    );
    assert_eq!(
        eval("(secure-hash 'sha1 \"hello\")"),
        "\"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\""
    );
    // START/END select a substring before hashing.
    assert_eq!(
        eval("(sha1 \"hello world\" 0 5)"),
        "\"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\""
    );
    // buffer-hash is SHA-1 of the buffer contents.
    assert_eq!(
        eval("(with-temp-buffer (insert \"hello\") (buffer-hash))"),
        "\"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\""
    );
}

#[test]
fn predicate_helpers() {
    assert_eq!(eval("(nlistp 5)"), "t");
    assert_eq!(eval("(nlistp (list 1))"), "nil");
    assert_eq!(eval("(nlistp nil)"), "nil");
    assert_eq!(eval("(symbol-with-pos-p 'a)"), "nil");
    assert_eq!(eval("(bare-symbol 'a)"), "a");
    assert_eq!(eval("(bare-symbol nil)"), "nil");
    assert_eq!(eval("(bare-symbol :k)"), ":k");
}

#[test]
fn key_description_inverse_of_kbd() {
    assert_eq!(eval("(key-description (kbd \"C-x C-f\"))"), "\"C-x C-f\"");
    assert_eq!(eval("(key-description (kbd \"M-x\"))"), "\"M-x\"");
    assert_eq!(eval("(key-description (kbd \"C-M-a\"))"), "\"C-M-a\"");
    assert_eq!(eval("(key-description \"abc\")"), "\"a b c\"");
    assert_eq!(eval("(key-description (kbd \"RET\"))"), "\"RET\"");
    assert_eq!(eval("(key-description [f1])"), "\"<f1>\"");
    assert_eq!(eval("(key-description [C-f1])"), "\"C-<f1>\"");
    assert_eq!(eval("(key-description (kbd \"C-S-a\"))"), "\"C-S-a\"");
    // ESC prefix collapses into Meta.
    assert_eq!(eval("(key-description (vector 27 ?x))"), "\"M-x\"");
    assert_eq!(eval("(single-key-description ?\\C-a)"), "\"C-a\"");
    // Round-trips with kbd.
    assert_eq!(
        eval("(equal (kbd (key-description (kbd \"C-c M-x a\"))) (kbd \"C-c M-x a\"))"),
        "t"
    );
}

#[test]
fn kbd_key_sequences() {
    // ASCII/control keys produce a string; check via aref to avoid literal ctrl chars.
    assert_eq!(eval("(kbd \"a\")"), "\"a\"");
    assert_eq!(eval("(aref (kbd \"C-a\") 0)"), "1");
    assert_eq!(eval("(append (kbd \"C-x C-f\") nil)"), "(24 6)");
    assert_eq!(eval("(aref (kbd \"RET\") 0)"), "13");
    assert_eq!(eval("(aref (kbd \"TAB\") 0)"), "9");
    assert_eq!(eval("(kbd \"abc\")"), "\"abc\""); // multi-char token → per-char keys
                                                  // Meta / function keys produce a vector.
    assert_eq!(eval("(kbd \"M-x\")"), "[134217848]");
    assert_eq!(eval("(kbd \"C-M-a\")"), "[134217729]");
    assert_eq!(eval("(kbd \"<f1>\")"), "[f1]");
    assert_eq!(eval("(kbd \"C-<f1>\")"), "[C-f1]");
    assert_eq!(eval("(kbd \"C-c M-x a\")"), "[3 134217848 97]");
}

#[test]
fn file_name_extras() {
    assert_eq!(
        eval("(file-name-with-extension \"foo\" \"txt\")"),
        "\"foo.txt\""
    );
    assert_eq!(
        eval("(file-name-with-extension \"foo.x\" \".txt\")"),
        "\"foo.txt\""
    );
    assert_eq!(eval("(file-name-parent-directory \"/a/b/c\")"), "\"/a/b/\"");
    assert_eq!(eval("(file-name-parent-directory \"/\")"), "nil");
    assert_eq!(eval("(file-name-quote \"/a/b\")"), "\"/:/a/b\"");
    assert_eq!(eval("(file-name-quote \"/:/a/b\")"), "\"/:/a/b\"");
    assert_eq!(eval("(file-name-unquote \"/:/a/b\")"), "\"/a/b\"");
    assert_eq!(eval("(file-name-quoted-p \"/:/x\")"), "t");
    assert_eq!(eval("(convert-standard-filename \"/a/b\")"), "\"/a/b\"");
    assert_eq!(eval("(directory-name-p \"/a/\")"), "t");
    assert_eq!(eval("(directory-name-p \"/a\")"), "nil");
}

#[test]
fn cl_struct_slot_introspection() {
    let prelude = "(cl-defstruct foo3 (a 1) (b 2)) ";
    assert_eq!(
        eval(&format!(
            "(progn {prelude}(cl-struct-slot-value 'foo3 'b (make-foo3 :b 9)))"
        )),
        "9"
    );
    assert_eq!(
        eval(&format!(
            "(progn {prelude}(cl-struct-slot-offset 'foo3 'b))"
        )),
        "2"
    );
    assert_eq!(
        eval(&format!("(progn {prelude}(cl-struct-slot-info 'foo3))")),
        "((cl-tag-slot) (a 1) (b 2))"
    );
    // Defaultless slots normalize to (NAME nil) in slot-info.
    assert_eq!(
        eval("(progn (cl-defstruct an legs (eyes 2)) (cl-struct-slot-info 'an))"),
        "((cl-tag-slot) (legs nil) (eyes 2))"
    );
}

#[test]
fn cl_loop_return_do_star_progv() {
    // cl-loop establishes a nil block, so cl-return exits it.
    assert_eq!(eval("(cl-loop for i from 1 to 3 do (cl-return i))"), "1");
    assert_eq!(
        eval("(cl-loop for i in (list 5 6 7) do (when (= i 6) (cl-return-from nil i)))"),
        "6"
    );
    assert_eq!(eval("(cl-loop for i from 1 to 3 collect i)"), "(1 2 3)");
    // cl-do* binds and steps sequentially.
    assert_eq!(
        eval("(cl-do* ((i 0 (1+ i)) (j (* i 2) (* i 2))) ((= i 3) j))"),
        "6"
    );
    // cl-progv dynamically binds, makunbound restores unbound state.
    assert_eq!(eval("(cl-progv (list 'my-pv) (list 42) my-pv)"), "42");
    assert_eq!(
        eval("(progn (defvar mk-v 1) (makunbound 'mk-v) (boundp 'mk-v))"),
        "nil"
    );
    assert_eq!(
        eval("(list (cl-progv (list 'zz1) (list 10) (symbol-value 'zz1)) (boundp 'zz1))"),
        "(10 nil)"
    );
}

#[test]
fn cl_concatenate_and_equalp() {
    assert_eq!(eval("(cl-concatenate 'list (list 1) (list 2))"), "(1 2)");
    assert_eq!(eval("(cl-concatenate 'string \"a\" \"b\")"), "\"ab\"");
    assert_eq!(eval("(cl-concatenate 'vector [1] [2 3])"), "[1 2 3]");
    // cl-equalp: numbers via =, strings case-insensitive, recursive.
    assert_eq!(eval("(cl-equalp \"ABC\" \"abc\")"), "t");
    assert_eq!(eval("(cl-equalp 1 1.0)"), "t");
    assert_eq!(eval("(cl-equalp ?A ?a)"), "nil"); // chars are ints
    assert_eq!(eval("(cl-equalp (list \"AB\" 1) (list \"ab\" 1.0))"), "t");
    assert_eq!(eval("(cl-equalp [1 2] [1 2.0])"), "t");
    assert_eq!(eval("(cl-equalp 5 \"5\")"), "nil");
}

#[test]
fn following_preceding_char_and_cl_prin1() {
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 1) (following-char))"),
        "97"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char 2) (preceding-char))"),
        "97"
    );
    assert_eq!(
        eval("(with-temp-buffer (insert \"abc\") (goto-char (point-max)) (following-char))"),
        "0"
    );
    assert_eq!(eval("(with-temp-buffer (preceding-char))"), "0");
    assert_eq!(eval("(cl-prin1-to-string (list 1 2 3))"), "\"(1 2 3)\"");
    assert_eq!(
        eval("(with-output-to-string (cl-prin1 (list 1 2)))"),
        "\"(1 2)\""
    );
}

#[test]
fn sxhash_self_consistent() {
    // Values aren't bit-compatible with Emacs, but equal objects hash equally.
    assert_eq!(
        eval("(= (sxhash-equal (list 1 2 3)) (sxhash-equal (list 1 2 3)))"),
        "t"
    );
    assert_eq!(
        eval("(= (sxhash-equal \"hello\") (sxhash-equal \"hello\"))"),
        "t"
    );
    assert_eq!(eval("(= (sxhash 5) (sxhash 5))"), "t");
    assert_eq!(eval("(= (sxhash-eq 'foo) (sxhash-eq 'foo))"), "t");
    assert_eq!(eval("(= (sxhash-eql 3.5) (sxhash-eql 3.5))"), "t");
    assert_eq!(
        eval("(= (sxhash-equal (vector 1 (list 2 3))) (sxhash-equal (vector 1 (list 2 3))))"),
        "t"
    );
    // Distinct structures should usually differ; non-negative fixnums always.
    assert_eq!(
        eval("(/= (sxhash-equal (list 1 2)) (sxhash-equal (list 2 1)))"),
        "t"
    );
    assert_eq!(
        eval("(and (integerp (sxhash-equal '(a))) (>= (sxhash-equal '(a)) 0))"),
        "t"
    );
}

#[test]
fn record_constructors() {
    assert_eq!(eval("(record 'foo 1 2 3)"), "#s(foo 1 2 3)");
    assert_eq!(eval("(type-of (record 'foo 1))"), "foo");
    assert_eq!(eval("(recordp (record 'x 1))"), "t");
    assert_eq!(eval("(recordp [1 2 3])"), "nil");
    assert_eq!(eval("(aref (record 'foo 10 20) 1)"), "10");
    assert_eq!(eval("(length (record 'foo 1 2))"), "3");
    assert_eq!(eval("(make-record 'foo 2 0)"), "#s(foo 0 0)");
    assert_eq!(eval("(equal (record 'a 1) (record 'a 1))"), "t");
    assert_eq!(eval("(copy-sequence (record 'a 1 2))"), "#s(a 1 2)");
}

#[test]
fn seqp_and_seq_doseq_return() {
    assert_eq!(eval("(seqp (list 1))"), "t");
    assert_eq!(eval("(seqp \"x\")"), "t");
    assert_eq!(eval("(seqp [1 2])"), "t");
    assert_eq!(eval("(seqp 5)"), "nil");
    assert_eq!(eval("(seqp (make-hash-table))"), "nil");
    // seq-doseq returns the sequence, like Emacs.
    assert_eq!(eval("(seq-doseq (x (list 1 2)) x)"), "(1 2)");
    assert_eq!(eval("(seq-doseq (x [4 5 6]) x)"), "[4 5 6]");
    assert_eq!(
        eval("(let ((r nil)) (seq-doseq (x (list 1 2 3)) (push x r)) r)"),
        "(3 2 1)"
    );
}

#[test]
fn symbol_property_helpers() {
    assert_eq!(
        eval("(progn (setplist 'sy (list 'a 1 'b 2)) (symbol-plist 'sy))"),
        "(a 1 b 2)"
    );
    assert_eq!(eval("(setplist 'sy3 (list 'x 9))"), "(x 9)");
    assert_eq!(eval("(function-get 'car 'foo)"), "nil");
    assert_eq!(
        eval("(progn (function-put 'myf 'pure t) (function-get 'myf 'pure))"),
        "t"
    );
    assert_eq!(eval("(define-symbol-prop 'zz 'p 5)"), "5");
    assert_eq!(
        eval("(progn (define-symbol-prop 'zz2 'p 7) (get 'zz2 'p))"),
        "7"
    );
}

#[test]
fn cl_callf_and_triple_cxr_places() {
    // cl-callf / cl-callf2 rewrite a place through a function.
    assert_eq!(eval("(cl-callf 1+ (car (list 5 6)))"), "6");
    assert_eq!(
        eval("(let ((l (list 5 6))) (cl-callf + (car l) 10) l)"),
        "(15 6)"
    );
    assert_eq!(
        eval("(let ((l (list \"a\" \"b\"))) (cl-callf upcase (car l)) l)"),
        "(\"A\" \"b\")"
    );
    assert_eq!(
        eval("(let ((l (list 5))) (cl-callf2 cons 0 (car l)) l)"),
        "((0 . 5))"
    );
    // Triple c[ad]{3}r combinators as setf places.
    assert_eq!(
        eval("(let ((l (list 1 2 3 4))) (setf (caddr l) 99) l)"),
        "(1 2 99 4)"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3 4))) (setf (cdddr l) '(x)) l)"),
        "(1 2 3 x)"
    );
    assert_eq!(eval("(cl-incf (caddr (list 1 2 3)))"), "4");
}

#[test]
fn setf_places_and_push_pop() {
    // push/pop on generalized places (not just symbols).
    assert_eq!(eval("(let ((l (list 1 2))) (push 0 (cdr l)) l)"), "(1 0 2)");
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (push 9 (nth 1 l)) l)"),
        "(1 (9 . 2) 3)"
    );
    assert_eq!(
        eval("(let ((h (make-hash-table))) (push 1 (gethash 'k h)) (gethash 'k h))"),
        "(1)"
    );
    assert_eq!(
        eval("(let ((l (list (list 1 2) 3))) (pop (car l)) l)"),
        "((2) 3)"
    );
    // seq-elt as a setf place.
    assert_eq!(
        eval("(let ((l (list 1 2 3 4))) (setf (seq-elt l 2) 99) l)"),
        "(1 2 99 4)"
    );
    // alist-get with TESTFN and REMOVE.
    assert_eq!(
        eval("(let ((al (list (cons \"x\" 1)))) (setf (alist-get \"x\" al nil nil #'equal) 9) al)"),
        "((\"x\" . 9))"
    );
    assert_eq!(
        eval("(let ((al (list (cons 'a 1)(cons 'b 2)))) (setf (alist-get 'a al nil 'remove) nil) al)"),
        "((b . 2))"
    );
    assert_eq!(
        eval("(let ((al (list (cons 'a 1)))) (setf (alist-get 'a al nil 'remove) 5) al)"),
        "((a . 5))"
    );
}

#[test]
fn random_in_range_and_typed() {
    // (random 1) is always 0; (random N) stays in [0, N); (random)/(random t) are integers.
    assert_eq!(eval("(random 1)"), "0");
    assert_eq!(
        eval("(let (bad) (dotimes (_ 500 (if bad 'bad 'ok)) (let ((r (random 7))) (when (or (< r 0) (>= r 7)) (setq bad t)))))"),
        "ok"
    );
    assert_eq!(eval("(integerp (random))"), "t");
    assert_eq!(eval("(integerp (random t))"), "t");
}

#[test]
fn truncate_string_to_width_ellipsis() {
    assert_eq!(
        eval("(truncate-string-to-width \"hello world\" 8 nil nil \"…\")"),
        "\"hello w…\""
    );
    assert_eq!(
        eval("(truncate-string-to-width \"hello world\" 8 nil nil t)"),
        "\"hello w…\""
    );
    assert_eq!(
        eval("(truncate-string-to-width \"hello\" 8 nil nil \"…\")"),
        "\"hello\""
    );
    assert_eq!(
        eval("(truncate-string-to-width \"hello world\" 8)"),
        "\"hello wo\""
    );
    assert_eq!(
        eval("(truncate-string-to-width \"abcdef\" 4 nil nil \"..\")"),
        "\"ab..\""
    );
}

#[test]
fn replace_match_string_mode() {
    assert_eq!(
        eval("(let ((s \"a1b\")) (string-match \"[0-9]\" s) (replace-match \"X\" t t s))"),
        "\"aXb\""
    );
    assert_eq!(
        eval("(let ((s \"x=5\")) (string-match \"\\\\(.\\\\)=\\\\(.\\\\)\" s) (replace-match \"\\\\2=\\\\1\" t nil s))"),
        "\"5=x\""
    );
}

#[test]
fn format_integer_precision() {
    assert_eq!(eval("(format \"%.2d\" 1)"), "\"01\"");
    assert_eq!(eval("(format \"%.5d\" 42)"), "\"00042\"");
    assert_eq!(eval("(format \"%5.3d\" 7)"), "\"  007\"");
    assert_eq!(eval("(format \"%.2x\" 5)"), "\"05\"");
    assert_eq!(eval("(format \"%.3d\" -5)"), "\"-005\"");
    assert_eq!(eval("(format \"%.0d\" 0)"), "\"\"");
}

#[test]
fn format_seconds_basic() {
    assert_eq!(eval("(format-seconds \"%h:%m:%s\" 3661)"), "\"1:1:1\"");
    assert_eq!(
        eval("(format-seconds \"%Y %D %H %M %S\" 90061)"),
        "\"0 years 1 day 1 hour 1 minute 1 second\""
    );
    assert_eq!(eval("(format-seconds \"%.2h hours\" 3600)"), "\"01 hours\"");
    assert_eq!(
        eval("(format-seconds \"%dd %hh %x%mm %ss\" 90000)"),
        "\"1d 1h\""
    );
}

#[test]
fn rx_optional_and_syntax() {
    // `?` (reads as char 32) / `??` (char 63) are the optional operators.
    assert_eq!(eval("(rx (seq \"a\" (? \"b\") \"c\"))"), "\"ab?c\"");
    assert_eq!(eval("(rx \"a\" (?? \"b\") \"c\")"), "\"ab??c\"");
    assert_eq!(eval("(rx (? (+ digit)))"), "\"\\\\(?:[[:digit:]]+\\\\)?\"");
    // (syntax CLASS): `\w` shorthand for word, `\sC` otherwise; atomic for quantifiers.
    assert_eq!(eval("(rx (syntax whitespace))"), "\"\\\\s-\"");
    assert_eq!(eval("(rx (syntax word))"), "\"\\\\w\"");
    assert_eq!(eval("(rx (1+ (syntax whitespace)))"), "\"\\\\s-+\"");
    // It actually matches.
    assert_eq!(
        eval("(string-match-p (rx \"a\" (? \"b\") \"c\") \"ac\")"),
        "0"
    );
}

#[test]
fn format_error_uses_error_message_string() {
    reset_host();
    let _ = eval_str(""); // load prelude
                          // Internal "condition: data" strings render like Emacs's error-message-string.
    assert_eq!(
        elisprs::format_error("void-variable: foo"),
        "Symbol's value as variable is void: foo"
    );
    assert_eq!(
        elisprs::format_error("wrong-type-argument: listp 5"),
        "Wrong type argument: listp, 5"
    );
    assert_eq!(elisprs::format_error("custom message"), "custom message");
}

#[test]
fn error_data_conditions() {
    // void-variable: catchable as its own condition, DATA is the symbol.
    assert_eq!(
        eval("(condition-case e (symbol-value 'zzz-unbound) (void-variable e))"),
        "(void-variable zzz-unbound)"
    );
    assert_eq!(eval("(condition-case e zzz9 (void-variable 'ok))"), "ok");
    // void-function DATA is the symbol, not a string.
    assert_eq!(
        eval("(condition-case e (funcall 'zzz-nofn) (void-function e))"),
        "(void-function zzz-nofn)"
    );
    // args-out-of-range carries (ARRAY INDEX) and uses the right condition.
    assert_eq!(
        eval("(condition-case e (aref [1 2 3] 5) (args-out-of-range e))"),
        "(args-out-of-range [1 2 3] 5)"
    );
    assert_eq!(
        eval("(condition-case e (aref \"abc\" 9) (error e))"),
        "(args-out-of-range \"abc\" 9)"
    );
}

#[test]
fn cl_fill_replace_nsubstitute() {
    assert_eq!(eval("(cl-nsubstitute 9 2 (list 1 2 3 2))"), "(1 9 3 9)");
    assert_eq!(
        eval("(cl-nsubstitute 9 2 (list 1 2 3 2) :count 1)"),
        "(1 9 3 2)"
    );
    assert_eq!(
        eval("(cl-nsubstitute-if 0 #'cl-evenp (list 1 2 3 4))"),
        "(1 0 3 0)"
    );
    assert_eq!(
        eval("(cl-fill (list 1 2 3 4) 0 :start 1 :end 3)"),
        "(1 0 0 4)"
    );
    assert_eq!(eval("(cl-fill (vector 1 2 3) 7)"), "[7 7 7]");
    assert_eq!(
        eval("(cl-replace (list 1 2 3 4) (list 9 8) :start1 1)"),
        "(1 9 8 4)"
    );
    assert_eq!(eval("(cl-replace (vector 1 2 3) (vector 7 8))"), "[7 8 3]");
    assert_eq!(eval("(cl-replace \"abcde\" \"XY\" :start1 2)"), "\"abXYe\"");
}

#[test]
fn cl_loop_by_step_and_index() {
    // `for V in LIST by FN` steps the tail with FN instead of cdr.
    assert_eq!(
        eval("(cl-loop for i in '(1 2 3 4 5 6) by #'cddr collect i)"),
        "(1 3 5)"
    );
    assert_eq!(
        eval("(cl-loop for (a b) in '((1 2)(3 4)(5 6)) by #'cddr collect a)"),
        "(1 5)"
    );
    // `using (index V)` binds a 0-based iteration counter.
    assert_eq!(
        eval(
            "(cl-loop for k being the elements of '(10 20 30) using (index i) collect (list i k))"
        ),
        "((0 10) (1 20) (2 30))"
    );
    // Regression: plain forms still work.
    assert_eq!(eval("(cl-loop for i in '(1 2 3) collect i)"), "(1 2 3)");
}

#[test]
fn print_quoted_flag() {
    // Default (t): quote/function forms abbreviate.
    assert_eq!(eval("(prin1-to-string ''a)"), "\"'a\"");
    assert_eq!(eval("(prin1-to-string '(function f))"), "\"#'f\"");
    // Bound to nil: print the forms in full.
    assert_eq!(
        eval("(let ((print-quoted nil)) (prin1-to-string '(quote a)))"),
        "\"(quote a)\""
    );
    assert_eq!(
        eval("(let ((print-quoted nil)) (prin1-to-string '(function f)))"),
        "\"(function f)\""
    );
    assert_eq!(
        eval("(let ((print-quoted nil)) (prin1-to-string '(a 'b c)))"),
        "\"(a (quote b) c)\""
    );
}

#[test]
fn reader_char_and_string_escapes() {
    // Char literals: hex / octal / unicode / space / control.
    assert_eq!(eval("?\\x41"), "65");
    assert_eq!(eval("?\\x1F600"), "128512");
    assert_eq!(eval("?\\101"), "65");
    assert_eq!(eval("?\\U0001F600"), "128512");
    assert_eq!(eval("?\\s"), "32");
    assert_eq!(eval("?\\^a"), "1");
    assert_eq!(eval("?\\C-a"), "1");
    // String escapes: \x and \U, \s (space), \^ / \C- (control).
    assert_eq!(eval("(aref \"\\x41\" 0)"), "65");
    assert_eq!(eval("(aref \"\\U0001F600\" 0)"), "128512");
    assert_eq!(eval("(length \"\\x41\\x42\")"), "2");
    assert_eq!(eval("(aref \"a\\sb\" 1)"), "32");
    assert_eq!(eval("(aref \"x\\^Iy\" 1)"), "9");
    assert_eq!(eval("(aref \"\\C-a\" 0)"), "1");
}

#[test]
fn string_compare_accepts_symbols() {
    // Emacs's string comparators coerce symbols via their print names.
    assert_eq!(eval("(string< 'abc 'abd)"), "t");
    assert_eq!(eval("(string< 'abc \"abd\")"), "t");
    assert_eq!(eval("(string= 'foo \"foo\")"), "t");
    assert_eq!(eval("(string= nil \"nil\")"), "t");
    assert_eq!(eval("(string> 'b 'a)"), "t");
    assert_eq!(eval("(string-greaterp 'b 'a)"), "t");
    assert_eq!(eval("(string-lessp 'a 'b)"), "t");
    assert_eq!(
        eval("(sort (list 'banana 'apple 'cherry) #'string<)"),
        "(apple banana cherry)"
    );
}

#[test]
fn cl_remf_removes_first_plist_entry() {
    assert_eq!(
        eval("(let ((p (list :a 1 :b 2))) (list (cl-remf p :a) p))"),
        "(t (:b 2))"
    );
    assert_eq!(
        eval("(let ((p (list :a 1 :b 2))) (cl-remf p :z) p)"),
        "(:a 1 :b 2)"
    );
    // Only the first occurrence is removed.
    assert_eq!(
        eval("(let ((p (list :a 1 :b 2 :a 3))) (cl-remf p :a) p)"),
        "(:b 2 :a 3)"
    );
    assert_eq!(eval("(let ((p (list :a 1))) (cl-remf p :a) p)"), "nil");
}

#[test]
fn string_lines_and_glyph_split() {
    assert_eq!(eval("(string-lines \"a\\nb\\nc\")"), "(\"a\" \"b\" \"c\")");
    assert_eq!(eval("(string-lines \"a\\n\\nb\" t)"), "(\"a\" \"b\")");
    assert_eq!(eval("(string-lines \"a\\nb\\n\")"), "(\"a\" \"b\")");
    assert_eq!(eval("(string-lines \"a\\nb\" nil t)"), "(\"a\n\" \"b\")");
    assert_eq!(eval("(string-lines \"\")"), "(\"\")");
    assert_eq!(eval("(string-glyph-split \"abc\")"), "(\"a\" \"b\" \"c\")");
    assert_eq!(eval("(string-glyph-split \"\")"), "nil");
    // negative pad length signals wrong-type-argument like Emacs
    assert!(eval_str("(string-pad \"hi\" -4)").is_err());
    assert_eq!(
        eval("(condition-case e (string-pad \"hi\" -4) (wrong-type-argument (cadr e)))"),
        "natnump"
    );
}

#[test]
fn seq_functions_with_testfn() {
    assert_eq!(
        eval("(seq-uniq '(1 2 3 4) (lambda (a b) (= (% a 2) (% b 2))))"),
        "(1 2)"
    );
    assert_eq!(eval("(seq-contains-p '(1 2 3) 2 #'=)"), "t");
    assert_eq!(eval("(seq-contains-p '(1 2 3) 9)"), "nil");
    assert_eq!(eval("(seq-position '(1 2 3) 2 #'=)"), "1");
    assert_eq!(eval("(seq-difference '(1 2 3) '(2) #'=)"), "(1 3)");
    assert_eq!(eval("(seq-intersection '(1 2 3) '(2 3 4) #'=)"), "(2 3)");
    assert_eq!(eval("(seq-union '(1 2) '(2 3) #'=)"), "(1 2 3)");
}

#[test]
fn seq_into_and_indexed_nonlist() {
    // seq-into converts any input type (string included) to the target type.
    assert_eq!(eval("(seq-into \"abc\" 'vector)"), "[97 98 99]");
    assert_eq!(eval("(seq-into \"abc\" 'list)"), "(97 98 99)");
    assert_eq!(eval("(seq-into '(1 2) 'vector)"), "[1 2]");
    // seq-do-indexed / seq-map-indexed iterate any sequence with its index.
    assert_eq!(
        eval("(let (r) (seq-do-indexed (lambda (e i) (push (list i e) r)) [10 20]) (reverse r))"),
        "((0 10) (1 20))"
    );
    assert_eq!(
        eval("(seq-map-indexed (lambda (e i) (cons i e)) [10 20])"),
        "((0 . 10) (1 . 20))"
    );
    assert_eq!(
        eval("(seq-map-indexed (lambda (e i) (cons i e)) \"ab\")"),
        "((0 . 97) (1 . 98))"
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
