//! The elisp evaluator: a Lisp-2 obarray with dynamic binding, layered over
//! rust_lisp's reader + `Value`/`List` data model.
//!
//! Why not rust_lisp's own `eval`? It is Lisp-1 (one namespace) and lexically
//! scoped. Emacs Lisp is Lisp-2 (separate value/function cells per symbol) and,
//! by default, dynamically scoped. Those are core semantics, so the evaluator,
//! environment, and special forms live here. We reuse rust_lisp strictly for
//! the S-expression reader and the value representation.
//!
//! ## Milestone-1 limitations (documented, not hidden)
//! - **No dotted pairs.** rust_lisp's `List` cons cell always has a list cdr,
//!   so `(cons 1 2)` / `(a . b)` can't be represented. `cons` errors on a
//!   non-list cdr. Replacing the cons model is the top milestone-2 item.
//! - **Dynamic scope only.** `lexical-binding` is not honored yet.
//! - **Reader quirks inherited from rust_lisp**: a bare `f` / `true` / `false`
//!   token is read as a boolean, not a symbol.

use crate::callable::{Callable, Params};
use crate::error::{ElError, ElResult};
use rust_lisp::model::{List, Symbol, Value};
use rust_lisp::parser::parse;
use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;

// ── value helpers ──────────────────────────────────────────────────────────

/// elisp falsiness: only `nil` (and the stray rust_lisp `False`) is false.
pub fn is_nil(v: &Value) -> bool {
    *v == Value::NIL || matches!(v, Value::False)
}
pub fn truthy(v: &Value) -> bool {
    !is_nil(v)
}
pub fn t_or_nil(b: bool) -> Value {
    if b { Value::True } else { Value::NIL }
}
pub fn list_from(items: Vec<Value>) -> Value {
    Value::List(items.into_iter().collect())
}
/// Collect a proper list into a Vec. Returns `None` for non-lists.
pub fn to_vec(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::List(l) => Some(List::into_iter(l).collect()),
        _ => None,
    }
}

/// Parse a lambda list `(a b &optional c &rest d)` into structured params.
/// Free function (needs no interpreter state) so closures can be built from
/// `funcall`/`apply` without `&mut self`.
pub fn parse_params(list: &Value) -> ElResult<Params> {
    let items = to_vec(list).ok_or_else(|| ElError::err("invalid lambda list"))?;
    let mut p = Params { required: vec![], optional: vec![], rest: None };
    let mut mode = 0u8; // 0 = required, 1 = &optional, 2 = &rest
    for it in items {
        let Value::Symbol(s) = it else {
            return Err(ElError::err("invalid lambda list element"));
        };
        match s.0.as_str() {
            "&optional" => mode = 1,
            "&rest" => mode = 2,
            n => match mode {
                0 => p.required.push(n.to_string()),
                1 => p.optional.push(n.to_string()),
                _ => p.rest = Some(n.to_string()),
            },
        }
    }
    Ok(p)
}

/// Build a closure/macro `Value` from `(PARAMS . BODY)` parts.
fn make_lambda(name: Option<String>, parts: &[Value], is_macro: bool) -> ElResult<Value> {
    if parts.is_empty() {
        return Err(ElError::err("lambda requires an argument list"));
    }
    let params = parse_params(&parts[0])?;
    let body = parts[1..].to_vec();
    let callable = if is_macro {
        Callable::Macro { name, params, body }
    } else {
        Callable::Closure { name, params, body }
    };
    Ok(Value::Foreign(Rc::new(callable)))
}

fn downcast_callable(v: &Value) -> Option<Rc<Callable>> {
    if let Value::Foreign(rc) = v {
        let any: Rc<dyn Any> = rc.clone();
        any.downcast::<Callable>().ok()
    } else {
        None
    }
}

// ── obarray ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Cell {
    value: Option<Value>,    // value cell (None = void)
    function: Option<Value>, // function cell (None = void)
    special: bool,           // declared via defvar/defconst
}

pub struct Interp {
    obarray: HashMap<String, Cell>,
    specstack: Vec<(String, Option<Value>)>,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Self {
        let mut it = Interp { obarray: HashMap::new(), specstack: Vec::new() };
        crate::builtins::install(&mut it);
        it
    }

