;;; define-widget.el --- widget.el widget-type registry  -*- lexical-binding: nil; -*-

;; Pins the widget-type registry faithfully ported from widget.el: `define-widget'
;; stores (CLASS . ARGS) on NAME's `widget-type' property and the doc string on
;; `widget-documentation', returning NAME.  This is the bootstrap half of the
;; widget system (the wid-edit.el widget UI -- widget-create/convert/apply -- is
;; not modeled).  The registry is what init files exercise at load time.  Also
;; covers the `define-widget-keywords' dummy macro (obsolete no-op) and the
;; `widget-plist-member' -> `plist-member' obsolete alias.  Every asserted value
;; was verified against `emacs -Q --batch' on Emacs 30.2.  Run through fusevm;
;; `ert-run-tests-batch-and-exit' gates the suite.

(require 'widget)

(message "== define-widget demo ==")

;; ---- define-widget return value + property storage (widget.el:72) ----
(ert-deftest define-widget-returns-name-and-stores-props ()
  ;; emacs -Q: (define-widget 'dw-int 'integer "an int" :size 5) => dw-int
  (should (eq 'dw-int
              (define-widget 'dw-int 'integer "an int" :size 5)))
  ;; `widget-type' holds (CLASS . ARGS) with the extra args as the tail plist.
  (should (equal '(integer :size 5) (get 'dw-int 'widget-type)))
  ;; The doc string is stored on `widget-documentation'.
  (should (equal "an int" (get 'dw-int 'widget-documentation)))
  ;; A widget derived from another widget keeps CLASS as the car.
  (should (eq 'dw-link
              (define-widget 'dw-link 'item "A link." :format "%[%t%]")))
  (should (equal '(item :format "%[%t%]") (get 'dw-link 'widget-type))))

;; ---- nil class / nil doc / no extra args (widget.el:88-90) ----
(ert-deftest define-widget-nil-class-and-doc ()
  ;; CLASS may be nil (widget from scratch); doc may be nil; args may be empty.
  (should (eq 'dw-scratch (define-widget 'dw-scratch nil nil)))
  ;; With no ARGS the `widget-type' cdr is nil, so the whole cell is (nil).
  (should (equal '(nil) (get 'dw-scratch 'widget-type)))
  ;; A nil doc is stored as nil (purecopy of nil is nil).
  (should (eq nil (get 'dw-scratch 'widget-documentation))))

;; ---- non-string, non-nil doc signals an error (widget.el:86-87) ----
(ert-deftest define-widget-bad-doc-errors ()
  ;; emacs -Q: (define-widget 'dw-bad 'x 42) =>
  ;;   error "Widget documentation must be nil or a string"
  (should-error (define-widget 'dw-bad 'integer 42) :type 'error)
  (should (equal "Widget documentation must be nil or a string"
                 (cadr (should-error (define-widget 'dw-bad2 'integer 42)))))
  ;; A failed definition must not have registered the type.
  (should (eq nil (get 'dw-bad 'widget-type))))

;; ---- define-widget-keywords dummy no-op (widget.el:38) ----
(ert-deftest define-widget-keywords-is-noop ()
  ;; The obsolete macro expands to nil regardless of its keyword args.
  (should (eq nil (define-widget-keywords :complete :button-face))))

;; ---- widget-plist-member obsolete alias -> plist-member (widget.el:137) ----
(ert-deftest widget-plist-member-alias ()
  ;; The alias returns the tail of the plist starting at the found key.
  (should (equal '(:b 2 :c 3) (widget-plist-member '(:a 1 :b 2 :c 3) :b)))
  ;; A missing key returns nil, matching `plist-member'.
  (should (eq nil (widget-plist-member '(:a 1) :z))))

(ert-run-tests-batch-and-exit)
