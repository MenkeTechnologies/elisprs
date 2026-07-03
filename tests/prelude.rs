//! Prelude-library coverage: the elisp defined in `prelude.rs` (alist/plist
//! accessors, the seq-* and cl-* families, the c[ad]+r combinators, list
//! utilities, and the place-mutating macros). These run as ordinary elisp on
//! top of the builtins, so they exercise re-entrancy and closure capture too.

use elisprs::{eval_str, print, reset_host};

fn eval(src: &str) -> String {
    reset_host();
    let v = eval_str(src).expect("eval failed");
    print(&v, true)
}

#[test]
fn cadr_family() {
    assert_eq!(eval("(caddr '(1 2 3 4))"), "3");
    assert_eq!(eval("(cddr '(1 2 3 4))"), "(3 4)");
    assert_eq!(eval("(list (caar '((1 2) 3)) (cdar '((1 . 9))))"), "(1 9)");
    assert_eq!(eval("(cadddr '(1 2 3 4 5))"), "4");
}

#[test]
fn alist_accessors() {
    assert_eq!(eval("(assoc 'b '((a . 1) (b . 2)))"), "(b . 2)");
    assert_eq!(eval("(assq 'b '((a . 1) (b . 2)))"), "(b . 2)");
    assert_eq!(eval("(alist-get 'b '((a . 1) (b . 2)))"), "2");
    assert_eq!(eval("(alist-get 'z '((a . 1)))"), "nil");
    assert_eq!(eval("(rassq 2 '((a . 1) (b . 2)))"), "(b . 2)");
    // assoc with list-valued entries composes with cadr.
    assert_eq!(eval("(cadr (assoc 'b '((a 1) (b 2) (c 3))))"), "2");
}

#[test]
fn plist_accessors() {
    assert_eq!(eval("(plist-get '(:a 1 :b 2) :b)"), "2");
    assert_eq!(eval("(plist-get '(:a 1 :b 2) :missing)"), "nil");
    assert_eq!(eval("(plist-member '(:a 1 :b 2) :b)"), "(:b 2)");
}

#[test]
fn membership() {
    assert_eq!(eval("(member 3 '(1 2 3 4))"), "(3 4)");
    assert_eq!(eval("(member 9 '(1 2 3))"), "nil");
    assert_eq!(eval("(memq 'c '(a b c d))"), "(c d)");
    assert_eq!(eval("(keywordp :foo)"), "t");
    assert_eq!(eval("(keywordp 'foo)"), "nil");
}

#[test]
fn list_construction_and_removal() {
    assert_eq!(eval("(number-sequence 1 5)"), "(1 2 3 4 5)");
    assert_eq!(eval("(number-sequence 0 10 2)"), "(0 2 4 6 8 10)");
    assert_eq!(eval("(make-list 3 'x)"), "(x x x)");
    assert_eq!(eval("(delq 2 (list 1 2 3 2 4))"), "(1 3 4)");
    assert_eq!(eval("(remove 2 '(1 2 3 2 4))"), "(1 3 4)");
    assert_eq!(eval("(delete 2 (list 1 2 2 3))"), "(1 3)");
    assert_eq!(eval("(remq 'a '(a b a c))"), "(b c)");
}

#[test]
fn list_accessors() {
    assert_eq!(eval("(last '(1 2 3 4))"), "(4)");
    assert_eq!(eval("(elt '(a b c) 1)"), "b");
    assert_eq!(eval("(elt (vector 10 20 30) 2)"), "30");
    assert_eq!(eval("(safe-length '(1 2 3))"), "3");
    assert_eq!(eval("(nthcdr 2 '(a b c d))"), "(c d)");
}

