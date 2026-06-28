//! The ElispHost: the elisp object heap, the symbol obarray, dynamic binding,
//! and the primitive subrs — reached from fusevm's extension handler. elisprs
//! has no VM; fusevm executes the lowered bytecode and calls back here.
//!
//! Functions (subrs AND user closures) are heap objects; a symbol's function
//! cell holds a `Value` pointing at one. A user closure carries a precompiled
//! `fusevm::Chunk` body, so calling it = running that chunk on a (nested) fusevm
//! VM. Binding is dynamic this milestone (classic elisp; lexical is next): a
//! `let`/closure param saves the symbol's value cell on `specstack` and restores
//! it on unwind.
//!
//! Re-entrancy: a subr that calls back into elisp (`funcall`/`mapcar`/…) must not
//! hold the host borrow while the callee runs. [`call_function`] is the single
//! re-entrant entry point and only ever borrows the host for short, nested-free
//! operations.

use fusevm::{Chunk, Value, VMResult, VM};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Extension-op IDs emitted by the compiler and dispatched here.
pub mod ops {
    pub const TRUTHY: u16 = 0; // pop v; push Bool(elisp-truthy(v))
    pub const CALL: u16 = 1; // arg=argc; stack [sym, args...] -> result
    pub const GETVAR: u16 = 2; // pop sym; push value cell
    pub const SETVAR: u16 = 3; // pop val, pop sym; set value cell; push val
    pub const FSET: u16 = 4; // pop def, pop sym; set function cell; push sym
    pub const SPECBIND: u16 = 5; // pop sym, pop val; dynamic-bind; push nothing
    pub const LETBIND: u16 = 6; // wide n: pop n (val,sym) pairs; dynamic-bind all
    pub const UNBIND: u16 = 7; // wide n: unwind n dynamic binds (keep stack value)
}

pub type SubrFn = fn(&mut ElispHost, &[Value]) -> Result<Value, String>;

/// A parsed lambda list (symbol handles).
pub struct Params {
    pub required: Vec<u32>,
    pub optional: Vec<u32>,
    pub rest: Option<u32>,
}

pub struct SymbolData {
    pub name: String,
    pub value: Option<Value>,
    pub function: Option<Value>, // points at an Obj::Subr / Obj::Closure / alias symbol
    pub special: bool,
}

pub enum Obj {
    Cons(Value, Value),
    Symbol(SymbolData),
    Vector(Vec<Value>),
    Subr { name: String, min: usize, max: Option<usize>, f: SubrFn },
    Closure { params: Rc<Params>, body: Rc<Chunk>, is_macro: bool },
}

/// Resolution of a function designator to something callable.
pub enum Resolved {
    Subr { f: SubrFn, min: usize, max: Option<usize>, name: String },
    Closure { params: Rc<Params>, body: Rc<Chunk>, is_macro: bool },
}

pub struct ElispHost {
    pub(crate) arena: Vec<Obj>,
    obarray: HashMap<String, u32>,
    /// Dynamic-binding save stack: (symbol handle, previous value cell).
    specstack: Vec<(u32, Option<Value>)>,
    pub(crate) error: Option<String>,
    /// A pending `throw`: (tag, value). Set by `throw`, consumed by `catch`.
    /// Distinguishes a non-local `throw` from an ordinary error during unwinding.
    pub(crate) pending_throw: Option<(Value, Value)>,
}

impl Default for ElispHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ElispHost {
    pub fn new() -> Self {
        let mut h = ElispHost {
            arena: Vec::new(),
            obarray: HashMap::new(),
            specstack: Vec::new(),
            error: None,
            pending_throw: None,
        };
        crate::builtins::install(&mut h);
        h
    }

