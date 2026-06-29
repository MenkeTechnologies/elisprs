;;; regexp.el --- elisp regexps on fusevm, ERT-tested  -*- lexical-binding: nil; -*-

;; Emacs regexps run host-side: the pattern is translated from elisp syntax
;; (\\( \\| \\{ for grouping/alternation/bounds) into fancy-regex's dialect in
;; src/regexp.rs, then matched. fancy-regex's backtracking handles
;; backreferences (\\1..\\9). Match data is char-indexed, like Emacs.
(message "== regexp demo ==")

(ert-deftest regexp-string-match ()
  "string-match returns the char index of the match, or nil."
  (should (= (string-match "world" "hello world") 6))
  (should (null (string-match "zzz" "hello")))
  ;; The optional START argument bounds where the search begins.
  (should (= (string-match "a" "banana" 2) 3)))

(ert-deftest regexp-groups ()
  "Capture groups via match-string / match-beginning / match-end."
  (string-match "\\([a-z]+\\)-\\([0-9]+\\)" "  abc-123 ")
  (should (equal (match-string 1 "  abc-123 ") "abc"))
  (should (equal (match-string 2 "  abc-123 ") "123"))
  (should (= (match-beginning 1) 2))
  (should (= (match-end 2) 9)))

(ert-deftest regexp-unicode-positions ()
  "Match positions count characters, not bytes."
  (string-match "é" "aébé")
  (should (= (match-beginning 0) 1))
  (should (= (match-end 0) 2)))

(ert-deftest regexp-replace ()
  "replace-regexp-in-string: \\N backrefs, \\& whole match, LITERAL flag."
  (should (equal (replace-regexp-in-string "\\([a-z]+\\)=\\([0-9]+\\)" "\\2:\\1" "x=1 yy=22")
                 "1:x 22:yy"))
  (should (equal (replace-regexp-in-string "[0-9]+" "<\\&>" "a1b22")
                 "a<1>b<22>"))
  (should (equal (replace-regexp-in-string "[0-9]+" "#" "a1b22c333" nil t)
                 "a#b#c#")))

(ert-deftest regexp-backreferences ()
  "Backreferences \\1..\\9 match the same text an earlier group captured."
  ;; Doubled character: \1 must match the SAME char, not just any char.
  (should (= (string-match "\\(.\\)\\1" "abccba") 2))
  (should (null (string-match "\\(.\\)\\1" "abcdef")))
  ;; Collapse doubled words to a single copy.
  (should (equal (replace-regexp-in-string "\\b\\(\\w+\\) \\1\\b" "\\1"
                                           "the the cat cat sat")
                 "the cat sat"))
  ;; A repeated captured group.
  (should (= (string-match "\\(ab\\)\\1+" "xabababy") 1)))

(ert-deftest regexp-quote-and-save ()
  "regexp-quote escapes metacharacters; save-match-data shields match state."
  (should (equal (regexp-quote "a.b*c") "a\\.b\\*c"))
  (string-match "b" "abc")
  (save-match-data (string-match "x" "xyz"))
  (should (= (match-beginning 0) 1)))

(ert-run-tests-batch-and-exit)