    // ── cell access ──
    fn cell_mut(&mut self, name: &str) -> &mut Cell {
        self.obarray.entry(name.to_string()).or_default()
    }
    pub fn get_value(&self, name: &str) -> Option<Value> {
        self.obarray.get(name).and_then(|c| c.value.clone())
    }
    pub fn set_value(&mut self, name: &str, val: Value) {
        self.cell_mut(name).value = Some(val);
    }
    pub fn get_function(&self, name: &str) -> Option<Value> {
        self.obarray.get(name).and_then(|c| c.function.clone())
    }
    pub fn set_function(&mut self, name: &str, val: Value) {
        self.cell_mut(name).function = Some(val);
    }
    pub fn is_bound(&self, name: &str) -> bool {
        self.obarray.get(name).is_some_and(|c| c.value.is_some())
    }
    pub fn is_fbound(&self, name: &str) -> bool {
        self.obarray.get(name).is_some_and(|c| c.function.is_some())
    }

    /// Register a native builtin under `name`'s function cell.
    pub fn defsubr(&mut self, name: &str, min: usize, max: Option<usize>, func: crate::callable::SubrFn) {
        let v = Value::Foreign(Rc::new(Callable::Subr { name: name.to_string(), min, max, func }));
        self.set_function(name, v);
    }

    // ── dynamic binding ──
    fn specbind(&mut self, name: &str, val: Value) {
        let old = self.get_value(name);
        self.specstack.push((name.to_string(), old));
        self.set_value(name, val);
    }
    fn unbind_to(&mut self, depth: usize) {
        while self.specstack.len() > depth {
            let (name, old) = self.specstack.pop().unwrap();
            self.cell_mut(&name).value = old;
        }
    }

    /// Resolve a function designator (symbol, lambda list, or callable) to a
    /// `Callable`, following symbol aliases through function cells.
    pub fn resolve(&self, v: &Value) -> ElResult<Rc<Callable>> {
        match v {
            Value::Foreign(_) => downcast_callable(v)
                .ok_or_else(|| ElError::new("invalid-function", "not a function")),
            Value::Symbol(s) => {
                let mut cur = s.0.clone();
                for _ in 0..64 {
                    let Some(fv) = self.get_function(&cur) else {
                        return Err(ElError::void_function(&cur));
                    };
                    match &fv {
                        Value::Foreign(_) => {
                            return downcast_callable(&fv)
                                .ok_or_else(|| ElError::void_function(&cur));
                        }
                        Value::Symbol(next) => cur = next.0.clone(),
                        _ => return Err(ElError::void_function(&cur)),
                    }
                }
                Err(ElError::void_function(&s.0))
            }
            Value::List(l) => {
                let parts: Vec<Value> = List::into_iter(l).collect();
                if let Some(Value::Symbol(s)) = parts.first() {
                    if s.0 == "lambda" {
                        let v = make_lambda(None, &parts[1..], false)?;
                        return downcast_callable(&v)
                            .ok_or_else(|| ElError::new("invalid-function", "bad lambda"));
                    }
                }
                Err(ElError::new("invalid-function", "not a function"))
            }
            _ => Err(ElError::new("invalid-function", "not a function")),
        }
    }

    // ── evaluation ──
    pub fn eval(&mut self, form: &Value) -> ElResult<Value> {
        match form {
            Value::Symbol(s) => self.eval_symbol(&s.0),
            Value::List(l) => {
                if *l == List::NIL {
                    Ok(Value::NIL)
                } else {
                    self.eval_list(l)
                }
            }
            other => Ok(other.clone()), // self-evaluating
        }
    }

    fn eval_symbol(&self, name: &str) -> ElResult<Value> {
        match name {
            "nil" => Ok(Value::NIL),
            "t" => Ok(Value::True),
            _ if name.starts_with(':') => Ok(Value::Symbol(Symbol(name.to_string()))),
            _ => self.get_value(name).ok_or_else(|| ElError::void_variable(name)),
        }
    }

