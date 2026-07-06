;;; help-fundoc-usage.el --- help.el usage helpers + typed/read-only cl-defstruct slots, ERT-tested  -*- lexical-binding: t; -*-

;; Faithful ports checked against `emacs -Q --batch' (GNU Emacs 30.2):
;;   * help.el usage/docstring helpers `help--docstring-quote',
;;     `help--make-usage', `help--make-usage-docstring', `help-add-fundoc-usage'
;;     and `help-split-fundoc' (called during `cl-defgeneric'/`cl-defmethod'
;;     macro expansion to append the "(fn ARGS)" usage line to a docstring);
;;   * per-slot `:type'/`:read-only'/`:documentation' options on `cl-defstruct';
;;   * `assq'/`assoc'/`rassq' skipping non-cons list elements (Emacs C semantics);
;;   * `macroexp-const-p'/`macroexp-copyable-p'/`macroexp--fgrep' (macroexp.el).
;; Every value asserted below was produced by the reference binary.
(message "== help-fundoc-usage demo ==")

;; Structs are defined at top level (as in oclosure.el): elisprs expands macros
;; when a form is read, so a struct accessor's `setf' expansion can only see the
;; slot index once the `cl-defstruct' has run at a prior top-level form.
(cl-defstruct (hfu-pt) (x 1 :type integer :read-only t) (y 2 :type integer))
(cl-defstruct (hfu-ro) (a 1 :read-only t))
(cl-defstruct (hfu-doc) "A documented struct." (a 7) (b 8))

(ert-deftest help-make-usage-docstring-upcases-and-formats ()
  "`help--make-usage-docstring' upcases parameter names and keeps lambda-list
