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

use fusevm::{Chunk, VMResult, Value, VM};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Sentinel prefix marking the AOT heap image stashed in `chunk.names`.
pub const HEAP_IMAGE_TAG: &str = "\u{0}ELHEAP\u{0}";

/// A serializable mirror of a heap object — everything except `Subr` (a native
/// fn pointer, re-installed by `install`). Used to ship the user/prelude heap
/// into an AOT object so `Value::Obj` handles resolve in the AOT-runtime host.
#[derive(Serialize, Deserialize)]
pub enum SerObj {
    Cons(Value, Value),
    Symbol {
        name: String,
        value: Option<Value>,
        function: Option<Value>,
        special: bool,
    },
    Vector(Vec<Value>),
    HashTable {
        test: u8,
        entries: Vec<(Value, Value)>,
    },
    Closure {
        required: Vec<u32>,
        optional: Vec<u32>,
        rest: Option<u32>,
        body: Chunk,
        is_macro: bool,
    },
}

/// Extension-op IDs emitted by the compiler and dispatched here.
pub mod ops {
    pub const TRUTHY: u16 = 0; // pop v; push Bool(elisp-truthy(v))
    pub const CALL: u16 = 1; // arg=argc; stack [sym, args...] -> result
    pub const GETVAR: u16 = 2; // pop sym; push value cell
    pub const SETVAR: u16 = 3; // pop val, pop sym; set value cell; push val
    pub const FSET: u16 = 4; // pop def, pop sym; set function cell; push sym
    pub const SPECBIND: u16 = 5; // pop sym, pop val; bind into current scope (BIND1)
    pub const LETBIND: u16 = 6; // wide n: open scope; pop n (val,sym) pairs; bind all
    pub const UNBIND: u16 = 7; // wide: close the innermost scope (keep stack value)
    pub const SCOPE_OPEN: u16 = 8; // open an empty lexical scope (for let*)
    pub const MAKE_CLOSURE: u16 = 9; // pop a closure template; push one capturing the env
}

pub type SubrFn = fn(&mut ElispHost, &[Value]) -> Result<Value, String>;

/// A parsed lambda list (symbol handles).
pub struct Params {
    pub required: Vec<u32>,
    pub optional: Vec<u32>,
    pub rest: Option<u32>,
}

/// A lexical scope frame: symbol→value bindings plus a parent link. Closures
/// capture the scope active at their definition (indefinite extent). Mutation
/// (via interior `RefCell`) lets `setq` update a lexical slot in place.
pub struct Scope {
    vars: RefCell<Vec<(u32, Value)>>,
    parent: Lex,
}
pub type Lex = Option<Rc<Scope>>;

impl Scope {
    fn child(parent: Lex) -> Rc<Scope> {
        Rc::new(Scope {
            vars: RefCell::new(Vec::new()),
            parent,
        })
    }
    fn lookup(self: &Rc<Scope>, sym: u32) -> Option<Value> {
        let mut cur = Some(self.clone());
        while let Some(s) = cur {
            for (k, v) in s.vars.borrow().iter() {
                if *k == sym {
                    return Some(v.clone());
                }
            }
            cur = s.parent.clone();
        }
        None
    }
    fn set(self: &Rc<Scope>, sym: u32, val: &Value) -> bool {
        let mut cur = Some(self.clone());
        while let Some(s) = cur {
            for (k, v) in s.vars.borrow_mut().iter_mut() {
                if *k == sym {
                    *v = val.clone();
                    return true;
                }
            }
            cur = s.parent.clone();
        }
        false
    }
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
    Subr {
        name: String,
        min: usize,
        max: Option<usize>,
        f: SubrFn,
    },
    Closure {
        params: Rc<Params>,
        body: Rc<Chunk>,
        is_macro: bool,
        /// Captured lexical environment (`None` for a template / dynamic macro).
        env: Lex,
    },
    /// An elisp hash table. `test`: 0 = eq, 1 = eql, 2 = equal. Association-vector
    /// storage (linear scan) — fine for the table sizes elisp config uses.
    HashTable {
        test: u8,
        entries: Vec<(Value, Value)>,
    },
}

