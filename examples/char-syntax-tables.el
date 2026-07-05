;;; char-syntax-tables.el --- char-tables + syntax tables, ERT-tested  -*- lexical-binding: nil; -*-

;; The char-table Value type (chartab.c documented behavior) and the syntax-table
;; subsystem built on it (syntax.c documented behavior):
;;
;;   char-tables: make-char-table, char-table-p, char-table-subtype,
;;   char-table-parent/set-char-table-parent, char-table-extra-slot/
;;   set-char-table-extra-slot, char-table-range/set-char-table-range, and
;;   aref/aset extended to accept a char-table (aref/char-table-range fall back
;;   own-value -> default -> parent, like Emacs `char_table_ref').
;;
;;   syntax tables: make-syntax-table, standard-syntax-table, syntax-table (the
;;   current one), set-syntax-table, copy-syntax-table, modify-syntax-entry,
;;   char-syntax, string-to-syntax, syntax-class, syntax-class-to-char, and the
;;   with-syntax-table macro.
;;
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
;; The `.'/`-' syntax classes are asserted against the STANDARD syntax table
;; (fundamental-mode / a temp buffer), where `.' is punctuation and `-' is a
;; symbol constituent -- not the lisp-interaction *scratch* table.  Run through
;; fusevm; `ert-run-tests-batch-and-exit' gates the suite.
(message "== char-tables + syntax tables ==")

;; ---- make-char-table: aset/aref, INIT fill, char-table-p, subtype, type-of ----
(ert-deftest ct-basic ()
  (let ((c (make-char-table 'test 9)))
    (aset c ?a 1)
    ;; emacs -Q: (1 9 t)  -- INIT (9) fills every unset char.
    (should (equal (list (aref c ?a) (aref c ?b) (char-table-p c)) '(1 9 t)))
    (should (eq (char-table-subtype c) 'test))
    (should (eq (type-of c) 'char-table))
    (should (eq (char-table-p [1 2]) nil))
    ;; char-tables are arrays and sequences (Emacs `arrayp'/`sequencep').
    (should (eq t (arrayp c)))
    (should (eq t (sequencep c)))))