    fn eval_list(&mut self, l: &List) -> ElResult<Value> {
        let elems: Vec<Value> = List::into_iter(l).collect();
        let head = elems[0].clone();
        let args = &elems[1..];
        match &head {
            Value::Symbol(s) => {
                if let Some(r) = self.try_special(&s.0, args) {
                    return r;
                }
                // function / macro call
                let call = self.resolve(&head).map_err(|_| ElError::void_function(&s.0))?;
                match &*call {
                    Callable::Macro { params, body, .. } => {
                        let expansion = self.apply_lambda(params, body, args.to_vec())?;
                        self.eval(&expansion)
                    }
                    _ => {
                        let av = self.eval_args(args)?;
                        self.apply(call, av)
                    }
                }
            }
            Value::List(_) => {
                // ((lambda ...) args...)
                let call = self.resolve(&head)?;
                let av = self.eval_args(args)?;
                self.apply(call, av)
            }
            _ => Err(ElError::new("invalid-function", self.print(&head, true))),
        }
    }

    fn eval_args(&mut self, args: &[Value]) -> ElResult<Vec<Value>> {
        args.iter().map(|a| self.eval(a)).collect()
    }

    fn eval_body(&mut self, body: &[Value]) -> ElResult<Value> {
        let mut last = Value::NIL;
        for form in body {
            last = self.eval(form)?;
        }
        Ok(last)
    }

    /// Apply a resolved callable to already-evaluated arguments.
    pub fn apply(&mut self, callable: Rc<Callable>, args: Vec<Value>) -> ElResult<Value> {
        match &*callable {
            Callable::Subr { name, min, max, func } => {
                if args.len() < *min || max.is_some_and(|m| args.len() > m) {
                    return Err(ElError::wrong_args(name));
                }
                func(self, &args)
            }
            Callable::Closure { params, body, .. } => self.apply_lambda(params, body, args),
            Callable::Macro { .. } => Err(ElError::err("cannot apply a macro")),
        }
    }

    fn apply_lambda(&mut self, params: &Params, body: &[Value], args: Vec<Value>) -> ElResult<Value> {
        if args.len() < params.required.len() {
            return Err(ElError::wrong_args("lambda"));
        }
        let max = params.required.len() + params.optional.len();
        if params.rest.is_none() && args.len() > max {
            return Err(ElError::wrong_args("lambda"));
        }
        let depth = self.specstack.len();
        let mut i = 0;
        for r in &params.required {
            self.specbind(r, args[i].clone());
            i += 1;
        }
        for o in &params.optional {
            let v = args.get(i).cloned().unwrap_or(Value::NIL);
            self.specbind(o, v);
            i += 1;
        }
        if let Some(rest) = &params.rest {
            let rem = args.get(i..).map(|s| s.to_vec()).unwrap_or_default();
            self.specbind(rest, list_from(rem));
        }
        let r = self.eval_body(body);
        self.unbind_to(depth);
        r
    }

    // ── special forms ──
    /// Returns `Some(result)` if `name` is a special form, else `None` so the
    /// caller falls through to function/macro dispatch.
    fn try_special(&mut self, name: &str, args: &[Value]) -> Option<ElResult<Value>> {
        let r = match name {
            "quote" => Ok(args.first().cloned().unwrap_or(Value::NIL)),
            "function" => self.sf_function(args),
            "lambda" => make_lambda(None, args, false),
            "progn" => self.eval_body(args),
            "prog1" => self.sf_prog1(args),
            "if" => self.sf_if(args),
            "when" => self.sf_when(args, true),
            "unless" => self.sf_when(args, false),
            "cond" => self.sf_cond(args),
            "and" => self.sf_and(args),
            "or" => self.sf_or(args),
            "while" => self.sf_while(args),
            "setq" => self.sf_setq(args),
            "let" => self.sf_let(args, false),
            "let*" => self.sf_let(args, true),
            "defun" => self.sf_defun(args, false),
            "defmacro" => self.sf_defun(args, true),
            "defvar" => self.sf_defvar(args, false),
            "defconst" => self.sf_defvar(args, true),
            "condition-case" => self.sf_condition_case(args),
            "unwind-protect" => self.sf_unwind_protect(args),
            _ => return None,
        };
        Some(r)
    }