#[test]
fn seq_family() {
    assert_eq!(eval("(seq-map (lambda (x) (* x x)) '(1 2 3))"), "(1 4 9)");
    assert_eq!(eval("(seq-find (lambda (x) (> x 2)) '(1 2 3 4))"), "3");
    assert_eq!(eval("(seq-every-p (lambda (x) (> x 0)) '(1 2 3))"), "t");
    assert_eq!(eval("(seq-every-p (lambda (x) (> x 1)) '(1 2 3))"), "nil");
    assert_eq!(
        eval("(seq-some (lambda (x) (and (> x 2) x)) '(1 2 3))"),
        "3"
    );
    assert_eq!(eval("(seq-contains-p '(1 2 3) 2)"), "t");
    assert_eq!(
        eval("(seq-count (lambda (x) (evenp x)) '(1 2 3 4 5 6))"),
        "3"
    );
    assert_eq!(eval("(seq-reverse '(1 2 3))"), "(3 2 1)");
    assert_eq!(eval("(seq-empty-p nil)"), "t");
    assert_eq!(eval("(seq-empty-p '(1))"), "nil");
}

#[test]
fn cl_family() {
    assert_eq!(eval("(cl-reduce (lambda (a b) (+ a b)) '(1 2 3 4))"), "10");
    assert_eq!(
        eval("(cl-remove-if (lambda (x) (evenp x)) '(1 2 3 4 5))"),
        "(1 3 5)"
    );
    assert_eq!(
        eval("(cl-remove-if-not (lambda (x) (evenp x)) '(1 2 3 4 5))"),
        "(2 4)"
    );
    assert_eq!(eval("(cl-find-if (lambda (x) (> x 3)) '(1 2 3 4 5))"), "4");
    assert_eq!(eval("(cl-some (lambda (x) (> x 4)) '(1 2 3 4 5))"), "t");
    assert_eq!(eval("(cl-every (lambda (x) (> x 0)) '(1 2 3))"), "t");
    assert_eq!(
        eval("(list (cl-first '(a b c)) (cl-second '(a b c)) (cl-third '(a b c)))"),
        "(a b c)"
    );
    assert_eq!(eval("(cl-rest '(a b c))"), "(b c)");
}

#[test]
fn cl_set_ops_test_and_key() {
    // Default comparison is `eql', not `equal': distinct string objects stay.
    assert_eq!(
        eval("(cl-remove-duplicates (list \"a\" \"a\" \"b\"))"),
        "(\"a\" \"a\" \"b\")"
    );
    assert_eq!(
        eval("(cl-remove-duplicates (list \"a\" \"a\" \"b\") :test #'equal)"),
        "(\"a\" \"b\")"
    );
    // Default keeps last occurrence; :from-end keeps first.
    assert_eq!(eval("(cl-remove-duplicates '(1 2 1 3 2))"), "(1 3 2)");
    assert_eq!(
        eval("(cl-remove-duplicates '(1 2 1 3 2) :from-end t)"),
        "(1 2 3)"
    );
    // :key selects the comparison value.
    assert_eq!(
        eval("(cl-remove-duplicates '((1 . a) (1 . b) (2 . c)) :key #'car)"),
        "((1 . b) (2 . c))"
    );
    assert_eq!(
        eval("(cl-remove-duplicates '((1 . a) (1 . b) (2 . c)) :key #'car :from-end t)"),
        "((1 . a) (2 . c))"
    );
    // Union order: non-dup items of shorter list prepended onto longer.
    assert_eq!(eval("(cl-union '(1 2 3) '(3 4 5))"), "(5 4 1 2 3)");
    assert_eq!(
        eval("(cl-union '(\"a\") '(\"a\" \"b\"))"),
        "(\"a\" \"a\" \"b\")"
    );
    assert_eq!(
        eval("(cl-union '((1 . a)) '((1 . b) (2 . c)) :key #'car)"),
        "((1 . b) (2 . c))"
    );
    assert_eq!(eval("(cl-union nil '(1 2))"), "(1 2)");
    // Intersection returns matches from the shorter list in push order.
    assert_eq!(eval("(cl-intersection '(1 2 3) '(2 3 4))"), "(3 2)");
    assert_eq!(
        eval("(cl-intersection '((1 . a) (3 . x)) '((1 . b) (2 . c)) :key #'car)"),
        "((1 . b))"
    );
    // Set-difference keeps LIST1 items absent from LIST2, original order.
    assert_eq!(eval("(cl-set-difference '(1 2 3 4) '(2 4))"), "(1 3)");
    assert_eq!(
        eval("(cl-set-difference '((1 . a) (3 . x)) '((1 . b)) :key #'car)"),
        "((3 . x))"
    );
}