    // ── arena / interning ──
    pub fn alloc(&mut self, obj: Obj) -> Value {
        let id = self.arena.len() as u32;
        self.arena.push(obj);
        Value::Obj(id)
    }
    pub fn intern(&mut self, name: &str) -> Value {
        if let Some(&id) = self.obarray.get(name) {
            return Value::Obj(id);
        }
        let id = self.arena.len() as u32;
        self.arena.push(Obj::Symbol(SymbolData {
            name: name.to_string(),
            value: None,
            function: None,
            special: false,
        }));
        self.obarray.insert(name.to_string(), id);
        Value::Obj(id)
    }
    pub fn obj(&self, v: &Value) -> Option<&Obj> {
        match v {
            Value::Obj(id) => self.arena.get(*id as usize),
            _ => None,
        }
    }
    fn sym_handle(&self, v: &Value) -> Option<u32> {
        match v {
            Value::Obj(id) if matches!(self.arena.get(*id as usize), Some(Obj::Symbol(_))) => {
                Some(*id)
            }
            _ => None,
        }
    }
    pub fn sym_name(&self, v: &Value) -> Option<String> {
        match self.obj(v) {
            Some(Obj::Symbol(s)) => Some(s.name.clone()),
            _ => match v {
                Value::Bool(true) => Some("t".to_string()),
                Value::Undef => Some("nil".to_string()),
                _ => None,
            },
        }
    }

    // ── cons ──
    pub fn cons(&mut self, a: Value, b: Value) -> Value {
        self.alloc(Obj::Cons(a, b))
    }
    pub fn list_from(&mut self, items: Vec<Value>) -> Value {
        let mut acc = Value::Undef;
        for x in items.into_iter().rev() {
            acc = self.cons(x, acc);
        }
        acc
    }
    pub fn list_vec(&self, v: &Value) -> Option<Vec<Value>> {
        let mut out = Vec::new();
        let mut cur = v.clone();
        loop {
            match &cur {
                Value::Undef => return Some(out),
                Value::Obj(id) => match self.arena.get(*id as usize) {
                    Some(Obj::Cons(a, d)) => {
                        out.push(a.clone());
                        let next = d.clone();
                        cur = next;
                    }
                    _ => return None,
                },
                _ => return None,
            }
        }
    }

