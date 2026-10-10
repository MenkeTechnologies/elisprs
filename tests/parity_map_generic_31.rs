//! map.el generic functions across alists, plists, hash tables and arrays, and
//! the small subr.el and subr-x.el functions the prelude lacked.
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
fn map_functions_follow_the_generic_methods() {
    check(
        r#"(progn (require (quote map)) (map-elt '((a . 1)) 'a))"#,
        r#"=1"#,
    );
    check(
        r#"(progn (require (quote map)) (map-elt '(a 1 b 2) 'b))"#,
        r#"=2"#,
    );
    check(
        r#"(progn (require (quote map)) (map-elt [10 20] 1))"#,
        r#"=20"#,
    );
    check(
        r#"(progn (require (quote map)) (map-elt [10 20] 5 'dflt))"#,
        r#"=dflt"#,
    );
    check(
        r#"(progn (require (quote map)) (map-elt #s(hash-table data (a 1)) 'a))"#,
        r#"=1"#,
    );
    check(
        r#"(progn (require (quote map)) (map-elt '((a . 1)) 'z 'd))"#,
        r#"=d"#,
    );
    check(
        r#"(progn (require (quote map)) (map-elt "abc" 1))"#,
        r#"=98"#,
    );
    check(
        r#"(progn (require (quote map)) (map-keys '((a . 1) (b . 2))))"#,
        r#"=(a b)"#,
    );
    check(
        r#"(progn (require (quote map)) (map-values [x y]))"#,
        r#"=(x y)"#,
    );
    check(
        r#"(progn (require (quote map)) (map-pairs '(a 1 b 2)))"#,
        r#"=((a . 1) (b . 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-length '((a . 1))))"#,
        r#"=1"#,
    );
    check(
        r#"(progn (require (quote map)) (map-length [1 2 3]))"#,
        r#"=3"#,
    );
    check(
        r#"(progn (require (quote map)) (map-copy '((a . 1))))"#,
        r#"=((a . 1))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-delete (list (cons 'a 1) (cons 'b 2)) 'a))"#,
        r#"=((b . 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-delete [1 2 3] 1))"#,
        r#"=[1 nil 3]"#,
    );
    check(
        r#"(progn (require (quote map)) (map-insert '((a . 1)) 'b 2))"#,
        r#"=((b . 2) (a . 1))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-insert [1 2] 0 9))"#,
        r#"=[9 2]"#,
    );
    check(
        r#"(progn (require (quote map)) (map-contains-key '((a . 1)) 'a))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (require (quote map)) (map-contains-key [1 2] 5))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require (quote map)) (map-some (lambda (k v) (and (> v 1) k)) '((a . 1) (b . 2))))"#,
        r#"=b"#,
    );
    check(
        r#"(progn (require (quote map)) (map-every-p (lambda (k v) (> v 0)) '((a . 1) (b . 2))))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (require (quote map)) (map-filter (lambda (k v) (> v 1)) '((a . 1) (b . 2))))"#,
        r#"=((b . 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-remove (lambda (k v) (> v 1)) '((a . 1) (b . 2))))"#,
        r#"=((a . 1))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-apply #'cons '((a . 1) (b . 2))))"#,
        r#"=((a . 1) (b . 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-do #'ignore '((a . 1))))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require (quote map)) (map-merge 'list '((a . 1)) '((b . 2) (a . 3))))"#,
        r#"=((a . 3) (b . 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-merge 'alist '((a . 1)) '(b 2)))"#,
        r#"=((a . 1) (b . 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-merge 'plist '((a . 1)) '((b . 2))))"#,
        r#"=(a 1 b 2)"#,
    );
    check(
        r#"(progn (require (quote map)) (map-merge 'hash-table '((a . 1)) '((b . 2))))"#,
        r#"=#s(hash-table test equal data (a 1 b 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-merge-with 'list #'+ '((a . 1)) '((a . 5))))"#,
        r#"=((a . 6))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-into '((a . 1)) 'plist))"#,
        r#"=(a 1)"#,
    );
    check(
        r#"(progn (require (quote map)) (map-into '((a . 1)) 'hash-table))"#,
        r#"=#s(hash-table test equal data (a 1))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-into [1 2] 'alist))"#,
        r#"=((0 . 1) (1 . 2))"#,
    );
    check(
        r#"(progn (require (quote map)) (map-empty-p nil))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (require (quote map)) (map-empty-p '((a . 1))))"#,
        r#"=nil"#,
    );
    check(r#"(progn (require (quote map)) (map-empty-p []))"#, r#"=t"#);
    check(
        r#"(progn (require (quote map)) (map-nested-elt '((a . ((b . 1)))) '(a b)))"#,
        r#"=1"#,
    );
    check(
        r#"(progn (require (quote map)) (map-nested-elt '((a . ((b . 1)))) '(a c) 'd))"#,
        r#"=d"#,
    );
    check(
        r#"(progn (require (quote map)) (map-put! (list (cons 'a 1)) 'a 5))"#,
        r#"=5"#,
    );
    check(
        r#"(progn (require (quote map)) (map-put! (list (cons 'a 1)) 'b 5))"#,
        r#"!(map-not-inplace ((a . 1)))"#,
    );
    check(
        r#"(progn (require (quote map)) (let ((m (list (cons 'a 1)))) (setf (map-elt m 'b) 2) m))"#,
        r#"=((b . 2) (a . 1))"#,
    );
    check(
        r#"(progn (require (quote map)) (let ((m (list 'a 1))) (setf (map-elt m 'a) 2) m))"#,
        r#"=(a 2)"#,
    );
    check(
        r#"(progn (require (quote map)) (let ((m (make-hash-table))) (setf (map-elt m 'a) 2) (hash-table-count m)))"#,
        r#"=1"#,
    );
    check(
        r#"(progn (require (quote map)) (let ((m (vector 1 2))) (setf (map-elt m 0) 9) m))"#,
        r#"=[9 2]"#,
    );
    check(
        r#"(progn (require (quote map)) (map-let (a b) '((a . 1) (b . 2)) (list a b)))"#,
        r#"=(1 2)"#,
    );
    check(
        r#"(progn (require (quote map)) (map-let (:a) '(:a 5) (list a)))"#,
        r#"=(5)"#,
    );
    check(r#"(progn (require (quote map)) (mapp '(a b)))"#, r#"=t"#);
    check(r#"(progn (require (quote map)) (mapp 5))"#, r#"=nil"#);
    check(r#"(progn (require (quote map)) (mapp [1]))"#, r#"=t"#);
    check(r#"(progn (require (quote map)) (mapp "x"))"#, r#"=t"#);
    check(
        r#"(progn (require 'map) (map-insert [1 2] 0 9))"#,
        r#"=[9 2]"#,
    );
    check(r#"(progn (require 'map) (map-elt [1 2] 'a))"#, r#"=nil"#);
    check(
        r#"(progn (require 'map) (map-elt '((a . 1) . 5) 'b))"#,
        r#"!(wrong-type-argument listp ((a . 1) . 5))"#,
    );
    check(r#"(progn (require 'map) (map-elt "abc" 'x 'd))"#, r#"=d"#);
    check(
        r#"(progn (require 'map) (map-into '((a . 1) (b . 2)) 'plist))"#,
        r#"=(a 1 b 2)"#,
    );
    check(
        r#"(progn (require 'map) (map-into '(a 1 b 2) '(hash-table :test eq)))"#,
        r#"=#s(hash-table test eq data (a 1 b 2))"#,
    );
    check(
        r#"(progn (require 'map) (map-into '((a . 1)) '(hash-table :size 3)))"#,
        r#"=#s(hash-table data (a 1))"#,
    );
    check(
        r#"(progn (require 'map) (map-merge 'plist '(a 1) '(b 2)))"#,
        r#"=(a 1 b 2)"#,
    );
    check(
        r#"(progn (require 'map) (map-keys (make-hash-table)))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'map) (map-pairs [a b]))"#,
        r#"=((0 . a) (1 . b))"#,
    );
    check(r#"(progn (require 'map) (map-length nil))"#, r#"=0"#);
    check(r#"(progn (require 'map) (map-length "abc"))"#, r#"=3"#);
    check(
        r#"(progn (require 'map) (map-values-apply #'1+ '((a . 1))))"#,
        r#"=(2)"#,
    );
    check(
        r#"(progn (require 'map) (map-keys-apply #'symbol-name '((a . 1))))"#,
        r#"=("a")"#,
    );
    check(
        r#"(progn (require 'map) (map-apply (lambda (k v) k) [a b]))"#,
        r#"=(0 1)"#,
    );
    check(
        r#"(progn (require 'map) (map-do (lambda (k v) (message "")) (let ((h (make-hash-table))) (puthash 1 2 h) h)))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'map) (map-put! [1 2] 5 1))"#,
        r#"!(args-out-of-range [1 2] 5)"#,
    );
    check(r#"(progn (require 'map) (map-put! '(a 1) 'b 2))"#, r#"=2"#);
    check(
        r#"(progn (require 'map) (let ((m (list 'a 1))) (map-put! m 'a 5) m))"#,
        r#"=(a 5)"#,
    );
    check(
        r#"(progn (require 'map) (map-filter #'always nil))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'map) (map-some #'always nil))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'map) (map-every-p #'ignore nil))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (require 'map) (map-contains-key '(a 1 b 2) 'b))"#,
        r#"=(b 2)"#,
    );
    check(
        r#"(progn (require 'map) (map-contains-key '((a . 1)) 'b))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'map) (map-contains-key "abc" 1))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (require 'map) (map-contains-key '((1 . 2)) 1.0 #'=))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (require 'map) (map-elt '((1 . 2)) 1.0 nil #'=))"#,
        r#"=2"#,
    );
    check(
        r#"(progn (require 'map) (map-elt '(("a" . 2)) "a"))"#,
        r#"=2"#,
    );
    check(
        r#"(progn (require 'map) (map-elt '(("a" . 2)) "a" nil #'equal))"#,
        r#"=2"#,
    );
    check(
        r#"(progn (require 'map) (let ((m (list (cons "a" 1)))) (setf (map-elt m "a" nil #'equal) 5) m))"#,
        r#"=(("a" . 5))"#,
    );
    check(
        r#"(progn (require 'map) (let ((m nil)) (setf (map-elt m 'a) 5) m))"#,
        r#"=((a . 5))"#,
    );
    check(
        r#"(progn (require 'map) (let ((m [1 2])) (setf (map-elt m 5) 5) m))"#,
        r#"!(args-out-of-range [1 2] 5)"#,
    );
    check(
        r#"(progn (require 'map) (let ((m '(a 1))) (setf (map-elt m 'b) 2) m))"#,
        r#"=(a 1 b 2)"#,
    );
    check(
        r#"(progn (require 'map) (let ((m (list 'a 1 'b 2))) (map-delete m 'a)))"#,
        r#"=(b 2)"#,
    );
    check(
        r#"(progn (require 'map) (let ((h (make-hash-table))) (puthash 1 2 h) (map-delete h 1) (hash-table-count h)))"#,
        r#"=0"#,
    );
    check(
        r#"(progn (require 'map) (map-delete '(a 1 b 2) 'c))"#,
        r#"=(a 1 b 2)"#,
    );
    check(r#"(progn (require 'map) (map-merge 'list))"#, r#"=nil"#);
    check(
        r#"(progn (require 'map) (map-merge 'hash-table))"#,
        r#"=#s(hash-table test equal)"#,
    );
    check(
        r#"(progn (require 'map) (map-nested-elt '((a . [1 2])) '(a 1)))"#,
        r#"=2"#,
    );
    check(
        r#"(progn (require 'map) (map-nested-elt nil '(a)))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'map) (map-nested-elt '((a . 1)) nil))"#,
        r#"=((a . 1))"#,
    );
    check(
        r#"(progn (require 'map) (map-nested-elt '((a . 1)) '(a b)))"#,
        r#"=nil"#,
    );
    check(
        r#"(progn (require 'map) (map-let (a b c) '((a . 1)) (list a b c)))"#,
        r#"=(1 nil nil)"#,
    );
    check(
        r#"(progn (require 'map) (map-let ((:k k)) '(:k 1) (list k)))"#,
        r#"=(1)"#,
    );
    check(
        r#"(progn (require 'map) (pcase '((a . 1) (b . 2)) ((map a b) (list a b))))"#,
        r#"=(1 2)"#,
    );
    check(
        r#"(progn (require 'map) (pcase '(:a 1 :b 2) ((map :a :b) (list a b))))"#,
        r#"=(1 2)"#,
    );
    check(
        r#"(progn (require 'map) (pcase '((a . 1)) ((map ('a x)) x)))"#,
        r#"=1"#,
    );
    check(
        r#"(progn (require 'map) (pcase [10 20] ((map (0 x) (1 y)) (list x y))))"#,
        r#"=(10 20)"#,
    );
    check(
        r#"(progn (require 'map) (map-copy '((a . 1))))"#,
        r#"=((a . 1))"#,
    );
    check(
        r#"(progn (require 'map) (let* ((m '((a . 1))) (c (map-copy m))) (eq m c)))"#,
        r#"=nil"#,
    );
    check(r#"(progn (require 'map) (map-copy [1 2]))"#, r#"=[1 2]"#);
    check(
        r#"(progn (require 'map) (map-copy (let ((h (make-hash-table))) (puthash 1 2 h) h)))"#,
        r#"=#s(hash-table data (1 2))"#,
    );
    check(r#"(progn (require 'map) (map-empty-p "abc"))"#, r#"=nil"#);
    check(
        r#"(progn (require 'map) (map-empty-p (make-hash-table)))"#,
        r#"=t"#,
    );
    check(
        r#"(progn (require 'map) (mapp (make-hash-table)))"#,
        r#"=t"#,
    );
    check(r#"(progn (require 'map) (mapp nil))"#, r#"=t"#);
    check(r#"(progn (require 'map) (mapp "abc"))"#, r#"=t"#);
    check(r#"(progn (require 'map) (mapp (lambda ())))"#, r#"=nil"#);
}

#[test]
fn small_subr_functions_and_predicates() {
    check(
        r#"(propertize "abc" 'a)"#,
        r#"!(wrong-number-of-arguments propertize 2)"#,
    );
    check(
        r#"(propertize "ab" 'a 1 'b 2 'a 3)"#,
        r#"=#("ab" 0 2 (b 2 a 1))"#,
    );
    check(
        r#"(text-properties-at 0 (propertize "ab" 'a 1 'b 2 'a 3))"#,
        r#"=(b 2 a 1)"#,
    );
    check(r#"(and-let* ())"#, r#"=t"#);
    check(r#"(and-let* ((x 5)))"#, r#"=5"#);
    check(r#"(and-let* ((x 5) ((> x 3))))"#, r#"=t"#);
    check(
        r#"(seq-remove-at-position "abc" 5)"#,
        r#"!(args-out-of-range "abc" 0 5)"#,
    );
    check(r#"(seq-remove-at-position [1 2 3] 1)"#, r#"=[1 3]"#);
    check(
        r#"(seq-remove-at-position '(1 2 3) 5)"#,
        r#"!(error "End index out of bounds: 5")"#,
    );
    check(
        r#"(byte-code-function-p (symbol-function 'string-trim))"#,
        r#"=t"#,
    );
    check(r#"(byte-code-function-p (lambda ()))"#, r#"=nil"#);
    check(r#"(interpreted-function-p (lambda ()))"#, r#"=t"#);
    check(
        r#"(interpreted-function-p (symbol-function 'string-trim))"#,
        r#"=nil"#,
    );
    check(
        r#"(compiled-function-p (symbol-function 'string-trim))"#,
        r#"=t"#,
    );
    check(r#"(compiled-function-p (symbol-function 'car))"#, r#"=t"#);
    check(r#"(compiled-function-p (lambda ()))"#, r#"=nil"#);
    check(
        r#"(let ((s (copy-sequence "abcd"))) (add-display-text-property 0 2 'height 2 s) s)"#,
        r#"=#("abcd" 0 2 (display (height 2)))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcd"))) (add-display-text-property 0 2 'height 2 s) (add-display-text-property 1 3 'raise 1 s) s)"#,
        r#"=#("abcd" 0 1 (display #1=(height 2)) 1 2 (display ((raise 1) #1#)) 2 3 (display (raise 1)))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcd"))) (add-display-text-property 0 2 'height 2 s) (remove-display-text-property 0 4 'height s) s)"#,
        r#"="abcd""#,
    );
    check(
        r#"(let ((s (copy-sequence "abcd"))) (add-display-text-property 0 2 'height 2 s) (add-display-text-property 0 2 'height 3 s) s)"#,
        r#"=#("abcd" 0 2 (display ((height 3))))"#,
    );
}