#[test]
fn seq_group_by_ordering() {
    // Reverse first-encounter key order, forward item order (Emacs fold order).
    assert_eq!(
        eval("(seq-group-by (lambda (x) (= 0 (mod x 2))) '(1 2 3 4 5))"),
        "((t 2 4) (nil 1 3 5))"
    );
    assert_eq!(
        eval("(seq-group-by #'car '((a . 1) (b . 2) (a . 3)))"),
        "((b (b . 2)) (a (a . 1) (a . 3)))"
    );
    assert_eq!(eval("(seq-group-by #'identity '())"), "nil");
}

#[test]
fn place_mutating_macros() {
    assert_eq!(eval("(let ((x 5)) (incf x) x)"), "6");
    assert_eq!(eval("(let ((x 5)) (decf x) x)"), "4");
    // incf/decf take an optional step amount.
    assert_eq!(eval("(let ((x 5)) (incf x 10) x)"), "15");
    assert_eq!(eval("(let ((x 5)) (decf x 3) x)"), "2");
    assert_eq!(eval("(let ((l '(1 2 3))) (pop l))"), "1");
    assert_eq!(eval("(let ((l '(1 2 3))) (pop l) l)"), "(2 3)");
    assert_eq!(eval("(let ((l '(2 3))) (push 1 l) l)"), "(1 2 3)");
}

#[test]
fn setf_generalized_places() {
    // plain variable
    assert_eq!(eval("(let ((x 1)) (setf x 9) x)"), "9");
    // cons cells
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (setf (car l) 9) l)"),
        "(9 2 3)"
    );
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (setf (cdr l) '(8)) l)"),
        "(1 8)"
    );
    // nth / elt into a list
    assert_eq!(
        eval("(let ((l (list 1 2 3))) (setf (nth 1 l) 9) l)"),
        "(1 9 3)"
    );
    assert_eq!(
        eval("(let ((l (list 10 20 30))) (setf (elt l 2) 99) l)"),
        "(10 20 99)"
    );
    // aref into a vector
    assert_eq!(
        eval("(let ((v (vector 0 0 0))) (setf (aref v 1) 7) v)"),
        "[0 7 0]"
    );
    // gethash into a hash table
    assert_eq!(
        eval("(let ((h (make-hash-table))) (setf (gethash 'k h) 42) (gethash 'k h))"),
        "42"
    );
    // multiple place/value pairs, left to right
    assert_eq!(
        eval("(let ((a 1) (b 2)) (setf a 10 b 20) (list a b))"),
        "(10 20)"
    );
}

#[test]
fn map_plist_shaped_lists() {
    // A list whose car is an atom is a plist map (KEY VALUE KEY VALUE...),
    // matching Emacs map.el's `map--plist-p'. Values captured from emacs 30.2.
    assert_eq!(eval("(map-elt '(1 2 3) 1)"), "2");
    assert_eq!(eval("(map-elt '(:a 1 :b 2) :b)"), "2");
    assert_eq!(eval("(map-elt '(:a 1 :b 2) :z 'nf)"), "nf");
    assert_eq!(eval("(map-keys '(:a 1 :b 2))"), "(:a :b)");
    assert_eq!(eval("(map-values '(:a 1 :b 2))"), "(1 2)");
    assert_eq!(eval("(map-pairs '(:a 1 :b 2))"), "((:a . 1) (:b . 2))");
    // Length counts key/value pairs, not cells.
    assert_eq!(eval("(map-length '(:a 1 :b 2))"), "2");
    // map-contains-key returns the plist tail (truthy), not `t'.
    assert_eq!(eval("(map-contains-key '(:a 1 :b 2) :b)"), "(:b 2)");
    assert_eq!(eval("(map-contains-key '(:a 1 :b 2) :z)"), "nil");
    assert_eq!(eval("(map-delete '(:a 1 :b 2 :c 3) :b)"), "(:a 1 :c 3)");
    assert_eq!(eval("(map-insert '(:a 1) :b 2)"), "(:b 2 :a 1)");
    assert_eq!(
        eval("(map-nested-elt '(:a (:b (:c 42))) '(:a :b :c))"),
        "42"
    );
    // setf on an existing plist key mutates in place; a new key appends.
    assert_eq!(
        eval("(let ((m (list :a 1 :b 2))) (setf (map-elt m :b) 99) m)"),
        "(:a 1 :b 99)"
    );
    assert_eq!(
        eval("(let ((m (list :a 1 :b 2))) (setf (map-elt m :c) 3) m)"),
        "(:a 1 :b 2 :c 3)"
    );
    // Alist-shaped lists (car is a cons) keep alist semantics.
    assert_eq!(eval("(map-elt '((a . 1) (b . 2)) 'b)"), "2");
    assert_eq!(eval("(map-length '((a . 1) (b . 2)))"), "2");
    assert_eq!(eval("(map-contains-key '((a . 1)) 'a)"), "t");
}

