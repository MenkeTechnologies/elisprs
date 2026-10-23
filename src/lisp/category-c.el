;;; category-c.el --- category.c, and characters.el's category data  -*- lexical-binding: t -*-

;; A port of category.c: category sets are 128-element bool-vectors, a
;; category table is a char-table of them whose extra slot 0 holds the
;; docstrings of categories ?\s..?~ and extra slot 1 the hash table that
;; shares equal category sets.  The `\cC'/`\CC' regexp escapes read the
;; current table through `ElispHost::category_ranges' (src/host.rs).

(put 'category-table 'char-table-extra-slots 2)

(defvar word-combining-categories nil
  "List of pairs (cons cells) of categories to determine word boundary.")
(defvar word-separating-categories nil
  "List of pairs (cons cells) of categories to determine word boundary.")

(defun category--check (category)
  "CHECK_CATEGORY: CATEGORY must be a fixnum between ?\\s and ?~."
  (unless (and (fixnump category) (<= #x20 category #x7E))
    (signal 'wrong-type-argument (list 'categoryp category)))
  category)

(defun category--set-p (object)
  "CATEGORY_SET_P: a bool-vector of length 128."
  (and (bool-vector-p object) (= (length object) 128)))

(defvar --standard-category-table--
  (let ((table (make-char-table 'category-table nil)))
    ;; A category set which contains nothing is the default.
    (set-char-table-range table nil (make-bool-vector 128 nil))
    (set-char-table-extra-slot table 0 (make-vector 95 nil))
    table)
  "category.c's `Vstandard_category_table'.")

(defvar-local --current-category-table-- nil
  "The current buffer's category table, nil for the standard one.")

(defun category-table-p (arg)
  "Return t if ARG is a category table."
  (and (char-table-p arg)
       (eq (char-table-subtype arg) 'category-table)))

(defun category--check-table (table)
  "check_category_table: nil is the current table, anything else is checked."
  (cond ((null table) (category-table))
        ((category-table-p table) table)
        (t (signal 'wrong-type-argument (list 'category-table-p table)))))

(defun category-table ()
  "Return the current category table.
This is the one specified by the current buffer."
  (or --current-category-table-- --standard-category-table--))

(defun standard-category-table ()
  "Return the standard category table.
This is the one used for new buffers."
  --standard-category-table--)

(defun make-category-set (categories)
  "Return a newly created category-set which contains CATEGORIES.
CATEGORIES is a string of category mnemonics.
The value is a bool-vector which has t at the indices corresponding to
those categories."
  (unless (stringp categories)
    (signal 'wrong-type-argument (list 'stringp categories)))
  (let ((val (make-bool-vector 128 nil)))
    (when (multibyte-string-p categories)
      (error "Multibyte string in `make-category-set'"))
    (let ((len (length categories)))
      (while (>= (setq len (1- len)) 0)
        (let ((cat (aref categories len)))
          (category--check cat)
          (aset val cat t))))
    val))

(defun define-category (category docstring &optional table)
  "Define CATEGORY as a category which is described by DOCSTRING.
CATEGORY should be an ASCII printing character in the range ` ' to `~'.
DOCSTRING is the documentation string of the category.  The first line
should be a terse text (preferably less than 16 characters),
and the rest lines should be the full description.
The category is defined only in category table TABLE, which defaults to
the current buffer's category table."
  (category--check category)
  (unless (stringp docstring)
    (signal 'wrong-type-argument (list 'stringp docstring)))
  (setq table (category--check-table table))
  (let ((docs (char-table-extra-slot table 0)))
    (when (aref docs (- category ?\s))
      (error "Category `%c' is already defined" category))
    (aset docs (- category ?\s) docstring))
  nil)

(defun category-docstring (category &optional table)
  "Return the documentation string of CATEGORY, as defined in TABLE.
TABLE should be a category table and defaults to the current buffer's
category table."
  (category--check category)
  (setq table (category--check-table table))
  (aref (char-table-extra-slot table 0) (- category ?\s)))

