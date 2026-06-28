;;; strings.el --- string building & formatting on fusevm  -*- lexical-binding: nil; -*-

;; Strings flow through fusevm as values; format/concat/mapconcat run host-side
;; via the subr table. A failed `expect` raises an error → non-zero exit.

(defun expect (label got want)
  (if (equal got want)
      (message "ok   %s" label)
    (error "FAIL %s: got %S, want %S" label got want)))

(expect "concat"     (concat "foo" "bar" "baz") "foobarbaz")
(expect "concat-empty" (concat "" "x" "") "x")
(expect "length"     (length "hello") 5)
(expect "format-d"   (format "%d + %d = %d" 2 3 5) "2 + 3 = 5")
(expect "format-s"   (format "<%s>" "x") "<x>")
(expect "format-S"   (format "%S" (list 1 2)) "(1 2)")
(expect "num->str"   (number-to-string 42) "42")
(expect "symbol-name" (symbol-name 'hello) "hello")
(expect "join"       (mapconcat 'identity (list "a" "b" "c") ", ") "a, b, c")
(expect "join-nums"  (mapconcat 'number-to-string (number-sequence 1 4) "") "1234")

(message "strings: all checks passed on fusevm")
