//! `nil`, `t`, and keywords are constants: their value cells cannot be written.
//!
//! Emacs marks exactly three kinds of symbol constant (`make_symbol_constant`):
//! `nil`, `t`, and every keyword interned in the standard obarray. Every writer
//! funnels through `set_internal`, which signals `setting-constant` with the
//! rejected symbol as the error data. elisprs had none of this — it either
//! signalled a made-up condition naming the *builtin* instead of the symbol
//! (`(set "not a symbol")` where Emacs says `(setting-constant nil)`), or, for
//! keywords, silently performed the write and returned the value.
//!
//! The keyword half is a `intern_driver` (lread.c) port: interning a
//! `:`-prefixed name in the standard obarray seeds the symbol's value cell with
//! the symbol itself, declares it special, and makes it constant. That single
//! omission is why `(boundp :a)` answered nil and `(default-value :a)` signalled
//! `void-variable` — both are `t` / `:a` in Emacs for a keyword nothing has ever
//! mentioned.
//!
//! The obarray is part of the keyword test, not just the spelling. Emacs applies
//! the treatment only for `initial_obarray`, so `(make-symbol ":u")` and
//! `(intern ":u" (obarray-make))` are ordinary writable symbols that merely read
//! back as `:u`. elisprs's `keywordp` was a name-prefix check in the prelude and
//! answered `t` for both; it is a subr now, because only the host can tell which
//! object the obarray holds.
//!
//! Every expectation below was measured against `GNU Emacs 30.2` via
//! `emacs -Q --batch`.

use elisprs::{eval_str, print, reset_host};

/// Evaluate SRC, rendering a signalled error the way `condition-case` sees it,
/// so the error symbol and data are part of the comparison.
fn eval(src: &str) -> String {
    reset_host();
    let wrapped = format!("(condition-case e {src} (error e))");
    let v = eval_str(&wrapped).expect("eval failed");
    print(&v, true)
}

/// `set` / `setq` on each of the three constant kinds.
///
/// Emacs 30.2 signals `(setting-constant SYM)` — the symbol itself is the data,
/// not a message string.
#[test]
fn set_on_a_constant_signals_setting_constant() {
    assert_eq!(eval("(set nil 1)"), "(setting-constant nil)");
    assert_eq!(eval("(set t 1)"), "(setting-constant t)");
    assert_eq!(eval("(set :kw 1)"), "(setting-constant :kw)");
    // Reaching the same symbols through `intern` rather than as literals.
    assert_eq!(eval("(set (intern \"nil\") 1)"), "(setting-constant nil)");
    assert_eq!(eval("(set (intern \"t\") 1)"), "(setting-constant t)");
    // `:` is a keyword; there is no exception for the one-character name.
    assert_eq!(eval("(set (intern \":\") 1)"), "(setting-constant :)");
    // `setq` is the same write. This one silently returned 1 before.
    assert_eq!(eval("(setq :a 1)"), "(setting-constant :a)");
}

/// The other writers Emacs routes through `set_internal`.
#[test]
fn every_writer_rejects_a_constant() {
    assert_eq!(eval("(set-default nil 1)"), "(setting-constant nil)");
    assert_eq!(eval("(set-default :kw 1)"), "(setting-constant :kw)");
    assert_eq!(eval("(setq-default nil 5)"), "(setting-constant nil)");
    assert_eq!(eval("(makunbound t)"), "(setting-constant t)");
    assert_eq!(
        eval("(makunbound (intern \":q\"))"),
        "(setting-constant :q)"
    );
    assert_eq!(eval("(make-local-variable nil)"), "(setting-constant nil)");
    assert_eq!(
        eval("(make-variable-buffer-local :kw)"),
        "(setting-constant :kw)"
    );
    assert_eq!(eval("(setq-local nil 1)"), "(setting-constant nil)");
    // `defconst` always assigns, so it hits the constant; `defvar` only
    // initializes a void variable and a keyword is never void.
    assert_eq!(eval("(defconst :dc 1)"), "(setting-constant :dc)");
    assert_eq!(eval("(defvar :dv 1)"), ":dv");
}

/// A constant in `let` binding position is `setting-constant`, not a
/// malformed-binding error: `let` binds through the same write.
///
/// Both spellings reach it — the bare `(let (nil) …)` and `(let ((nil 1)) …)`.
#[test]
fn let_rejects_a_constant_binding() {
    assert_eq!(eval("(let ((nil 1)) nil)"), "(setting-constant nil)");
    assert_eq!(eval("(let ((t 1)) t)"), "(setting-constant t)");
    assert_eq!(eval("(let ((:kw 1)) :kw)"), "(setting-constant :kw)");
    assert_eq!(eval("(let* ((nil 1)) nil)"), "(setting-constant nil)");
    assert_eq!(eval("(let (nil) 1)"), "(setting-constant nil)");
    assert_eq!(eval("(let (t) 1)"), "(setting-constant t)");
    assert_eq!(eval("(let (:kw) 1)"), "(setting-constant :kw)");
}