    fn sf_function(&mut self, args: &[Value]) -> ElResult<Value> {
        let a = args.first().cloned().unwrap_or(Value::NIL);
        if let Value::List(l) = &a {
            let parts: Vec<Value> = List::into_iter(l).collect();
            if let Some(Value::Symbol(s)) = parts.first() {
                if s.0 == "lambda" {
                    return make_lambda(None, &parts[1..], false);
                }
            }
        }
        Ok(a)
    }

    fn sf_prog1(&mut self, args: &[Value]) -> ElResult<Value> {
        let first = self.eval(args.first().unwrap_or(&Value::NIL))?;
        for a in &args[args.len().min(1)..] {
            self.eval(a)?;
        }
        Ok(first)
    }

    fn sf_if(&mut self, args: &[Value]) -> ElResult<Value> {
        let test = self.eval(args.first().unwrap_or(&Value::NIL))?;
        if truthy(&test) {
            self.eval(args.get(1).unwrap_or(&Value::NIL))
        } else {
            self.eval_body(args.get(2..).unwrap_or(&[]))
        }
    }

    fn sf_when(&mut self, args: &[Value], polarity: bool) -> ElResult<Value> {
        let test = self.eval(args.first().unwrap_or(&Value::NIL))?;
        if truthy(&test) == polarity {
            self.eval_body(args.get(1..).unwrap_or(&[]))
        } else {
            Ok(Value::NIL)
        }
    }

    fn sf_cond(&mut self, args: &[Value]) -> ElResult<Value> {
        for clause in args {
            let parts = to_vec(clause).ok_or_else(|| ElError::err("cond: bad clause"))?;
            if parts.is_empty() {
                continue;
            }
            let test = self.eval(&parts[0])?;
            if truthy(&test) {
                return if parts.len() == 1 { Ok(test) } else { self.eval_body(&parts[1..]) };
            }
        }
        Ok(Value::NIL)
    }

    fn sf_and(&mut self, args: &[Value]) -> ElResult<Value> {
        let mut last = Value::True;
        for a in args {
            last = self.eval(a)?;
            if is_nil(&last) {
                return Ok(Value::NIL);
            }
        }
        Ok(last)
    }

    fn sf_or(&mut self, args: &[Value]) -> ElResult<Value> {
        for a in args {
            let v = self.eval(a)?;
            if truthy(&v) {
                return Ok(v);
            }
        }
        Ok(Value::NIL)
    }

    fn sf_while(&mut self, args: &[Value]) -> ElResult<Value> {
        while truthy(&self.eval(args.first().unwrap_or(&Value::NIL))?) {
            self.eval_body(args.get(1..).unwrap_or(&[]))?;
        }
        Ok(Value::NIL)
    }

    fn sf_setq(&mut self, args: &[Value]) -> ElResult<Value> {
        let mut last = Value::NIL;
        let mut i = 0;
        while i + 1 < args.len() {
            let Value::Symbol(s) = &args[i] else {
                return Err(ElError::err("setq: expected symbol"));
            };
            let v = self.eval(&args[i + 1])?;
            self.set_value(&s.0, v.clone());
            last = v;
            i += 2;
        }
        Ok(last)
    }

    fn sf_let(&mut self, args: &[Value], sequential: bool) -> ElResult<Value> {
        let bindings = to_vec(args.first().unwrap_or(&Value::NIL)).unwrap_or_default();
        let depth = self.specstack.len();
        if sequential {
            for b in &bindings {
                let (name, val) = self.parse_binding(b)?;
                let v = self.eval(&val)?;
                self.specbind(&name, v);
            }
        } else {
            // evaluate all init forms in the OUTER environment, then bind
            let mut pending = Vec::new();
            for b in &bindings {
                let (name, val) = self.parse_binding(b)?;
                pending.push((name, self.eval(&val)?));
            }
            for (name, v) in pending {
                self.specbind(&name, v);
            }
        }
        let r = self.eval_body(args.get(1..).unwrap_or(&[]));
        self.unbind_to(depth);
        r
    }