/// Resolution of a function designator to something callable.
pub enum Resolved {
    Subr {
        f: SubrFn,
        min: usize,
        max: Option<usize>,
        name: String,
    },
    Closure {
        params: Rc<Params>,
        body: Rc<Chunk>,
        is_macro: bool,
        env: Lex,
    },
}

pub struct ElispHost {
    pub(crate) arena: Vec<Obj>,
    obarray: HashMap<String, u32>,
    /// Arena length right after `install` (the builtin objects). Everything at or
    /// above this index is user/prelude data — the portion serialized for AOT.
    builtin_count: usize,
    /// Dynamic-binding save stack: (symbol handle, previous value cell).
    specstack: Vec<(u32, Option<Value>)>,
    /// Current lexical environment (the chain of `let`/closure frames).
    lex: Lex,
    /// Per-scope unwind info: (saved lexical env, specstack depth at entry).
    frame_stack: Vec<(Lex, usize)>,
    pub(crate) error: Option<String>,
    /// A pending `throw`: (tag, value). Set by `throw`, consumed by `catch`.
    /// Distinguishes a non-local `throw` from an ordinary error during unwinding.
    pub(crate) pending_throw: Option<(Value, Value)>,
    /// Regexp match data from the last successful `string-match`: the subject
    /// string plus char-position spans for the whole match (group 0) and each
    /// capture group. `match-beginning`/`match-end`/`match-string` read it.
    pub(crate) match_data: Option<MatchData>,
}

