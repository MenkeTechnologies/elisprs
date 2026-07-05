;;; helper.el --- fixture loaded by examples/load.el  -*- lexical-binding: nil; -*-

;; This file is NOT run directly by run_examples.sh (it lives in a subdirectory,
;; so the examples/*.el glob skips it). It is pulled in by `load' from
;; examples/load.el, and exists to prove three things about the `load' builtin:
;;   1. its top-level forms are evaluated in the caller's host (the defvar below
;;      is visible to load.el afterward),
;;   2. `load-file-name' is dynamically bound to this file's absolute path while
;;      these forms run (captured into loadtest-file-name-during-load), and
;;   3. `load-in-progress' is non-nil during the load.
(defvar loadtest-marker 'helper-was-loaded)
(setq loadtest-file-name-during-load load-file-name)
(setq loadtest-in-progress-during-load load-in-progress)
