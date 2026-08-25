//! Which lexical variables a closure body actually references.
//!
//! Emacs prunes an interpreted closure's captured environment down to the
//! variables its body uses, so `(let ((n 1) (m 2)) (lambda () n))` closes over
//! `((n . 1))` and not over `m`. That is observable — through `prin1`, through
//! `equal`, and through what the closure keeps alive — so elisprs has to do the
//! same analysis.
//!
//! This is a port of `cconv-fv` / `cconv-analyze-form`
//! (`lisp/emacs-lisp/cconv.el`), reduced to the one question elisprs asks: the
//! SET of free lexical variables. Emacs's analyzer also classifies each
//! variable as read / mutated / captured / called to drive lambda lifting and
//! byte-compiler warnings; none of that survives into the interpreted closure,
//! so it is not modelled here.
//!
//! The input is post-macroexpansion source. Every elisprs eval path runs
//! `macroexpand_all` before compiling (`lib.rs` `eval_str` / `run_top_forms`,
//! `host.rs`'s `eval` intrinsic), which is the same precondition
//! `cconv-analyze-form`'s doc comment states ("FORM is a piece of Elisp code
//! after macroexpansion"). A macro that somehow reached here unexpanded can only
//! make the analysis *over*-approximate: its arguments are walked as ordinary
//! forms, so every symbol in them is treated as a reference. Over-capturing
//! prints a larger environment than Emacs; under-capturing would make a
//! variable read fall through to the global value cell, so the walk is written
//! to err in the first direction wherever it is unsure.

use std::collections::HashSet;

use fusevm::Value;

use crate::host::{ElispHost, Obj};

/// The free lexical variables of `(lambda ARGLIST . BODY)`, as arena symbol
/// handles.
///
/// Only names matter here: the caller intersects this set with the lexical
/// environment in force, so a symbol that names a global, a keyword, or a
/// function is dropped there rather than needing a test here.
pub fn free_vars(h: &ElispHost, arglist: &Value, body: &[Value]) -> HashSet<u32> {
    let mut w = Walk {
        h,
        out: HashSet::new(),
    };
    let mut bound = Vec::new();
    w.params(arglist, &mut bound);
    for form in body {
        w.form(form, &bound);
    }
    w.out
}

struct Walk<'a> {
    h: &'a ElispHost,
    out: HashSet<u32>,
}