keywords and default values verbatim."
  (should (equal (help--make-usage-docstring 'fn '(a b &optional c))
                 "(fn A B &optional C)"))
  (should (equal (help--make-usage-docstring 'fn '(a (b 3) &rest args))
                 "(fn A (B 3) &rest ARGS)"))
  ;; `_'-prefixed names drop the underscore and upcase the remainder; `&' names
  ;; pass through as the bare symbol.
  (should (equal (help--make-usage 'foo '(a _b &rest cs))
                 '(foo A B &rest CS))))

(ert-deftest help-docstring-quote-escapes-quote-chars ()
  "`help--docstring-quote' guards quote-like characters so `substitute-command-keys'
round-trips the string."
  (should (equal (help--docstring-quote "a`b'c") "a\\=`b\\='c")))

(ert-deftest help-add-fundoc-usage-appends-usage-line ()
  "`help-add-fundoc-usage' appends a blank-line-separated \"(fn ...)\" usage line,
built from an arglist, a nil docstring, or a pre-formatted usage string."
  (should (equal (help-add-fundoc-usage nil '(a b)) "\n\n(fn A B)"))
  (should (equal (help-add-fundoc-usage "Hello doc." '(a &optional b))
                 "Hello doc.\n\n(fn A &optional B)"))
  ;; A single trailing newline is padded to two; ARGLIST t returns DOCSTRING as-is.
  (should (equal (help-add-fundoc-usage "Doc\n" '(a)) "Doc\n\n(fn A)"))
  (should (equal (help-add-fundoc-usage "Doc" t) "Doc"))
  ;; A string arglist "(FUN ARG...)" keeps the args, replacing FUN with `fn'.
  (should (equal (help-add-fundoc-usage "Doc" "(foo BAR BAZ)")
                 "Doc\n\n(fn BAR BAZ)")))

(ert-deftest help-split-fundoc-splits-usage-and-doc ()
  "`help-split-fundoc' returns (USAGE . DOC) when a docstring carries a usage
line, and honors the SECTION argument."
  (should (equal (help-split-fundoc "Body text.\n\n(fn A B)" 'foo)
                 '("(foo A B)" . "Body text.")))
  (should (eq (help-split-fundoc "Just body" 'foo) nil))
  (should (equal (help-split-fundoc "Just body" 'foo t) '(nil . "Just body"))))

(ert-deftest cl-defstruct-typed-slots-define-and-access ()
  "A `cl-defstruct' with per-slot `:type'/`:read-only' options defines its
constructor, accessors and predicate; `:type' is parsed but not enforced."
  (let ((p (make-hfu-pt :x 10 :y 20)))
    (should (= (hfu-pt-x p) 10))
    (should (= (hfu-pt-y p) 20))
    (should (eq (hfu-pt-p p) t))
    (should (eq (hfu-pt-p 5) nil))
    ;; A writable slot round-trips through setf.
    (setf (hfu-pt-y p) 99)
    (should (= (hfu-pt-y p) 99))))

(ert-deftest cl-defstruct-read-only-slot-setf-errors ()
  "setf on a `:read-only' slot signals an error, matching cl-macs.el's
gv-define-expander.  (elisprs expands macros eagerly, so the setf is deferred to
runtime via `eval' to place the signal inside the test's dynamic extent.)"
  (let ((r (make-hfu-ro)))
    (should-error (eval (list 'setf (list 'hfu-ro-a r) 9) t))
    ;; The value is untouched by the rejected setf.
    (should (= (hfu-ro-a r) 1))))

(ert-deftest cl-defstruct-leading-docstring-is-not-a-slot ()
  "A docstring preceding the slot specs is dropped, not treated as a slot."
  (let ((d (make-hfu-doc)))
    (should (= (hfu-doc-a d) 7))
    (should (= (hfu-doc-b d) 8))))

(ert-deftest alist-search-skips-non-cons-elements ()
  "`assq'/`assoc'/`rassq' skip non-cons list elements (Emacs C `FOR_EACH_TAIL'
+ `CONSP' guard) instead of signalling `wrong-type-argument listp'."
  (should (equal (assq 'b '("s" (a . 1) 5 (b . 2))) '(b . 2)))
  (should (equal (assoc "b" '("s" ("a" . 1) 5 ("b" . 2))) '("b" . 2)))
  (should (equal (rassq 2 '("s" (a . 1) 5 (b . 2))) '(b . 2)))
  ;; The interpretation `cl-defmethod' relies on: a docstring in a body list.
  (should (eq (assq 'interactive '("doc" (if a 1 2))) nil)))

(ert-deftest macroexp-const-and-fgrep ()
  "`macroexp-const-p'/`macroexp-copyable-p' classify constant forms and
`macroexp--fgrep' returns the bindings whose symbol occurs in a form."
  (should (eq (macroexp-const-p ''x) t))
  (should (eq (macroexp-const-p '(function foo)) t))
  (should (eq (macroexp-const-p 5) t))
  (should (equal (macroexp-const-p t) '(t)))       ;memq returns the tail
  (should (eq (macroexp-const-p 'foo) nil))
  (should (eq (macroexp-const-p '(f 1)) nil))
  (should (eq (macroexp-copyable-p 'x) t))
  (should (eq (macroexp-copyable-p '(f 1)) nil))
  (should (equal (macroexp--fgrep '((a) (b)) '(foo a (bar c))) '((a))))
  (should (eq (macroexp--fgrep '((z)) '(foo a b)) nil)))

(cl-defgeneric hfu-g (x)
  "Generic used to exercise type/eql dispatch specificity.")
(cl-defmethod hfu-g ((n integer)) 'int)
(cl-defmethod hfu-g ((n number)) 'num)
(cl-defmethod hfu-g ((s string)) 'str)
(cl-defmethod hfu-g ((n (eql 42))) 'the-answer)
(cl-defmethod hfu-g (x) 'other)

(ert-deftest cl-defmethod-dispatch-specificity ()
  "`cl-defmethod' dispatch picks the most specific applicable method: `integer'
beats `number', `(eql 42)' beats `integer', a type beats the unspecialized
catch-all.  (This exercises the value each `help-add-fundoc-usage'-annotated
method carries; the same helpers back the upstream cl-generic.el port.)"
  (should (eq (hfu-g 5) 'int))
  (should (eq (hfu-g 3.14) 'num))
  (should (eq (hfu-g "hi") 'str))
  (should (eq (hfu-g 42) 'the-answer))
  (should (eq (hfu-g 'sym) 'other)))

(ert-run-tests-batch-and-exit)
