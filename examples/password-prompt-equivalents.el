;;; password-prompt-equivalents.el --- password-prompt recognition data, ERT-tested  -*- lexical-binding: nil; -*-

;; `password-word-equivalents' and `password-colon-equivalents' are defined by
;; international/mule-conf.el, which Emacs preloads, so both are always bound.
;; comint.el/shell.el and friends read them to build
;; `comint-password-prompt-regexp' (recognizing "password:" prompts across
;; languages). Both are fixed, build-independent i18n data lists — ported
;; value-for-value into the elisprs prelude from GNU Emacs 30.2
;; mule-conf.el:1681/1739.
;;
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
(message "== password-prompt recognition data ==")

;; ---- password-word-equivalents: 49-element i18n word list ----
(ert-deftest pwe-word-list ()
  ;; emacs -Q: (length password-word-equivalents) => 49
  (should (= (length password-word-equivalents) 49))
  ;; The English seeds head the list in fixed order.
  (should (equal (nth 0 password-word-equivalents) "password"))
  (should (equal (nth 1 password-word-equivalents) "passcode"))
  (should (equal (nth 2 password-word-equivalents) "passphrase"))
  ;; The locale-sorted tail ends with the zh_TW entry.
  (should (equal (car (last password-word-equivalents)) "密碼"))
  ;; A representative non-English member is present.
  (should (member "mot de passe" password-word-equivalents))
  ;; Every element is a string.
  (should (seq-every-p #'stringp password-word-equivalents)))

;; ---- password-colon-equivalents: 5 colon codepoints ----
(ert-deftest pwe-colon-list ()
  ;; emacs -Q: password-colon-equivalents => (58 65306 65109 65043 6102)
  (should (equal password-colon-equivalents '(58 65306 65109 65043 6102)))
  (should (= (length password-colon-equivalents) 5))
  ;; The plain ASCII colon leads; the fullwidth colon is present.
  (should (= (car password-colon-equivalents) ?:))
  (should (memq ?： password-colon-equivalents))
  ;; Every element is a character (integer).
  (should (seq-every-p #'integerp password-colon-equivalents)))

;; ---- customizable options: registered under the `processes' group ----
(ert-deftest pwe-are-custom-options ()
  ;; defcustom records a `standard-value' on the symbol (custom.el behavior);
  ;; a plain defvar would not.
  (should (get 'password-word-equivalents 'standard-value))
  (should (get 'password-colon-equivalents 'standard-value)))

(ert-run-tests-batch-and-exit)
