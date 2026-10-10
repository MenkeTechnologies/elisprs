//! Text-property intervals: Emacs never merges intervals that merely became
//! equal, so the splits each primitive makes (and no others) are visible in
//! the printed string. Also `format`/`concat`/`replace-match` property
//! transfer, the interval walks, `object-intervals` and
//! `add-face-text-property`.
//!
//! Every expectation is the output of `emacs -Q --batch` (GNU Emacs 31.1) for
//! the same form in a fresh process: `=` and the printed value, or `!` and the
//! printed error object.

use elisprs::{eval_str, print, reset_host};

fn check(form: &str, expected: &str) {
    reset_host();
    let src = format!(
        "(let ((print-escape-newlines t) (print-circle t)) \
           (condition-case e (eval (quote {form}) t) \
             (:success (concat \"=\" (prin1-to-string e))) \
             (error (concat \"!\" (prin1-to-string e)))))"
    );
    let v = eval_str(&src).expect("eval failed");
    assert_eq!(print(&v, false), expected, "{form}");
}

#[test]
fn intervals_split_where_emacs_splits() {
    check(
        r#"(propertize "abc" 'face 'bold)"#,
        r#"=#("abc" 0 3 (face bold))"#,
    );
    check(
        r#"(propertize "abc" 'a 1 'b 2)"#,
        r#"=#("abc" 0 3 (a 1 b 2))"#,
    );
    check(r#"(propertize "abc")"#, r#"="abc""#);
    check(
        r#"(propertize "abc" 'a)"#,
        r#"!(wrong-number-of-arguments propertize 2)"#,
    );
    check(
        r#"(propertize 5 'a 1)"#,
        r#"!(wrong-type-argument stringp 5)"#,
    );
    check(r#"(propertize "" 'a 1)"#, r#"="""#);
    check(
        r#"(concat (propertize "a" 'p 1) (propertize "b" 'p 1))"#,
        r#"=#("ab" 0 1 (p 1) 1 2 (p 1))"#,
    );
    check(
        r#"(concat (propertize "a" 'p 1) "b" (propertize "c" 'p 1))"#,
        r#"=#("abc" 0 1 (p 1) 2 3 (p 1))"#,
    );
    check(
        r#"(concat (propertize "a" 'p 1) (propertize "b" 'p 2))"#,
        r#"=#("ab" 0 1 (p 1) 1 2 (p 2))"#,
    );
    check(
        r#"(substring (propertize "abcdef" 'p 1) 1 3)"#,
        r#"=#("bc" 0 2 (p 1))"#,
    );
    check(
        r#"(substring (concat (propertize "ab" 'p 1) (propertize "cd" 'q 2)) 1 3)"#,
        r#"=#("bc" 0 1 (p 1) 1 2 (q 2))"#,
    );
    check(
        r#"(copy-sequence (propertize "abc" 'p 1))"#,
        r#"=#("abc" 0 3 (p 1))"#,
    );
    check(r#"(string-to-list (propertize "ab" 'p 1))"#, r#"=(97 98)"#);
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 1 3 'p 1 s) s)"#,
        r#"=#("abcdef" 1 3 (p 1))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 1 3 'p 1 s) (put-text-property 2 5 'p 1 s) s)"#,
        r#"=#("abcdef" 1 3 (p 1) 3 5 (p 1))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 1 3 'p 1 s) (put-text-property 3 5 'p 1 s) s)"#,
        r#"=#("abcdef" 1 3 (p 1) 3 5 (p 1))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 1 3 'p 1 s) (put-text-property 3 5 'p 2 s) s)"#,
        r#"=#("abcdef" 1 3 (p 1) 3 5 (p 2))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (add-text-properties 0 6 '(a 1 b 2) s) (remove-text-properties 2 4 '(a nil) s) s)"#,
        r#"=#("abcdef" 0 2 (b 2 a 1) 2 4 (b 2) 4 6 (b 2 a 1))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (add-text-properties 0 6 '(a 1 b 2) s) (remove-list-of-text-properties 2 4 '(a b) s) s)"#,
        r#"=#("abcdef" 0 2 (b 2 a 1) 4 6 (b 2 a 1))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (set-text-properties 1 3 '(z 1) s) s)"#,
        r#"=#("abcdef" 1 3 (z 1))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (add-text-properties 1 3 '(z 1) s))"#,
        r#"=t"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (add-text-properties 1 3 '(z 1) s) (add-text-properties 1 3 '(z 1) s))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (text-properties-at 1 s))"#,
        r#"=(a 1)"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (text-properties-at 3 s))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (text-properties-at 6 s))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (text-properties-at 7 s))"#,
        r#"!(args-out-of-range 7 7)"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (get-text-property 1 'a s))"#,
        r#"=1"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (next-property-change 0 s))"#,
        r#"=3"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (next-property-change 3 s))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 0 3 'a 1 s) (next-property-change 3 s 5))"#,
        r#"=5"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (next-single-property-change 0 'a s))"#,
        r#"=2"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (next-single-property-change 4 'a s))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (next-single-property-change 4 'a s 6))"#,
        r#"=6"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (previous-single-property-change 6 'a s))"#,
        r#"=4"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (previous-single-property-change 2 'a s))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (previous-property-change 6 s))"#,
        r#"=4"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (text-property-any 0 6 'a 1 s))"#,
        r#"=2"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (text-property-not-all 2 6 'a 1 s))"#,
        r#"=4"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (text-property-search-forward 'a 1 t))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (next-single-char-property-change 0 'a s))"#,
        r#"=2"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (next-single-char-property-change 4 'a s))"#,
        r#"=6"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (previous-single-char-property-change 6 'a s))"#,
        r#"=4"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcdef"))) (put-text-property 2 4 'a 1 s) (next-char-property-change 0 5))"#,
        r#"!(args-out-of-range 0 0)"#,
    );
    check(
        r#"(get-char-property 1 'a (propertize "abc" 'a 1))"#,
        r#"=1"#,
    );
    check(
        r#"(get-pos-property 1 'a (propertize "abc" 'a 1))"#,
        r#"=1"#,
    );
    check(
        r#"(get-char-property-and-overlay 1 'a (propertize "abc" 'a 1))"#,
        r#"=(1)"#,
    );
    check(
        r#"(object-intervals (concat (propertize "ab" 'a 1) "cd" (propertize "e" 'b 2)))"#,
        r#"=((0 2 (a 1)) (2 4 nil) (4 5 (b 2)))"#,
    );
    check(r#"(object-intervals "abc")"#, r#"=nil"#);
    check(
        r#"(with-temp-buffer (insert (propertize "ab" 'a 1) "cd") (object-intervals (current-buffer)))"#,
        r#"=((0 2 (a 1)) (2 4 nil))"#,
    );
    check(
        r#"(with-temp-buffer (insert "abcd") (put-text-property 2 3 'x 1) (buffer-string))"#,
        r#"=#("abcd" 1 2 (x 1))"#,
    );
    check(
        r#"(with-temp-buffer (insert (propertize "ab" 'a 1)) (buffer-substring 1 2))"#,
        r#"=#("a" 0 1 (a 1))"#,
    );
    check(
        r#"(with-temp-buffer (insert (propertize "ab" 'a 1)) (buffer-substring-no-properties 1 2))"#,
        r#"="a""#,
    );
    check(
        r#"(with-temp-buffer (insert (propertize "ab" 'a 1)) (goto-char 3) (insert "c") (buffer-string))"#,
        r#"=#("abc" 0 2 (a 1))"#,
    );
    check(
        r#"(with-temp-buffer (insert "abc") (put-text-property 1 3 'a 1) (delete-region 2 3) (buffer-string))"#,
        r#"=#("ac" 0 1 (a 1))"#,
    );
    check(
        r#"(with-temp-buffer (insert "abc") (put-text-property 1 3 'a 1) (erase-buffer) (buffer-string))"#,
        r#"="""#,
    );
    check(
        r#"(with-temp-buffer (insert (propertize "abc" 'a 1)) (add-face-text-property 1 3 'bold) (buffer-string))"#,
        r#"=#("abc" 0 2 (face bold a 1) 2 3 (a 1))"#,
    );
    check(
        r#"(with-temp-buffer (insert "abc") (add-face-text-property 1 3 'bold) (add-face-text-property 2 3 'italic) (buffer-string))"#,
        r#"=#("abc" 0 1 (face bold) 1 2 (face (italic bold)))"#,
    );
    check(
        r#"(with-temp-buffer (insert "abc") (add-face-text-property 1 3 'bold) (add-face-text-property 2 3 'italic t) (buffer-string))"#,
        r#"=#("abc" 0 1 (face bold) 1 2 (face (bold italic)))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abc"))) (add-face-text-property 0 2 'bold nil s) s)"#,
        r#"=#("abc" 0 2 (face bold))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abc"))) (add-face-text-property 0 2 '(:weight bold) nil s) (add-face-text-property 0 3 'italic nil s) s)"#,
        r#"=#("abc" 0 2 (face (italic (:weight bold))) 2 3 (face italic))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abc"))) (alter-text-property 0 3 'a (lambda (x) (1+ (or x 0))) s) s)"#,
        r#"=#("abc" 0 3 (a 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (remove-text-properties 0 3 '(b nil) s))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (remove-text-properties 0 3 '(a nil) s))"#,
        r#"=t"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (set-text-properties 0 3 nil s))"#,
        r#"=t"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (set-text-properties 0 3 nil s) s)"#,
        r#"="abc""#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (set-text-properties 5 6 nil s))"#,
        r#"!(args-out-of-range 5 6)"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (put-text-property 0 5 'b 1 s))"#,
        r#"!(args-out-of-range 0 5)"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (put-text-property 2 1 'b 1 s) s)"#,
        r#"=#("abc" 0 1 (a 1) 1 2 (b 1 a 1) 2 3 (a 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (put-text-property -1 2 'b 1 s))"#,
        r#"!(args-out-of-range -1 2)"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (get-text-property 5 'a s))"#,
        r#"!(args-out-of-range 5 5)"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'a 1))) (text-properties-at 0 s))"#,
        r#"=(a 1)"#,
    );
    check(
        r#"(equal-including-properties (propertize "a" 'p 1) (propertize "a" 'p 1))"#,
        r#"=t"#,
    );
    check(
        r#"(equal-including-properties (propertize "a" 'p 1) "a")"#,
        r#"=nil"#,
    );
    check(r#"(equal (propertize "a" 'p 1) "a")"#, r#"=t"#);
    check(
        r#"(equal-including-properties (propertize "a" 'p '(1)) (propertize "a" 'p '(1)))"#,
        r#"=t"#,
    );
    check(r#"(string-to-multibyte (propertize "a" 'p 1))"#, r#"="a""#);
    check(
        r#"(upcase (propertize "abc" 'p 1))"#,
        r#"=#("ABC" 0 3 (p 1))"#,
    );
    check(
        r#"(capitalize (propertize "abc def" 'p 1))"#,
        r#"=#("Abc Def" 0 7 (p 1))"#,
    );
    check(
        r#"(format "%s" (propertize "abc" 'p 1))"#,
        r#"=#("abc" 0 3 (p 1))"#,
    );
    check(
        r#"(format (propertize "%s" 'q 1) "abc")"#,
        r#"=#("abc" 0 3 (q 1))"#,
    );
    check(
        r#"(format "%s-%s" (propertize "a" 'p 1) (propertize "b" 'p 2))"#,
        r#"=#("a-b" 0 1 (p 1) 2 3 (p 2))"#,
    );
    check(
        r#"(format "%5s" (propertize "a" 'p 1))"#,
        r#"=#("    a" 4 5 (p 1))"#,
    );
    check(
        r#"(format "%-5s|" (propertize "a" 'p 1))"#,
        r#"=#("a    |" 0 5 (p 1))"#,
    );
    check(r#"(format "%d" 5)"#, r#"="5""#);
    check(
        r#"(format (propertize "%d%%" 'z 1) 5)"#,
        r#"=#("5%" 0 2 (z 1))"#,
    );
    check(
        r#"(format-message (propertize "`a'" 'z 1))"#,
        r#"=#("‘a’" 0 3 (z 1))"#,
    );
    check(
        r#"(replace-regexp-in-string "b" "X" (propertize "abc" 'p 1))"#,
        r#"=#("aXc" 0 1 (p 1) 2 3 (p 1))"#,
    );
    check(
        r#"(replace-regexp-in-string "b" (propertize "X" 'r 1) (propertize "abc" 'p 1))"#,
        r#"=#("aXc" 0 1 (p 1) 1 2 (r 1) 2 3 (p 1))"#,
    );
    check(
        r#"(string-trim (propertize "  abc  " 'p 1))"#,
        r#"=#("abc" 0 3 (p 1))"#,
    );
    check(
        r#"(split-string (propertize "a b" 'p 1))"#,
        r#"=(#("a" 0 1 (p 1)) #("b" 0 1 (p 1)))"#,
    );
    check(
        r#"(mapconcat #'identity (list (propertize "a" 'p 1) "b") "")"#,
        r#"=#("ab" 0 1 (p 1))"#,
    );
    check(
        r#"(mapconcat #'identity (list (propertize "a" 'p 1) "b") (propertize "-" 'q 1))"#,
        r#"=#("a-b" 0 1 (p 1) 1 2 (q 1))"#,
    );
    check(
        r#"(string-join (list (propertize "a" 'p 1) "b") ",")"#,
        r#"=#("a,b" 0 1 (p 1))"#,
    );
    check(
        r#"(apply #'string (string-to-list (propertize "ab" 'p 1)))"#,
        r#"="ab""#,
    );
    check(r#"(char-to-string ?a)"#, r#"="a""#);
    check(r#"(read (propertize "(a b)" 'p 1))"#, r#"=(a b)"#);
    check(
        r#"(read-from-string (propertize "abc" 'p 1))"#,
        r#"=(abc . 3)"#,
    );
    check(
        r#"(prin1-to-string (propertize "abc" 'p 1) t)"#,
        r#"="abc""#,
    );
    check(
        r#"(with-output-to-string (princ (propertize "abc" 'p 1)))"#,
        r#"="abc""#,
    );
    check(
        r#"(let ((standard-output (current-buffer))) (with-temp-buffer (princ (propertize "ab" 'p 1)) (buffer-string)))"#,
        r#"="""#,
    );
    check(
        r#"(propertize "ab" 'a '(1 2))"#,
        r#"=#("ab" 0 2 (a (1 2)))"#,
    );
    check(
        r#"(propertize "ab" 'a "x" 'b [1])"#,
        r#"=#("ab" 0 2 (a "x" b [1]))"#,
    );
    check(r#"(propertize "a\nb" 'a 1)"#, r#"=#("a\nb" 0 3 (a 1))"#);
    check(r#"(propertize "ab" nil 1)"#, r#"=#("ab" 0 2 (nil 1))"#);
    check(r#"(propertize "ab" 1 2)"#, r#"=#("ab" 0 2 (1 2))"#);
    check(r#"(propertize "ab" 'a 1 'a 2)"#, r#"=#("ab" 0 2 (a 1))"#);
    check(
        r#"(text-properties-at 0 (propertize "ab" 'a 1 'a 2))"#,
        r#"=(a 1)"#,
    );
    check(
        r#"(let ((s (copy-sequence "abc"))) (put-text-property 0 1 'face 'bold s) (put-text-property 1 2 'face 'bold s) s)"#,
        r#"=#("abc" 0 1 (face bold) 1 2 (face bold))"#,
    );
    check(
        r#"(let ((s (copy-sequence "abcde"))) (put-text-property 0 5 'p 1 s) (put-text-property 1 2 'p 2 s) (put-text-property 1 2 'p 1 s) s)"#,
        r#"=#("abcde" 0 1 (p 1) 1 2 (p 1) 2 5 (p 1))"#,
    );
    check(
        r#"(let ((s (concat (propertize "a" 'p 1) (propertize "b" 'p 1)))) s)"#,
        r#"=#("ab" 0 1 (p 1) 1 2 (p 1))"#,
    );
    check(
        r#"(let ((s (concat (propertize "a" 'p '(1)) (propertize "b" 'p '(1))))) s)"#,
        r#"=#("ab" 0 1 (p (1)) 1 2 (p (1)))"#,
    );
    check(
        r#"(let ((v '(1))) (concat (propertize "a" 'p v) (propertize "b" 'p v)))"#,
        r#"=#("ab" 0 1 (p #1=(1)) 1 2 (p #1#))"#,
    );
    check(
        r#"(let ((s (concat (propertize "a" 'p 1) (propertize "b" 'p 1)))) (put-text-property 0 2 'q 1 s) s)"#,
        r#"=#("ab" 0 1 (q 1 p 1) 1 2 (q 1 p 1))"#,
    );
    check(
        r#"(substring-no-properties (propertize "abc" 'p 1) 1)"#,
        r#"="bc""#,
    );
    check(r#"(remove-text-properties 0 1 '(a nil) "abc")"#, r#"=nil"#);
    check(r#"(text-properties-at 0 "abc")"#, r#"=nil"#);
    check(
        r#"(text-properties-at 0 nil)"#,
        r#"!(args-out-of-range 0 0)"#,
    );
    check(
        r#"(text-properties-at 0 1)"#,
        r#"!(wrong-type-argument buffer-or-string-p 1)"#,
    );
    check(
        r#"(propertize 'sym 'a 1)"#,
        r#"!(wrong-type-argument stringp sym)"#,
    );
    check(r#"(string-to-char (propertize "abc" 'p 1))"#, r#"=97"#);
    check(
        r#"(font-lock-append-text-property 0 1 'face 'bold (copy-sequence "ab"))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (copy-sequence "ab"))) (font-lock-prepend-text-property 0 2 'face 'bold s) (font-lock-prepend-text-property 0 2 'face 'italic s) s)"#,
        r#"=#("ab" 0 2 (face (italic bold)))"#,
    );
    check(
        r#"(let ((s (copy-sequence "ab"))) (insert-for-yank s) )"#,
        r#"=nil"#,
    );
    check(
        r#"(with-temp-buffer (insert-for-yank (propertize "ab" 'yank-handler '(nil "XX"))) (buffer-string))"#,
        r#"="XX""#,
    );
    check(
        r#"(with-temp-buffer (insert-for-yank (propertize "ab" 'invisible t 'field 1 'mouse-face 'x)) (buffer-string))"#,
        r#"="ab""#,
    );
    check(
        r#"(let ((s (propertize "ab" 'p 1))) (list (string-prefix-p "a" s) (string-suffix-p "b" s)))"#,
        r#"=(t t)"#,
    );
    check(
        r#"(let ((s (propertize "ab" 'p 1))) (string-search "b" s))"#,
        r#"=1"#,
    );
    check(
        r#"(let ((s (propertize "ab" 'p 1))) (string-replace "a" "z" s))"#,
        r#"=#("zb" 1 2 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (truncate-string-to-width s 2))"#,
        r#"=#("ab" 0 2 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (truncate-string-to-width s 2 nil nil t))"#,
        r#"=#("a…" 0 1 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abcdef" 'p 1))) (truncate-string-to-width s 4 nil nil "..."))"#,
        r#"=#("a..." 0 1 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (string-pad s 5))"#,
        r#"=#("abc  " 0 3 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (string-limit s 2))"#,
        r#"=#("ab" 0 2 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (reverse s))"#,
        r#"="cba""#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (nreverse (copy-sequence s)))"#,
        r#"="cba""#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (sort (copy-sequence s) #'<))"#,
        r#"!(wrong-type-argument list-or-vector-p #("abc" 0 3 (p 1)))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (seq-take s 2))"#,
        r#"=#("ab" 0 2 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (seq-drop s 1))"#,
        r#"=#("bc" 0 2 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (seq-subseq s 1))"#,
        r#"=#("bc" 0 2 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (append s nil))"#,
        r#"=(97 98 99)"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (vconcat s))"#,
        r#"=[97 98 99]"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (string-to-vector s))"#,
        r#"=[97 98 99]"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (seq-into s 'string))"#,
        r#"=#("abc" 0 3 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (apply #'concat (list s s)))"#,
        r#"=#("abcabc" 0 3 (p 1) 3 6 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (concat s))"#,
        r#"=#("abc" 0 3 (p 1))"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (eq s (concat s)))"#,
        r#"=nil"#,
    );
    check(
        r#"(let ((s (propertize "abc" 'p 1))) (eq s (copy-sequence s)))"#,
        r#"=nil"#,
    );
}

#[test]
fn format_carries_properties_of_template_and_arguments() {
    check(
        r#"(format "%5s|" (propertize "ab" 'a 1))"#,
        r#"=#("   ab|" 3 5 (a 1))"#,
    );
    check(
        r#"(format "%-5s|" (propertize "ab" 'a 1))"#,
        r#"=#("ab   |" 0 5 (a 1))"#,
    );
    check(
        r#"(format "%.1s|" (propertize "ab" 'a 1))"#,
        r#"=#("a|" 0 1 (a 1))"#,
    );
    check(
        r#"(format "%5.1s|" (propertize "ab" 'a 1))"#,
        r#"=#("    a|" 4 5 (a 1))"#,
    );
    check(
        r#"(format "x%sy" (propertize "ab" 'a 1))"#,
        r#"=#("xaby" 1 3 (a 1))"#,
    );
    check(
        r#"(format (propertize "x%sy" 'f 1) "ab")"#,
        r#"=#("xaby" 0 4 (f 1))"#,
    );
    check(
        r#"(format (propertize "x%sy" 'f 1) (propertize "ab" 'a 1))"#,
        r#"=#("xaby" 0 1 (f 1) 1 3 (a 1 f 1) 3 4 (f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%5s" 'f 1) "y") "ab")"#,
        r#"=#("x   aby" 1 6 (f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%-5s" 'f 1) "y") "ab")"#,
        r#"=#("xab   y" 1 6 (f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%5s" 'f 1) "y") (propertize "ab" 'a 1))"#,
        r#"=#("x   aby" 1 6 (a 1 f 1))"#,
    );
    check(
        r#"(format (concat "ab" (propertize "%d" 'f 1) (propertize "cd" 'g 2)) 5)"#,
        r#"=#("ab5cd" 2 3 (f 1) 3 5 (g 2))"#,
    );
    check(
        r#"(format (concat (propertize "a%%b" 'f 1) "c%sd") "X")"#,
        r#"=#("a%bcXd" 0 3 (f 1))"#,
    );
    check(
        r#"(format (concat "a%%b" (propertize "c%sd" 'f 1)) "X")"#,
        r#"=#("a%bcXd" 3 6 (f 1))"#,
    );
    check(
        r#"(format (propertize "%s" 'f 1) (propertize "ab" 'a 1 'f 2))"#,
        r#"=#("ab" 0 2 (a 1 f 2))"#,
    );
    check(
        r#"(format "%s%s" (propertize "a" 'p 1) (propertize "b" 'p 1))"#,
        r#"=#("ab" 0 1 (p 1) 1 2 (p 1))"#,
    );
    check(
        r#"(format "%s" (concat (propertize "a" 'p 1) (propertize "b" 'p 1)))"#,
        r#"=#("ab" 0 1 (p 1) 1 2 (p 1))"#,
    );
    check(
        r#"(format "%s %s" (propertize "ab" 'p 1) (propertize "cd" 'q 2))"#,
        r#"=#("ab cd" 0 2 (p 1) 3 5 (q 2))"#,
    );
    check(
        r#"(format "%2$s %1$s" (propertize "ab" 'p 1) "x")"#,
        r#"=#("x ab" 2 4 (p 1))"#,
    );
    check(
        r#"(format-message (propertize "`a' %s" 'z 1) "q")"#,
        r#"=#("‘a’ q" 0 5 (z 1))"#,
    );
    check(
        r#"(format (propertize "%-8s" 'f 1) "ab")"#,
        r#"=#("ab      " 0 8 (f 1))"#,
    );
    check(
        r#"(format (propertize "ab%scd" 'f 1) "XY")"#,
        r#"=#("abXYcd" 0 6 (f 1))"#,
    );
    check(r#"(format "%c" ?a)"#, r#"="a""#);
    check(
        r#"(format (propertize "%c" 'f 1) ?a)"#,
        r#"=#("a" 0 1 (f 1))"#,
    );
    check(
        r#"(format (propertize "%5d" 'f 1) 12)"#,
        r#"=#("   12" 0 5 (f 1))"#,
    );
    check(
        r#"(format (propertize "abc" 'f 1))"#,
        r#"=#("abc" 0 3 (f 1))"#,
    );
    check(
        r#"(format (concat (propertize "a" 'f 1) "b" (propertize "c" 'f 1)))"#,
        r#"=#("abc" 0 1 (f 1) 2 3 (f 1))"#,
    );
    check(
        r#"(format (propertize "%5s" 'f 1) (propertize "ab" 'a 1))"#,
        r#"=#("   ab" 0 5 (a 1 f 1))"#,
    );
    check(
        r#"(format (propertize "%-5s" 'f 1) (propertize "ab" 'a 1))"#,
        r#"=#("ab   " 0 5 (a 1 f 1))"#,
    );
    check(
        r#"(format (propertize "%5s" 'f 1) (propertize "abcdef" 'a 1))"#,
        r#"=#("abcdef" 0 6 (a 1 f 1))"#,
    );
    check(
        r#"(format (propertize "%.3s" 'f 1) (propertize "abcdef" 'a 1))"#,
        r#"=#("abc" 0 3 (a 1 f 1))"#,
    );
    check(
        r#"(format (concat (propertize "x" 'g 1) "%5s") (propertize "ab" 'a 1))"#,
        r#"=#("x   ab" 0 1 (g 1) 1 6 (a 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%3s" 'f 1) "y") (propertize "ab" 'a 1))"#,
        r#"=#("x aby" 1 4 (a 1 f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%3s" 'f 1) "y") (concat (propertize "a" 'a 1) "b"))"#,
        r#"=#("x aby" 1 2 (a 1 f 1) 2 4 (f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%5s" 'f 1) "y") (concat (propertize "a" 'a 1) "b"))"#,
        r#"=#("x   aby" 1 2 (a 1 f 1) 2 6 (f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%5s" 'f 1) "y") (propertize "ab" 'f 1))"#,
        r#"=#("x   aby" 1 6 (f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%5s" 'f 1) "y") (propertize "ab" 'f 2))"#,
        r#"=#("x   aby" 1 6 (f 2))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%5s" 'f 1) "y") (propertize "ab" 'a 1 'f 1))"#,
        r#"=#("x   aby" 1 6 (a 1 f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%2s" 'f 1) "y") (propertize "ab" 'a 1))"#,
        r#"=#("xaby" 1 3 (a 1 f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%s" 'f 1) "y") (propertize "ab" 'a 1))"#,
        r#"=#("xaby" 1 3 (a 1 f 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%s" 'f 1) "y") (propertize "ab" 'f 1))"#,
        r#"=#("xaby" 1 3 (f 1))"#,
    );
    check(
        r#"(format "%5s|" (concat (propertize "a" 'a 1) "b"))"#,
        r#"=#("   ab|" 3 4 (a 1))"#,
    );
    check(
        r#"(format "%5s|" (concat "a" (propertize "b" 'a 1)))"#,
        r#"=#("   ab|" 4 5 (a 1))"#,
    );
    check(
        r#"(format "%5s|" (concat (propertize "a" 'a 1) (propertize "b" 'b 1)))"#,
        r#"=#("   ab|" 3 4 (a 1) 4 5 (b 1))"#,
    );
    check(
        r#"(format "%-5s|" (concat (propertize "a" 'a 1) "b"))"#,
        r#"=#("ab   |" 0 1 (a 1))"#,
    );
    check(
        r#"(format "%-5s|" (concat "a" (propertize "b" 'a 1)))"#,
        r#"=#("ab   |" 1 5 (a 1))"#,
    );
    check(
        r#"(format "x%5s|" (concat (propertize "a" 'a 1) "b"))"#,
        r#"=#("x   ab|" 4 5 (a 1))"#,
    );
    check(
        r#"(format "%5s%5s|" (propertize "a" 'a 1) (propertize "b" 'b 1))"#,
        r#"=#("    a    b|" 4 5 (a 1) 9 10 (b 1))"#,
    );
    check(
        r#"(format "%s %5s|" (propertize "a" 'a 1) (propertize "b" 'b 1))"#,
        r#"=#("a     b|" 0 1 (a 1) 6 7 (b 1))"#,
    );
    check(
        r#"(format "%5s|" (propertize "abcdefg" 'a 1))"#,
        r#"=#("abcdefg|" 0 7 (a 1))"#,
    );
    check(
        r#"(format "%3.2s|" (concat (propertize "a" 'a 1) "bcd"))"#,
        r#"=#(" ab|" 1 2 (a 1))"#,
    );
    check(
        r#"(format (propertize "%5s" 'f 1) (concat "a" (propertize "b" 'b 1)))"#,
        r#"=#("   ab" 0 1 (f 1) 1 5 (b 1 f 1))"#,
    );
    check(
        r#"(format (concat (propertize "x" 'f 1) "%5s") (concat "a" (propertize "b" 'b 1)))"#,
        r#"=#("x   ab" 0 1 (f 1) 2 6 (b 1))"#,
    );
    check(
        r#"(format (concat "x" (propertize "%5s|" 'f 1)) (concat "a" (propertize "b" 'b 1)))"#,
        r#"=#("x   ab|" 1 2 (f 1) 2 6 (b 1 f 1) 6 7 (f 1))"#,
    );
}
