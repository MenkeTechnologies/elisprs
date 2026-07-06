;;; exec-path.el --- exec-path / file-location subsystem vs Emacs 30.2  -*- lexical-binding: nil; -*-

;; Differential-tested against real `emacs -Q --batch' 30.2. Pins the port of
;; `executable-find', `locate-file' (+ `locate-file-internal', the `openp'
;; search), `file-executable-p', and the `exec-path'/`exec-suffixes'/
;; `path-separator' variables. Values that are deterministic across machines use
;; `sh'/`ls' (present under /bin on every unix) and /etc/hosts (never +x).
(require 'cl-lib)
(message "== exec-path / file-location subsystem ==")

(ert-deftest exec-suffixes-and-path-separator ()
  "Unix OS-defined constants match Emacs."
  (should (equal path-separator ":"))
  (should (equal exec-suffixes '(""))))

(ert-deftest exec-path-structure ()
  "`exec-path' is $PATH split on `path-separator' (empty elements → \".\")
followed by `exec-directory' with its trailing slash stripped."
  (should (listp exec-path))
  (should (cl-every #'stringp exec-path))
  (should (equal (butlast exec-path)
                 (mapcar (lambda (d) (if (string= d "") "." d))
                         (split-string (getenv "PATH") path-separator))))
  (should (equal (car (last exec-path)) (directory-file-name exec-directory))))

(ert-deftest file-executable-p-basic ()
  "Executable bit is honored: /bin/sh is +x, /etc/hosts is not."
  (should (file-executable-p "/bin/sh"))
  (should-not (file-executable-p "/etc/hosts"))
  (should-not (file-executable-p "/no/such/path/xyz")))

(ert-deftest executable-find-real-and-missing ()
  "Finds real binaries by absolute path; nil for a name in no directory."
  (should (equal (executable-find "sh") "/bin/sh"))
  (should (equal (executable-find "ls") "/bin/ls"))
  (should-not (executable-find "definitely-not-a-real-binary-xyz")))

(ert-deftest locate-file-search-and-suffixes ()
  "PATH + SUFFIXES search; dir-major order; the empty suffix matches the bare name."
  (should (equal (locate-file "sh" exec-path) "/bin/sh"))
  (should (equal (locate-file "ls" '("/bin" "/usr/bin") '(".foo" "")) "/bin/ls"))
  ;; An absolute FILENAME is tried once regardless of PATH.
  (should (equal (locate-file "/bin/sh" nil) "/bin/sh"))
  (should-not (locate-file "/no/such/xyz" nil))
  ;; A relative name with no PATH has nowhere to look.
  (should-not (locate-file "sh" nil)))

(ert-deftest locate-file-directory-skip ()
  "Directories are skipped unless a function PREDICATE returns `dir-ok'."
  (should-not (locate-file "bin" '("/usr")))
  (should-not (locate-file "bin" '("/usr") nil 1))
  (should-not (locate-file "bin" '("/usr") nil (lambda (_f) t)))
  (should (equal (locate-file "bin" '("/usr") nil (lambda (_f) 'dir-ok)) "/usr/bin")))

(ert-run-tests-batch-and-exit)
