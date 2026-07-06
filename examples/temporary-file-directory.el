;;; temporary-file-directory.el --- temp-dir identity vars, ERT-tested  -*- lexical-binding: t; -*-

;; Pins the C-level temp-directory variables from callproc.c that real init
;; files read at load time: `temporary-file-directory' (Vtemporary_file_directory,
;; set in `init_callproc') and `small-temporary-file-directory'. Before these
;; existed, loading stock lisp (arc-mode.el, files.el, jka-compr.el, ...) died
;; with "Symbol's value as variable is void: temporary-file-directory".
;;
;; The value is derived exactly as Emacs's C does: `$TMPDIR' if present in the
;; environment (even when empty), else the macOS Darwin per-user temp dir from
;; confstr(_CS_DARWIN_USER_TEMP_DIR), else "/tmp/" -- then wrapped in
;; `file-name-as-directory'. Every `should' is verified against GNU Emacs 30.2
;; -Q --batch and uses structural assertions (no host-specific literals).

(ert-deftest temporary-file-directory-is-a-directory-string ()
  ;; A non-empty string naming a directory: it always carries a trailing slash
  ;; because `file-name-as-directory' is applied to the raw value.
  (should (stringp temporary-file-directory))
  (should (> (length temporary-file-directory) 0))
  (should (string-suffix-p "/" temporary-file-directory))
  (should (directory-name-p temporary-file-directory)))

(ert-deftest temporary-file-directory-tracks-tmpdir ()
  ;; When TMPDIR is set (as it is under a normal login/test environment), the
  ;; value is exactly `file-name-as-directory' of TMPDIR -- the first branch of
  ;; the C resolution order. Skipped only if the harness runs with TMPDIR unset.
  (let ((tmp (getenv "TMPDIR")))
    (when tmp
      (should (string= temporary-file-directory
                       (file-name-as-directory tmp))))))

(ert-deftest temporary-file-directory-is-absolute-or-dot ()
  ;; Either an absolute path, or "./" for the empty-TMPDIR edge case -- matching
  ;; `file-name-as-directory' of the empty string.
  (should (or (file-name-absolute-p temporary-file-directory)
              (string= temporary-file-directory "./"))))

(ert-deftest temporary-file-directory-small-default-nil ()
  ;; `small-temporary-file-directory' defaults to nil under `emacs -Q'.
  (should (eq small-temporary-file-directory nil)))

(ert-run-tests-batch-and-exit)
