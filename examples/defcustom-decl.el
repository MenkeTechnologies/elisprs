;;; defcustom-decl.el --- Customize declaration machinery, self-checked  -*- lexical-binding: nil; -*-
;; Run me:  elisp examples/defcustom-decl.el
;;
;; Regression gate for the faithful port of custom.el's DECLARATION half:
;; `defgroup', `defcustom', `defface' and the `custom-declare-*' functions they
;; expand into. Only the observable symbol properties Emacs stores at
;; declaration time are in scope (standard-value, custom-type, custom-requests,
;; custom-group, group-documentation, face-defface-spec, ...); the Customize UI
;; (widgets, `custom-set-variables' persistence) and live face objects are not.
;;
;; Every expected value below was captured from the real Emacs 30.2 binary with
;; `lexical-binding' nil (matching elisprs's dynamic-binding milestone), e.g.
;;   emacs -Q --batch --eval '(setq lexical-binding nil)' \
;;     --eval "(eval '(defcustom my-x 5 \"doc\" :type 'integer) nil)" \
;;     --eval "(prin1 (get 'my-x 'standard-value))"   =>  (5)
;;
;; ERT batch runner errors (→ non-zero exit) on any failure, so this doubles as
;; the CI self-test.
(message "== defcustom / defgroup / defface declaration demo ==")

(ert-deftest defgroup-stores-group-metadata ()
  "`defgroup' calls `custom-declare-group', storing `group-documentation'.
Emacs: (get 'dcd-g 'group-documentation) => \"Test group doc.\"."
  (defgroup dcd-g nil "Test group doc." :group 'emacs)
  (should (equal (get 'dcd-g 'group-documentation) "Test group doc."))
  ;; The group's own `custom-group' member list starts empty.
  (should (equal (get 'dcd-g 'custom-group) nil)))

(ert-deftest defcustom-defines-and-records-standard ()
  "`defcustom' binds the var (like `defvar') and records the custom metadata.
Emacs: dcd-x => 5, (get 'dcd-x 'standard-value) => (5),
       (get 'dcd-x 'custom-type) => integer,
       (special-variable-p 'dcd-x) => t,
       (get 'dcd-x 'variable-documentation) => \"An integer option.\"."
  (defgroup dcd-g2 nil "G2." :group 'emacs)
  (defcustom dcd-x 5 "An integer option." :type 'integer :group 'dcd-g2)
  (should (= dcd-x 5))
  (should (equal (get 'dcd-x 'standard-value) '(5)))
  (should (eq (get 'dcd-x 'custom-type) 'integer))
  (should (eq (special-variable-p 'dcd-x) t))
  (should (equal (get 'dcd-x 'variable-documentation) "An integer option."))
  ;; custom-requests defaults to nil; :group is recorded on the GROUP, not the
  ;; option (the option's own `custom-group' prop stays nil).
  (should (equal (get 'dcd-x 'custom-requests) nil))
  (should (equal (get 'dcd-x 'custom-group) nil))
  ;; The option is added to its group's member list as a `custom-variable'.
  (should (equal (get 'dcd-g2 'custom-group) '((dcd-x custom-variable)))))

(ert-deftest defcustom-respects-existing-binding ()
  "Like `defvar', `defcustom' does not clobber an already-bound value, but it
still records the STANDARD expression under `standard-value'.
Emacs: after (setq dcd-p 99) then (defcustom dcd-p 5 ...),
       dcd-p => 99  and  (get 'dcd-p 'standard-value) => (5)."
  (setq dcd-p 99)
  (defcustom dcd-p 5 "Pre-bound option." :type 'integer)
  (should (= dcd-p 99))
  (should (equal (get 'dcd-p 'standard-value) '(5))))

(ert-deftest defcustom-keyword-args ()
  "`:require', `:set' and `:options' land where Emacs puts them.
Emacs: (get 'dcd-r 'custom-requests) => (somefeat);
       :set runs at init so (10 doubled) dcd-s => 20 and custom-set is a fn;
       (get 'dcd-o 'custom-options) => (a b c)."
  (defcustom dcd-r 1 "d" :type 'integer :require 'somefeat)
  (should (equal (get 'dcd-r 'custom-requests) '(somefeat)))
  (defcustom dcd-s 10 "d" :type 'integer
    :set (lambda (sym v) (set-default sym (* v 2))))
  (should (= dcd-s 20))
  (should (functionp (get 'dcd-s 'custom-set)))
  (defcustom dcd-o nil "d" :type 'hook :options '(a b c))
  (should (equal (get 'dcd-o 'custom-options) '(a b c))))

(ert-deftest defcustom-keyword-doc-string-error ()
  "A keyword where the doc string belongs is an error (custom.el:172).
Emacs: (defcustom bad 1 :type 'integer) => error \"Doc string is missing\"."
  (should-error
   (eval '(defcustom dcd-bad 1 :type 'integer) nil)))

(ert-deftest defface-stores-defface-spec ()
  "`defface' calls `custom-declare-face', storing `face-defface-spec' and
`face-documentation', and adding the face to its group as a `custom-face'.
Emacs: (get 'dcd-face 'face-defface-spec) => ((t :foreground \"red\"));
       (get 'dcd-face 'face-documentation) => \"A red face.\"."
  (defgroup dcd-g3 nil "G3." :group 'emacs)
  (defface dcd-face '((t :foreground "red")) "A red face." :group 'dcd-g3)
  (should (equal (get 'dcd-face 'face-defface-spec) '((t :foreground "red"))))
  (should (equal (get 'dcd-face 'face-documentation) "A red face."))
  (should (equal (get 'dcd-g3 'custom-group) '((dcd-face custom-face))))
  ;; Re-declaring a face that already has a spec is a no-op (custom.el guard):
  ;; the original spec is kept, not overwritten.
  (defface dcd-face '((t :foreground "blue")) "Changed." :group 'dcd-g3)
  (should (equal (get 'dcd-face 'face-defface-spec) '((t :foreground "red")))))

(ert-deftest custom-declare-variable-direct ()
  "`custom-declare-variable' evaluates SYMBOL and DEFAULT as normal args and is
what `defcustom' expands into.
Emacs: after (custom-declare-variable 'dcd-d (+ 2 3) \"doc\" :type 'integer),
       dcd-d => 5 and (get 'dcd-d 'standard-value) => (5)."
  (custom-declare-variable 'dcd-d (+ 2 3) "doc" :type 'integer)
  (should (= dcd-d 5))
  (should (equal (get 'dcd-d 'standard-value) '(5)))
  (should (eq (get 'dcd-d 'custom-type) 'integer)))

(ert-run-tests-batch-and-exit)
;;; defcustom-decl.el ends here