    // ── symbol cells (dynamic / value cell) ──
    pub fn set_value(&mut self, v: &Value, val: Value) -> Result<(), String> {
        let id = self.sym_handle(v).ok_or("set: not a symbol")?;
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.value = Some(val);
        }
        Ok(())
    }
    pub fn get_value(&self, v: &Value) -> Result<Value, String> {
        match self.obj(v) {
            Some(Obj::Symbol(s)) => s
                .value
                .clone()
                .ok_or_else(|| format!("Symbol's value as variable is void: {}", s.name)),
            _ => match v {
                Value::Bool(true) => Ok(Value::Bool(true)),
                Value::Undef => Ok(Value::Undef),
                _ => Err("not a symbol".to_string()),
            },
        }
    }
    pub fn set_function_value(&mut self, sym: &Value, def: Value) -> Result<(), String> {
        let id = self.sym_handle(sym).ok_or("fset: not a symbol")?;
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.function = Some(def);
        }
        Ok(())
    }
    pub fn set_function(&mut self, name: &str, def: Value) {
        let v = self.intern(name);
        let _ = self.set_function_value(&v, def);
    }
    pub fn defsubr(&mut self, name: &str, min: usize, max: Option<usize>, f: SubrFn) {
        let subr = self.alloc(Obj::Subr { name: name.to_string(), min, max, f });
        self.set_function(name, subr);
    }
    pub fn is_bound(&self, v: &Value) -> bool {
        matches!(self.obj(v), Some(Obj::Symbol(s)) if s.value.is_some())
    }
    pub fn is_fbound(&self, v: &Value) -> bool {
        matches!(self.obj(v), Some(Obj::Symbol(s)) if s.function.is_some())
    }

    // ── dynamic binding ──
    pub fn specdepth(&self) -> usize {
        self.specstack.len()
    }
    pub fn specbind(&mut self, sym: &Value, val: Value) -> Result<(), String> {
        let id = self.sym_handle(sym).ok_or("cannot bind a non-symbol")?;
        let old = if let Obj::Symbol(s) = &self.arena[id as usize] { s.value.clone() } else { None };
        self.specstack.push((id, old));
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.value = Some(val);
        }
        Ok(())
    }
    pub fn unbind_to(&mut self, depth: usize) {
        while self.specstack.len() > depth {
            let (id, old) = self.specstack.pop().unwrap();
            if let Obj::Symbol(s) = &mut self.arena[id as usize] {
                s.value = old;
            }
        }
    }
    /// Bind a closure's params to args (dynamic). Returns the pre-bind depth.
    pub fn bind_params(&mut self, params: &Params, args: &[Value]) -> Result<usize, String> {
        if args.len() < params.required.len() {
            return Err("wrong-number-of-arguments".to_string());
        }
        let max = params.required.len() + params.optional.len();
        if params.rest.is_none() && args.len() > max {
            return Err("wrong-number-of-arguments".to_string());
        }
        let depth = self.specstack.len();
        let mut i = 0;
        for &h in &params.required {
            self.specbind(&Value::Obj(h), args[i].clone())?;
            i += 1;
        }
        for &h in &params.optional {
            let v = args.get(i).cloned().unwrap_or(Value::Undef);
            self.specbind(&Value::Obj(h), v)?;
            i += 1;
        }
        if let Some(h) = params.rest {
            let rest = args.get(i..).map(|s| s.to_vec()).unwrap_or_default();
            let lst = self.list_from(rest);
            self.specbind(&Value::Obj(h), lst)?;
        }
        Ok(depth)
    }

    /// Parse a lambda list form into structured params (interning the symbols).
    pub fn parse_params(&mut self, arglist: &Value) -> Result<Params, String> {
        let items = self.list_vec(arglist).ok_or("malformed lambda list")?;
        let mut p = Params { required: vec![], optional: vec![], rest: None };
        let mut mode = 0u8;
        for it in items {
            let id = self.sym_handle(&it).ok_or("lambda list: expected symbol")?;
            let name = self.sym_name(&it).unwrap_or_default();
            match name.as_str() {
                "&optional" => mode = 1,
                "&rest" => mode = 2,
                _ => match mode {
                    0 => p.required.push(id),
                    1 => p.optional.push(id),
                    _ => p.rest = Some(id),
                },
            }
        }
        Ok(p)
    }

    /// Resolve a function designator (symbol → function cell, following aliases;
    /// or a literal closure/subr object).
    pub fn resolve_function(&self, f: &Value) -> Result<Resolved, String> {
        let mut cur = f.clone();
        for _ in 0..64 {
            match self.obj(&cur) {
                Some(Obj::Subr { f, min, max, name }) => {
                    return Ok(Resolved::Subr { f: *f, min: *min, max: *max, name: name.clone() })
                }
                Some(Obj::Closure { params, body, is_macro }) => {
                    return Ok(Resolved::Closure {
                        params: params.clone(),
                        body: body.clone(),
                        is_macro: *is_macro,
                    })
                }
                Some(Obj::Symbol(s)) => match &s.function {
                    Some(def) => cur = def.clone(),
                    None => return Err(format!("void-function: {}", s.name)),
                },
                _ => return Err("invalid-function".to_string()),
            }
        }
        Err("function indirection too deep".to_string())
    }

    // ── printing ──
    pub fn print(&self, v: &Value, readable: bool) -> String {
        match v {
            Value::Undef => "nil".to_string(),
            Value::Bool(true) => "t".to_string(),
            Value::Bool(false) => "nil".to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            Value::Str(s) => {
                if readable {
                    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                } else {
                    s.to_string()
                }
            }
            Value::Obj(id) => match self.arena.get(*id as usize) {
                Some(Obj::Symbol(s)) => s.name.clone(),
                Some(Obj::Cons(..)) => self.print_list(v, readable),
                Some(Obj::Vector(items)) => {
                    let parts: Vec<String> =
                        items.iter().map(|e| self.print(e, readable)).collect();
                    format!("[{}]", parts.join(" "))
                }
                Some(Obj::Subr { name, .. }) => format!("#<subr {name}>"),
                Some(Obj::Closure { is_macro, .. }) => {
                    if *is_macro { "#<macro>".to_string() } else { "#<closure>".to_string() }
                }
                None => "#<dangling>".to_string(),
            },
            other => other.as_str_cow().into_owned(),
        }
    }
    fn print_list(&self, v: &Value, readable: bool) -> String {
        let mut out = String::from("(");
        let mut cur = v.clone();
        let mut first = true;
        loop {
            match self.obj(&cur) {
                Some(Obj::Cons(a, d)) => {
                    if !first {
                        out.push(' ');
                    }
                    first = false;
                    out.push_str(&self.print(a, readable));
                    let next = d.clone();
                    match next {
                        Value::Undef => break,
                        Value::Obj(id)
                            if matches!(self.arena.get(id as usize), Some(Obj::Cons(..))) =>
                        {
                            cur = next;
                        }
                        _ => {
                            out.push_str(" . ");
                            out.push_str(&self.print(&next, readable));
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
        out.push(')');
        out
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

/// elisp truthiness: only `nil` (fusevm `Undef`) is false.
pub fn el_truthy(v: &Value) -> bool {
    !matches!(v, Value::Undef | Value::Bool(false))
}

// ── thread-local host ────────────────────────────────────────────────────────

thread_local! {
    static HOST: RefCell<ElispHost> = RefCell::new(ElispHost::new());
    static PRELUDE_LOADED: Cell<bool> = const { Cell::new(false) };
}

pub fn with_host<R>(f: impl FnOnce(&mut ElispHost) -> R) -> R {
    HOST.with(|h| f(&mut h.borrow_mut()))
}
pub fn reset_host() {
    HOST.with(|h| *h.borrow_mut() = ElispHost::new());
    PRELUDE_LOADED.with(|c| c.set(false));
}
pub fn prelude_loaded() -> bool {
    PRELUDE_LOADED.with(|c| c.get())
}
pub fn set_prelude_loaded(b: bool) {
    PRELUDE_LOADED.with(|c| c.set(b));
}

/// Call a function designator with already-evaluated args. The single
/// re-entrant entry point: it never holds the host borrow across a callee, so a
/// closure body (run on a nested fusevm VM) can re-borrow the host freely.
pub fn call_function(f: &Value, args: &[Value]) -> Result<Value, String> {
    // Higher-order primitives are intercepted here so they don't run inside a
    // host borrow (which would deadlock the nested call).
    if let Some(name) = with_host(|h| h.sym_name(f)) {
        match name.as_str() {
            "funcall" => return call_function(&args[0], &args[1..]),
            "apply" => {
                let mut a = args[1..args.len().saturating_sub(1)].to_vec();
                if let Some(last) = args.last() {
                    let tail = with_host(|h| h.list_vec(last)).ok_or("apply: not a list")?;
                    a.extend(tail);
                }
                return call_function(&args[0], &a);
            }
            "mapcar" => {
                let seq = with_host(|h| h.list_vec(&args[1])).ok_or("mapcar: not a list")?;
                let mut out = Vec::with_capacity(seq.len());
                for e in seq {
                    out.push(call_function(&args[0], &[e])?);
                }
                return Ok(with_host(|h| h.list_from(out)));
            }
            "mapc" => {
                let seq = with_host(|h| h.list_vec(&args[1])).ok_or("mapc: not a list")?;
                for e in seq {
                    call_function(&args[0], &[e])?;
                }
                return Ok(args[1].clone());
            }
            _ => {}
        }
    }

    let resolved = with_host(|h| h.resolve_function(f))?;
    match resolved {
        Resolved::Subr { f, min, max, name } => {
            if args.len() < min || max.is_some_and(|m| args.len() > m) {
                return Err(format!("wrong-number-of-arguments: {name}"));
            }
            with_host(|h| f(h, args))
        }
        Resolved::Closure { params, body, is_macro } => {
            if is_macro {
                return Err("macro called as a function (use it in a macro position)".to_string());
            }
            run_closure(&params, &body, args)
        }
    }
}

/// Bind a closure's params to `args`, run its body on a nested fusevm VM, unwind.
/// Used by both function application and macro expansion (where `args` are the
/// unevaluated argument forms). Holds no host borrow across the nested run.
fn run_closure(params: &Rc<Params>, body: &Rc<Chunk>, args: &[Value]) -> Result<Value, String> {
    let depth = with_host(|h| h.bind_params(params, args))?;
    let result = run_chunk((**body).clone());
    with_host(|h| h.unbind_to(depth));
    result
}

/// One step of macro expansion: if `form` is `(macro-name . arg-forms)`, run the
/// macro on the *unevaluated* arg forms and return the expansion. Else `None`.
pub fn macroexpand_1(form: &Value) -> Result<Option<Value>, String> {
    let info = with_host(|h| {
        let elems = h.list_vec(form)?;
        if elems.is_empty() {
            return None;
        }
        match h.resolve_function(&elems[0]) {
            Ok(Resolved::Closure { params, body, is_macro: true }) => {
                Some((params, body, elems[1..].to_vec()))
            }
            _ => None,
        }
    });
    match info {
        Some((params, body, args)) => Ok(Some(run_closure(&params, &body, &args)?)),
        None => Ok(None),
    }
}

/// Fully expand macros in `form` (top-level to fixpoint, then recursively into
/// sub-forms), without descending into quoted data. Run before lowering.
pub fn macroexpand_all(form: &Value) -> Result<Value, String> {
    let mut f = form.clone();
    while let Some(e) = macroexpand_1(&f)? {
        f = e;
    }
    let elems = with_host(|h| {
        if matches!(h.obj(&f), Some(Obj::Cons(..))) {
            h.list_vec(&f)
        } else {
            None
        }
    });
    let Some(elems) = elems else { return Ok(f) };
    if elems.is_empty() {
        return Ok(f);
    }
    if with_host(|h| h.sym_name(&elems[0])).as_deref() == Some("quote") {
        return Ok(f);
    }
    let mut out = Vec::with_capacity(elems.len());
    for e in &elems {
        out.push(macroexpand_all(e)?);
    }
    Ok(with_host(|h| h.list_from(out)))
}

/// fusevm extension handler. Non-capturing (satisfies `Send`); reaches the heap
/// through the thread-local host.
pub fn ext_dispatch(vm: &mut VM, id: u16, arg: u8) {
    match id {
        ops::TRUTHY => {
            let v = vm.pop();
            vm.push(Value::Bool(el_truthy(&v)));
        }
        ops::CALL => {
            let argc = arg as usize;
            let mut args = Vec::with_capacity(argc);
            for _ in 0..argc {
                args.push(vm.pop());
            }
            args.reverse();
            let symv = vm.pop();
            match call_function(&symv, &args) {
                Ok(v) => vm.push(v),
                Err(e) => {
                    with_host(|h| h.error = Some(e));
                    vm.push(Value::Undef);
                }
            }
        }
        ops::GETVAR => {
            let symv = vm.pop();
            match with_host(|h| h.get_value(&symv)) {
                Ok(v) => vm.push(v),
                Err(e) => {
                    with_host(|h| h.error = Some(e));
                    vm.push(Value::Undef);
                }
            }
        }
        ops::SETVAR => {
            let val = vm.pop();
            let symv = vm.pop();
            let _ = with_host(|h| h.set_value(&symv, val.clone()));
            vm.push(val);
        }
        ops::FSET => {
            let def = vm.pop();
            let symv = vm.pop();
            let _ = with_host(|h| h.set_function_value(&symv, def));
            vm.push(symv);
        }
        ops::SPECBIND => {
            let symv = vm.pop();
            let val = vm.pop();
            if let Err(e) = with_host(|h| h.specbind(&symv, val)) {
                with_host(|h| h.error = Some(e));
            }
        }
        _ => {}
    }
}

/// Wide extension handler — for ops with a usize payload (LETBIND/UNBIND counts).
pub fn ext_dispatch_wide(vm: &mut VM, id: u16, n: usize) {
    match id {
        ops::LETBIND => {
            // stack: val1,sym1,...,valn,symn  (symn on top)
            let mut pairs = Vec::with_capacity(n);
            for _ in 0..n {
                let sym = vm.pop();
                let val = vm.pop();
                pairs.push((sym, val));
            }
            with_host(|h| {
                for (sym, val) in pairs.into_iter().rev() {
                    let _ = h.specbind(&sym, val);
                }
            });
        }
        ops::UNBIND => {
            with_host(|h| {
                let target = h.specdepth().saturating_sub(n);
                h.unbind_to(target);
            });
        }
        _ => {}
    }
}

/// Run a compiled chunk on a fresh fusevm VM, returning the elisp result.
pub fn run_chunk(chunk: Chunk) -> Result<Value, String> {
    with_host(|h| h.error = None);
    let mut vm = VM::new(chunk);
    vm.set_extension_handler(Box::new(ext_dispatch));
    vm.set_extension_wide_handler(Box::new(ext_dispatch_wide));
    let outcome = vm.run();
    if let Some(e) = with_host(|h| h.take_error()) {
        return Err(e);
    }
    match outcome {
        VMResult::Ok(v) => Ok(v),
        VMResult::Halted => Ok(vm.stack.last().cloned().unwrap_or(Value::Undef)),
        VMResult::Error(e) => Err(e),
    }
}
