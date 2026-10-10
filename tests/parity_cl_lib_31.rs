//! Differential cases against GNU Emacs 31.1 for the macro and library layers
//! that were reimplemented subsets before: the `cl-loop` clause parser, cl-seq's
//! keyword handling, `cl-case`/`cl-parse-integer`/`cl-endp`, and the order in
//! which a call to a void function signals relative to its argument forms.
//!
//! Every expectation is the output of `emacs -Q --batch` (with `cl-lib`, `seq`
//! and `subr-x` loaded) for the same form: `=` and the printed value, or `!` and
//! the printed error object.

use elisprs::{eval_str, print, reset_host};

/// `eval` FORM under `condition-case` and render the value or the error object the
/// way the reference run did. Going through `eval` keeps macro expansion lazy, so
/// an expander failure is caught by the `condition-case` as it is in Emacs.
fn check(form: &str, expected: &str) {
    reset_host();
    let src = format!(
        "(condition-case e (eval '{form} t) (:success (concat \"=\" (prin1-to-string e))) \
         (error (concat \"!\" (prin1-to-string e))))"
    );
    let v = eval_str(&src).expect("eval failed");
    assert_eq!(print(&v, false), expected, "{form}");
}

#[test]
fn cl_loop_parallel_and_accumulation_clauses() {
    check(
        r#"(cl-loop for i from 1 to 3 and j = 0 then i collect (list i j))"#,
        r#"=((1 0) (2 1) (3 2))"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3) and y = 0 then x collect (list x y))"#,
        r#"=((1 0) (2 1) (3 2))"#,
    );
    check(
        r#"(cl-loop for x on '(1 2 3 4) by #'cddr collect x)"#,
        r#"=((1 2 3 4) (3 4))"#,
    );
    check(
        r#"(cl-loop for (a . b) on '(1 2 3) by #'cddr collect (cons a b))"#,
        r#"=((1 2 3) (3))"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3 4) if (cl-evenp x) collect x into e else collect x into o end finally return (list e o))"#,
        r#"=((2 4) (1 3))"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3 4) when (cl-evenp x) collect x and sum x into s finally return s)"#,
        r#"=6"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3 4) when (> x 1) when (< x 4) collect x)"#,
        r#"=(2 3)"#,
    );
    check(
        r#"(cl-loop for x being the elements of '(1 2 3) using (index i) collect (list i x))"#,
        r#"=((0 1) (1 2) (2 3))"#,
    );
    check(
        r#"(cl-loop for k being the hash-keys of (let ((h (make-hash-table))) (puthash 1 2 h) h) using (hash-values v) collect (list k v))"#,
        r#"=((1 2))"#,
    );
    check(
        r#"(cl-loop for x across-ref (vector 1 2 3) collect x)"#,
        r#"=(1 2 3)"#,
    );
    check(
        r#"(cl-loop with (a b) = '(1 2) and c = 3 return (list a b c))"#,
        r#"=(1 2 3)"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3) minimize x into mn maximize x into mx finally return (list mn mx))"#,
        r#"=(1 3)"#,
    );
    check(
        r#"(cl-loop named outer for i in '(1 2) do (cl-loop for j in '(3 4) do (when (= j 4) (cl-return-from outer (list i j)))))"#,
        r#"=(1 4)"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3) for acc = x then (+ acc x) collect acc)"#,
        r#"=(1 3 6)"#,
    );
    check(r#"(cl-loop for i from 3 above 0 collect i)"#, r#"=(3 2 1)"#);
    check(
        r#"(cl-loop for i downfrom 3 to 1 collect i)"#,
        r#"=(3 2 1)"#,
    );
    check(r#"(cl-loop for x in nil maximize x)"#, r#"=nil"#);
    check(
        r#"(cl-loop for x in '(1 2 3) vconcat (vector x))"#,
        r#"=[1 2 3]"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3) thereis (and (> x 1) x))"#,
        r#"=2"#,
    );
    check(r#"(cl-loop for x in '(1 2 3) never (> x 5))"#, r#"=t"#);
    check(r#"(cl-loop repeat 3 collect 'x)"#, r#"=(x x x)"#);
}

#[test]
fn cl_loop_errors_use_the_macros_own_messages() {
    check(
        r#"(cl-loop bogus 1)"#,
        r#"!(error "Expected a cl-loop keyword, found bogus")"#,
    );
    check(
        r#"(cl-loop for x in '(1 2) collect x bogus)"#,
        r#"!(error "Expected a cl-loop keyword, found bogus")"#,
    );
    check(
        r#"(cl-loop collect)"#,
        r#"!(error "Malformed ‘cl-loop’ macro")"#,
    );
    check(
        r#"(cl-loop for x in '(1 2 3) into r finally return r)"#,
        r#"!(error "Expected a cl-loop keyword, found into")"#,
    );
}

#[test]
fn cl_seq_keyword_corners() {
    check(
        r#"(cl-remove-duplicates (list 1 2) :test nil)"#,
        r#"=(1 2)"#,
    );
    check(r#"(cl-find 1 (list 1) :key nil)"#, r#"=1"#);
    check(
        r#"(cl-remove 1 (list 1 2 1) :from-end t :count 1)"#,
        r#"=(2 1)"#,
    );
    check(r#"(cl-remove 1 (list 1 2 1) :count 1)"#, r#"=(2 1)"#);
    check(r#"(cl-position 1 (list 1 2 1) :from-end t)"#, r#"=2"#);
    check(r#"(cl-count 1 (list 1 2 1) :start 1)"#, r#"=1"#);
    check(r#"(cl-find 2 (list 1 2) :start 1)"#, r#"=2"#);
    check(r#"(cl-find 1 (list 1 2) :end 0)"#, r#"=nil"#);
    check(
        r#"(cl-reduce #'+ (list 1 2 3) :initial-value 10)"#,
        r#"=16"#,
    );
    check(
        r#"(cl-reduce #'list (list 1 2 3) :from-end t)"#,
        r#"=(1 (2 3))"#,
    );
    check(r#"(cl-search (list 2) (list 1 2 3))"#, r#"=1"#);
    check(r#"(cl-mismatch (list 1 2) (list 1 3))"#, r#"=1"#);
    check(
        r#"(cl-merge 'list (list 1 3) (list 2 4) #'<)"#,
        r#"=(1 2 3 4)"#,
    );
    check(r#"(cl-union (list 1 2) (list 2 3))"#, r#"=(3 1 2)"#);
    check(
        r#"(cl-set-exclusive-or (list 1 2) (list 2 3))"#,
        r#"=(1 3)"#,
    );
    check(
        r#"(cl-find 1 (list 1) :bogus 1)"#,
        r#"!(error "Bad keyword argument :bogus")"#,
    );
    check(
        r#"(cl-find 1 (list 1) :bogus 1 :allow-other-keys t)"#,
        r#"=1"#,
    );
    check(
        r#"(let ((s (copy-sequence "abc"))) (setf (cl-subseq s 1) "XY") s)"#,
        r#"="aXY""#,
    );
}

#[test]
fn cl_check_type_and_case_contracts() {
    check(r#"(cl-endp 5)"#, r#"!(wrong-type-argument list 5 x)"#);
    check(
        r#"(cl-list-length 5)"#,
        r#"!(wrong-type-argument list 5 x)"#,
    );
    check(r#"(cl-list-length '(1 2))"#, r#"=2"#);
    check(
        r#"(cl-case 'a (t 'x) (a 'y))"#,
        r#"!(error "Misplaced t or ‘otherwise’ clause")"#,
    );
    check(r#"(cl-case 'a ((a b) 1) (t 2))"#, r#"=1"#);
    check(r#"(cl-case 'z (a 1) (otherwise 2))"#, r#"=2"#);
    check(
        r#"(cl-case 'a (a 1) (a 2))"#,
        r#"!(error "Duplicate key in case: a")"#,
    );
    check(
        r#"(cl-ecase 9 (1 'a))"#,
        r#"!(error "cl-ecase failed: 9, (1)")"#,
    );
    check(
        r#"(cl-etypecase 'a (string 's))"#,
        r#"!(error "cl-etypecase failed: a, (string)")"#,
    );
    check(r#"(cl-typecase 3 (string 's) (integer 'i))"#, r#"=i"#);
    check(r#"(cl-parse-integer "x12" :start 1)"#, r#"=12"#);
    check(r#"(cl-parse-integer "zz" :junk-allowed t)"#, r#"=nil"#);
    check(
        r#"(cl-parse-integer "zz")"#,
        r#"!(error "Not an integer string: ‘zz’")"#,
    );
    check(r#"(cl-parse-integer "  -42  ")"#, r#"=-42"#);
    check(r#"(cl-parse-integer "ff" :radix 16)"#, r#"=255"#);
    check(
        r#"(cl-parse-integer "12" :start 1 :end 5)"#,
        r#"!(error "Bad interval: [1, 5)")"#,
    );
}

#[test]
fn sort_argument_list_follows_fsort() {
    check(
        r#"(sort '(1 2) :bogus 1)"#,
        r#"!(error "Invalid keyword argument" :bogus)"#,
    );
    check(r#"(sort '(1 2) :key)"#, r#"!(void-function :key)"#);
    check(r#"(sort '(2 1) :lessp)"#, r#"!(void-function :lessp)"#);
    check(
        r#"(sort '(2 1) :lessp #'> :reverse)"#,
        r#"!(error "Invalid argument list")"#,
    );
    check(
        r#"(sort '(2 1 3) #'< :key #'-)"#,
        r#"!(error "Invalid argument list")"#,
    );
    check(
        r#"(sort '(2 1 3) :key #'- #'<)"#,
        r#"!(error "Invalid argument list")"#,
    );
    check(r#"(sort '(2 1 3) :key #'- :lessp #'<)"#, r#"=(3 2 1)"#);
    check(r#"(sort '(2 1 3) :reverse t :reverse nil)"#, r#"=(1 2 3)"#);
    check(
        r#"(sort '((1 . a) (1 . b) (0 . c)) :key #'car :reverse t)"#,
        r#"=((1 . a) (1 . b) (0 . c))"#,
    );
    check(
        r#"(sort '((1 . a) (1 . b) (0 . c)) :key #'car)"#,
        r#"=((0 . c) (1 . a) (1 . b))"#,
    );
    check(
        r#"(sort '(2 1 3) :predicate #'<)"#,
        r#"!(error "Invalid keyword argument" :predicate)"#,
    );
    check(
        r#"(sort 5 :bogus 1)"#,
        r#"!(error "Invalid keyword argument" :bogus)"#,
    );
    check(r#"(let ((l (list 3 1 2))) (sort l) l)"#, r#"=(3 1 2)"#);
    check(
        r#"(let ((l (list 3 1 2))) (sort l :in-place t) l)"#,
        r#"=(1 2 3)"#,
    );
    check(r#"(let ((l (list 3 1 2))) (sort l #'<) l)"#, r#"=(1 2 3)"#);
}

#[test]
fn void_head_is_signalled_before_its_arguments() {
    check(
        r#"(let ((x 1)) (condition-case nil (nonexistent-fn-xyz (setq x 2)) (void-function nil)) x)"#,
        r#"=1"#,
    );
    check(
        r#"(let ((x 1)) (condition-case nil (nil (setq x 2)) (void-function nil)) x)"#,
        r#"=1"#,
    );
    check(
        r#"(let ((x 1)) (condition-case nil (t (setq x 2)) (void-function nil)) x)"#,
        r#"=1"#,
    );
    check(
        r#"(let ((x 1)) (condition-case nil (progn (defalias 'void-alias-xyz 'nonexistent-fn-xyz) (void-alias-xyz (setq x 2))) (void-function nil)) x)"#,
        r#"=1"#,
    );
    check(
        r#"(let ((x 1)) (condition-case e (nonexistent-fn-xyz (setq x 2)) (void-function e)))"#,
        r#"=(void-function nonexistent-fn-xyz)"#,
    );
    check(
        r#"(let ((x 1)) (condition-case e (nil (setq x 2)) (void-function e)))"#,
        r#"=(void-function nil)"#,
    );
    check(
        r#"(let ((x 1)) (condition-case e (void-alias-xyz2 (setq x 2)) (void-function e)))"#,
        r#"=(void-function void-alias-xyz2)"#,
    );
    check(
        r#"(let ((x 1)) (condition-case nil (funcall 'nonexistent-fn-xyz (setq x 2)) (void-function nil)) x)"#,
        r#"=2"#,
    );
}
