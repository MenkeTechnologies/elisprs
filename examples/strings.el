;;; strings.el --- string building & formatting on fusevm, ERT-tested  -*- lexical-binding: nil; -*-

;; Strings flow through fusevm as values; format/concat/mapconcat run host-side
;; via the subr table.
(message "== strings demo ==")

(ert-deftest strings-concat ()
  "concat / length."
  (should (equal (concat "foo" "bar" "baz") "foobarbaz"))
  (should (equal (concat "" "x" "") "x"))
  (should (= (length "hello") 5)))

(ert-deftest strings-format ()
  "format directives %d / %s / %S, and number-to-string / symbol-name."
  (should (equal (format "%d + %d = %d" 2 3 5) "2 + 3 = 5"))
  (should (equal (format "<%s>" "x") "<x>"))
  (should (equal (format "%S" (list 1 2)) "(1 2)"))
  (should (equal (number-to-string 42) "42"))
  (should (equal (symbol-name 'hello) "hello")))

(ert-deftest strings-join ()
  "mapconcat as string join."
  (should (equal (mapconcat 'identity (list "a" "b" "c") ", ") "a, b, c"))
  (should (equal (mapconcat 'number-to-string (number-sequence 1 4) "") "1234")))

(ert-run-tests-batch-and-exit)
