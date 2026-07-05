;;; keymap.el --- keymap data subsystem, ERT-tested  -*- lexical-binding: nil; -*-

;; The keymap DATA subsystem: keymap.c primitives (make-sparse-keymap, keymapp,
;; define-key, lookup-key, keymap-parent/set-keymap-parent, make-composed-keymap)
;; and the keymap.el string API (key-parse, key-valid-p, keymap-set, keymap-lookup,
;; define-keymap, defvar-keymap, define-prefix-command, suppress-keymap).
;;
;; Every asserted value/structure was verified against `emacs -Q --batch' on
;; Emacs 30.2.  Buffer-integration ops (use-local-map / current-local-map /
;; global-set-key into a live global map) are intentionally NOT covered: they
;; need buffer-local/global state elisprs does not model.  Run through fusevm;
;; `ert-run-tests-batch-and-exit' gates the suite.
(message "== keymap demo ==")

;; ---- make-sparse-keymap / keymapp ----
(ert-deftest keymap-make-sparse ()
  ;; emacs -Q: (make-sparse-keymap) => (keymap)
  (should (equal (make-sparse-keymap) '(keymap)))
  ;; emacs -Q: (make-sparse-keymap "m") => (keymap "m")
  (should (equal (make-sparse-keymap "m") '(keymap "m")))
  (should (eq t (keymapp (make-sparse-keymap))))
  (should (eq nil (keymapp 5)))
  (should (eq nil (keymapp '(notkeymap))))
  ;; A symbol whose function cell is a keymap is itself a keymap.
  (define-prefix-command 'km-test-prefix)
  (should (eq t (keymapp 'km-test-prefix))))

;; ---- define-key / lookup-key: single, replace, return, remove ----
(ert-deftest keymap-define-key-single ()
  (let ((m (make-sparse-keymap)))
    ;; emacs -Q: define-key returns DEF.
    (should (eq 'foo (define-key m "a" 'foo)))
    ;; emacs -Q: (keymap (97 . foo))
    (should (equal m '(keymap (97 . foo))))
    (should (eq 'foo (lookup-key m "a")))
    ;; Redefinition replaces the binding in place.
    (define-key m "a" 'baz)
    (should (equal m '(keymap (97 . baz))))
    ;; emacs -Q: lookup of an undefined key => nil
    (should (eq nil (lookup-key m "z")))
    ;; emacs -Q: a key longer than the binding => count of events used (1)
    (should (= 1 (lookup-key m "ab")))
    ;; REMOVE strips the binding entirely.
    (define-key m "a" nil t)
    (should (equal m '(keymap)))))

;; ---- multi-event sequences build nested prefix keymaps ----
(ert-deftest keymap-define-key-prefix ()
  (let ((m (make-sparse-keymap)))
    (define-key m "\C-xf" 'bar)
    ;; emacs -Q: (keymap (24 keymap (102 . bar)))
    (should (equal m '(keymap (24 keymap (102 . bar)))))
    (should (eq 'bar (lookup-key m "\C-xf")))
    ;; The prefix itself looks up to a keymap.
    (should (equal (lookup-key m "\C-x") '(keymap (102 . bar))))
    (should (eq t (keymapp (lookup-key m "\C-x"))))))

;; ---- vector keys and function-key / meta events ----
(ert-deftest keymap-define-key-vector ()
  (let ((m (make-sparse-keymap)))
    (define-key m [f5] 'foo)
    ;; emacs -Q: (keymap (f5 . foo))
    (should (equal m '(keymap (f5 . foo))))
    (should (eq 'foo (lookup-key m [f5]))))
  ;; A meta-modified integer event expands into ESC (27) + base char.
  (let ((m (make-sparse-keymap)))
    (define-key m (key-parse "M-a") 'foo)
    ;; emacs -Q: (keymap (27 keymap (97 . foo)))
    (should (equal m '(keymap (27 keymap (97 . foo)))))
    (should (eq 'foo (lookup-key m (key-parse "M-a"))))))

;; ---- parent keymaps: structure, chaining, shadowing ----
;; (Test name must not collide with the `keymap-parent' function -- ert-deftest
;;  binds the test onto its name's function cell.)
(ert-deftest keymap-parent-chain ()
  (let ((p (make-sparse-keymap)) (m (make-sparse-keymap)))
    (define-key p "x" 'px)
    (define-key m "a" 'ma)
    (should (eq p (set-keymap-parent m p)))
    ;; emacs -Q: (keymap (97 . ma) keymap (120 . px))  -- parent shared at tail
    (should (equal m '(keymap (97 . ma) keymap (120 . px))))
    ;; keymap-parent returns the parent keymap object.
    (should (equal (keymap-parent m) '(keymap (120 . px))))
    ;; Lookup falls through to the parent.
    (should (eq 'px (lookup-key m "x")))
    ;; A child binding shadows the parent.
    (define-key m "x" 'mx)
    (should (eq 'mx (lookup-key m "x"))))
  ;; No parent => nil.
  (should (eq nil (keymap-parent (make-sparse-keymap)))))

;; ---- make-composed-keymap: list form, single-keymap form, lookup ----
(ert-deftest keymap-composed ()
  ;; emacs -Q: (make-composed-keymap (list (make-sparse-keymap)) (make-sparse-keymap))
  ;;        => (keymap (keymap) keymap)
  (should (equal (make-composed-keymap (list (make-sparse-keymap))
                                       (make-sparse-keymap))
                 '(keymap (keymap) keymap)))
  ;; A single keymap as MAPS is wrapped (keymapp check), NOT flattened.
  (let ((a (make-sparse-keymap)))
    (define-key a "z" 'az)
    ;; emacs -Q: (keymap (keymap (122 . az)) keymap)
    (should (equal (make-composed-keymap a (make-sparse-keymap))
                   '(keymap (keymap (122 . az)) keymap))))
  ;; Lookup descends into each composed sub-keymap.
  (let* ((a (make-sparse-keymap)) (b (make-sparse-keymap))
         (c (make-composed-keymap (list a b))))
    (define-key b "x" 'bx)
    (should (eq 'bx (lookup-key c "x")))))

;; ---- key-parse: strings -> internal event vectors ----
(ert-deftest keymap-key-parse ()
  ;; All verified against emacs -Q -- key-parse always returns a vector.
  (should (equal (key-parse "a") [97]))
  (should (equal (key-parse "C-x C-f") [24 6]))
  (should (equal (key-parse "M-a") [134217825]))
  (should (equal (key-parse "M-<left>") [M-left]))
  (should (equal (key-parse "<f5>") [f5]))
  (should (equal (key-parse "S-SPC") [33554464]))
  (should (equal (key-parse "<header-line> <mouse-1>") [header-line mouse-1]))
  (should (equal (key-parse "") [])))

;; ---- key-valid-p ----
(ert-deftest keymap-key-valid-p ()
  (should (eq t (and (key-valid-p "a") t)))
  (should (eq t (and (key-valid-p "C-c o") t)))
  (should (eq t (and (key-valid-p "<f6>") t)))
  (should (eq t (and (key-valid-p "RET") t)))
  ;; Empty and raw control chars are invalid.
  (should (eq nil (key-valid-p "")))
  (should (eq nil (key-valid-p "\C-x"))))

;; ---- keymap-set / keymap-lookup (string API on key-parse) ----
(ert-deftest keymap-string-api ()
  (let ((m (make-sparse-keymap)))
    (keymap-set m "a" 'foo)
    (should (eq 'foo (keymap-lookup m "a")))
    (keymap-set m "C-c c" 'bar)
    ;; emacs -Q: (keymap (3 keymap (99 . bar)) (97 . foo))
    (should (equal m '(keymap (3 keymap (99 . bar)) (97 . foo))))
    (should (eq 'bar (keymap-lookup m "C-c c")))))

;; ---- define-keymap ----
(ert-deftest keymap-define-keymap ()
  ;; emacs -Q: (define-keymap "a" 'foo "b" 'bar) => (keymap (98 . bar) (97 . foo))
  (should (equal (define-keymap "a" 'foo "b" 'bar)
                 '(keymap (98 . bar) (97 . foo))))
  ;; :parent keyword chains the parent.
  (let ((p (make-sparse-keymap)))
    (define-key p "z" 'pz)
    (should (equal (define-keymap :parent p "a" 'foo)
                   '(keymap (97 . foo) keymap (122 . pz))))))

;; ---- defvar-keymap: variable + structure (incl. :parent composed chain) ----
(ert-deftest keymap-defvar-keymap ()
  (defvar-keymap km-dv "a" 'foo "C-c c" 'bar)
  (should (eq t (boundp 'km-dv)))
  ;; emacs -Q: (keymap (3 keymap (99 . bar)) (97 . foo))
  (should (equal km-dv '(keymap (3 keymap (99 . bar)) (97 . foo))))
  ;; :doc and :parent, mirroring tabulated-list-mode-map's shape.
  (defvar-keymap km-dv2 :doc "d" :parent special-mode-map "n" 'next-line)
  (should (eq 'next-line (lookup-key km-dv2 "n")))
  ;; Parent (special-mode-map) is reachable through the composed tail.
  (should (eq 'quit-window (lookup-key km-dv2 "q"))))

;; ---- define-prefix-command / suppress-keymap ----
(ert-deftest keymap-prefix-and-suppress ()
  ;; define-prefix-command sets both the function and value cells to one keymap.
  (should (eq 'km-pfx (define-prefix-command 'km-pfx)))
  (should (equal (symbol-function 'km-pfx) '(keymap)))
  (should (eq (symbol-value 'km-pfx) (symbol-function 'km-pfx)))
  ;; suppress-keymap remaps self-insert-command and binds digits/minus.
  (let ((m (make-sparse-keymap)))
    (suppress-keymap m)
    (should (eq 'undefined (lookup-key m [remap self-insert-command])))
    (should (eq 'digit-argument (lookup-key m "5")))
    (should (eq 'negative-argument (lookup-key m "-")))))

;; ---- the goal: button-buffer-map / special-mode-map preloaded, byte-identical ----
(ert-deftest keymap-preloaded-parents ()
  ;; emacs -Q: (require 'button) then button-buffer-map.
  (should (equal button-buffer-map
                 '(keymap (backtab . backward-button)
                          (27 keymap (9 . backward-button))
                          (9 . forward-button))))
  ;; emacs -Q: special-mode-map (built via defvar-keymap :suppress t).
  (should (eq 'quit-window (lookup-key special-mode-map "q")))
  (should (eq 'scroll-up-command (lookup-key special-mode-map " ")))
  (should (eq 'undefined (lookup-key special-mode-map [remap self-insert-command])))
  ;; Reconstruct tabulated-list-mode-map's parent expression and pin its shape:
  ;; (make-composed-keymap button-buffer-map special-mode-map) nests button-buffer-map.
  (let ((composed (make-composed-keymap button-buffer-map special-mode-map)))
    ;; forward-button reachable through the nested button-buffer-map,
    ;; revert-buffer through the special-mode-map tail.
    (should (eq 'forward-button (lookup-key composed "\t")))
    (should (eq 'revert-buffer (lookup-key composed "g")))))

(ert-run-tests-batch-and-exit)
