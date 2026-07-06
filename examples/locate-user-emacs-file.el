;;; locate-user-emacs-file.el --- per-user Emacs file location, ERT-tested  -*- lexical-binding: t; -*-

;; Faithful port of `locate-user-emacs-file' (files.el) plus the four support
;; variables real init files read at load time: `init-file-user' (startup.el),
;; `user-emacs-directory' (startup.el; effective runtime value), `dump-mode'
;; (emacs.c) and `user-emacs-directory-warning' (files.el).  Before these
;; existed, loading stock lisp (subr.el, abbrev.el, js.el, ecomplete.el,
;; gnus-registry.el, esh-mode.el, ...) died with
;; "Symbol's function definition is void: locate-user-emacs-file".
;;
;; The function is copied verbatim from Emacs 30.2 files.el; the batch/dump
;; short-circuit `(or noninteractive dump-mode ...)' means the directory-create
;; branch is never reached here.  Every `should' is structural (derived from the
;; live `user-emacs-directory' value, no host-specific literals) and verified
;; against GNU Emacs 30.2 `emacs -Q --batch'.

(ert-deftest luef-returns-string-under-user-emacs-directory ()
  ;; A bare NEW-NAME resolves under `user-emacs-directory'.  The result is
  ;; exactly the documented composition, so derive the expected value instead
  ;; of hardcoding "~/.emacs.d/init.el".
  (let ((got (locate-user-emacs-file "init.el")))
    (should (stringp got))
    (should (string= got
                     (convert-standard-filename
                      (abbreviate-file-name
                       (expand-file-name "init.el" user-emacs-directory)))))))

(ert-deftest luef-preserves-nested-and-trailing-slash ()
  ;; Sub-paths and directory names (trailing slash) pass through unchanged.
  (should (string= (locate-user-emacs-file "foo/bar.el")
                   (convert-standard-filename
                    (abbreviate-file-name
                     (expand-file-name "foo/bar.el" user-emacs-directory)))))
  (should (string-suffix-p "/" (locate-user-emacs-file "elpa/"))))

(ert-deftest luef-honors-let-bound-directory ()
  ;; `user-emacs-directory' is a plain dynamic variable: rebinding it redirects
  ;; the result.  Absolute directory in => absolute file out, verbatim.
  (let ((user-emacs-directory "/tmp/uedtest/"))
    (should (string= (locate-user-emacs-file "cache.el") "/tmp/uedtest/cache.el"))))

(ert-deftest luef-old-name-fallback-to-bestname ()
  ;; With OLD-NAME given but neither ~/OLD-NAME nor the new path readable, the
  ;; function returns the new-name location (BESTNAME), not the old one.
  (let* ((new "elisprs-nonexistent-new-xyz.el")
         (old ".elisprs-nonexistent-old-xyz")
         (got (locate-user-emacs-file new old)))
    (should (string= got
                     (convert-standard-filename
                      (abbreviate-file-name
                       (expand-file-name new user-emacs-directory)))))
    (should-not (string-match-p (regexp-quote old) got))))

(ert-deftest luef-support-variable-defaults ()
  ;; Defaults match `emacs -Q --batch': init-file-user nil, dump-mode nil,
  ;; user-emacs-directory-warning t, user-emacs-directory a slash-terminated dir.
  (should (eq init-file-user nil))
  (should (eq (bound-and-true-p dump-mode) nil))
  (should (eq user-emacs-directory-warning t))
  (should (stringp user-emacs-directory))
  (should (directory-name-p user-emacs-directory)))

(ert-run-tests-batch-and-exit)