/// Result of the most recent `string-match`, in *character* positions (elisp
/// indexes strings by character, not byte).
#[derive(Clone, Debug)]
pub struct MatchData {
    pub subject: String,
    /// `spans[0]` is the whole match; `spans[n]` is capture group `n`. A group
    /// that did not participate is `None`.
    pub spans: Vec<Option<(usize, usize)>>,
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
            builtin_count: 0,
            specstack: Vec::new(),
            lex: None,
            frame_stack: Vec::new(),
            error: None,
            pending_throw: None,
            match_data: None,
        };
        crate::builtins::install(&mut h);
        h.builtin_count = h.arena.len();
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
    /// Allocate a fresh *uninterned* symbol: it carries `name` but is not put in
    /// the obarray, so each call yields a distinct object (`make-symbol`).
    pub fn make_symbol(&mut self, name: &str) -> Value {
        self.alloc(Obj::Symbol(SymbolData {
            name: name.to_string(),
            value: None,
            function: None,
            special: false,
        }))
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
        // A lexical binding shadows the value cell.
        if self.lex.as_ref().is_some_and(|s| s.set(id, &val)) {
            return Ok(());
        }
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.value = Some(val);
        }
        Ok(())
    }
    pub fn get_value(&self, v: &Value) -> Result<Value, String> {
        if let Some(id) = self.sym_handle(v) {
            if let Some(val) = self.lex.as_ref().and_then(|s| s.lookup(id)) {
                return Ok(val);
            }
        }
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
    /// Mark a symbol special (dynamically scoped) — used by `defvar`/`defconst`.
    pub fn set_special(&mut self, v: &Value) {
        if let Some(id) = self.sym_handle(v) {
            if let Obj::Symbol(s) = &mut self.arena[id as usize] {
                s.special = true;
            }
        }
    }
    fn is_special(&self, id: u32) -> bool {
        matches!(self.arena.get(id as usize), Some(Obj::Symbol(s)) if s.special)
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
    /// The symbol's function cell (what `symbol-function` returns), if any.
    pub fn function_cell(&self, sym: &Value) -> Option<Value> {
        match self.obj(sym) {
            Some(Obj::Symbol(s)) => s.function.clone(),
            _ => None,
        }
    }
    /// Look up an already-interned symbol by name without creating one
    /// (`intern-soft`); returns `None` if absent.
    pub fn find_symbol(&self, name: &str) -> Option<Value> {
        self.obarray.get(name).map(|&id| Value::Obj(id))
    }
    pub fn defsubr(&mut self, name: &str, min: usize, max: Option<usize>, f: SubrFn) {
        let subr = self.alloc(Obj::Subr {
            name: name.to_string(),
            min,
            max,
            f,
        });
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
        let old = if let Obj::Symbol(s) = &self.arena[id as usize] {
            s.value.clone()
        } else {
            None
        };
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
    // ── lexical scope management ──
    /// Push a new lexical scope as a child of the current one.
    pub fn open_scope(&mut self) {
        self.frame_stack
            .push((self.lex.clone(), self.specstack.len()));
        self.lex = Some(Scope::child(self.lex.clone()));
    }
    /// Push a new lexical scope as a child of `env` (a closure's captured env).
    pub fn open_scope_in(&mut self, env: Lex) {
        self.frame_stack
            .push((self.lex.clone(), self.specstack.len()));
        self.lex = Some(Scope::child(env));
    }
    /// Pop the innermost scope: restore the prior lexical env and unwind any
    /// dynamic (special-var) bindings made within it.
    pub fn close_scope(&mut self) {
        if let Some((saved, depth)) = self.frame_stack.pop() {
            self.unbind_to(depth);
            self.lex = saved;
        }
    }
    /// Bind `id` to `val` in the current scope — lexically, unless the symbol is
    /// special (`defvar`'d), in which case dynamically (saved on the specstack).
    pub fn bind_here(&mut self, id: u32, val: Value) {
        if self.is_special(id) {
            let _ = self.specbind(&Value::Obj(id), val);
        } else if let Some(scope) = &self.lex {
            scope.vars.borrow_mut().push((id, val));
        } else {
            if let Obj::Symbol(s) = &mut self.arena[id as usize] {
                s.value = Some(val);
            }
        }
    }
    /// Bind a symbol value into the current scope (lexical/dynamic per special).
    pub fn bind_value(&mut self, symv: &Value, val: Value) {
        if let Some(id) = self.sym_handle(symv) {
            self.bind_here(id, val);
        }
    }
    /// Instantiate a closure from a compile-time template, capturing the current
    /// lexical environment. Templates are stored with `env: None`.
    pub fn instantiate_closure(&mut self, template: &Value) -> Value {
        if let Some(Obj::Closure {
            params,
            body,
            is_macro,
            ..
        }) = self.obj(template)
        {
            let (params, body, is_macro) = (params.clone(), body.clone(), *is_macro);
            let env = self.lex.clone();
            return self.alloc(Obj::Closure {
                params,
                body,
                is_macro,
                env,
            });
        }
        template.clone()
    }
    // ── AOT heap image ──
    /// Serialize the user/prelude heap (arena ≥ `builtin_count`) for embedding
    /// into an AOT object. Builtins are excluded — they are re-created by
    /// `install` in the AOT-runtime host, at the same handles.
    pub fn export_heap_image(&self) -> Vec<SerObj> {
        self.arena[self.builtin_count..]
            .iter()
            .map(|o| match o {
                Obj::Cons(a, b) => SerObj::Cons(a.clone(), b.clone()),
                Obj::Symbol(s) => SerObj::Symbol {
                    name: s.name.clone(),
                    value: s.value.clone(),
                    function: s.function.clone(),
                    special: s.special,
                },
                Obj::Vector(v) => SerObj::Vector(v.clone()),
                Obj::HashTable { test, entries } => SerObj::HashTable {
                    test: *test,
                    entries: entries.clone(),
                },
                Obj::Closure {
                    params,
                    body,
                    is_macro,
                    ..
                } => SerObj::Closure {
                    required: params.required.clone(),
                    optional: params.optional.clone(),
                    rest: params.rest,
                    body: (**body).clone(),
                    is_macro: *is_macro,
                },
                // No Subr ever lives in the user range (only `install` makes them).
                Obj::Subr { .. } => SerObj::Symbol {
                    name: "--unexpected-subr--".to_string(),
                    value: None,
                    function: None,
                    special: false,
                },
            })
            .collect()
    }
    pub fn builtin_count(&self) -> usize {
        self.builtin_count
    }
    /// A fingerprint of the builtin object layout: the ordered names of every
    /// interned builtin symbol. Compiled chunks bake in builtin arena handles, so
    /// adding / removing / reordering subrs must invalidate the on-disk bytecode
    /// cache; folding this into the cache key makes that automatic (see
    /// `cache::schema_key`).
    pub fn builtin_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.builtin_count.hash(&mut hasher);
        for obj in &self.arena[..self.builtin_count] {
            if let Obj::Symbol(s) = obj {
                s.name.hash(&mut hasher);
            }
        }
        hasher.finish()
    }
    /// True if `name`'s function cell still holds its original primitive subr
    /// (not redefined by the user). The compiler only lowers `+`/`<`/… to native
    /// fusevm ops when this holds, so a user `(defun + …)` keeps host semantics.
    pub fn is_primitive_fn(&self, name: &str) -> bool {
        self.obarray
            .get(name)
            .and_then(|&id| self.arena.get(id as usize))
            .and_then(|o| match o {
                Obj::Symbol(s) => s.function.clone(),
                _ => None,
            })
            .map(|f| matches!(self.obj(&f), Some(Obj::Subr { .. })))
            .unwrap_or(false)
    }
    pub fn arena_len(&self) -> usize {
        self.arena.len()
    }
    /// Snapshot the value cells of symbols in `[start, end)` (used to capture the
    /// post-prelude baseline before running a user script for the cache).
    pub fn snapshot_values(&self, start: usize, end: usize) -> Vec<Option<Value>> {
        (start..end)
            .map(|i| match self.arena.get(i) {
                Some(Obj::Symbol(s)) => s.value.clone(),
                _ => None,
            })
            .collect()
    }
    /// Like `export_heap_image`, but reset symbol value cells to a clean baseline
    /// so re-running cached chunks reproduces the original execution exactly
    /// (no double-applied global mutations). Symbols below `prelude_end` get
    /// their `baseline` value; user symbols (≥ prelude_end) reset to unbound.
    pub fn export_heap_image_clean(
        &self,
        prelude_end: usize,
        baseline: &[Option<Value>],
    ) -> Vec<SerObj> {
        self.arena[self.builtin_count..]
            .iter()
            .enumerate()
            .map(|(off, o)| {
                let idx = self.builtin_count + off;
                match o {
                    Obj::Symbol(s) => {
                        let value = if idx < prelude_end {
                            baseline.get(idx - self.builtin_count).cloned().flatten()
                        } else {
                            None
                        };
                        SerObj::Symbol {
                            name: s.name.clone(),
                            value,
                            function: s.function.clone(),
                            special: s.special,
                        }
                    }
                    Obj::Cons(a, b) => SerObj::Cons(a.clone(), b.clone()),
                    Obj::Vector(v) => SerObj::Vector(v.clone()),
                    Obj::HashTable { test, entries } => SerObj::HashTable {
                        test: *test,
                        entries: entries.clone(),
                    },
                    Obj::Closure {
                        params,
                        body,
                        is_macro,
                        ..
                    } => SerObj::Closure {
                        required: params.required.clone(),
                        optional: params.optional.clone(),
                        rest: params.rest,
                        body: (**body).clone(),
                        is_macro: *is_macro,
                    },
                    Obj::Subr { .. } => SerObj::Symbol {
                        name: "--unexpected-subr--".to_string(),
                        value: None,
                        function: None,
                        special: false,
                    },
                }
            })
            .collect()
    }
    /// Rebuild the user/prelude heap from an image. Must be called on a fresh
    /// host (arena == builtins only) so handles line up with compile time.
    pub fn import_heap_image(&mut self, image: Vec<SerObj>) {
        for ser in image {
            let id = self.arena.len() as u32;
            let obj = match ser {
                SerObj::Cons(a, b) => Obj::Cons(a, b),
                SerObj::Symbol {
                    name,
                    value,
                    function,
                    special,
                } => {
                    self.obarray.insert(name.clone(), id);
                    Obj::Symbol(SymbolData {
                        name,
                        value,
                        function,
                        special,
                    })
                }
                SerObj::Vector(v) => Obj::Vector(v),
                SerObj::HashTable { test, entries } => Obj::HashTable { test, entries },
                SerObj::Closure {
                    required,
                    optional,
                    rest,
                    body,
                    is_macro,
                } => Obj::Closure {
                    params: Rc::new(Params {
                        required,
                        optional,
                        rest,
                    }),
                    body: Rc::new(body),
                    is_macro,
                    env: None,
                },
            };
            self.arena.push(obj);
        }
    }
    /// Bind a closure's params into the already-open current scope.
    pub fn bind_params_into_scope(
        &mut self,
        params: &Params,
        args: &[Value],
    ) -> Result<(), String> {
        if args.len() < params.required.len() {
            return Err("wrong-number-of-arguments".to_string());
        }
        let max = params.required.len() + params.optional.len();
        if params.rest.is_none() && args.len() > max {
            return Err("wrong-number-of-arguments".to_string());
        }
        let mut i = 0;
        for &id in &params.required {
            self.bind_here(id, args[i].clone());
            i += 1;
        }
        for &id in &params.optional {
            let v = args.get(i).cloned().unwrap_or(Value::Undef);
            self.bind_here(id, v);
            i += 1;
        }
        if let Some(id) = params.rest {
            let rest = args.get(i..).map(|s| s.to_vec()).unwrap_or_default();
            let lst = self.list_from(rest);
            self.bind_here(id, lst);
        }
        Ok(())
    }

    /// Parse a lambda list form into structured params (interning the symbols).
    pub fn parse_params(&mut self, arglist: &Value) -> Result<Params, String> {
        let items = self.list_vec(arglist).ok_or("malformed lambda list")?;
        let mut p = Params {
            required: vec![],
            optional: vec![],
            rest: None,
        };
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
                    return Ok(Resolved::Subr {
                        f: *f,
                        min: *min,
                        max: *max,
                        name: name.clone(),
                    })
                }
                Some(Obj::Closure {
                    params,
                    body,
                    is_macro,
                    env,
                }) => {
                    return Ok(Resolved::Closure {
                        params: params.clone(),
                        body: body.clone(),
                        is_macro: *is_macro,
                        env: env.clone(),
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
                // Emacs's read syntax for the non-finite floats.
                if f.is_nan() {
                    if f.is_sign_negative() {
                        "-0.0e+NaN"
                    } else {
                        "0.0e+NaN"
                    }
                    .to_string()
                } else if f.is_infinite() {
                    if *f < 0.0 { "-1.0e+INF" } else { "1.0e+INF" }.to_string()
                } else if f.fract() == 0.0 {
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
                    if *is_macro {
                        "#<macro>".to_string()
                    } else {
                        "#<closure>".to_string()
                    }
                }
                Some(Obj::HashTable { entries, .. }) => {
                    format!("#s(hash-table size {})", entries.len())
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
        while let Some(Obj::Cons(a, d)) = self.obj(&cur) {
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
                    out.push_str(" . ");
                    out.push_str(&self.print(&next, readable));
                    break;
                }
            }
        }
        out.push(')');
        out
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// `eq`-style identity comparison (used for `catch`/`throw` tags).
    pub fn values_eq(&self, a: &Value, b: &Value) -> bool {
        if !el_truthy(a) && !el_truthy(b) {
            return true;
        }
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
            (Value::Obj(x), Value::Obj(y)) => x == y,
            (Value::Bool(true), Value::Bool(true)) => true,
            _ => false,
        }
    }

    /// Build the `(error-symbol "message")` object a `condition-case` handler
    /// binds its variable to, from a rendered "symbol: message" error string.
    pub fn make_error_object(&mut self, e: &str) -> Value {
        let (sym, msg) = match e.split_once(':') {
            Some((s, m)) => (s.trim().to_string(), m.trim().to_string()),
            None => ("error".to_string(), e.to_string()),
        };
        let s = self.intern(&sym);
        let m = Value::str(msg);
        self.list_from(vec![s, m])
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
            "sort" => {
                // (sort SEQ PRED): stable sort a list/vector by the less-than
                // predicate, which re-enters elisp — so it lives here, not as a
                // plain subr. Returns a freshly built sequence of the same kind.
                let pred = &args[1];
                let (mut items, was_vec) = with_host(|h| match h.obj(&args[0]) {
                    Some(Obj::Vector(v)) => (v.clone(), true),
                    _ => (h.list_vec(&args[0]).unwrap_or_default(), false),
                });
                merge_sort_by(&mut items, pred)?;
                return Ok(with_host(|h| {
                    if was_vec {
                        h.alloc(Obj::Vector(items))
                    } else {
                        h.list_from(items)
                    }
                }));
            }
            "maphash" => {
                let entries = with_host(|h| match h.obj(&args[1]) {
                    Some(Obj::HashTable { entries, .. }) => Some(entries.clone()),
                    _ => None,
                })
                .ok_or("maphash: not a hash table")?;
                for (k, v) in entries {
                    call_function(&args[0], &[k, v])?;
                }
                return Ok(Value::Undef);
            }
            // `replace-regexp-in-string` with a *function* REP must call that
            // function per match — VM re-entry — so it's handled here rather than
            // in the (host-borrowing) subr, which only does string templates.
            "replace-regexp-in-string" if args.len() >= 3 && !matches!(args[1], Value::Str(_)) => {
                return replace_regexp_with_fn(args);
            }
            // Nonlocal-exit intrinsics (the compiler rewrites catch/unwind-protect/
            // condition-case into these, passing lambda thunks).
            "--catch--" => return intrinsic_catch(args),
            "--unwind--" => return intrinsic_unwind(args),
            "--condition-case--" => return intrinsic_condition_case(args),
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
        Resolved::Closure {
            params,
            body,
            is_macro,
            env,
        } => {
            if is_macro {
                return Err("macro called as a function (use it in a macro position)".to_string());
            }
            run_closure(&params, &body, env, args)
        }
    }
}

/// `(replace-regexp-in-string REGEXP FUNC STRING …)` where FUNC is called on each
/// match's text and returns its replacement. Match data is set before each call
/// so the function can use `match-string`. Runs outside any host borrow.
fn replace_regexp_with_fn(args: &[Value]) -> Result<Value, String> {
    let pat = match &args[0] {
        Value::Str(s) => s.to_string(),
        _ => return Err("replace-regexp-in-string: regexp must be a string".to_string()),
    };
    let subject = match &args[2] {
        Value::Str(s) => s.to_string(),
        _ => return Err("replace-regexp-in-string: not a string".to_string()),
    };
    let repfn = args[1].clone();
    let cf = with_host(|h| crate::builtins::case_fold_search(h));
    let re = crate::builtins::compile_cf(&pat, cf)?;
    let mut out = String::with_capacity(subject.len());
    let mut last = 0usize;
    for caps in re.captures_iter(&subject) {
        let m = caps.get(0).unwrap();
        out.push_str(&subject[last..m.start()]);
        // Char-indexed match data so `match-string`/`match-beginning` work in FUNC.
        let spans: Vec<Option<(usize, usize)>> = (0..caps.len())
            .map(|i| {
                caps.get(i).map(|g| {
                    (
                        crate::builtins::char_of_byte(&subject, g.start()),
                        crate::builtins::char_of_byte(&subject, g.end()),
                    )
                })
            })
            .collect();
        let matched = Value::str(subject[m.start()..m.end()].to_string());
        with_host(|h| {
            h.match_data = Some(MatchData {
                subject: subject.clone(),
                spans,
            })
        });
        let r = call_function(&repfn, &[matched])?;
        match r {
            Value::Str(s) => out.push_str(&s),
            _ => return Err("replace-regexp-in-string: replacement must be a string".to_string()),
        }
        last = m.end();
        if m.start() == m.end() {
            if let Some(c) = subject[last..].chars().next() {
                out.push(c);
                last += c.len_utf8();
            }
        }
    }
    out.push_str(&subject[last.min(subject.len())..]);
    Ok(Value::str(out))
}

/// Stable merge sort driven by an elisp less-than predicate. `pred` is called as
/// `(pred a b)`; a non-nil result means `a` precedes `b`. Equal elements keep
/// their input order (the merge takes from the left run on ties).
fn merge_sort_by(items: &mut Vec<Value>, pred: &Value) -> Result<(), String> {
    let n = items.len();
    if n < 2 {
        return Ok(());
    }
    let mid = n / 2;
    let mut right = items.split_off(mid);
    merge_sort_by(items, pred)?;
    merge_sort_by(&mut right, pred)?;
    let left = std::mem::take(items);
    let (mut i, mut j) = (0, 0);
    items.reserve(left.len() + right.len());
    while i < left.len() && j < right.len() {
        // Take from the right only when right[j] strictly precedes left[i].
        let rhs_first = call_function(pred, &[right[j].clone(), left[i].clone()])?;
        if matches!(rhs_first, Value::Undef | Value::Bool(false)) {
            items.push(left[i].clone());
            i += 1;
        } else {
            items.push(right[j].clone());
            j += 1;
        }
    }
    items.extend_from_slice(&left[i..]);
    items.extend_from_slice(&right[j..]);
    Ok(())
}

/// Open a lexical scope (child of the closure's captured `env`), bind `args` to
/// the params, run the body on a nested fusevm VM, then close the scope. Used by
/// both function application and macro expansion (where `args` are the
/// unevaluated argument forms). Holds no host borrow across the nested run.
fn run_closure(
    params: &Rc<Params>,
    body: &Rc<Chunk>,
    env: Lex,
    args: &[Value],
) -> Result<Value, String> {
    let setup = with_host(|h| {
        h.open_scope_in(env.clone());
        h.bind_params_into_scope(params, args)
    });
    if let Err(e) = setup {
        with_host(|h| h.close_scope());
        return Err(e);
    }
    let result = run_chunk((**body).clone());
    with_host(|h| h.close_scope());
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
            Ok(Resolved::Closure {
                params,
                body,
                is_macro: true,
                env,
            }) => Some((params, body, env, elems[1..].to_vec())),
            _ => None,
        }
    });
    match info {
        Some((params, body, env, args)) => Ok(Some(run_closure(&params, &body, env, &args)?)),
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

/// `(catch TAG THUNK)` — run the thunk; if a `throw` to a matching tag unwinds
/// out of it, return the thrown value; otherwise re-propagate.
fn intrinsic_catch(args: &[Value]) -> Result<Value, String> {
    let tag = args.first().cloned().unwrap_or(Value::Undef);
    let thunk = args.get(1).cloned().unwrap_or(Value::Undef);
    match call_function(&thunk, &[]) {
        Ok(v) => Ok(v),
        Err(e) => {
            let pend = with_host(|h| h.pending_throw.clone());
            match pend {
                Some((ttag, tval)) if with_host(|h| h.values_eq(&ttag, &tag)) => {
                    with_host(|h| h.pending_throw = None);
                    Ok(tval)
                }
                _ => Err(e), // not our throw (or a real error): keep unwinding
            }
        }
    }
}

/// `(unwind-protect BODY-THUNK CLEANUP-THUNK)` — always run cleanup, preserving
/// an in-flight throw across it, then propagate the body's result.
fn intrinsic_unwind(args: &[Value]) -> Result<Value, String> {
    let body = args.first().cloned().unwrap_or(Value::Undef);
    let cleanup = args.get(1).cloned().unwrap_or(Value::Undef);
    let r = call_function(&body, &[]);
    let saved = with_host(|h| h.pending_throw.take());
    let _ = call_function(&cleanup, &[]);
    with_host(|h| {
        if h.pending_throw.is_none() {
            h.pending_throw = saved;
        }
    });
    r
}

/// `(condition-case VAR BODY-THUNK HANDLERS)` where HANDLERS is a list of
/// `(CONDITION HANDLER-THUNK)`. Catches *errors* (not throws); binds VAR to the
/// error object while the matching handler runs.
fn intrinsic_condition_case(args: &[Value]) -> Result<Value, String> {
    let var = args.first().cloned().unwrap_or(Value::Undef);
    let body = args.get(1).cloned().unwrap_or(Value::Undef);
    let handlers = args.get(2).cloned().unwrap_or(Value::Undef);
    match call_function(&body, &[]) {
        Ok(v) => Ok(v),
        Err(e) => {
            // A throw is not an error — let it keep unwinding to its catch.
            if with_host(|h| h.pending_throw.is_some()) {
                return Err(e);
            }
            let esym: String = e.split(':').next().unwrap_or("error").trim().to_string();
            let hlist = with_host(|h| h.list_vec(&handlers)).unwrap_or_default();
            for hp in hlist {
                let parts = with_host(|h| h.list_vec(&hp)).unwrap_or_default();
                if parts.len() < 2 {
                    continue;
                }
                let cname = with_host(|h| h.sym_name(&parts[0])).unwrap_or_default();
                if cname == "error" || cname == "t" || cname == esym {
                    let depth = with_host(|h| {
                        let d = h.specdepth();
                        if matches!(h.obj(&var), Some(Obj::Symbol(_))) {
                            let eobj = h.make_error_object(&e);
                            let _ = h.specbind(&var, eobj);
                        }
                        d
                    });
                    let hr = call_function(&parts[1], &[]);
                    with_host(|h| h.unbind_to(depth));
                    return hr;
                }
            }
            Err(e)
        }
    }
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
                Err(e) => abort(vm, e),
            }
        }
        ops::GETVAR => {
            let symv = vm.pop();
            match with_host(|h| h.get_value(&symv)) {
                Ok(v) => vm.push(v),
                Err(e) => abort(vm, e),
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
            // BIND1: bind into the current (already-open) scope; used by let*.
            let symv = vm.pop();
            let val = vm.pop();
            with_host(|h| h.bind_value(&symv, val));
        }
        ops::SCOPE_OPEN => {
            with_host(|h| h.open_scope());
        }
        ops::MAKE_CLOSURE => {
            let template = vm.pop();
            let clo = with_host(|h| h.instantiate_closure(&template));
            vm.push(clo);
        }
        _ => {}
    }
}