#[test]
fn string_truncate_left_prepends_ellipsis() {
    // Keeps the rightmost chars; "..." always prepended when truncating, so the
    // result can exceed LENGTH when LENGTH <= 3. Values from emacs 30.2.
    assert_eq!(
        eval("(string-truncate-left \"hello world\" 5)"),
        "\"...ld\""
    );
    assert_eq!(
        eval("(string-truncate-left \"hello world\" 8)"),
        "\"...world\""
    );
    assert_eq!(eval("(string-truncate-left \"hello\" 10)"), "\"hello\"");
    assert_eq!(eval("(string-truncate-left \"abcdef\" 3)"), "\"...f\"");
    assert_eq!(eval("(string-truncate-left \"ab\" 0)"), "\"...b\"");
}

#[test]
fn mapconcat_over_function_quote() {
    assert_eq!(
        eval("(mapconcat #'number-to-string '(1 2 3) \"+\")"),
        "\"1+2+3\""
    );
}

#[test]
fn cl_from_end_count_removes_last_matches() {
    // :from-end with :count deletes/substitutes the LAST COUNT matches, keeping
    // original order. Every expected value captured from emacs 30.2 + cl-lib.
    assert_eq!(
        eval("(cl-remove 2 (list 1 2 3 2 1) :count 1 :from-end t)"),
        "(1 2 3 1)"
    );
    assert_eq!(
        eval("(cl-remove-if #'cl-evenp (list 1 2 3 4 5 6) :count 2 :from-end t)"),
        "(1 2 3 5)"
    );
    assert_eq!(
        eval("(cl-remove-if-not #'cl-evenp (list 1 2 3 4 5 6) :count 2 :from-end t)"),
        "(1 2 4 6)"
    );
    assert_eq!(
        eval("(cl-substitute 9 2 (list 1 2 3 2 1) :count 1 :from-end t)"),
        "(1 2 3 9 1)"
    );
    assert_eq!(
        eval("(cl-substitute-if 9 #'cl-evenp (list 1 2 3 4 5 6) :count 2 :from-end t)"),
        "(1 2 3 9 5 9)"
    );
    // cl-remove-if now honors :key too (previously errored on non-numbers).
    assert_eq!(
        eval("(cl-remove-if #'cl-evenp (list '(1) '(2) '(3) '(4)) :key #'car)"),
        "((1) (3))"
    );
    // Forward (no :from-end) removal still removes the FIRST COUNT matches.
    assert_eq!(eval("(cl-remove 2 (list 1 2 3 2 1) :count 1)"), "(1 3 2 1)");
}

#[test]
fn cl_mismatch_search_from_end() {
    // :from-end reports the trailing mismatch / rightmost subsequence match.
    assert_eq!(
        eval("(cl-mismatch (list 1 2 3 4) (list 1 2) :from-end t)"),
        "3"
    );
    assert_eq!(eval("(cl-mismatch (list 1 2 3) (list 1 2 3))"), "nil");
    assert_eq!(
        eval("(cl-mismatch (list '(1) '(2) '(9)) (list '(1) '(2) '(3)) :key #'car)"),
        "2"
    );
    assert_eq!(
        eval("(cl-search (list 2 3) (list 1 2 3 2 3) :from-end t)"),
        "3"
    );
    assert_eq!(eval("(cl-search (list 2 3) (list 1 2 3 2 3))"), "1");
    assert_eq!(eval("(cl-search (list) (list 1 2 3) :from-end t)"), "3");
}