    fn parse_binding(&self, b: &Value) -> ElResult<(String, Value)> {
        match b {
            Value::Symbol(s) => Ok((s.0.clone(), Value::NIL)),
            Value::List(_) => {
                let parts = to_vec(b).unwrap();
                let Some(Value::Symbol(s)) = parts.first() else {
                    return Err(ElError::err("let: bad binding"));
                };
                Ok((s.0.clone(), parts.get(1).cloned().unwrap_or(Value::NIL)))
            }
            _ => Err(ElError::err("let: bad binding")),
        }
    }

    fn sf_defun(&mut self, args: &[Value], is_macro: bool) -> ElResult<Value> {
        let Some(Value::Symbol(s)) = args.first() else {
            return Err(ElError::err("defun: expected name"));
        };
        let name = s.0.clone();
        let v = make_lambda(Some(name.clone()), &args[1..], is_macro)?;
        self.set_function(&name, v);
        Ok(Value::Symbol(Symbol(name)))
    }

    fn sf_defvar(&mut self, args: &[Value], force: bool) -> ElResult<Value> {
        let Some(Value::Symbol(s)) = args.first() else {
            return Err(ElError::err("defvar: expected name"));
        };
        let name = s.0.clone();
        if force || !self.is_bound(&name) {
            if let Some(init) = args.get(1) {
                let v = self.eval(init)?;
                self.set_value(&name, v);
            }
        }
        self.cell_mut(&name).special = true;
        Ok(Value::Symbol(Symbol(name)))
    }

    fn sf_condition_case(&mut self, args: &[Value]) -> ElResult<Value> {
        let var = match args.first() {
            Some(Value::Symbol(s)) => Some(s.0.clone()),
            _ => None,
        };
        let body = args.get(1).cloned().unwrap_or(Value::NIL);
        match self.eval(&body) {
            Ok(v) => Ok(v),
            Err(e) => {
                for handler in &args[args.len().min(2)..] {
                    let parts = to_vec(handler).ok_or_else(|| ElError::err("bad handler"))?;
                    let matches = matches!(parts.first(),
                        Some(Value::Symbol(s)) if s.0 == "error" || s.0 == e.symbol);
                    if matches {
                        let depth = self.specstack.len();
                        if let Some(v) = &var {
                            let obj = list_from(vec![
                                Value::Symbol(Symbol(e.symbol.clone())),
                                Value::String(e.data.clone()),
                            ]);
                            self.specbind(v, obj);
                        }
                        let r = self.eval_body(&parts[1..]);
                        self.unbind_to(depth);
                        return r;
                    }
                }
                Err(e)
            }
        }
    }

    fn sf_unwind_protect(&mut self, args: &[Value]) -> ElResult<Value> {
        let r = self.eval(args.first().unwrap_or(&Value::NIL));
        let _ = self.eval_body(args.get(1..).unwrap_or(&[]));
        r
    }

    // ── top-level entry + printing ──
    pub fn eval_str(&mut self, src: &str) -> ElResult<Value> {
        let mut last = Value::NIL;
        for form in parse(src) {
            let form = form.map_err(|e| ElError::err(format!("parse error: {e:?}")))?;
            last = self.eval(&form)?;
        }
        Ok(last)
    }

    /// Render a value. `readable` = prin1 style (quoted strings); else princ.
    pub fn print(&self, v: &Value, readable: bool) -> String {
        match v {
            Value::True => "t".to_string(),
            Value::False => "nil".to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format!("{f:?}"),
            Value::String(s) => {
                if readable {
                    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                } else {
                    s.clone()
                }
            }
            Value::Symbol(s) => s.0.clone(),
            Value::List(l) => {
                if *l == List::NIL {
                    "nil".to_string()
                } else {
                    let parts: Vec<String> =
                        List::into_iter(l).map(|e| self.print(&e, readable)).collect();
                    format!("({})", parts.join(" "))
                }
            }
            Value::Foreign(_) => downcast_callable(v)
                .map(|c| c.label())
                .unwrap_or_else(|| "#<foreign>".to_string()),
            Value::HashMap(_) => "#<hash-table>".to_string(),
            _ => "#<object>".to_string(),
        }
    }
}
