;;; coding-systems.el --- coding-system registry predicates, ERT-tested  -*- lexical-binding: nil; -*-

;; The predicate/registry surface of Emacs's coding-system subsystem (coding.c
;; documented behavior), ported faithfully to GNU Emacs 30.2:
;;
;;   coding-system-p       -- nil, base, alias, or EOL variant of any of those.
;;   coding-system-base    -- resolves alias + EOL variant to the base system.
;;   coding-system-list    -- all non-subsidiary systems; base-only with arg.
;;   check-coding-system   -- returns the system or signals `coding-system-error'.
;;
;; Only registration/prediction is ported here; the actual encode/decode
;; machinery (define-coding-system-internal + charset codecs) is not.
;;
;; Every asserted value was verified against `emacs -Q --batch' on Emacs 30.2.
;; Structural assertions (memq/length/base-resolution) are preferred over the
;; full 269-element list literal, which drifts across Emacs versions.
;; Run through fusevm; `ert-run-tests-batch-and-exit' gates the suite.
(message "== coding-system registry predicates ==")

;; ---- coding-system-p: nil, real system, bogus, non-symbol ----
(ert-deftest csp-basic ()
  ;; emacs -Q: (coding-system-p 'utf-8) => t, 'nonesuch => nil, nil => t, 0 => nil
  (should (eq (coding-system-p 'utf-8) t))
  (should (eq (coding-system-p 'nonesuch) nil))
  (should (eq (coding-system-p nil) t))
  (should (eq (coding-system-p 0) nil))
  ;; Non-symbols never match (strings included).
  (should (eq (coding-system-p t) nil))
  (should (eq (coding-system-p "utf-8") nil)))

;; ---- coding-system-p: EOL subsidiaries and aliases ----
(ert-deftest csp-eol-and-alias ()
  ;; Each base system has -unix/-dos/-mac subsidiaries that also satisfy the
  ;; predicate; emacs -Q: both => t.
  (should (eq (coding-system-p 'utf-8-unix) t))
  (should (eq (coding-system-p 'utf-8-dos) t))
  ;; Aliases satisfy it too, as do EOL subsidiaries of aliases.
  (should (eq (coding-system-p 'latin-1) t))
  (should (eq (coding-system-p 'iso-8859-1) t))
  (should (eq (coding-system-p 'iso-8859-1-unix) t))
  ;; A bogus base with an EOL suffix is still bogus; only ONE suffix is
  ;; stripped, and matching is case-sensitive.  emacs -Q: all => nil.
  (should (eq (coding-system-p 'nope-unix) nil))
  (should (eq (coding-system-p 'utf-8-unix-unix) nil))
  (should (eq (coding-system-p 'UTF-8) nil)))

;; ---- coding-system-base: alias + EOL resolution ----
(ert-deftest csbase-resolve ()
  ;; emacs -Q verified value-for-value.
  (should (eq (coding-system-base 'utf-8) 'utf-8))
  (should (eq (coding-system-base 'utf-8-unix) 'utf-8))
  (should (eq (coding-system-base 'latin-1) 'iso-latin-1))
  (should (eq (coding-system-base 'iso-8859-1-unix) 'iso-latin-1))
  ;; nil resolves to no-conversion (Emacs's default binary base).
  (should (eq (coding-system-base nil) 'no-conversion))
  ;; The base of a base is itself for every listed base system.
  (dolist (cs (coding-system-list t))
    (should (eq (coding-system-base cs) cs)))
  ;; An invalid system signals, exactly like check-coding-system.
  (should-error (coding-system-base 'nope) :type 'coding-system-error))

;; ---- coding-system-list: full vs base-only, membership ----
(ert-deftest cslist-structure ()
  (let ((full (coding-system-list))
        (base (coding-system-list t)))
    ;; Every base system is a member of the full list.
    (dolist (cs base) (should (memq cs full)))
    ;; The base-only list is a strict subset (aliases dropped).
    (should (< (length base) (length full)))
    ;; Canonical members present; the alias `binary' is in the full list only.
    (should (memq 'utf-8 base))
    (should (memq 'no-conversion base))
    (should (memq 'binary full))
    (should (not (memq 'binary base)))
    ;; EOL subsidiaries are NOT listed (they are subsidiary, not primary).
    (should (not (memq 'utf-8-unix full)))
    ;; The list is a fresh copy; nreverse of it must not corrupt the registry.
    (nreverse (coding-system-list))
    (should (memq 'utf-8 (coding-system-list)))))

;; ---- check-coding-system: pass-through vs signal ----
(ert-deftest cscheck ()
  (should (eq (check-coding-system 'utf-8) 'utf-8))
  ;; nil is a valid coding system and checks through as itself.
  (should (eq (check-coding-system nil) nil))
  (should-error (check-coding-system 'nope) :type 'coding-system-error)
  ;; The error datum carries the offending system, like Emacs's Fcheck_coding_system.
  (should (equal (condition-case e (check-coding-system 'nope) (error e))
                 '(coding-system-error nope))))

;; ---- load-time usage pattern from gnus/mm-util.el ----
(ert-deftest cs-load-time-branch ()
  ;; mm-util defines mm-coding-system-p as (and (coding-system-p cs) cs) and
  ;; uses it at top level to decide which alias forms to install.  Exercise the
  ;; exact shape so a real init file's macro expansion resolves.
  (let ((mm-cs (lambda (cs) (and (coding-system-p cs) cs))))
    (should (eq (funcall mm-cs 'utf-8) 'utf-8))
    (should (eq (funcall mm-cs 'no-such-charset) nil))
    ;; utf-16-le is a real alias in 30.2, so the fallback branch is skipped.
    (should (funcall mm-cs 'utf-16-le))))

;; ---- coding-system-eol-type: fixed integer vs subsidiary vector ----
(ert-deftest cs-eol-type ()
  ;; Fixed-EOL systems return the integer 0 (unix), never a vector.
  (should (eq (coding-system-eol-type 'no-conversion) 0))
  (should (eq (coding-system-eol-type 'no-conversion-multibyte) 0))
  (should (eq (coding-system-eol-type 'binary) 0))
  ;; nil is treated as `no-conversion'.
  (should (eq (coding-system-eol-type nil) 0))
  ;; The three EOL subsidiaries map to 0/1/2 by suffix.
  (should (eq (coding-system-eol-type 'utf-8-unix) 0))
  (should (eq (coding-system-eol-type 'utf-8-dos) 1))
  (should (eq (coding-system-eol-type 'utf-8-mac) 2))
  ;; An EOL-undecided base returns the [base-unix base-dos base-mac] vector.
  (should (equal (coding-system-eol-type 'utf-8)
                 [utf-8-unix utf-8-dos utf-8-mac]))
  (should (equal (coding-system-eol-type 'raw-text)
                 [raw-text-unix raw-text-dos raw-text-mac]))
  (should (equal (coding-system-eol-type 'undecided)
                 [undecided-unix undecided-dos undecided-mac]))
  ;; For an alias the vector is built from the resolved BASE, not the alias
  ;; name: `latin-1' -> `iso-latin-1', so the vector is iso-latin-1-*.
  (should (equal (coding-system-eol-type 'latin-1)
                 [iso-latin-1-unix iso-latin-1-dos iso-latin-1-mac]))
  ;; A subsidiary of a fixed-EOL system does not exist -> nil.
  (should (eq (coding-system-eol-type 'no-conversion-dos) nil))
  ;; A non-coding-system returns nil, not an error.
  (should (eq (coding-system-eol-type 'no-such-charset) nil)))

;; ---- coding-system-eol-type: structural over the whole registry ----
(ert-deftest cs-eol-type-registry ()
  ;; Every base (non-alias, non-fixed) system yields the derived vector, and
  ;; its three subsidiaries invert back to 0/1/2.  Derive over the registry so
  ;; the assertion tracks the real table instead of drifting literals.
  (dolist (cs (coding-system-list t))
    (let ((eol (coding-system-eol-type cs)))
      (if (integerp eol)
          ;; Every fixed-EOL base system in 30.2 is `unix' (0).
          (should (eq eol 0))
        (let ((name (symbol-name cs)))
          (should (equal eol (vector (intern (concat name "-unix"))
                                     (intern (concat name "-dos"))
                                     (intern (concat name "-mac")))))
          (should (eq (coding-system-eol-type (aref eol 0)) 0))
          (should (eq (coding-system-eol-type (aref eol 1)) 1))
          (should (eq (coding-system-eol-type (aref eol 2)) 2)))))))

;; ---- coding-system-change-eol-conversion: symbol and integer EOL args ----
(ert-deftest cs-change-eol ()
  ;; Symbol EOL types select the matching subsidiary of an undecided system.
  (should (eq (coding-system-change-eol-conversion 'utf-8 'unix) 'utf-8-unix))
  (should (eq (coding-system-change-eol-conversion 'utf-8 'dos) 'utf-8-dos))
  (should (eq (coding-system-change-eol-conversion 'utf-8 'mac) 'utf-8-mac))
  ;; nil EOL returns the base (undecided) system.
  (should (eq (coding-system-change-eol-conversion 'utf-8 nil) 'utf-8))
  ;; Integer EOL types 0/1/2 are accepted just like the symbols.
  (should (eq (coding-system-change-eol-conversion 'utf-8 0) 'utf-8-unix))
  (should (eq (coding-system-change-eol-conversion 'utf-8 1) 'utf-8-dos))
  (should (eq (coding-system-change-eol-conversion 'utf-8 2) 'utf-8-mac))
  ;; Starting from a subsidiary, changing EOL crosses to the sibling.
  (should (eq (coding-system-change-eol-conversion 'utf-8-dos 'unix) 'utf-8-unix))
  (should (eq (coding-system-change-eol-conversion 'utf-8-unix 'mac) 'utf-8-mac))
  ;; Same EOL as the current one returns the coding system unchanged.
  (should (eq (coding-system-change-eol-conversion 'utf-8-unix 'unix) 'utf-8-unix))
  ;; nil EOL on a subsidiary returns the undecided base.
  (should (eq (coding-system-change-eol-conversion 'utf-8-unix nil) 'utf-8))
  ;; Aliases resolve through the base: latin-1 -> iso-latin-1-mac.
  (should (eq (coding-system-change-eol-conversion 'latin-1 'mac) 'iso-latin-1-mac))
  ;; A fixed-EOL system: unix keeps it, but dos/mac have no variant -> nil.
  (should (eq (coding-system-change-eol-conversion 'no-conversion 'unix) 'no-conversion))
  (should (eq (coding-system-change-eol-conversion 'no-conversion 'dos) nil))
  (should (eq (coding-system-change-eol-conversion 'no-conversion 'mac) nil))
  ;; `binary' is an alias of no-conversion: unix keeps binary, nil -> base.
  (should (eq (coding-system-change-eol-conversion 'binary 'unix) 'binary))
  (should (eq (coding-system-change-eol-conversion 'binary nil) 'no-conversion)))

;; ---- change-eol-conversion: structural round-trip over the registry ----
(ert-deftest cs-change-eol-registry ()
  ;; For every undecided base system, change-eol to dos must equal the second
  ;; slot of its eol-type vector; changing back to nil returns the base.
  (dolist (cs (coding-system-list t))
    (let ((eol (coding-system-eol-type cs)))
      (when (vectorp eol)
        (should (eq (coding-system-change-eol-conversion cs 'unix) (aref eol 0)))
        (should (eq (coding-system-change-eol-conversion cs 'dos) (aref eol 1)))
        (should (eq (coding-system-change-eol-conversion cs 'mac) (aref eol 2)))
        (should (eq (coding-system-change-eol-conversion cs nil)
                    (coding-system-base cs)))))))

(ert-run-tests-batch-and-exit)
