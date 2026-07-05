;;; noprov.el --- fixture: a file that does NOT provide its feature  -*- lexical-binding: nil; -*-

;; Loaded by examples/custom-load-preload.el to exercise the `require' "failed
;; to provide" error path. Under load-fixtures/ so it is not run standalone.
(defvar noprov-marker 'noprov-was-loaded)
;;; noprov.el ends here