#[test]
fn assoc_default_test_and_default() {
    // assoc-default takes optional TEST and DEFAULT; TEST is called (ELEM KEY).
    assert_eq!(eval("(assoc-default 2 (list '(1 . a) '(2 . b)) #'=)"), "b");
    assert_eq!(
        eval("(assoc-default \"x\" (list '(\"a\" . 1)) nil 'def)"),
        "nil"
    );
    // Non-cons element that matches returns DEFAULT, not the element.
    assert_eq!(eval("(assoc-default 5 (list 3 5 7) #'= 'hit)"), "hit");
    assert_eq!(
        eval("(assoc-default \"b\" (list '(\"a\" . 1) '(\"b\" . 2)))"),
        "2"
    );
}

#[test]
fn assoc_testfn_arg_order() {
    // `assoc' with a TESTFN calls (funcall TEST (car ELEMENT) KEY): element-car
    // first, key second — matching real Emacs. All values from emacs 30.2.
    assert_eq!(
        eval("(assoc 3 '((1 . a) (2 . b)) (lambda (elem key) (< elem key)))"),
        "(1 . a)"
    );
    assert_eq!(
        eval("(assoc 3 '((4 . a) (2 . b)) (lambda (elem key) (< elem key)))"),
        "(2 . b)"
    );
}

#[test]
fn cl_seq_test_not() {
    // :test-not selects an element when (funcall TEST-NOT item elt) is nil.
    assert_eq!(eval("(cl-member 2 '(1 2 3) :test-not #'eql)"), "(1 2 3)");
    assert_eq!(
        eval("(cl-assoc 2 '((1 . a) (2 . b) (3 . c)) :test-not #'eql)"),
        "(1 . a)"
    );
    assert_eq!(
        eval("(cl-rassoc 2 '((a . 1) (b . 2)) :test-not #'eql)"),
        "(a . 1)"
    );
    assert_eq!(eval("(cl-find 2 '(1 2 3) :test-not #'eql)"), "1");
    assert_eq!(eval("(cl-remove 2 '(1 2 3 2) :test-not #'eql)"), "(2 2)");
    assert_eq!(eval("(cl-count 5 '(5 1 5 2 5) :test-not #'eql)"), "2");
    assert_eq!(eval("(cl-position 2 '(1 2 3) :test-not #'eql)"), "0");
}

#[test]
fn cl_seq_start_end() {
    // :start/:end confine the active window on remove/substitute/find/find-if.
    assert_eq!(
        eval("(cl-remove 2 '(2 1 2 3 2) :start 1 :end 4)"),
        "(2 1 3 2)"
    );
    assert_eq!(
        eval("(cl-remove 5 '(5 1 5 2 5) :test-not #'eql :start 1 :end 4)"),
        "(5 5 5)"
    );
    assert_eq!(
        eval("(cl-substitute 9 2 '(2 1 2 3 2) :start 1 :end 4)"),
        "(2 1 9 3 2)"
    );
    assert_eq!(eval("(cl-find 2 '(2 1 2 3) :start 1 :end 2)"), "nil");
    assert_eq!(eval("(cl-find-if #'cl-evenp '(1 2 3 4) :start 2)"), "4");
    assert_eq!(
        eval("(cl-remove-if #'cl-evenp '(2 1 2 3 2) :start 1 :end 4)"),
        "(2 1 3 2)"
    );
    // :start/:end + :count + :from-end (last COUNT match within the window).
    assert_eq!(
        eval("(cl-substitute 9 5 '(5 5 5 5 5) :start 1 :end 4 :count 1 :from-end t)"),
        "(5 5 5 9 5)"
    );
}
