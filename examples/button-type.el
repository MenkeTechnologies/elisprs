;;; button-type.el --- button.el button-type registry  -*- lexical-binding: nil; -*-

;; Pins the button-type system faithfully ported from button.el: `define-button-type'
;; and the `button-type-{put,get,subtype-p}' accessors, plus the `button-category-symbol'
;; indirection and the `default-button' default plist.  A button type stores its default
;; properties on a separate uninterned `NAME-button' symbol so `category' properties can
;; point at it without name clashes.  Overlay/text-property button placement is not modeled;
;; only the type registry, which is what init files (ansi-osc, apropos, backtrace, ...)
;; exercise at load time.  Every asserted value was verified against `emacs -Q --batch' on
;; Emacs 30.2.  Run through fusevm; `ert-run-tests-batch-and-exit' gates the suite.

(require 'button)

(message "== button-type demo ==")

;; ---- define-button-type return value + default inheritance (button.el:121) ----
(ert-deftest button-type-define-and-defaults ()
  ;; emacs -Q: (define-button-type 'bt-base ...) => bt-base  (returns NAME)
  (should (eq 'bt-base
              (define-button-type 'bt-base
                'action (lambda (b) 'clicked)
                'help-echo "hi")))
  ;; The `type' property is set to NAME on the category symbol.
  (should (eq 'bt-base (button-type-get 'bt-base 'type)))
  ;; Global defaults from `default-button' are inherited: evaporate=t, mouse-face=highlight.
  (should (eq t (button-type-get 'bt-base 'evaporate)))
  (should (eq 'highlight (button-type-get 'bt-base 'mouse-face)))
  ;; Explicitly supplied properties are stored verbatim.
  (should (equal "hi" (button-type-get 'bt-base 'help-echo)))
  ;; With no :supertype, `supertype' defaults to `button'.
  (should (eq 'button (button-type-get 'bt-base 'supertype)))
  ;; A type with no properties still gets supertype `button'.
  (should (eq 'bt-r (define-button-type 'bt-r)))
  (should (eq 'button (button-type-get 'bt-r 'supertype))))

;; ---- button-category-symbol indirection (button.el:115) ----
(ert-deftest button-type-category-symbol ()
  (define-button-type 'bt-cat)
  ;; The category symbol is named "NAME-button" ...
  (should (equal "bt-cat-button" (symbol-name (button-category-symbol 'bt-cat))))
  ;; ... and is UNINTERNED (make-symbol, not intern): intern-soft finds nothing.
  (should (eq nil (intern-soft (symbol-name (button-category-symbol 'bt-cat)))))
  ;; The built-in `button' type points at `default-button'.
  (should (eq 'default-button (get 'button 'button-category-symbol)))
  ;; An unknown type signals an error rather than returning nil.
  (should (eq 'error (condition-case e (button-category-symbol 'bt-nope-xyz)
                       (error (car e))))))

;; ---- :supertype inheritance is a one-time snapshot (button.el:134) ----
(ert-deftest button-type-supertype-inheritance ()
  (define-button-type 'bt-parent 'help-echo "hi" 'face 'parent-face)
  (define-button-type 'bt-child :supertype 'bt-parent 'face 'child-face)
  ;; Child records its supertype (the :supertype keyword is rewritten to `supertype').
  (should (eq 'bt-parent (button-type-get 'bt-child 'supertype)))
  ;; Child overrides an inherited property ...
  (should (eq 'child-face (button-type-get 'bt-child 'face)))
  ;; ... but inherits the ones it does not set.
  (should (equal "hi" (button-type-get 'bt-child 'help-echo)))
  ;; Its own `type' is the child NAME, not the parent's.
  (should (eq 'bt-child (button-type-get 'bt-child 'type))))

;; ---- button-type-subtype-p walks the supertype chain (button.el:168) ----
(ert-deftest button-type-subtype-p-relation ()
  (define-button-type 'bt-sp-parent)
  (define-button-type 'bt-sp-child :supertype 'bt-sp-parent)
  ;; Direct subtype.
  (should (eq t (button-type-subtype-p 'bt-sp-child 'bt-sp-parent)))
  ;; Every type is ultimately a subtype of `button'.
  (should (eq t (button-type-subtype-p 'bt-sp-child 'button)))
  ;; A type is a subtype of itself.
  (should (eq t (button-type-subtype-p 'bt-sp-parent 'bt-sp-parent)))
  ;; The relation is not symmetric: parent is NOT a subtype of child.
  (should (eq nil (button-type-subtype-p 'bt-sp-parent 'bt-sp-child))))

;; ---- button-type-put mutates the category symbol's plist (button.el:160) ----
(ert-deftest button-type-put-accessor ()
  (define-button-type 'bt-put)
  (should (eq nil (button-type-get 'bt-put 'my-prop)))
  (button-type-put 'bt-put 'my-prop 99)
  (should (eql 99 (button-type-get 'bt-put 'my-prop))))

(ert-run-tests-batch-and-exit)