impl Walk<'_> {
    /// `form`'s free variables, given the names already bound around it.
    fn form(&mut self, form: &Value, bound: &[u32]) {
        let Value::Obj(_) = form else { return };
        match self.h.obj(form) {
            Some(Obj::Symbol(_)) => {
                if let Some(sym) = self.sym_handle(form) {
                    if !bound.contains(&sym) {
                        self.out.insert(sym);
                    }
                }
            }
            Some(Obj::Cons(..)) => self.call(form, bound),
            // A vector, a record, a string, a bignum … are self-evaluating:
            // `cconv-analyze-form`'s pcase matches neither `(HEAD . ARGS)` nor
            // `(pred symbolp)` for them, so nothing is referenced.
            _ => {}
        }
    }

    /// A cons form: dispatch on the head the way `cconv-analyze-form` does.
    fn call(&mut self, form: &Value, bound: &[u32]) {
        let Some(elems) = self.h.list_vec(form) else {
            // An improper form is not code cconv would accept. Walking the
            // spine would risk missing a reference, so leave the caller's
            // conservative default (nothing removed) by capturing every symbol
            // reachable in it.
            self.capture_all(form);
            return;
        };
        let Some(head) = elems.first() else { return };
        let name = self.h.sym_name(head);
        match name.as_deref() {
            // `(quote . _)` and `(function SYM)` reference nothing;
            // `(function (lambda ...))` is the one shape that carries code.
            Some("quote") => {}
            Some("function") => {
                if let Some(inner) = elems.get(1) {
                    if self.head_is(inner, "lambda") {
                        self.lambda(inner, bound);
                    }
                }
            }
            Some("lambda") => self.lambda(form, bound),
            Some("let") | Some("let*") => {
                let sequential = name.as_deref() == Some("let*");
                let binders = elems
                    .get(1)
                    .and_then(|b| self.h.list_vec(b))
                    .unwrap_or_default();
                let mut inner = bound.to_vec();
                for binder in binders {
                    // A binder is either `SYM` or `(SYM [VALUE])`. The VALUE of a
                    // `let` binder is analyzed in the ORIGINAL environment; a
                    // `let*` binder sees the ones before it.
                    let (sym, value) = match self.h.list_vec(&binder) {
                        Some(pair) if !pair.is_empty() => (pair[0].clone(), pair.get(1).cloned()),
                        _ => (binder.clone(), None),
                    };
                    if let Some(v) = value {
                        self.form(&v, if sequential { &inner } else { bound });
                    }
                    if let Some(s) = self.sym_handle(&sym) {
                        inner.push(s);
                    }
                }
                for f in elems.get(2..).unwrap_or(&[]) {
                    self.form(f, &inner);
                }
            }
            // `(setq SYM VAL ...)`: writing a variable captures it just as
            // reading it does — `(let ((n 1)) (lambda () (setq n 2)))` closes
            // over `((n . 1))`.
            Some("setq") => {
                let mut rest = &elems[1..];
                while !rest.is_empty() {
                    if let Some(s) = self.sym_handle(&rest[0]) {
                        if !bound.contains(&s) {
                            self.out.insert(s);
                        }
                    }
                    if let Some(v) = rest.get(1) {
                        self.form(v, bound);
                    }
                    rest = rest.get(2..).unwrap_or(&[]);
                }
            }
            // `(condition-case VAR PROTECTED HANDLERS...)`: VAR is bound only
            // over the handler bodies.
            Some("condition-case") => {
                if let Some(p) = elems.get(2) {
                    self.form(p, bound);
                }
                let mut inner = bound.to_vec();
                if let Some(v) = elems.get(1).and_then(|v| self.sym_handle(v)) {
                    inner.push(v);
                }
                for handler in elems.get(3..).unwrap_or(&[]) {
                    for f in self.h.list_vec(handler).unwrap_or_default().iter().skip(1) {
                        self.form(f, &inner);
                    }
                }
            }
            // A `defvar`/`defconst` names a DYNAMIC variable; only its value
            // form is code.
            Some("defvar") | Some("defconst") => {
                if let Some(v) = elems.get(2) {
                    self.form(v, bound);
                }
            }
            // `(defun NAME ARGS . BODY)` — the name is a function cell, not a
            // variable reference; the rest is an ordinary lambda.
            Some("defun") | Some("defmacro") => {
                let arglist = elems.get(2).cloned().unwrap_or(Value::Undef);
                let mut inner = bound.to_vec();
                self.params(&arglist, &mut inner);
                for f in elems.get(3..).unwrap_or(&[]) {
                    self.form(f, &inner);
                }
            }
            // An `interactive` spec is analyzed by Emacs as part of the
            // enclosing lambda, never on its own.
            Some("interactive") => {}
            // `(cond (TEST . BODY) ...)` — every clause element is code.
            Some("cond") => {
                for clause in elems.get(1..).unwrap_or(&[]) {
                    for f in self.h.list_vec(clause).unwrap_or_default() {
                        self.form(&f, bound);
                    }
                }
            }
            // Everything else: the head names a function (not a variable — a
            // bare `(n)` does NOT capture `n`), and every argument is code. The
            // deprecated `((lambda ...) ARGS)` spelling puts a lambda in head
            // position, which cconv rewrites to `(function (lambda ...))`.
            _ => {
                if name.is_none() && self.head_is(head, "lambda") {
                    self.lambda(head, bound);
                }
                for f in &elems[1..] {
                    self.form(f, bound);
                }
            }
        }
    }

    /// `(lambda ARGLIST . BODY)` — its own parameters shadow the enclosing
    /// bindings, and whatever is still free escapes to the enclosing closure.
    fn lambda(&mut self, form: &Value, bound: &[u32]) {
        let elems = self.h.list_vec(form).unwrap_or_default();
        let arglist = elems.get(1).cloned().unwrap_or(Value::Undef);
        let mut inner = bound.to_vec();
        self.params(&arglist, &mut inner);
        for f in elems.get(2..).unwrap_or(&[]) {
            self.form(f, &inner);
        }
    }

    /// Push every parameter name in `arglist` onto `bound`. `&optional` and
    /// `&rest` are markers, not parameters, but pushing them is harmless — no
    /// binding is ever named `&optional`, so they can never shadow anything.
    fn params(&mut self, arglist: &Value, bound: &mut Vec<u32>) {
        for p in self.h.list_vec(arglist).unwrap_or_default() {
            if let Some(s) = self.sym_handle(&p) {
                bound.push(s);
            }
        }
    }

    /// Every symbol reachable in `form`, bound or not — the safe answer for a
    /// shape the walk does not model.
    fn capture_all(&mut self, form: &Value) {
        match self.h.obj(form) {
            Some(Obj::Symbol(_)) => {
                if let Some(s) = self.sym_handle(form) {
                    self.out.insert(s);
                }
            }
            Some(Obj::Cons(car, cdr)) => {
                let (car, cdr) = (car.clone(), cdr.clone());
                self.capture_all(&car);
                self.capture_all(&cdr);
            }
            _ => {}
        }
    }

    /// `v`'s arena handle when `v` is a heap symbol. `nil` and `t` are
    /// `Value::Undef`/`Value::Bool` here rather than heap symbols, so they
    /// answer None and can never be captured — which is right: neither is
    /// lexically bindable.
    fn sym_handle(&self, v: &Value) -> Option<u32> {
        match (v, self.h.obj(v)) {
            (Value::Obj(id), Some(Obj::Symbol(_))) => Some(*id),
            _ => None,
        }
    }

    /// Whether `form` is a cons whose head is the symbol `name`.
    fn head_is(&self, form: &Value, name: &str) -> bool {
        match self.h.obj(form) {
            Some(Obj::Cons(car, _)) => self.h.sym_name(car).as_deref() == Some(name),
            _ => false,
        }
    }
}