;; ---- set-char-table-range: char, cons (FROM . TO), t (whole), nil (default) ----
(ert-deftest ct-range ()
  (let ((c (make-char-table 'test)))
    (set-char-table-range c '(?0 . ?9) 'digit)
    ;; emacs -Q: (digit nil digit)
    (should (equal (list (aref c ?5) (aref c ?a) (char-table-range c ?5))
                   '(digit nil digit)))
    ;; A single char, then a whole-table `t', then re-narrow the digit range.
    (aset c ?5 'five)
    (should (equal (list (aref c ?4) (aref c ?5) (aref c ?6)) '(digit five digit)))
    (set-char-table-range c t 'all)
    (set-char-table-range c '(?0 . ?9) 'd)
    ;; emacs -Q: (all d)
    (should (equal (list (aref c ?a) (aref c ?5)) '(all d)))))

;; ---- nil range sets the DEFAULT slot (own char values still win) ----
(ert-deftest ct-default-slot ()
  (let ((c (make-char-table 'test 5)))
    (set-char-table-range c nil 'x)
    ;; emacs -Q: own char value (5) wins over the default; char-table-range nil
    ;; returns the default -> (5 x).
    (should (equal (list (aref c ?a) (char-table-range c nil)) '(5 x)))))

;; ---- lookup fallback: own value -> default -> parent (char_table_ref) ----
(ert-deftest ct-parent-fallback ()
  (let ((p (make-char-table 'test))
        (c (make-char-table 'test)))
    (aset p ?a 'pv)
    (set-char-table-parent c p)
    ;; emacs -Q: (pv nil pv)  -- ?a falls through to the parent, ?b stays nil.
    (should (equal (list (aref c ?a) (aref c ?b) (char-table-range c ?a))
                   '(pv nil pv)))
    (should (eq (char-table-parent c) p)))
  ;; A non-nil DEFAULT takes priority over the parent.
  (let ((p (make-char-table 'test))
        (c (make-char-table 'test 'def)))
    (aset p ?a 'pv)
    (set-char-table-parent c p)
    ;; emacs -Q: (def def)
    (should (equal (list (aref c ?a) (aref c ?b)) '(def def)))))

;; ---- extra slots (sized from SUBTYPE's char-table-extra-slots property) ----
(ert-deftest ct-extra-slots ()
  (put 'ct-big 'char-table-extra-slots 3)
  (let ((c (make-char-table 'ct-big)))
    (set-char-table-extra-slot c 1 'hi)
    ;; emacs -Q: (nil hi)
    (should (equal (list (char-table-extra-slot c 0) (char-table-extra-slot c 1))
                   '(nil hi)))))

;; ---- string-to-syntax: class code + matching char + flag bits ----
(ert-deftest st-string-to-syntax ()
  ;; emacs -Q: ((2) (0) (1) (4 . 41))
  (should (equal (list (string-to-syntax "w") (string-to-syntax " ")
                       (string-to-syntax ".") (string-to-syntax "(){"))
                 '((2) (0) (1) (4 . 41))))
  ;; `-' is an alias for whitespace (class 0).
  (should (equal (string-to-syntax "-") '(0)))
  ;; Flag chars set the high bits: emacs -Q ((65538) (196609) (1048579)).
  (should (equal (list (string-to-syntax "w  1") (string-to-syntax ". 12")
                       (string-to-syntax "_ p"))
                 '((65538) (196609) (1048579)))))

;; ---- syntax-class / syntax-class-to-char round-trip ----
(ert-deftest st-syntax-class ()
  ;; emacs -Q: (4 2 nil)
  (should (equal (list (syntax-class '(4 . 41)) (syntax-class (string-to-syntax "w"))
                       (syntax-class nil))
                 '(4 2 nil)))
  ;; The class->designator spec (syntax.c syntax_code_spec).
  (should (equal (mapcar #'syntax-class-to-char (number-sequence 0 15))
                 '(?\s ?. ?w ?_ ?\( ?\) ?' ?\" ?$ ?\\ ?/ ?< ?> ?@ ?! ?|))))

;; ---- the standard syntax table: raw entries + char-syntax designators ----
(ert-deftest st-standard-table ()
  (should (syntax-table-p (standard-syntax-table)))
  ;; emacs -Q (temp buffer): raw entries are (CLASS . MATCH) conses.
  (should (equal (list (aref (standard-syntax-table) ?a)
                       (aref (standard-syntax-table) ?\()
                       (aref (standard-syntax-table) ?0))
                 '((2) (4 . 41) (2))))
  ;; char-syntax designators in the standard table (word, digit=word, space,
  ;; punctuation, open-paren, symbol).  emacs -Q temp buffer: (?w ?w ?\s ?. ?\( ?_).
  (should (equal (list (char-syntax ?a) (char-syntax ?0) (char-syntax ?\s)
                       (char-syntax ?.) (char-syntax ?\() (char-syntax ?-))
                 '(?w ?w ?\s ?. ?\( ?_))))

;; ---- make-syntax-table parents the standard table; with-syntax-table scopes ----
(ert-deftest st-make-and-with ()
  (let ((tbl (make-syntax-table)))
    (should (eq (char-table-parent tbl) (standard-syntax-table)))
    (should (eq (char-table-subtype tbl) 'syntax-table)))
  ;; modify-syntax-entry inside with-syntax-table, then restore.  emacs -Q: ?w.
  (should (eq ?w (with-syntax-table (make-syntax-table)
                   (modify-syntax-entry ?- "w")
                   (char-syntax ?-))))
  ;; The temporary table did not leak: `-' is a symbol again in the standard table.
  (should (eq ?_ (char-syntax ?-))))

;; ---- set-syntax-table selects the current buffer's table ----
(ert-deftest st-set-syntax-table ()
  (let ((tb (make-syntax-table)))
    (set-syntax-table tb)
    (should (eq (syntax-table) tb))
    ;; Restore the standard table so later tests see the default.
    (set-syntax-table (standard-syntax-table))
    (should (eq (syntax-table) (standard-syntax-table)))))

;; ---- special-mode (simple.el) runs on the char-table/syntax substrate ----
(ert-deftest st-special-mode ()
  (special-mode)
  ;; emacs -Q: (special-mode "Special" t)
  (should (equal (list major-mode mode-name buffer-read-only)
                 '(special-mode "Special" t)))
  ;; The mode installed a syntax table parenting the standard one.
  (should (syntax-table-p (syntax-table)))
  (should (char-table-p special-mode-syntax-table)))

(ert-run-tests-batch-and-exit)