/// The rejection happens at bind time, after every init has run.
///
/// This pins the *placement* of the check, not just its existence. A
/// compile-time rejection would answer the same error object but would skip the
/// initializer and — worse — escape the enclosing `condition-case`, since
/// nothing would ever start running. Emacs 30.2 answers
/// `((setting-constant nil) 1)` for both forms below: `x` is 1, so the init ran.
#[test]
fn the_inits_run_before_a_constant_binding_is_rejected() {
    assert_eq!(
        eval("(let ((x 0)) (list (condition-case e (let ((nil (setq x 1))) nil) (error e)) x))"),
        "((setting-constant nil) 1)"
    );
    assert_eq!(
        eval(
            "(let ((x 0)) \
               (list (condition-case e (let ((a (setq x 1)) (nil 2)) nil) (error e)) x))"
        ),
        "((setting-constant nil) 1)"
    );
    // `let*` binds as it goes, so the first binding is established and only the
    // second is rejected. Emacs 30.2: `((setting-constant nil) 1)`.
    assert_eq!(
        eval(
            "(let ((x 0)) \
               (list (condition-case e (let* ((a (setq x 1)) (nil 2)) nil) (error e)) x))"
        ),
        "((setting-constant nil) 1)"
    );
    // The failed `let` must not leave a scope open behind it: an ordinary
    // binding afterwards still resolves, and the outer `x` is still visible.
    assert_eq!(
        eval("(let ((x 7)) (ignore-errors (let ((nil 1)) nil)) (let ((y 2)) (+ x y)))"),
        "9"
    );
}

/// A keyword is its own value the moment it is interned.
///
/// `boundp` answered nil and `default-value` signalled `void-variable` before —
/// `intern` never seeded the cell, despite a docstring that claimed it did.
#[test]
fn a_keyword_is_bound_to_itself_without_being_mentioned() {
    assert_eq!(eval("(boundp :fresh)"), "t");
    assert_eq!(eval("(default-boundp :fresh)"), "t");
    assert_eq!(eval("(symbol-value :fresh)"), ":fresh");
    assert_eq!(eval("(default-value :fresh)"), ":fresh");
    assert_eq!(eval("(eq :fresh (symbol-value :fresh))"), "t");
    // Declared special, like every constant.
    assert_eq!(eval("(special-variable-p :kw)"), "t");
    assert_eq!(eval("(special-variable-p t)"), "t");
    assert_eq!(eval("(special-variable-p nil)"), "t");
}

/// `keywordp` tests obarray identity, not the leading colon.
///
/// A `:`-spelled symbol outside the standard obarray is an ordinary, writable,
/// unbound symbol — `intern_driver` keys the keyword treatment on
/// `initial_obarray`. Emacs 30.2 answers `(nil nil 1)` and `(nil nil 7)` for the
/// two cases below; a name-prefix `keywordp` answers `t` for both.
#[test]
fn a_colon_name_outside_the_standard_obarray_is_not_a_keyword() {
    assert_eq!(
        eval(
            "(let ((s (make-symbol \":u\"))) \
               (list (keywordp s) (boundp s) (progn (set s 1) (symbol-value s))))"
        ),
        "(nil nil 1)"
    );
    assert_eq!(
        eval(
            "(let* ((ob (obarray-make)) (s (intern \":ob\" ob))) \
               (list (keywordp s) (boundp s) (progn (set s 7) (symbol-value s))))"
        ),
        "(nil nil 7)"
    );
    // The genuine article still is one.
    assert_eq!(eval("(keywordp :real)"), "t");
    assert_eq!(eval("(keywordp (intern \":\"))"), "t");
    assert_eq!(eval("(keywordp 'ordinary)"), "nil");
}

/// `fset` guards only `nil`.
///
/// The constant set applies to *value* cells. `t` and keywords have writable
/// function cells, so `(fset :k (lambda () 7))` installs a callable definition —
/// treating them as constant here would be over-application of the rule.
#[test]
fn fset_protects_nil_but_not_t_or_keywords() {
    assert_eq!(eval("(fset nil (lambda ()))"), "(setting-constant nil)");
    assert_eq!(
        eval("(fset (intern \"nil\") (lambda ()))"),
        "(setting-constant nil)"
    );
    assert_eq!(eval("(progn (fset :k (lambda () 7)) (funcall :k))"), "7");
}

/// A constant cannot be given a forwarding cell.
///
/// `Fdefvaralias` reports this as a plain `error` naming the alias, not as
/// `setting-constant`.
#[test]
fn defvaralias_rejects_a_constant_alias() {
    assert_eq!(
        eval("(defvaralias nil 'x)"),
        "(error \"Cannot make a constant an alias: nil\")"
    );
    assert_eq!(
        eval("(defvaralias t 'y)"),
        "(error \"Cannot make a constant an alias: t\")"
    );
    assert_eq!(
        eval("(defvaralias :k 'y)"),
        "(error \"Cannot make a constant an alias: :k\")"
    );
}

/// An ordinary symbol is untouched by any of this.
///
/// The guard is narrow on purpose: a regression here would break every `setq`.
#[test]
fn ordinary_symbols_are_still_writable() {
    assert_eq!(eval("(progn (setq ordinary 5) ordinary)"), "5");
    assert_eq!(eval("(progn (set 'ordinary 6) ordinary)"), "6");
    assert_eq!(eval("(let ((v 1)) v)"), "1");
    assert_eq!(eval("(progn (set-default 'od 7) (default-value 'od))"), "7");
    assert_eq!(
        eval("(progn (setq gone 1) (makunbound 'gone) (boundp 'gone))"),
        "nil"
    );
    // An uninterned symbol *named* "t" is not the constant `t`.
    assert_eq!(
        eval("(let ((s (make-symbol \"t\"))) (set s 3) (symbol-value s))"),
        "3"
    );
}