/// Wide extension handler — for ops with a usize payload (LETBIND/UNBIND counts).
pub fn ext_dispatch_wide(vm: &mut VM, id: u16, n: usize) {
    match id {
        ops::LETBIND => {
            // stack: val1,sym1,...,valn,symn  (symn on top). Inits were evaluated
            // in the outer scope; now open a fresh scope and bind them in parallel.
            let mut pairs = Vec::with_capacity(n);
            for _ in 0..n {
                let sym = vm.pop();
                let val = vm.pop();
                pairs.push((sym, val));
            }
            with_host(|h| {
                h.open_scope();
                for (sym, val) in pairs.into_iter().rev() {
                    h.bind_value(&sym, val);
                }
            });
        }
        ops::UNBIND => {
            let _ = n;
            with_host(|h| h.close_scope());
        }
        _ => {}
    }
}

/// Abort the running chunk: record the error and halt the VM immediately (so
/// code after a failing/throwing call does not run). The loop guard
/// `ip < ops.len()` makes this safe.
fn abort(vm: &mut VM, e: String) {
    with_host(|h| h.error = Some(e));
    vm.ip = vm.chunk.ops.len();
}

/// Run a compiled chunk on a fresh fusevm VM, returning the elisp result.
pub fn run_chunk(chunk: Chunk) -> Result<Value, String> {
    with_host(|h| h.error = None);
    let mut vm = VM::new(chunk);
    vm.set_extension_handler(Box::new(ext_dispatch));
    vm.set_extension_wide_handler(Box::new(ext_dispatch_wide));
    // Hot loops trace-compile through fusevm's Cranelift JIT; with the
    // `jit-disk-cache` feature, compiled native code is persisted across runs.
    vm.enable_tracing_jit();
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
