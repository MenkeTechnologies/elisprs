;;; decode-char.el --- decode-char charset lookup, ERT-tested  -*- lexical-binding: nil; -*-

;; `decode-char' (src/charset.c `Fdecode_char') maps a CHARSET plus a CODE-POINT
;; to a character, or nil when the code point is out of range in that charset.
;;
;; The charsets whose mapping is pure arithmetic -- no external mule map tables
;; -- are covered here: `ascii', `eight-bit', `iso-8859-1', and the Unicode
;; charsets (`ucs'/`unicode'/`iso-10646-1').  The mapping constants:
;;   ascii        code 0..127     -> code            (else nil)
;;   iso-8859-1   code 0..255     -> code            (else nil)
;;   ucs/unicode  code 0..#x10FFFF -> code           (else nil)
;;   unicode-bmp  code 0..#xFFFF   -> code           (else nil)
;;   eight-bit    code 128..255   -> #x3FFF00 + code (else nil)
;; CODE-POINT may be an integer, an integral float, or the obsolescent cons
;; form (HIGH . LOW) = HIGH*#x10000 + LOW; it must lie in 0..#xFFFFFFFF or the
;; call signals a plain `error'.  An unknown charset symbol signals
;; `wrong-type-argument charsetp SYM'.
;;
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
;; Run through fusevm; `ert-run-tests-batch-and-exit' gates the suite.
(message "== decode-char ==")

;; ---- identity charsets: ascii / iso-8859-1 / ucs ----
(ert-deftest dc-identity ()
  ;; ascii: only 0..127 are in range.
  (should (equal (decode-char 'ascii 0) 0))
  (should (equal (decode-char 'ascii 65) 65))
  (should (equal (decode-char 'ascii 127) 127))
  (should (eq (decode-char 'ascii 128) nil))
  ;; iso-8859-1: the whole 0..255 byte range maps to itself.
  (should (equal (decode-char 'iso-8859-1 128) 128))
  (should (equal (decode-char 'iso-8859-1 255) 255))
  (should (eq (decode-char 'iso-8859-1 256) nil))
  ;; ucs / unicode: full Unicode range, surrogates included.
  (should (equal (decode-char 'ucs 65) 65))
  (should (equal (decode-char 'unicode #x10FFFF) #x10FFFF))
  (should (equal (decode-char 'ucs 55296) 55296))
  (should (eq (decode-char 'ucs #x110000) nil))
  ;; unicode-bmp is limited to the Basic Multilingual Plane (0..#xFFFF).
  (should (equal (decode-char 'unicode-bmp #xFFFF) #xFFFF))
  (should (eq (decode-char 'unicode-bmp #x10000) nil)))

;; ---- eight-bit: raw bytes 128..255 -> #x3FFF00 + byte ----
(ert-deftest dc-eight-bit ()
  ;; This is the mapping china-util.el's `hz-set-msb-table' relies on.
  (should (equal (decode-char 'eight-bit 128) (+ #x3FFF00 128)))
  (should (equal (decode-char 'eight-bit 128) 4194176))
  (should (equal (decode-char 'eight-bit 255) 4194303))
  ;; Below 128 (and above 255) there is no eight-bit character.
  (should (eq (decode-char 'eight-bit 127) nil))
  (should (eq (decode-char 'eight-bit 0) nil))
  (should (eq (decode-char 'eight-bit 256) nil))
  ;; The eight-bit char is exactly the one `unibyte-char-to-multibyte' produces,
  ;; and `multibyte-char-to-unibyte' inverts it back to the raw byte.
  (should (equal (decode-char 'eight-bit 200) (unibyte-char-to-multibyte 200)))
  (should (equal (multibyte-char-to-unibyte (decode-char 'eight-bit 200)) 200)))

;; ---- CODE-POINT forms: integral float and (HIGH . LOW) cons ----
(ert-deftest dc-code-point-forms ()
  ;; An integral float is accepted like the same integer.
  (should (equal (decode-char 'ucs 65.0) 65))
  ;; Cons (HIGH . LOW) = HIGH*#x10000 + LOW.
  (should (equal (decode-char 'ucs '(0 . 65)) 65))
  (should (equal (decode-char 'ucs '(1 . 0)) #x10000))
  (should (equal (decode-char 'eight-bit '(0 . 200)) 4194248)))

;; ---- error paths: out-of-range code point, unknown charset ----
(ert-deftest dc-errors ()
  ;; A negative or >#xFFFFFFFF code point is not a valid code point at all.
  (should-error (decode-char 'ucs -1) :type 'error)
  (should-error (decode-char 'ucs (1+ #xFFFFFFFF)) :type 'error)
  ;; #xFFFFFFFF itself is in range for the code-point check (just out of range
  ;; for the charset, so nil rather than an error).
  (should (eq (decode-char 'ucs #xFFFFFFFF) nil))
  ;; An unregistered charset symbol fails the charsetp check.
  (should (eq 'wrong-type-argument
             (condition-case e (decode-char 'no-such-charset 65)
               (error (car e))))))

(ert-run-tests-batch-and-exit)
