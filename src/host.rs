//! The ElispHost: the elisp object heap + primitive subrs, reached from
//! fusevm's extension handler. elisprs has no VM — fusevm executes the lowered
//! bytecode and calls back here for elisp-specific operations.
//!
//! Heap objects (cons/symbol/vector) live in an arena; they ride through the
//! fusevm value stack as `Value::Obj(handle)`. elisp `nil` is fusevm `Undef`,
//! elisp `t` is fusevm `Bool(true)`. The host is a `thread_local` because
//! fusevm's `ExtensionHandler` is `Send` and so cannot capture an `Rc` heap.

use fusevm::{Chunk, Value, VMResult, VM};
use std::cell::RefCell;
use std::collections::HashMap;

/// Extension-op IDs emitted by the compiler and dispatched here.
pub mod ops {
    pub const TRUTHY: u16 = 0; // pop v; push Bool(elisp-truthy(v))
    pub const CALL: u16 = 1; // arg=argc; stack: [sym, args...] -> result
    pub const GETVAR: u16 = 2; // pop sym; push its dynamic value
    pub const SETVAR: u16 = 3; // pop val, pop sym; set value cell; push val
}

pub type SubrFn = fn(&mut ElispHost, &[Value]) -> Result<Value, String>;

pub enum Func {
    Subr { name: String, min: usize, max: Option<usize>, f: SubrFn },
    // Closures/macros arrive with the calling-convention milestone.
}

pub struct SymbolData {
    pub name: String,
    pub value: Option<Value>,
    pub function: Option<Func>,
    pub special: bool,
}

pub enum Obj {
    Cons(Value, Value),
    Symbol(SymbolData),
    Vector(Vec<Value>),
}

pub struct ElispHost {
    pub(crate) arena: Vec<Obj>,
    obarray: HashMap<String, u32>,
    /// Dynamic-binding save stack: (symbol handle, previous value cell).
    #[allow(dead_code)]
    specstack: Vec<(u32, Option<Value>)>,
    pub(crate) error: Option<String>,
}

impl ElispHost {
    pub fn new() -> Self {
        let mut h = ElispHost {
            arena: Vec::new(),
            obarray: HashMap::new(),
            specstack: Vec::new(),
            error: None,
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

    // ── cons accessors ──
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
    /// Collect a proper list into a Vec (nil → empty).
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

    // ── symbol cells ──
    fn sym_handle(&mut self, v: &Value) -> Option<u32> {
        match v {
            Value::Obj(id) if matches!(self.arena.get(*id as usize), Some(Obj::Symbol(_))) => {
                Some(*id)
            }
            _ => None,
        }
    }
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
    pub fn set_function(&mut self, name: &str, f: Func) {
        let v = self.intern(name);
        if let Value::Obj(id) = v {
            if let Obj::Symbol(s) = &mut self.arena[id as usize] {
                s.function = Some(f);
            }
        }
    }
    pub fn defsubr(&mut self, name: &str, min: usize, max: Option<usize>, f: SubrFn) {
        self.set_function(name, Func::Subr { name: name.to_string(), min, max, f });
    }

    // ── application ──
    /// Apply the function bound to symbol value `symv` to `args`.
    pub fn apply(&mut self, symv: &Value, args: &[Value]) -> Result<Value, String> {
        // Extract a copyable view of the subr from the function cell.
        let (name, min, max, f) = match self.obj(symv) {
            Some(Obj::Symbol(s)) => match &s.function {
                Some(Func::Subr { name, min, max, f }) => (name.clone(), *min, *max, *f),
                None => return Err(format!("void-function: {}", s.name)),
            },
            _ => return Err("invalid-function".to_string()),
        };
        if args.len() < min || max.is_some_and(|m| args.len() > m) {
            return Err(format!("wrong-number-of-arguments: {name}"));
        }
        f(self, args)
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
                Some(Obj::Cons(_, _)) => self.print_list(v, readable),
                Some(Obj::Vector(items)) => {
                    let parts: Vec<String> =
                        items.iter().map(|e| self.print(e, readable)).collect();
                    format!("[{}]", parts.join(" "))
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
                        Value::Obj(id) if matches!(self.arena.get(id as usize), Some(Obj::Cons(..))) => {
                            cur = next;
                        }
                        _ => {
                            // dotted pair
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

impl Default for ElispHost {
    fn default() -> Self {
        Self::new()
    }
}

/// elisp truthiness: only `nil` (fusevm `Undef`) is false.
pub fn el_truthy(v: &Value) -> bool {
    !matches!(v, Value::Undef | Value::Bool(false))
}

// ── thread-local host ────────────────────────────────────────────────────────

thread_local! {
    static HOST: RefCell<ElispHost> = RefCell::new(ElispHost::new());
}

pub fn with_host<R>(f: impl FnOnce(&mut ElispHost) -> R) -> R {
    HOST.with(|h| f(&mut h.borrow_mut()))
}
pub fn reset_host() {
    HOST.with(|h| *h.borrow_mut() = ElispHost::new());
}

/// fusevm extension handler. Non-capturing (so it satisfies `Send`); reaches
/// the heap through the thread-local host.
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
            let r = with_host(|h| h.apply(&symv, &args));
            match r {
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
        _ => {}
    }
}

/// Run a compiled chunk on a fresh fusevm VM, returning the elisp result.
pub fn run_chunk(chunk: Chunk) -> Result<Value, String> {
    with_host(|h| h.error = None);
    let mut vm = VM::new(chunk);
    vm.set_extension_handler(Box::new(ext_dispatch));
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