(defun get-unused-category (&optional table)
  "Return a category which is not yet defined in TABLE.
If no category remains available, return nil.
The optional argument TABLE specifies which category table to modify;
it defaults to the current buffer's category table."
  (setq table (category--check-table table))
  (let ((docs (char-table-extra-slot table 0))
        (i ?\s)
        (found nil))
    (while (and (not found) (<= i ?~))
      (if (aref docs (- i ?\s))
          (setq i (1+ i))
        (setq found i)))
    found))

(defun copy-category-table (&optional table)
  "Construct a new category table and return it.
It is a copy of the TABLE, which defaults to the standard category table."
  (if table
      (category--check-table table)
    (setq table --standard-category-table--))
  ;; copy_category_table: no category set may be shared with the original.
  (let ((new (copy-sequence table)))
    (when (char-table-range new nil)
      (set-char-table-range new nil (copy-sequence (char-table-range new nil))))
    (set-char-table-extra-slot new 0
                               (copy-sequence (char-table-extra-slot new 0)))
    (map-char-table (lambda (c val)
                      (set-char-table-range new c (copy-sequence val)))
                    new)
    new))

(defun make-category-table ()
  "Construct a new and empty category table and return it."
  (let ((val (make-char-table 'category-table nil)))
    (set-char-table-range val nil (make-bool-vector 128 nil))
    ;; Each of the (1 << CHARTAB_SIZE_BITS_0) top-level slots gets a
    ;; category set of its own.
    (dotimes (i 64)
      (set-char-table-range val (cons (* i #x10000) (1- (* (1+ i) #x10000)))
                            (make-bool-vector 128 nil)))
    (set-char-table-extra-slot val 0 (make-vector 95 nil))
    val))

(defun set-category-table (table)
  "Specify TABLE as the category table for the current buffer.
Return TABLE."
  (setq table (category--check-table table))
  (setq --current-category-table-- table)
  table)

(defun char-category-set (char)
  "Return the category set of CHAR."
  (unless (characterp char)
    (signal 'wrong-type-argument (list 'characterp char)))
  (char-table-range (category-table) char))

(defun category-set-mnemonics (category-set)
  "Return a string containing mnemonics of the categories in CATEGORY-SET.
CATEGORY-SET is a bool-vector, and the categories \"in\" it are those
that are indexes where t occurs in the bool-vector.
The return value is a string containing those same categories."
  (unless (category--set-p category-set)
    (signal 'wrong-type-argument (list 'categorysetp category-set)))
  (let ((chars nil))
    (let ((i 126))
      (while (>= i 32)
        (when (aref category-set i)
          (push i chars))
        (setq i (1- i))))
    (apply #'string chars)))

(defun category--hash-get-category-set (table category-set)
  "hash_get_category_set: the category set in TABLE `equal' to CATEGORY-SET."
  (let ((h (or (char-table-extra-slot table 1)
               (set-char-table-extra-slot table 1 (make-hash-table :test #'equal)))))
    (or (gethash category-set h)
        (puthash category-set category-set h))))

(defun modify-category-entry (character category &optional table reset)
  "Modify the category set of CHARACTER by adding CATEGORY to it.
The category is changed only for table TABLE, which defaults to
the current buffer's category table.
CHARACTER can be either a single character or a cons representing the
lower and upper ends of an inclusive character range to modify.
CATEGORY must be a category name (a character between ` ' and `~').
Use `describe-categories' to see existing category names.
If optional fourth argument RESET is non-nil,
then delete CATEGORY from the category set instead of adding it."
  (let (start end)
    (if (fixnump character)
        (progn
          (unless (characterp character)
            (signal 'wrong-type-argument (list 'characterp character)))
          (setq start character end character))
      (unless (consp character)
        (signal 'wrong-type-argument (list 'consp character)))
      (unless (characterp (car character))
        (signal 'wrong-type-argument (list 'characterp (car character))))
      (unless (characterp (cdr character))
        (signal 'wrong-type-argument (list 'characterp (cdr character))))
      (setq start (car character) end (cdr character)))
    (category--check category)
    (setq table (category--check-table table))
    (unless (category-docstring category table)
      (error "Undefined category: %c" category))
    (while (<= start end)
      (let ((to (--char-table-run-end-- table start end))
            (category-set (char-table-range table start)))
        (unless (eq (and (aref category-set category) t) (null reset))
          (setq category-set (copy-sequence category-set))
          (aset category-set category (null reset))
          (setq category-set
                (category--hash-get-category-set table category-set))
          (set-char-table-range table (cons start to) category-set))
        (setq start (1+ to))))
    nil))

;;;; characters.el (GNU Emacs 31.1), verbatim: every top-level form that
;;;; defines a category or modifies a category entry, in file order, except
;;;; those that also need what elisprs does not model -- charset code-point
;;;; maps (`map-charset-chars', `decode-char' of a charset), the Unicode
;;;; property tables (`unicode-property-table-internal': the bidi `?L'/`?R',
;;;; base `?.', combining `?^' and digit `?6' categories), and the forms that
;;;; set syntax or case entries alongside categories (the Lao, Thai, Tibetan
;;;; and Tai Viet tables, and the Latin/Greek/Cyrillic case-table block).

(define-category ?a "ASCII
ASCII graphic characters 32-126 (ISO646 IRV:1983[4/0])")
(define-category ?l "Latin")
(define-category ?t "Thai")
(define-category ?g "Greek")
(define-category ?b "Arabic")
(define-category ?w "Hebrew")
(define-category ?y "Cyrillic")
(define-category ?k "Katakana
Japanese katakana")
(define-category ?r "Roman
Japanese roman")
(define-category ?c "Chinese")
(define-category ?j "Japanese")
(define-category ?h "Korean")
(define-category ?e "Ethiopic
Ethiopic (Ge'ez)")
(define-category ?v "Viet
Vietnamese")
(define-category ?i "Indian")
(define-category ?o "Lao")
(define-category ?q "Tibetan")
(define-category ?A "2-byte alnum
Alphanumeric characters of 2-byte character sets")
(define-category ?C "2-byte han
Chinese (Han) characters of 2-byte character sets")
(define-category ?G "2-byte Greek
Greek characters of 2-byte character sets")
(define-category ?H "2-byte Hiragana
Japanese Hiragana characters of 2-byte character sets")
(define-category ?K "2-byte Katakana
Japanese Katakana characters of 2-byte character sets")
(define-category ?N "2-byte Korean
Korean Hangul characters of 2-byte character sets")
(define-category ?Y "2-byte Cyrillic
Cyrillic characters of 2-byte character sets")
(define-category ?I "Indian Glyphs")
(define-category ?0 "consonant")
(define-category ?1 "base vowel
Base (independent) vowel")
(define-category ?2 "upper diacritic
Upper diacritical mark (including upper vowel)")
(define-category ?3 "lower diacritic
Lower diacritical mark (including lower vowel)")
(define-category ?4 "combining tone
Combining tone mark")
(define-category ?5 "symbol")
(define-category ?6 "digit")
(define-category ?7 "vowel diacritic
Vowel-modifying diacritical mark")
(define-category ?8 "vowel-signs")
(define-category ?9 "semivowel lower")
(define-category ?| "line breakable
While filling, we can break a line at this character.")
(define-category ?\s
  "space for indent
This character counts as a space for indentation purposes.")
(define-category ?> "Not at bol
A character which can't be placed at beginning of line.")
(define-category ?< "Not at eol
A character which can't be placed at end of line.")
(define-category ?. "Base
Base characters (Unicode General Category L,N,P,S,Zs)")
(define-category ?^ "Combining
Combining diacritic or mark (Unicode General Category M)")
(define-category ?R "Strong R2L
Characters with \"strong\" right-to-left directionality, i.e.
with R, AL, RLE, or RLO Unicode bidi character type.")
(define-category ?L "Strong L2R
Characters with \"strong\" left-to-right directionality, i.e.
with L, LRE, or LRO Unicode bidi character type.")
(modify-category-entry '(32 . 127) ?a)
(modify-category-entry '(32 . 127) ?l)
(modify-category-entry '(#x2E80 . #x312F) ?|)
(modify-category-entry '(#x3190 . #x33FF) ?|)
(modify-category-entry '(#x3400 . #x4DBF) ?C)
(modify-category-entry '(#x4E00 . #x9FFF) ?C)
(modify-category-entry '(#x3400 . #x9FFF) ?c)
(modify-category-entry '(#x3400 . #x9FFF) ?|)
(modify-category-entry '(#xF900 . #xFAFF) ?C)
(modify-category-entry '(#xF900 . #xFAFF) ?c)
(modify-category-entry '(#xF900 . #xFAFF) ?|)
(modify-category-entry '(#x1B170 . #x1B2FF) ?c)
(modify-category-entry '(#x20000 . #x2FFFF) ?|)
(modify-category-entry '(#x20000 . #x2FFFF) ?C)
(modify-category-entry '(#x20000 . #x2FFFF) ?c)
(modify-category-entry '(#x30000 . #x323AF) ?|)
(modify-category-entry '(#x30000 . #x323AF) ?C)
(modify-category-entry '(#x30000 . #x323AF) ?c)
(modify-category-entry '(#xff01 . #xff60) ?\|)
(let ((range '(#xff61 . #xff9f)))
  (modify-category-entry range  ?k)
  (modify-category-entry range ?j)
  (modify-category-entry range ?\|))
(modify-category-entry '(#x3099 . #x309C) ?K)
(modify-category-entry '(#x30A0 . #x30FF) ?K)
(modify-category-entry '(#x31F0 . #x31FF) ?K)
(modify-category-entry '(#x30A0 . #x30FA) ?\|)
(modify-category-entry #x30FF ?\|)
(modify-category-entry '(#x1AFF0 . #x1B000) ?K)
(modify-category-entry '(#x1B120 . #x1B122) ?K)
(modify-category-entry '(#x1B164 . #x1B167) ?K)
(modify-category-entry '(#x3040 . #x309F) ?H)
(modify-category-entry '(#x3040 . #x3096) ?\|)
(modify-category-entry #x309F ?\|)
(modify-category-entry #x30A0 ?H)
(modify-category-entry #x30FC ?H)
(modify-category-entry #x1B001 ?H)
(modify-category-entry #x1B11F ?H)
(modify-category-entry '(#x1B150 . #x1B152) ?H)
(modify-category-entry '(#x1B002 . #x1B11E) ?H)
(modify-category-entry '(#x1AFF0 . #x1B1FF) ?j)
(let ((chars '(?仝 ?々 ?〆 ?〇)))
  (while chars
    (modify-category-entry (car chars) ?C)
    (setq chars (cdr chars))))
(modify-category-entry '(#x600 . #x6ff) ?b)
(modify-category-entry '(#x870 . #x8ff) ?b)
(modify-category-entry '(#xfb50 . #xfdcf) ?b)
(modify-category-entry '(#xfdf0 . #xfdff) ?b)
(modify-category-entry '(#xfe70 . #xfefe) ?b)
(modify-category-entry '(#x10ec0 . #x10eff) ?b)
(modify-category-entry '(#x1200 . #x1399) ?e)
(modify-category-entry '(#X2D80 . #X2DDE) ?e)
(modify-category-entry '(#xAB01 . #xAB2E) ?e)
(modify-category-entry '(#x1E7E0 . #x1E7FE) ?e)
(modify-category-entry '(#x901 . #x970) ?i)
(modify-category-entry '(#xe80 . #xeff) ?o)
(modify-category-entry '(#xe00 . #xe7f) ?t)
(modify-category-entry '(#xf00 . #xfff) ?q)
(modify-category-entry '(#x80 . #x024F) ?l)
