;;; language-info-alist.el --- language environment registry, ERT-tested  -*- lexical-binding: nil; -*-

;; `set-language-info-alist' (international/mule-cmds.el) is how every
;; `language/*.el' file declares a language environment: it stores an alist of
;; (KEY . INFO) pairs under LANGUAGE-NAME in the global `language-info-alist',
;; and registers menu entries in the Describe/Set Language Environment maps.
;; `get-language-info' / `set-language-info' are the accessors.
;;
;; The port covers the load-time data surface exercised when init/language files
;; declare environments.  `set-language-info-internal' prepends new language and
;; key slots (so `get-language-info' sees the most recent INFO), replaces INFO on
;; a repeated KEY, and refreshes the `current-language-environment' custom-type.
;; The heavy `set-language-environment*' switch functions only fire when a
;; definition targets the CURRENT environment and are out of scope here.
;;
;; Supporting keymap helpers `bindings--define-key' (bindings.el) and
;; `define-key-after' (subr.el) are ported verbatim; the parents branch of
;; `set-language-info-alist' uses them to build submenus.
;;
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
;; Run through fusevm; `ert-run-tests-batch-and-exit' gates the suite.
(message "== language-info-alist ==")

;; ---- registration + retrieval ----
(ert-deftest lia-register-and-get ()
  (set-language-info-alist "Testlang" '((sample-text . "hola")
                                        (documentation . "doc here")
                                        (charset foo bar)))
  ;; The full slot: keys are prepended, so declaration order reverses.
  (should (equal (assoc-string "Testlang" language-info-alist t)
                 '("Testlang" (charset foo bar)
                              (documentation . "doc here")
                              (sample-text . "hola"))))
  ;; get-language-info accepts a string or a symbol LANG-ENV.
  (should (equal (get-language-info "Testlang" 'sample-text) "hola"))
  (should (equal (get-language-info 'Testlang 'documentation) "doc here"))
  (should (equal (get-language-info "Testlang" 'charset) '(foo bar)))
  ;; A missing KEY returns nil (assq on the key alist).
  (should (eq (get-language-info "Testlang" 'no-such-key) nil))
  ;; A missing LANG-ENV returns nil (assoc-string finds no slot).
  (should (eq (get-language-info "Nonexistent" 'sample-text) nil)))

;; ---- set-language-info adds/replaces a single KEY ----
(ert-deftest lia-set-language-info ()
  (set-language-info-alist "Setl" '((sample-text . "hola")))
  ;; A brand-new key is added.
  (set-language-info "Setl" 'input-method "foo-im")
  (should (equal (get-language-info "Setl" 'input-method) "foo-im"))
  ;; Re-setting an existing key replaces its INFO in place.
  (set-language-info "Setl" 'input-method "bar-im")
  (should (equal (get-language-info "Setl" 'input-method) "bar-im"))
  ;; The unrelated key is untouched.
  (should (equal (get-language-info "Setl" 'sample-text) "hola")))

;; ---- a second alist declaration overwrites keys, keeps others ----
(ert-deftest lia-redeclare-merges ()
  (set-language-info-alist "Redecl" '((sample-text . "hola")
                                      (documentation . "doc here")))
  ;; Redeclaring with a subset only overwrites the keys it names; because
  ;; set-language-info-internal prepends, the newer sample-text wins and the
  ;; old documentation is still reachable via assq (first match).
  (set-language-info-alist "Redecl" '((sample-text . "bonjour")))
  (should (equal (get-language-info "Redecl" 'sample-text) "bonjour"))
  (should (equal (get-language-info "Redecl" 'documentation) "doc here")))

;; ---- custom-type of current-language-environment tracks the alist ----
(ert-deftest lia-custom-type-sorted ()
  ;; Fresh interpreter: only the environments declared in this file exist.
  ;; set-language-info-internal rebuilds the custom-type as a sorted choice.
  (set-language-info-alist "Zulu" '((documentation . "z")))
  (set-language-info-alist "Alpha" '((documentation . "a")))
  (let ((ct (get 'current-language-environment 'custom-type)))
    (should (eq (car ct) 'choice))
    ;; Alpha sorts before Zulu (string<).
    (should (member '(const "Alpha") ct))
    (should (member '(const "Zulu") ct))
    ;; Sorted: Alpha's position precedes Zulu's.
    (let ((names (mapcar #'cadr (cdr ct))))
      (should (< (seq-position names "Alpha") (seq-position names "Zulu"))))))

;; ---- parents branch builds submenus via define-key-after ----
(ert-deftest lia-parents-submenu ()
  ;; Declaring under a parent creates a `setup-PARENT-environment-map' prefix
  ;; command and stores the environment inside it.  Assertions read the raw
  ;; menu structure the ported code builds (via assq on the keymap alist), which
  ;; is state-independent and holds against the oracle's preloaded maps too.
  (set-language-info-alist "Basquelang" '((documentation . "Basque"))
                           '("European"))
  ;; The top-level setup map gained a European menu entry whose binding is the
  ;; prefix-command symbol: (European "European" . setup-european-environment-map).
  (let ((entry (assq 'European (cdr setup-language-environment-map))))
    (should (equal (cddr entry) 'setup-european-environment-map))
    (should (keymapp (symbol-value 'setup-european-environment-map)))
    ;; Basquelang is registered inside that submenu's keymap.
    (should (equal (assq 'Basquelang (symbol-value 'setup-european-environment-map))
                   '(Basquelang "Basquelang" . setup-specified-language-environment))))
  ;; Same structure in the describe map (documentation key present -> entry).
  (let ((entry (assq 'European (cdr describe-language-environment-map))))
    (should (equal (cddr entry) 'describe-european-environment-map))
    (should (equal (assq 'Basquelang (symbol-value 'describe-european-environment-map))
                   '(Basquelang "Basquelang" . describe-specified-language-support)))))

;; ---- define-key-after appends after an existing binding ----
(ert-deftest lia-define-key-after ()
  (let ((m (make-sparse-keymap "X")))
    (define-key m [Default] '(menu-item "D" foo))
    (define-key-after m [Other] '(menu-item "O" bar))
    ;; New binding lands after the existing one and the prompt string.
    (should (equal m '(keymap (Default menu-item "D" foo)
                              "X"
                              (Other menu-item "O" bar))))))

(ert-run-tests-batch-and-exit)
