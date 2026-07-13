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
use std::sync::{Arc, Weak};

/// Sentinel prefix marking the AOT heap image stashed in `chunk.names`.
pub const HEAP_IMAGE_TAG: &str = "\u{0}ELHEAP\u{0}";

/// Largest valid character code (`(max-char)` in Emacs 30.2 = #x3FFFFF). Char
/// indices into a char-table run `0..=MAX_CHAR`.
pub const MAX_CHAR: u32 = 0x3F_FFFF;

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
        #[serde(default)]
        buffer_local_auto: bool,
        #[serde(default)]
        alias_of: Option<u32>,
    },
    Vector(Vec<Value>),
    HashTable {
        test: u8,
        entries: Vec<(Value, Value)>,
    },
    CharTable {
        subtype: Value,
        default: Value,
        parent: Value,
        extra: Vec<Value>,
        ranges: Vec<(u32, Value)>,
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

/// One dynamic (`let`) binding recorded on the specstack, restored by `unbind_to`.
enum SpecEntry {
    /// A binding of a symbol's global (default) value cell: (sym, previous value).
    Global(u32, Option<Value>),
    /// A binding of a buffer-local slot, matching Emacs `let` over a buffer-local
    /// variable: (sym, buffer index, previous local slot). The previous slot is
    /// `None` when no local existed (a temporary local created for the binding's
    /// extent) or `Some(prev)` when one did.
    Local(u32, usize, Option<Option<Value>>),
}

/// A parsed lambda list (symbol handles).
pub struct Params {
    pub required: Vec<u32>,
    pub optional: Vec<u32>,
    pub rest: Option<u32>,
}

/// One lexical binding: a `symbol → value` cell plus a link to the rest of the
/// environment. The environment is a persistent singly-linked list — each
/// binding conses a fresh node onto the front (matching Emacs's lexical
/// environment alist). A closure captures the current head (`Rc` clone); later
/// bindings cons *new* heads, so they are invisible to an already-captured
/// closure. `setq` mutates the found cell in place (via `RefCell`), so a
/// binding shared by a closure and its enclosing body updates for both.
pub struct Scope {
    sym: u32,
    val: RefCell<Value>,
    parent: Lex,
}
pub type Lex = Option<Rc<Scope>>;

impl Scope {
    fn lookup(self: &Rc<Scope>, sym: u32) -> Option<Value> {
        let mut cur = Some(self.clone());
        while let Some(s) = cur {
            // Head is the newest binding: the first match down the chain
            // shadows older same-name bindings (Emacs lexical `let*`).
            if s.sym == sym {
                return Some(s.val.borrow().clone());
            }
            cur = s.parent.clone();
        }
        None
    }
    fn set(self: &Rc<Scope>, sym: u32, val: &Value) -> bool {
        let mut cur = Some(self.clone());
        while let Some(s) = cur {
            // Newest binding wins (see `lookup`): `setq` updates the most
            // recently established cell for the symbol.
            if s.sym == sym {
                *s.val.borrow_mut() = val.clone();
                return true;
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
    /// Set by `make-variable-buffer-local`: any `set`/`setq` in a buffer that has
    /// no local binding yet automatically creates one (Emacs "automatically
    /// buffer-local"). Persisted in the AOT heap image so a cache hit keeps it.
    pub buffer_local_auto: bool,
    /// Set by `defvaralias`: this symbol is a variable alias forwarding all value
    /// operations to the base symbol at this arena handle (Emacs `SYMBOL_VARALIAS`).
    /// `None` for an ordinary variable. Chains are followed by `indirect_var`.
    pub alias_of: Option<u32>,
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
    /// An Emacs char-table (`make-char-table`). Maps char codes `0..=MAX_CHAR`
    /// to values, with a `subtype` symbol, a `default` slot, an optional `parent`
    /// char-table for lookup fallback, and `extra` slots. See [`CharTable`].
    CharTable(CharTable),
    /// An editing buffer object (`get-buffer-create`/`generate-new-buffer`). The
    /// payload is the index into `ElispHost::buffers`, which is stable for the
    /// buffer's whole lifetime (killed buffers keep their slot, marked dead by a
    /// `None` name). Buffer objects are runtime-only and never serialized.
    Buffer(usize),
    /// A general marker object (`make-marker`/`point-marker`/`copy-marker`). The
    /// payload is shared (`Rc<RefCell<..>>`) with the buffer's live-marker registry
    /// so a single edit updates every reference; see [`MarkerData`]. Runtime-only,
    /// never serialized.
    Marker(Rc<RefCell<MarkerData>>),
    /// A first-class obarray (`obarray-make`): a private namespace of interned
    /// symbols. Each name maps to a distinct symbol arena id created via
    /// [`ElispHost::make_symbol`], so a private obarray's symbols never collide
    /// with the global ones. The single global obarray (the value of the
    /// `obarray` variable) is the one with `global == true`; its symbol set is
    /// `ElispHost::obarray`, not `symbols`. See [`ObarrayData`].
    Obarray(ObarrayData),
}

/// An `Obj::Obarray` payload. A private obarray owns its `symbols` map
/// (name → symbol arena id); the global obarray (`global == true`) leaves
/// `symbols` empty and routes every operation to `ElispHost::obarray`.
pub struct ObarrayData {
    pub symbols: HashMap<String, u32>,
    pub global: bool,
}

/// The mutable state of an Emacs marker. A marker points into a buffer at a
/// 1-based `pos`; `buffer` is the buffer's slot index, or `None` for a detached
/// marker (`make-marker`, or `set-marker … nil`) whose `pos` is meaningless.
/// `insertion_type` t means text inserted exactly at the marker moves the marker
/// past it (nil = the marker stays before the inserted text). The `Rc<RefCell>`
/// is shared between the `Obj::Marker` and the owning buffer's `markers` list, so
/// [`ElispHost::cur_insert`]/[`ElispHost::cur_delete`] adjust every live marker.
pub struct MarkerData {
    pub buffer: Option<usize>,
    pub pos: usize,
    pub insertion_type: bool,
}

/// An Emacs char-table's payload. Per-char values use efficient range storage:
/// `ranges` is a sorted list of `(start, value)` breakpoints whose first entry
/// always starts at `0`; a breakpoint `(s, v)` means every char in `s..next_s`
/// maps to `v` (the last breakpoint runs through `MAX_CHAR`). Setting a whole
/// range is O(range-count), not O(chars), so `(set-char-table-range t …)` is cheap.
///
/// Lookup (`aref`, `char-table-range`) falls back like Emacs's `char_table_ref`:
/// own char value; if nil → `default`; if that is nil and `parent` is a char-table
/// → recurse into the parent.
pub struct CharTable {
    pub subtype: Value,
    pub default: Value,
    pub parent: Value,
    pub extra: Vec<Value>,
    pub ranges: Vec<(u32, Value)>,
}

/// Shallow `eq`-style equality for coalescing adjacent char-table breakpoints
/// (identical adjacent runs collapse to one entry). Mirrors [`ElispHost::values_eq`]
/// but is a free function usable while the arena is mutably borrowed.
fn ct_val_eq(a: &Value, b: &Value) -> bool {
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

impl CharTable {
    pub fn new(subtype: Value, init: Value, n_extra: usize) -> CharTable {
        CharTable {
            subtype,
            default: Value::Undef,
            parent: Value::Undef,
            extra: vec![Value::Undef; n_extra],
            ranges: vec![(0, init)],
        }
    }
    /// The raw value stored for char `c` in this table alone (no parent/default
    /// fallback): the value of the breakpoint that covers `c`.
    pub fn raw_get(&self, c: u32) -> Value {
        // `ranges` is sorted by start with ranges[0].0 == 0, so the covering
        // breakpoint is the last one whose start <= c.
        let idx = match self.ranges.binary_search_by(|(s, _)| s.cmp(&c)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        self.ranges[idx].1.clone()
    }
    /// Set every char in `from..=to` to `val`, splicing/coalescing breakpoints.
    pub fn set_range(&mut self, from: u32, to: u32, val: Value) {
        // The value covering the char just past the range (to restore after it).
        let after = if to < MAX_CHAR {
            Some(self.raw_get(to + 1))
        } else {
            None
        };
        // Drop breakpoints strictly inside (from, to].
        self.ranges.retain(|(s, _)| *s <= from || *s > to);
        Self::upsert(&mut self.ranges, from, val);
        if let Some(after) = after {
            Self::upsert(&mut self.ranges, to + 1, after);
        }
        // Coalesce adjacent equal-valued runs.
        let mut i = 1;
        while i < self.ranges.len() {
            if ct_val_eq(&self.ranges[i].1, &self.ranges[i - 1].1) {
                self.ranges.remove(i);
            } else {
                i += 1;
            }
        }
    }
    fn upsert(ranges: &mut Vec<(u32, Value)>, start: u32, val: Value) {
        match ranges.binary_search_by(|(s, _)| s.cmp(&start)) {
            Ok(i) => ranges[i].1 = val,
            Err(i) => ranges.insert(i, (start, val)),
        }
    }
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

/// print.c `PRINT_CIRCLE`: the max print nesting depth. With `print-circle`
/// nil, an object nested this deep signals "Apparently circular structure being
/// printed" instead of printing (Emacs errors at exactly this depth).
const PRINT_CIRCLE: usize = 200;

pub struct ElispHost {
    pub(crate) arena: Vec<Obj>,
    obarray: HashMap<String, u32>,
    /// Arena length right after `install` (the builtin objects). Everything at or
    /// above this index is user/prelude data — the portion serialized for AOT.
    builtin_count: usize,
    /// Dynamic-binding save stack. Each `let`/param binding of a special variable
    /// pushes one entry; `unbind_to` pops and restores.
    specstack: Vec<SpecEntry>,
    /// Current lexical environment (the chain of `let`/closure frames).
    lex: Lex,
    /// Per-scope unwind info: (saved lexical env, specstack depth at entry).
    frame_stack: Vec<(Lex, usize)>,
    pub(crate) error: Option<String>,
    /// A pending `throw`: (tag, value). Set by `throw`, consumed by `catch`.
    /// Distinguishes a non-local `throw` from an ordinary error during unwinding.
    pub(crate) pending_throw: Option<(Value, Value)>,
    /// Tags of the `catch` frames currently active, so `throw` can detect when no
    /// matching catch exists and signal `no-catch` (like Emacs) instead of leaking.
    pub(crate) catch_tags: Vec<Value>,
    /// The structured error object `(ERROR-SYMBOL . DATA)` from the most recent
    /// `signal`/`error`, so `condition-case` can bind the handler variable to the
    /// real list (not a re-parsed string). Cleared when entering a c-c body.
    pub(crate) pending_error: Option<Value>,
    /// Regexp match data from the last successful `string-match`: the subject
    /// string plus char-position spans for the whole match (group 0) and each
    /// capture group. `match-beginning`/`match-end`/`match-string` read it.
    pub(crate) match_data: Option<MatchData>,
    /// Output-capture stack for `with-output-to-string`: when non-empty,
    /// `princ`/`prin1`/`print`/`terpri` append to the top buffer instead of stdout.
    pub(crate) output_capture: Vec<String>,
    /// Set by `print_inner` when nesting reaches `PRINT_CIRCLE`; the print entry
    /// points (`prin1`/`print`/`princ`/`format`) read it to signal Emacs's
    /// `error "Apparently circular structure being printed"`. `Cell` so the
    /// `&self` printer can record it. Reset at the top of every `print` call.
    pub(crate) print_overflow: Cell<bool>,
    /// The global buffer registry. Index 0 is the default buffer (`*scratch*`).
    /// Slots are never removed — `kill-buffer` marks a buffer dead (`name: None`)
    /// so its index (and any live buffer object referencing it) stays valid.
    pub(crate) buffers: Vec<EditBuffer>,
    /// Index into `buffers` of the current buffer (`current-buffer`/`set-buffer`).
    pub(crate) current: usize,
    /// Text properties for strings. `Value::Str` is an `Arc<String>` value with no
    /// room for interval storage, so a propertized string's per-char plists live in
    /// this side table keyed by the `Arc`'s pointer identity. The stored `Weak`
    /// guards against pointer reuse: a lookup only trusts the entry when the weak
    /// still upgrades to the same allocation (a freed-then-reused address fails to
    /// upgrade → treated as unpropertized). Properties therefore travel with cheap
    /// `Arc` clones (`eq` strings) exactly like Emacs, but are lost across `concat`/
    /// `substring` (which mint fresh allocations) unless re-registered explicitly.
    pub(crate) string_props: HashMap<usize, (Weak<String>, Vec<Value>)>,
    /// OClosure metadata, keyed by the closure object's arena handle. An OClosure
    /// (`oclosure.el`) is an ordinary [`Obj::Closure`] that also carries a *type*
    /// symbol and an ordered list of *slot* symbol handles. The slot *values* are
    /// not stored here — they live in the closure's captured lexical env (the same
    /// storage the closure body reads), so `oclosure--set` and a body `setq` stay
    /// mutually visible, exactly as Emacs stores oclosure slots in the closure's
    /// env alist. This side table (rather than a field on `Obj::Closure`) keeps the
    /// compiler's closure-template construction untouched. Session-local: not part
    /// of the AOT heap image (oclosure-heavy libraries load at runtime).
    pub(crate) oclosure_meta: HashMap<u32, OClosureMeta>,
}

/// Type + slot layout attached to an [`Obj::Closure`] to make it an OClosure.
/// `ty` is the type symbol's handle; `slots` are the slot symbols' handles in
/// declaration order (index 0 = first slot). Values live in the closure's env.
pub struct OClosureMeta {
    pub ty: u32,
    pub slots: Vec<u32>,
}

/// An editing buffer: a char vector, a 1-based point, narrowing bounds, the mark,
/// plus the buffer-local variable slots and the local keymap slot. Positions are
/// 1-based (`point-min` = `begv`, `point-max` = `zv`). `begv`/`zv`/`mark`/the
/// save stacks track edits with Emacs marker semantics (see
/// [`ElispHost::cur_insert`]/[`ElispHost::cur_delete`]).
#[derive(Default)]
pub struct EditBuffer {
    /// The buffer's name, or `None` once killed (the slot is retained so existing
    /// buffer objects keep resolving; `buffer-live-p` reads this).
    pub name: Option<String>,
    /// This buffer's own `Obj::Buffer` handle, allocated once so buffer objects
    /// are `eq`-stable. `Value::Undef` only during initial construction.
    pub self_obj: Value,
    pub text: Vec<char>,
    /// Text-property plists, one per character (parallel to `text`, same length).
    /// Each entry is a plist `Value` (`Value::Undef` = no properties). Kept in sync
    /// with every `cur_insert`/`cur_delete` (inserted chars get nil props — plain
    /// `insert` does not inherit, matching Emacs).
    pub props: Vec<Value>,
    /// Live markers pointing into this buffer, adjusted on every edit. Shared
    /// (`Rc`) with the corresponding `Obj::Marker`; a marker is removed here when
    /// it is re-pointed (`set-marker`) elsewhere or detached.
    pub markers: Vec<Rc<RefCell<MarkerData>>>,
    /// Point: 1-based, always kept within `[begv, zv]`.
    pub point: usize,
    /// Narrowing lower bound (`point-min`); 1 when un-narrowed. Marker-like with
    /// insertion-type nil.
    pub begv: usize,
    /// Narrowing upper bound (`point-max`); `text.len()+1` when un-narrowed.
    /// Marker-like with insertion-type t (text inserted at `zv` extends the region).
    pub zv: usize,
    /// The mark, or `None` when unset. Marker-like (insertion-type nil). Active
    /// region / mark-ring semantics are not modeled — this is a bare position.
    pub mark: Option<usize>,
    /// `save-excursion` point markers (insertion-type nil), a per-buffer LIFO
    /// stack: unwind-protect guarantees strict nesting, so the top entry is always
    /// the matching one.
    pub se_markers: Vec<usize>,
    /// `save-restriction` saved `(begv, zv)` pairs, adjusted for edits inside the
    /// body so the restored restriction tracks intervening insertions/deletions.
    pub restrict_stack: Vec<(usize, usize)>,
    /// Buffer-local variable bindings. `Some(v)` is a bound local; `None` is a
    /// *void* local (created by `make-local-variable` on a void variable — reading
    /// it still signals `void-variable`, but `local-variable-p` is non-nil). Key
    /// absence means the variable is not local in this buffer.
    pub locals: HashMap<u32, Option<Value>>,
    /// The buffer's local keymap slot (`use-local-map`/`current-local-map`).
    pub local_map: Value,
}

/// Adjust a marker-like position `m` for an insertion of `len` chars at `pos`.
/// `advance_at_pos` selects insertion-type t (a marker exactly at `pos` moves
/// past the inserted text) vs nil (it stays before it).
fn adj_ins(m: &mut usize, pos: usize, len: usize, advance_at_pos: bool) {
    if *m > pos || (advance_at_pos && *m == pos) {
        *m += len;
    }
}

/// Adjust a marker-like position `m` for a deletion of the region `[from, to)`.
fn adj_del(m: &mut usize, from: usize, to: usize) {
    if *m >= to {
        *m -= to - from;
    } else if *m > from {
        *m = from;
    }
}

/// Result of the most recent `string-match`, in *character* positions (elisp
/// indexes strings by character, not byte).
#[derive(Clone, Debug)]
pub struct MatchData {
    pub subject: String,
    /// `spans[0]` is the whole match; `spans[n]` is capture group `n`. A group
    /// that did not participate is `None`.
    pub spans: Vec<Option<(usize, usize)>>,
    /// True if the last match was a buffer search: spans are 1-based buffer
    /// positions and `match-string` reads from `subject` accordingly.
    pub from_buffer: bool,
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
            catch_tags: Vec::new(),
            pending_error: None,
            match_data: None,
            output_capture: Vec::new(),
            print_overflow: Cell::new(false),
            buffers: vec![EditBuffer {
                name: Some("*scratch*".to_string()),
                self_obj: Value::Undef,
                text: Vec::new(),
                props: Vec::new(),
                markers: Vec::new(),
                point: 1,
                begv: 1,
                zv: 1,
                mark: None,
                se_markers: Vec::new(),
                restrict_stack: Vec::new(),
                locals: HashMap::new(),
                local_map: Value::Undef,
            }],
            current: 0,
            string_props: HashMap::new(),
            oclosure_meta: HashMap::new(),
        };
        crate::builtins::install(&mut h);
        // The default buffer's own object handle (allocated after the arena
        // exists, before `builtin_count` is fixed so it stays in the stable
        // built-in prefix and is never serialized as user heap).
        let scratch = h.alloc(Obj::Buffer(0));
        h.buffers[0].self_obj = scratch;
        // The global obarray object — the value of the `obarray` variable. Its
        // symbol set lives in `self.obarray` (the HashMap), so its own `symbols`
        // map stays empty and `global` routes every lookup there. Allocated in
        // the built-in prefix so it is never serialized as user heap.
        let global_ob = h.alloc(Obj::Obarray(ObarrayData {
            symbols: HashMap::new(),
            global: true,
        }));
        if let Value::Obj(sid) = h.intern("obarray") {
            if let Some(Obj::Symbol(s)) = h.arena.get_mut(sid as usize) {
                s.value = Some(global_ob);
                s.special = true;
            }
        }
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
            buffer_local_auto: false,
            alias_of: None,
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
            buffer_local_auto: false,
            alias_of: None,
        }))
    }
    pub fn obj(&self, v: &Value) -> Option<&Obj> {
        match v {
            Value::Obj(id) => self.arena.get(*id as usize),
            _ => None,
        }
    }
    // ── first-class obarrays (`obarray-make` and friends) ──
    /// `(intern NAME OB)` into the private obarray at arena id `ob_id`: return the
    /// existing interned symbol if present, else create a fresh symbol (like
    /// `make-symbol`) and record it. Mirrors C `intern`, which allocates a new
    /// symbol on a miss.
    pub fn obarray_intern(&mut self, ob_id: u32, name: &str) -> Value {
        if let Some(Obj::Obarray(d)) = self.arena.get(ob_id as usize) {
            if let Some(&sid) = d.symbols.get(name) {
                return Value::Obj(sid);
            }
        }
        let sym = self.make_symbol(name);
        if let Value::Obj(sid) = sym {
            if let Some(Obj::Obarray(d)) = self.arena.get_mut(ob_id as usize) {
                d.symbols.insert(name.to_string(), sid);
            }
        }
        sym
    }
    /// `(intern-soft NAME OB)` into the private obarray at arena id `ob_id`: the
    /// interned symbol if present, else `nil` (`Value::Undef`).
    pub fn obarray_intern_soft(&self, ob_id: u32, name: &str) -> Value {
        match self.arena.get(ob_id as usize) {
            Some(Obj::Obarray(d)) => d
                .symbols
                .get(name)
                .map(|&sid| Value::Obj(sid))
                .unwrap_or(Value::Undef),
            _ => Value::Undef,
        }
    }
    /// `(unintern NAME OB)` from the private obarray at arena id `ob_id`: remove
    /// the mapping, returning whether a symbol was actually removed.
    pub fn obarray_unintern(&mut self, ob_id: u32, name: &str) -> bool {
        match self.arena.get_mut(ob_id as usize) {
            Some(Obj::Obarray(d)) => d.symbols.remove(name).is_some(),
            _ => false,
        }
    }
    /// `(unintern NAME)` from the global obarray: drop NAME's mapping (the symbol
    /// object itself survives in the arena but is no longer interned), returning
    /// whether it was present.
    pub fn obarray_unintern_global(&mut self, name: &str) -> bool {
        self.obarray.remove(name).is_some()
    }
    /// The symbol objects interned in an obarray (private map values, or the
    /// global obarray's), for `mapatoms`.
    pub fn obarray_symbols(&self, ob: &Value) -> Vec<Value> {
        match self.obj(ob) {
            Some(Obj::Obarray(d)) if d.global => {
                self.obarray.values().map(|&id| Value::Obj(id)).collect()
            }
            Some(Obj::Obarray(d)) => d.symbols.values().map(|&id| Value::Obj(id)).collect(),
            _ => Vec::new(),
        }
    }
    /// Public form of `Self::sym_handle`: the arena handle of `v` if it is a
    /// symbol object, else `None`. Used by the OClosure builtins.
    pub fn as_sym_handle(&self, v: &Value) -> Option<u32> {
        self.sym_handle(v)
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
    /// Coerce any sequence — list, vector, or string — to a `Vec<Value>` (string
    /// chars become integer char codes). `mapcar`/`seq-*` accept all of these.
    pub fn seq_vec(&self, v: &Value) -> Option<Vec<Value>> {
        match v {
            Value::Str(s) => Some(s.chars().map(|c| Value::Int(c as i64)).collect()),
            Value::Obj(id) => match self.arena.get(*id as usize) {
                Some(Obj::Vector(items)) => Some(items.clone()),
                _ => self.list_vec(v),
            },
            _ => self.list_vec(v),
        }
    }

    // ── symbol cells (dynamic / value cell) ──
    /// Follow the `defvaralias` chain from SYM's handle to the base variable's
    /// handle (Emacs `indirect_variable`). Ordinary variables resolve to
    /// themselves. Bounded to break any accidental cycle.
    pub fn indirect_var(&self, id: u32) -> u32 {
        let mut cur = id;
        for _ in 0..64 {
            match self.arena.get(cur as usize) {
                Some(Obj::Symbol(s)) => match s.alias_of {
                    Some(base) if base != cur => cur = base,
                    _ => return cur,
                },
                _ => return cur,
            }
        }
        cur
    }
    /// `(defvaralias ALIAS BASE)` — make ALIAS forward all value operations to
    /// BASE (Emacs `Fdefvaralias`). If BASE is unbound and ALIAS holds a value,
    /// BASE inherits it; BASE (and thus ALIAS) becomes special. Returns BASE.
    /// Signals `cyclic-variable-indirection` if the alias chain would loop.
    pub fn defvaralias(&mut self, alias: &Value, base: &Value) -> Result<Value, String> {
        let aid = self.sym_handle(alias).ok_or("defvaralias: not a symbol")?;
        let bid = self.sym_handle(base).ok_or("defvaralias: not a symbol")?;
        // Reject a chain that would make BASE indirect back to ALIAS.
        let mut probe = bid;
        for _ in 0..64 {
            if probe == aid {
                return Err(format!(
                    "cyclic-variable-indirection: {}",
                    self.sym_name(alias).unwrap_or_default()
                ));
            }
            match self.arena.get(probe as usize) {
                Some(Obj::Symbol(s)) => match s.alias_of {
                    Some(next) if next != probe => probe = next,
                    _ => break,
                },
                _ => break,
            }
        }
        let base_id = self.indirect_var(bid);
        // If BASE is void but ALIAS has a value, BASE inherits ALIAS's value.
        let base_void =
            matches!(self.arena.get(base_id as usize), Some(Obj::Symbol(s)) if s.value.is_none());
        if base_void {
            let alias_val = match self.arena.get(aid as usize) {
                Some(Obj::Symbol(s)) => s.value.clone(),
                _ => None,
            };
            if let Some(val) = alias_val {
                if let Obj::Symbol(s) = &mut self.arena[base_id as usize] {
                    s.value = Some(val);
                }
            }
        }
        // Aliased variables are special (Emacs marks the base variable forwarded).
        if let Obj::Symbol(s) = &mut self.arena[base_id as usize] {
            s.special = true;
        }
        if let Obj::Symbol(s) = &mut self.arena[aid as usize] {
            s.alias_of = Some(base_id);
            s.special = true;
        }
        Ok(base.clone())
    }
    pub fn set_value(&mut self, v: &Value, val: Value) -> Result<(), String> {
        let id0 = self.sym_handle(v).ok_or("set: not a symbol")?;
        let id = self.indirect_var(id0);
        // A lexical binding shadows both the buffer-local and global cells.
        if self.lex.as_ref().is_some_and(|s| s.set(id, &val)) {
            return Ok(());
        }
        // Write the current buffer's local slot if it already has one, or if the
        // variable is automatically buffer-local (create the local on first set).
        let bi = self.cur_buf_idx();
        if self.buffers[bi].locals.contains_key(&id) || self.is_auto_local(id) {
            self.buffers[bi].locals.insert(id, Some(val));
            return Ok(());
        }
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.value = Some(val);
        }
        Ok(())
    }
    fn is_auto_local(&self, id: u32) -> bool {
        matches!(self.arena.get(id as usize), Some(Obj::Symbol(s)) if s.buffer_local_auto)
    }

    // ── buffer-local variables ──
    /// `(make-local-variable SYM)` — give SYM a buffer-local binding in the
    /// current buffer. The local starts with the value SYM currently has (its
    /// default), snapshotting it; a void default yields a void local. No-op if a
    /// local already exists. Returns SYM.
    pub fn make_local_variable(&mut self, v: &Value) -> Result<Value, String> {
        let id0 = self
            .sym_handle(v)
            .ok_or("make-local-variable: not a symbol")?;
        let id = self.indirect_var(id0);
        let bi = self.cur_buf_idx();
        if !self.buffers[bi].locals.contains_key(&id) {
            let snapshot = match &self.arena[id as usize] {
                Obj::Symbol(s) => s.value.clone(),
                _ => None,
            };
            self.buffers[bi].locals.insert(id, snapshot);
        }
        Ok(v.clone())
    }
    /// `(make-variable-buffer-local SYM)` — mark SYM automatically buffer-local
    /// (and special, like Emacs). Returns SYM.
    pub fn make_variable_buffer_local(&mut self, v: &Value) -> Result<Value, String> {
        let id0 = self
            .sym_handle(v)
            .ok_or("make-variable-buffer-local: not a symbol")?;
        let id = self.indirect_var(id0);
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.buffer_local_auto = true;
            s.special = true;
        }
        Ok(v.clone())
    }
    /// `(local-variable-p SYM)` — non-nil if SYM has a buffer-local binding in the
    /// current buffer.
    pub fn local_variable_p(&self, v: &Value) -> bool {
        match self.sym_handle(v) {
            Some(id) => self.buffers[self.cur_buf_idx()]
                .locals
                .contains_key(&self.indirect_var(id)),
            None => false,
        }
    }
    /// `(local-variable-if-set-p SYM)` — non-nil if SYM is local in the current
    /// buffer or would become local when set (automatically buffer-local).
    pub fn local_variable_if_set_p(&self, v: &Value) -> bool {
        match self.sym_handle(v) {
            Some(id0) => {
                let id = self.indirect_var(id0);
                self.buffers[self.cur_buf_idx()].locals.contains_key(&id) || self.is_auto_local(id)
            }
            None => false,
        }
    }
    /// `(kill-local-variable SYM)` — remove the current buffer's local binding for
    /// SYM (the default becomes effective again). Returns SYM.
    pub fn kill_local_variable(&mut self, v: &Value) -> Result<Value, String> {
        if let Some(id0) = self.sym_handle(v) {
            let id = self.indirect_var(id0);
            let bi = self.cur_buf_idx();
            self.buffers[bi].locals.remove(&id);
        }
        Ok(v.clone())
    }
    /// Symbol handles with a buffer-local binding in the current buffer, for the
    /// prelude port of `kill-all-local-variables`/`buffer-local-variables`.
    pub fn buffer_local_symbols(&mut self) -> Value {
        let ids: Vec<u32> = self.buffers[self.cur_buf_idx()]
            .locals
            .keys()
            .copied()
            .collect();
        let items: Vec<Value> = ids.into_iter().map(Value::Obj).collect();
        self.list_from(items)
    }
    /// `(use-local-map MAP)` — install MAP as the current buffer's local keymap.
    pub fn use_local_map(&mut self, map: Value) {
        let bi = self.cur_buf_idx();
        self.buffers[bi].local_map = map;
    }
    /// `(current-local-map)` — the current buffer's local keymap, or nil.
    pub fn current_local_map(&self) -> Value {
        self.buffers[self.cur_buf_idx()].local_map.clone()
    }
    /// `(buffer-local-value SYM BUFFER)` — SYM's value in BUFFER: its buffer-local
    /// slot if present, else the global default. Skips lexical bindings (this reads
    /// a buffer's variable, not the caller's scope). `buf_idx` is BUFFER's slot; the
    /// caller resolves it (defaulting to the current buffer).
    pub fn buffer_local_or_default(&self, v: &Value, buf_idx: usize) -> Result<Value, String> {
        if let Some(id0) = self.sym_handle(v) {
            let id = self.indirect_var(id0);
            if let Some(slot) = self.buffers[buf_idx].locals.get(&id) {
                return slot.clone().ok_or_else(|| {
                    format!("void-variable: {}", self.sym_name(v).unwrap_or_default())
                });
            }
        }
        self.raw_global_value(v)
    }
    /// Clear a symbol's global value cell (`makunbound`). Lexical bindings are
    /// left untouched — they shadow the cell and unwind on their own.
    pub fn unset_value(&mut self, v: &Value) -> Result<(), String> {
        let id0 = self.sym_handle(v).ok_or("makunbound: not a symbol")?;
        let id = self.indirect_var(id0);
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.value = None;
        }
        Ok(())
    }
    pub fn get_value(&self, v: &Value) -> Result<Value, String> {
        if let Some(id0) = self.sym_handle(v) {
            let id = self.indirect_var(id0);
            // Precedence: lexical binding, then the current buffer's local
            // binding, then the global (default) value cell.
            if let Some(val) = self.lex.as_ref().and_then(|s| s.lookup(id)) {
                return Ok(val);
            }
            if let Some(slot) = self.buffers[self.cur_buf_idx()].locals.get(&id) {
                return slot.clone().ok_or_else(|| {
                    format!("void-variable: {}", self.sym_name(v).unwrap_or_default())
                });
            }
            return match &self.arena[id as usize] {
                Obj::Symbol(s) => s
                    .value
                    .clone()
                    .ok_or_else(|| format!("void-variable: {}", s.name)),
                _ => Err("not a symbol".to_string()),
            };
        }
        match v {
            Value::Bool(true) => Ok(Value::Bool(true)),
            Value::Undef => Ok(Value::Undef),
            _ => Err("not a symbol".to_string()),
        }
    }
    /// Index of the current buffer.
    fn cur_buf_idx(&self) -> usize {
        self.current
    }
    /// The global (default) value cell, bypassing lexical and buffer-local
    /// bindings — the reader used by `default-value`/`default-boundp`.
    pub fn raw_global_value(&self, v: &Value) -> Result<Value, String> {
        if let Some(id0) = self.sym_handle(v) {
            let id = self.indirect_var(id0);
            return match &self.arena[id as usize] {
                Obj::Symbol(s) => s
                    .value
                    .clone()
                    .ok_or_else(|| format!("void-variable: {}", s.name)),
                _ => Err("not a symbol".to_string()),
            };
        }
        match v {
            Value::Bool(true) => Ok(Value::Bool(true)),
            Value::Undef => Ok(Value::Undef),
            _ => Err("not a symbol".to_string()),
        }
    }
    /// True if the global (default) value cell is bound (`default-boundp`).
    pub fn default_boundp_raw(&self, v: &Value) -> bool {
        match self.sym_handle(v) {
            Some(id0) => {
                let id = self.indirect_var(id0);
                matches!(self.arena.get(id as usize), Some(Obj::Symbol(s)) if s.value.is_some())
            }
            None => false,
        }
    }
    /// Write the global (default) value cell directly (`set-default`), bypassing
    /// lexical and buffer-local bindings.
    pub fn set_raw_global(&mut self, v: &Value, val: Value) -> Result<(), String> {
        let id0 = self.sym_handle(v).ok_or("set-default: not a symbol")?;
        let id = self.indirect_var(id0);
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.value = Some(val);
        }
        Ok(())
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
    /// True if V is a symbol marked special (defvar/defconst), for `special-variable-p`.
    pub fn symbol_special(&self, v: &Value) -> bool {
        self.sym_handle(v)
            .map(|id| self.is_special(self.indirect_var(id)))
            .unwrap_or(false)
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
        match self.sym_handle(v) {
            Some(id0) => {
                let id = self.indirect_var(id0);
                matches!(self.arena.get(id as usize), Some(Obj::Symbol(s)) if s.value.is_some())
            }
            None => false,
        }
    }
    pub fn is_fbound(&self, v: &Value) -> bool {
        matches!(self.obj(v), Some(Obj::Symbol(s)) if s.function.is_some())
    }

    // ── dynamic binding ──
    pub fn specdepth(&self) -> usize {
        self.specstack.len()
    }
    pub fn specbind(&mut self, sym: &Value, val: Value) -> Result<(), String> {
        let id0 = self.sym_handle(sym).ok_or("cannot bind a non-symbol")?;
        let id = self.indirect_var(id0);
        let bi = self.cur_buf_idx();
        // `let` over a buffer-local variable rebinds the current buffer's local
        // slot (Emacs SPECPDL_LET_LOCAL), not the global default.
        if self.buffers[bi].locals.contains_key(&id) || self.is_auto_local(id) {
            let old = self.buffers[bi].locals.get(&id).cloned();
            self.specstack.push(SpecEntry::Local(id, bi, old));
            self.buffers[bi].locals.insert(id, Some(val));
            return Ok(());
        }
        let old = if let Obj::Symbol(s) = &self.arena[id as usize] {
            s.value.clone()
        } else {
            None
        };
        self.specstack.push(SpecEntry::Global(id, old));
        if let Obj::Symbol(s) = &mut self.arena[id as usize] {
            s.value = Some(val);
        }
        Ok(())
    }
    pub fn unbind_to(&mut self, depth: usize) {
        while self.specstack.len() > depth {
            match self.specstack.pop().unwrap() {
                SpecEntry::Global(id, old) => {
                    if let Obj::Symbol(s) = &mut self.arena[id as usize] {
                        s.value = old;
                    }
                }
                SpecEntry::Local(id, buf, old) => {
                    if let Some(b) = self.buffers.get_mut(buf) {
                        match old {
                            None => {
                                b.locals.remove(&id);
                            }
                            Some(prev) => {
                                b.locals.insert(id, prev);
                            }
                        }
                    }
                }
            }
        }
    }
    // ── lexical scope management ──
    /// Open a lexical scope: record an unwind boundary (the current lexical
    /// head + specstack depth). No binding node is created yet — each
    /// `bind_here` conses one; `close_scope` restores the saved head, dropping
    /// every node bound within this scope.
    pub fn open_scope(&mut self) {
        self.frame_stack
            .push((self.lex.clone(), self.specstack.len()));
    }
    /// Open a scope whose bindings extend `env` (a closure's captured env):
    /// record the unwind boundary, then make `env` the active lexical head so
    /// subsequent `bind_here` calls cons the params onto it.
    pub fn open_scope_in(&mut self, env: Lex) {
        self.frame_stack
            .push((self.lex.clone(), self.specstack.len()));
        self.lex = env;
    }
    /// Pop the innermost scope: restore the prior lexical env and unwind any
    /// dynamic (special-var) bindings made within it.
    pub fn close_scope(&mut self) {
        if let Some((saved, depth)) = self.frame_stack.pop() {
            self.unbind_to(depth);
            self.lex = saved;
        }
    }
    /// Pop scopes until `frame_stack` is back to `target_len`. A non-local exit
    /// (`throw`/error) out of an inner `let` skips its `UNBIND`, leaking the
    /// lexical scope; `run_closure` calls this to recover, so the caller's
    /// lexical environment isn't corrupted.
    pub fn unwind_scopes_to(&mut self, target_len: usize) {
        while self.frame_stack.len() > target_len {
            self.close_scope();
        }
    }
    pub fn scope_depth(&self) -> usize {
        self.frame_stack.len()
    }
    /// Bind `id` to `val` in the current scope — lexically, unless the symbol is
    /// special (`defvar`'d), in which case dynamically (saved on the specstack).
    pub fn bind_here(&mut self, id: u32, val: Value) {
        if self.is_special(id) {
            let _ = self.specbind(&Value::Obj(id), val);
        } else {
            // Cons a fresh single-binding node onto the lexical chain. A later
            // same-name rebind conses another node in front (shadows it);
            // closures that captured the earlier head never see it.
            self.lex = Some(Rc::new(Scope {
                sym: id,
                val: RefCell::new(val),
                parent: self.lex.take(),
            }));
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

    // ── OClosure seam (oclosure.el's C primitives) ──
    // These implement the host-specific primitives `oclosure.el` builds on. In
    // Emacs they poke at an interpreted-function's `aref` slots; elisprs closures
    // are compiled (a `Chunk` + captured env), so the seam instead attaches a
    // type + slot-name list (side table) and reads/writes slot values in the
    // closure's captured lexical env by symbol. The observable oclosure API
    // (define / lambda / accessors / `oclosure-type`) matches Emacs exactly.

    /// True if `v` is a closure (`closurep`).
    pub fn is_closure(&self, v: &Value) -> bool {
        matches!(self.obj(v), Some(Obj::Closure { .. }))
    }

    /// Mark closure `v` as an OClosure of type `ty` with the given ordered slot
    /// symbols (`oclosure--fix-type`). No-op if `v` is not a closure.
    pub fn oclosure_set_meta(&mut self, v: &Value, ty: u32, slots: Vec<u32>) {
        if let Value::Obj(id) = v {
            if matches!(self.arena.get(*id as usize), Some(Obj::Closure { .. })) {
                self.oclosure_meta.insert(*id, OClosureMeta { ty, slots });
            }
        }
    }

    /// The type symbol handle of OClosure `v`, or `None` (`oclosure-type`).
    pub fn oclosure_type_of(&self, v: &Value) -> Option<u32> {
        match v {
            Value::Obj(id) if self.is_closure(v) => self.oclosure_meta.get(id).map(|m| m.ty),
            _ => None,
        }
    }

    /// Clone a closure's captured env (for slot access), or `None`.
    fn closure_env(&self, v: &Value) -> Option<Lex> {
        match self.obj(v) {
            Some(Obj::Closure { env, .. }) => Some(env.clone()),
            _ => None,
        }
    }

    /// Read slot `index` of OClosure `v` (`oclosure--get`): look up the slot
    /// symbol in the closure's captured env.
    pub fn oclosure_get(&self, v: &Value, index: usize) -> Option<Value> {
        let id = match v {
            Value::Obj(id) => *id,
            _ => return None,
        };
        let sym = *self.oclosure_meta.get(&id)?.slots.get(index)?;
        let env = self.closure_env(v)?;
        env.as_ref().and_then(|h| h.lookup(sym))
    }

    /// Write slot `index` of OClosure `v` (`oclosure--set`): mutate the slot
    /// symbol's cell in the closure's captured env. Returns false if not found.
    pub fn oclosure_set(&self, v: &Value, index: usize, val: &Value) -> bool {
        let id = match v {
            Value::Obj(id) => *id,
            _ => return false,
        };
        let sym = match self.oclosure_meta.get(&id).and_then(|m| m.slots.get(index)) {
            Some(s) => *s,
            None => return false,
        };
        match self.closure_env(v).flatten() {
            Some(head) => head.set(sym, val),
            None => false,
        }
    }

    /// Functional copy of OClosure `src` (`oclosure--copy`): a new closure with the
    /// same code + type, whose first `args.len()` slots take the new values and
    /// whose remaining slots keep `src`'s values. Fresh slot bindings are prepended
    /// to `src`'s env so they shadow the originals (the copy's body reads the new
    /// values). Returns `None` if `src` is not an OClosure closure.
    pub fn oclosure_copy(&mut self, src: &Value, args: &[Value]) -> Option<Value> {
        let id = match src {
            Value::Obj(id) => *id,
            _ => return None,
        };
        let slots = self.oclosure_meta.get(&id)?.slots.clone();
        let ty = self.oclosure_meta.get(&id)?.ty;
        let (params, body, is_macro, base_env) = match self.obj(src) {
            Some(Obj::Closure {
                params,
                body,
                is_macro,
                env,
            }) => (params.clone(), body.clone(), *is_macro, env.clone()),
            _ => return None,
        };
        // New value for each slot: the passed arg, else the original slot value.
        let mut vals: Vec<Value> = Vec::with_capacity(slots.len());
        for (k, &sym) in slots.iter().enumerate() {
            let v = if k < args.len() {
                args[k].clone()
            } else {
                base_env
                    .as_ref()
                    .and_then(|h| h.lookup(sym))
                    .unwrap_or(Value::Undef)
            };
            vals.push(v);
        }
        // Prepend in reverse so slot[0] ends up frontmost (found first on lookup).
        let mut env = base_env;
        for (k, &sym) in slots.iter().enumerate().rev() {
            env = Some(Rc::new(Scope {
                sym,
                val: RefCell::new(vals[k].clone()),
                parent: env.take(),
            }));
        }
        let newv = self.alloc(Obj::Closure {
            params,
            body,
            is_macro,
            env,
        });
        if let Value::Obj(nid) = newv {
            self.oclosure_meta.insert(nid, OClosureMeta { ty, slots });
        }
        Some(newv)
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
                    buffer_local_auto: s.buffer_local_auto,
                    alias_of: s.alias_of,
                },
                Obj::Vector(v) => SerObj::Vector(v.clone()),
                Obj::HashTable { test, entries } => SerObj::HashTable {
                    test: *test,
                    entries: entries.clone(),
                },
                Obj::CharTable(t) => SerObj::CharTable {
                    subtype: t.subtype.clone(),
                    default: t.default.clone(),
                    parent: t.parent.clone(),
                    extra: t.extra.clone(),
                    ranges: t.ranges.clone(),
                },
                // Buffer/marker/obarray objects are runtime-only (created after
                // prelude load) and never appear in a compiled/AOT heap image;
                // emit a harmless placeholder so the match stays exhaustive.
                Obj::Buffer(_) | Obj::Marker(_) | Obj::Obarray(_) => SerObj::Symbol {
                    name: "--unexpected-runtime-obj--".to_string(),
                    value: None,
                    function: None,
                    special: false,
                    buffer_local_auto: false,
                    alias_of: None,
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
                    buffer_local_auto: false,
                    alias_of: None,
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
                            buffer_local_auto: s.buffer_local_auto,
                            alias_of: s.alias_of,
                        }
                    }
                    Obj::Cons(a, b) => SerObj::Cons(a.clone(), b.clone()),
                    Obj::Vector(v) => SerObj::Vector(v.clone()),
                    Obj::HashTable { test, entries } => SerObj::HashTable {
                        test: *test,
                        entries: entries.clone(),
                    },
                    Obj::CharTable(t) => SerObj::CharTable {
                        subtype: t.subtype.clone(),
                        default: t.default.clone(),
                        parent: t.parent.clone(),
                        extra: t.extra.clone(),
                        ranges: t.ranges.clone(),
                    },
                    // Runtime-only; never legitimately serialized (see above).
                    Obj::Buffer(_) | Obj::Marker(_) | Obj::Obarray(_) => SerObj::Symbol {
                        name: "--unexpected-runtime-obj--".to_string(),
                        value: None,
                        function: None,
                        special: false,
                        buffer_local_auto: false,
                        alias_of: None,
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
                        buffer_local_auto: false,
                        alias_of: None,
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
                    buffer_local_auto,
                    alias_of,
                } => {
                    self.obarray.insert(name.clone(), id);
                    Obj::Symbol(SymbolData {
                        name,
                        value,
                        function,
                        special,
                        buffer_local_auto,
                        alias_of,
                    })
                }
                SerObj::Vector(v) => Obj::Vector(v),
                SerObj::HashTable { test, entries } => Obj::HashTable { test, entries },
                SerObj::CharTable {
                    subtype,
                    default,
                    parent,
                    extra,
                    ranges,
                } => Obj::CharTable(CharTable {
                    subtype,
                    default,
                    parent,
                    extra,
                    ranges,
                }),
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

    /// If `items` is a cl-defstruct instance (slot 0 is a symbol `cl-struct-NAME`),
    /// return the struct NAME; otherwise `None`.
    pub fn struct_tag_name(&self, items: &[Value]) -> Option<String> {
        let tag = self.sym_name(items.first()?)?;
        tag.strip_prefix("cl-struct-").map(|s| s.to_string())
    }

    // ── printing ──
    pub fn print(&self, v: &Value, readable: bool) -> String {
        self.print_overflow.set(false);
        self.print_inner(v, readable, 0)
    }

    /// Like `print`, but returns Emacs's `error "Apparently circular structure
    /// being printed"` when the value nested `PRINT_CIRCLE` deep (matching
    /// print.c: with `print-circle` nil, that depth signals rather than prints).
    pub fn print_checked(&self, v: &Value, readable: bool) -> Result<String, String> {
        let s = self.print(v, readable);
        if self.print_overflow.get() {
            return Err("Apparently circular structure being printed".to_string());
        }
        Ok(s)
    }

    /// Read a non-negative integer dynamic var (`print-length`/`print-level`) for
    /// the printer; None when unset/nil/negative (i.e. no limit).
    fn print_limit(&self, name: &str) -> Option<usize> {
        let id = *self.obarray.get(name)?;
        match self.arena.get(id as usize)? {
            Obj::Symbol(s) => match s.value.as_ref()? {
                Value::Int(n) if *n >= 0 => Some(*n as usize),
                _ => None,
            },
            _ => None,
        }
    }

    /// True if a printer flag dynamic var (e.g. `print-escape-newlines`) is non-nil.
    fn print_flag(&self, name: &str) -> bool {
        self.print_flag_or(name, false)
    }

    /// Like `print_flag` but uses DEFAULT when the variable is unbound (e.g.
    /// `print-quoted` defaults to t).
    fn print_flag_or(&self, name: &str, default: bool) -> bool {
        match self
            .obarray
            .get(name)
            .and_then(|id| self.arena.get(*id as usize))
        {
            Some(Obj::Symbol(s)) => s.value.as_ref().map(el_truthy).unwrap_or(default),
            _ => default,
        }
    }

    /// Print a sequence's elements honoring `print-length` (truncate with `...`).
    fn print_seq(&self, items: &[Value], readable: bool, depth: usize) -> Vec<String> {
        let limit = self.print_limit("print-length");
        let mut parts = Vec::new();
        for (i, e) in items.iter().enumerate() {
            if limit.is_some_and(|lim| i >= lim) {
                parts.push("...".to_string());
                break;
            }
            parts.push(self.print_inner(e, readable, depth));
        }
        parts
    }

    fn print_inner(&self, v: &Value, readable: bool, depth: usize) -> String {
        // Faithful to print.c `PRINT_CIRCLE` = 200: with `print-circle` nil,
        // any object nested this deep aborts printing with "Apparently circular
        // structure being printed". Stop recursing here (both to match Emacs and
        // to keep the Rust call stack bounded) and flag it for `print_checked`.
        if depth >= PRINT_CIRCLE {
            self.print_overflow.set(true);
            return String::new();
        }
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
                } else {
                    format_float(*f)
                }
            }
            Value::Str(s) => {
                if readable {
                    let mut t = s.replace('\\', "\\\\").replace('"', "\\\"");
                    // print-escape-newlines: render newline/formfeed as \n / \f.
                    if self.print_flag("print-escape-newlines") {
                        t = t.replace('\n', "\\n").replace('\u{c}', "\\f");
                    }
                    // A propertized string prints as `#("text" START END (plist) …)`.
                    let intervals = self.string_prop_intervals(s, depth);
                    if intervals.is_empty() {
                        format!("\"{t}\"")
                    } else {
                        format!("#(\"{t}\"{intervals})")
                    }
                } else {
                    s.to_string()
                }
            }
            Value::Obj(id) => match self.arena.get(*id as usize) {
                Some(Obj::Symbol(s)) => {
                    if readable {
                        print_symbol_readable(&s.name)
                    } else {
                        s.name.clone()
                    }
                }
                Some(Obj::Cons(..)) => self.print_list(v, readable, depth),
                Some(Obj::Vector(items)) => {
                    // print-level: a vector/record one level too deep prints `...`.
                    if self
                        .print_limit("print-level")
                        .is_some_and(|lvl| depth + 1 > lvl)
                    {
                        return "...".to_string();
                    }
                    // A cl-defstruct instance is a vector tagged `cl-struct-NAME`
                    // in slot 0; print it as Emacs's record syntax `#s(NAME …)`.
                    if let Some(name) = self.struct_tag_name(items) {
                        let parts = self.print_seq(&items[1..], readable, depth + 1);
                        format!("#s({name} {})", parts.join(" "))
                    } else {
                        let parts = self.print_seq(items, readable, depth + 1);
                        format!("[{}]", parts.join(" "))
                    }
                }
                Some(Obj::CharTable(t)) => {
                    // Emacs prints char-tables as `#^[DEFAULT PARENT SUBTYPE …]`
                    // where `…` is the raw sub-char-table tree layout. Reproducing
                    // that tree byte-for-byte is infeasible without modeling the
                    // exact multi-level bucket structure, so we print the readable
                    // header slots only. Identity/`char-table-p`/`aref`/`equal`
                    // (all `eq`-based) behave correctly regardless; only the printed
                    // per-char body differs from the binary. NAMED limitation.
                    format!(
                        "#^[{} {} {}]",
                        self.print_inner(&t.default, readable, depth + 1),
                        self.print_inner(&t.parent, readable, depth + 1),
                        self.print_inner(&t.subtype, readable, depth + 1),
                    )
                }
                Some(Obj::Buffer(idx)) => {
                    match self.buffers.get(*idx).and_then(|b| b.name.as_ref()) {
                        Some(name) => format!("#<buffer {name}>"),
                        None => "#<killed buffer>".to_string(),
                    }
                }
                Some(Obj::Marker(m)) => {
                    let md = m.borrow();
                    match md
                        .buffer
                        .and_then(|bi| self.buffers.get(bi).and_then(|b| b.name.as_ref()))
                    {
                        Some(name) => format!("#<marker at {} in {}>", md.pos, name),
                        None => "#<marker in no buffer>".to_string(),
                    }
                }
                Some(Obj::Obarray(d)) => {
                    let n = if d.global {
                        self.obarray.len()
                    } else {
                        d.symbols.len()
                    };
                    format!("#<obarray n={n}>")
                }
                Some(Obj::Subr { name, .. }) => format!("#<subr {name}>"),
                Some(Obj::Closure { is_macro, .. }) => {
                    if *is_macro {
                        "#<macro>".to_string()
                    } else {
                        "#<closure>".to_string()
                    }
                }
                Some(Obj::HashTable { test, entries }) => {
                    // Emacs-30 syntax: omit `test` when eql (the default), and
                    // `data` when empty — `#s(hash-table test equal data (k v …))`.
                    let mut s = String::from("#s(hash-table");
                    match test {
                        0 => s.push_str(" test eq"),
                        2 => s.push_str(" test equal"),
                        _ => {}
                    }
                    if !entries.is_empty() {
                        s.push_str(" data (");
                        for (i, (k, v)) in entries.iter().enumerate() {
                            if i > 0 {
                                s.push(' ');
                            }
                            s.push_str(&self.print_inner(k, readable, depth + 1));
                            s.push(' ');
                            s.push_str(&self.print_inner(v, readable, depth + 1));
                        }
                        s.push(')');
                    }
                    s.push(')');
                    s
                }
                None => "#<dangling>".to_string(),
            },
            other => other.as_str_cow().into_owned(),
        }
    }
    fn print_list(&self, v: &Value, readable: bool, depth: usize) -> String {
        // print-level: a list one level too deep prints as `...`.
        if self
            .print_limit("print-level")
            .is_some_and(|lvl| depth + 1 > lvl)
        {
            return "...".to_string();
        }
        let nd = depth + 1;
        // Emacs abbreviates the two-element forms `(quote X)`/`(function X)`/`` (` X) ``
        // as `'X`/`#'X`/`` `X ``; longer lists with those heads print in full.
        // Honored only when `print-quoted` is non-nil (its default).
        if let Some(Obj::Cons(head, tail)) = self.obj(v) {
            let prefix = if self.print_flag_or("print-quoted", true) {
                match self.obj(head) {
                    Some(Obj::Symbol(s)) => match s.name.as_str() {
                        "quote" => Some("'"),
                        "function" => Some("#'"),
                        "`" => Some("`"),
                        _ => None,
                    },
                    _ => None,
                }
            } else {
                None
            };
            if let Some(prefix) = prefix {
                if let Some(Obj::Cons(arg, rest)) = self.obj(tail) {
                    if !el_truthy(rest) {
                        return format!("{prefix}{}", self.print_inner(arg, readable, nd));
                    }
                }
            }
        }
        let limit = self.print_limit("print-length");
        let mut out = String::from("(");
        let mut cur = v.clone();
        let mut first = true;
        let mut count = 0usize;
        while let Some(Obj::Cons(a, d)) = self.obj(&cur) {
            if !first {
                out.push(' ');
            }
            first = false;
            if limit.is_some_and(|lim| count >= lim) {
                out.push_str("...");
                break;
            }
            out.push_str(&self.print_inner(a, readable, nd));
            count += 1;
            let next = d.clone();
            match next {
                // Both nil representations terminate the list (a `(1 . nil)` cdr
                // is the one-element list `(1)`, never a dotted pair).
                Value::Undef | Value::Bool(false) => break,
                Value::Obj(id) if matches!(self.arena.get(id as usize), Some(Obj::Cons(..))) => {
                    cur = next;
                }
                _ => {
                    out.push_str(" . ");
                    out.push_str(&self.print_inner(&next, readable, nd));
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

    /// Write program output, honoring an active `with-output-to-string` capture.
    pub fn emit(&mut self, s: &str) {
        if let Some(buf) = self.output_capture.last_mut() {
            buf.push_str(s);
        } else {
            use std::io::Write;
            print!("{s}");
            let _ = std::io::stdout().flush();
        }
    }

    /// The current editing buffer.
    pub fn cur_buf(&mut self) -> &mut EditBuffer {
        &mut self.buffers[self.current]
    }
    /// The current editing buffer (shared).
    pub fn cur_buf_ref(&self) -> &EditBuffer {
        &self.buffers[self.current]
    }

    // ── buffer registry ──────────────────────────────────────────────────────
    /// The current buffer's object handle (`current-buffer`).
    pub fn current_buffer(&self) -> Value {
        self.buffers[self.current].self_obj.clone()
    }
    /// Resolve a buffer-or-name to a live buffer index. A buffer object resolves
    /// to its slot (even if killed → `None`); a string is looked up by name.
    pub fn resolve_buffer(&self, v: &Value) -> Option<usize> {
        match self.obj(v) {
            Some(Obj::Buffer(idx)) => {
                let idx = *idx;
                self.buffers.get(idx).filter(|b| b.name.is_some())?;
                Some(idx)
            }
            _ => match v {
                Value::Str(s) => self.find_buffer_by_name(s),
                _ => None,
            },
        }
    }
    /// Index of the live buffer named `name`, if any.
    pub fn find_buffer_by_name(&self, name: &str) -> Option<usize> {
        self.buffers
            .iter()
            .position(|b| b.name.as_deref() == Some(name))
    }
    /// Allocate a fresh buffer slot named `name` and return its `Obj::Buffer`
    /// handle. The caller guarantees `name` is not already taken.
    fn new_buffer(&mut self, name: String) -> Value {
        let idx = self.buffers.len();
        self.buffers.push(EditBuffer {
            name: Some(name),
            self_obj: Value::Undef,
            text: Vec::new(),
            props: Vec::new(),
            markers: Vec::new(),
            point: 1,
            begv: 1,
            zv: 1,
            mark: None,
            se_markers: Vec::new(),
            restrict_stack: Vec::new(),
            locals: HashMap::new(),
            local_map: Value::Undef,
        });
        let handle = self.alloc(Obj::Buffer(idx));
        self.buffers[idx].self_obj = handle.clone();
        handle
    }
    /// `(get-buffer-create NAME)` — the live buffer named NAME, creating it if
    /// absent. Returns its buffer object.
    pub fn get_buffer_create(&mut self, name: &str) -> Value {
        match self.find_buffer_by_name(name) {
            Some(idx) => self.buffers[idx].self_obj.clone(),
            None => self.new_buffer(name.to_string()),
        }
    }
    /// `(generate-new-buffer-name STARTING)` — STARTING if free, else the first
    /// `STARTING<N>` (N≥2) that is free.
    pub fn generate_new_buffer_name(&self, starting: &str) -> String {
        if self.find_buffer_by_name(starting).is_none() {
            return starting.to_string();
        }
        let mut n = 2;
        loop {
            let cand = format!("{starting}<{n}>");
            if self.find_buffer_by_name(&cand).is_none() {
                return cand;
            }
            n += 1;
        }
    }
    /// `(set-buffer BUFFER-OR-NAME)` — make it current, returning its object.
    /// Signals if the buffer does not exist.
    pub fn set_buffer(&mut self, v: &Value) -> Result<Value, String> {
        let idx = self
            .resolve_buffer(v)
            .ok_or_else(|| format!("error: No buffer named {}", self.print(v, true)))?;
        self.current = idx;
        Ok(self.buffers[idx].self_obj.clone())
    }
    /// `(kill-buffer &optional BUFFER)` — mark BUFFER (default current) dead.
    /// Returns t if a live buffer was killed, nil otherwise.
    pub fn kill_buffer(&mut self, v: Option<&Value>) -> Value {
        let idx = match v {
            Some(v) if el_truthy(v) => match self.resolve_buffer(v) {
                Some(i) => i,
                None => return Value::Undef,
            },
            _ => self.current,
        };
        if self.buffers[idx].name.is_none() {
            return Value::Undef;
        }
        // Clear the slot's contents but keep it so the object stays resolvable
        // (as a killed buffer). If the current buffer is killed, fall back to the
        // first live buffer (Emacs would switch to another buffer).
        let b = &mut self.buffers[idx];
        b.name = None;
        b.text.clear();
        b.props.clear();
        b.locals.clear();
        b.se_markers.clear();
        b.restrict_stack.clear();
        // Detach every marker that pointed into the killed buffer.
        for mk in b.markers.drain(..) {
            let mut md = mk.borrow_mut();
            md.buffer = None;
            md.pos = 0;
        }
        if self.current == idx {
            self.current = self
                .buffers
                .iter()
                .position(|b| b.name.is_some())
                .unwrap_or(0);
        }
        Value::Bool(true)
    }
    /// `(rename-buffer NEWNAME)` — rename the current buffer. Returns the new name.
    pub fn rename_buffer(&mut self, newname: &str) -> Result<Value, String> {
        if let Some(other) = self.find_buffer_by_name(newname) {
            if other != self.current {
                return Err(format!("error: Buffer name '{newname}' is in use"));
            }
        }
        self.buffers[self.current].name = Some(newname.to_string());
        Ok(Value::str(newname.to_string()))
    }
    /// `(buffer-list)` — live buffer objects, in creation order.
    pub fn buffer_list(&mut self) -> Value {
        let items: Vec<Value> = self
            .buffers
            .iter()
            .filter(|b| b.name.is_some())
            .map(|b| b.self_obj.clone())
            .collect();
        self.list_from(items)
    }

    // ── text mutation (marker-adjusting) ─────────────────────────────────────
    /// Apply an insertion of `len` chars at 1-based `pos` in the current buffer to
    /// every marker-like position (begv/zv/mark and the save stacks). Point is
    /// handled by the caller.
    fn adjust_for_insert(&mut self, pos: usize, len: usize) {
        let b = &mut self.buffers[self.current];
        adj_ins(&mut b.begv, pos, len, false);
        adj_ins(&mut b.zv, pos, len, true);
        if let Some(m) = b.mark.as_mut() {
            adj_ins(m, pos, len, false);
        }
        for m in b.se_markers.iter_mut() {
            adj_ins(m, pos, len, false);
        }
        for (lo, hi) in b.restrict_stack.iter_mut() {
            adj_ins(lo, pos, len, false);
            adj_ins(hi, pos, len, true);
        }
        for mk in b.markers.iter() {
            let mut md = mk.borrow_mut();
            let ins_type = md.insertion_type;
            adj_ins(&mut md.pos, pos, len, ins_type);
        }
    }
    /// Apply a deletion of `[from, to)` in the current buffer to every marker-like
    /// position, including point.
    fn adjust_for_delete(&mut self, from: usize, to: usize) {
        let b = &mut self.buffers[self.current];
        adj_del(&mut b.point, from, to);
        adj_del(&mut b.begv, from, to);
        adj_del(&mut b.zv, from, to);
        if let Some(m) = b.mark.as_mut() {
            adj_del(m, from, to);
        }
        for m in b.se_markers.iter_mut() {
            adj_del(m, from, to);
        }
        for (lo, hi) in b.restrict_stack.iter_mut() {
            adj_del(lo, from, to);
            adj_del(hi, from, to);
        }
        for mk in b.markers.iter() {
            adj_del(&mut mk.borrow_mut().pos, from, to);
        }
    }
    /// Insert `chars` at point in the current buffer. `leave_after` puts point
    /// after the inserted text (the `insert` default); otherwise point is left at
    /// the start (`insert-file-contents`). Markers are adjusted per Emacs rules.
    pub fn cur_insert(&mut self, chars: Vec<char>, leave_after: bool) {
        let pos = self.buffers[self.current].point;
        let len = chars.len();
        if len == 0 {
            return;
        }
        let b = &mut self.buffers[self.current];
        b.text.splice((pos - 1)..(pos - 1), chars);
        // Plain insert gives the new characters nil properties (no inheritance).
        b.props
            .splice((pos - 1)..(pos - 1), std::iter::repeat_n(Value::Undef, len));
        self.adjust_for_insert(pos, len);
        self.buffers[self.current].point = if leave_after { pos + len } else { pos };
    }
    /// `insert-before-markers`: like `cur_insert` (leaving point after), but every
    /// marker sitting exactly at the insertion point is relocated *after* the new
    /// text regardless of its insertion type (Emacs `insert_before_markers`).
    pub fn cur_insert_before_markers(&mut self, chars: Vec<char>) {
        let pos = self.buffers[self.current].point;
        let len = chars.len();
        if len == 0 {
            return;
        }
        let b = &mut self.buffers[self.current];
        b.text.splice((pos - 1)..(pos - 1), chars);
        b.props
            .splice((pos - 1)..(pos - 1), std::iter::repeat_n(Value::Undef, len));
        self.adjust_for_insert(pos, len);
        // Bump any live marker that ended up exactly at the insertion point.
        for mk in self.buffers[self.current].markers.iter() {
            let mut md = mk.borrow_mut();
            if md.pos == pos {
                md.pos = pos + len;
            }
        }
        self.buffers[self.current].point = pos + len;
    }
    /// Delete the region `[from, to)` (1-based, `from <= to`) from the current
    /// buffer, adjusting point and all markers.
    pub fn cur_delete(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        let b = &mut self.buffers[self.current];
        b.text.drain((from - 1)..(to - 1));
        b.props.drain((from - 1)..(to - 1));
        self.adjust_for_delete(from, to);
    }
    /// `(narrow-to-region BEG END)` on the current buffer: clamp `begv`/`zv` to the
    /// region and pull point inside it.
    pub fn narrow(&mut self, beg: usize, end: usize) {
        let (lo, hi) = if beg <= end { (beg, end) } else { (end, beg) };
        let b = &mut self.buffers[self.current];
        let maxzv = b.text.len() + 1;
        b.begv = lo.clamp(1, maxzv);
        b.zv = hi.clamp(1, maxzv);
        b.point = b.point.clamp(b.begv, b.zv);
    }
    /// `(widen)` — remove any narrowing on the current buffer.
    pub fn widen(&mut self) {
        let b = &mut self.buffers[self.current];
        b.begv = 1;
        b.zv = b.text.len() + 1;
    }

    // ── markers ──────────────────────────────────────────────────────────────
    /// Allocate an `Obj::Marker`; when it points into a buffer, register it in
    /// that buffer's live-marker list so edits keep it up to date.
    pub fn alloc_marker(&mut self, buffer: Option<usize>, pos: usize, itype: bool) -> Value {
        let md = Rc::new(RefCell::new(MarkerData {
            buffer,
            pos,
            insertion_type: itype,
        }));
        if let Some(bi) = buffer {
            self.buffers[bi].markers.push(md.clone());
        }
        self.alloc(Obj::Marker(md))
    }
    /// The shared marker cell behind V, if V is a marker.
    fn marker_rc(&self, v: &Value) -> Option<Rc<RefCell<MarkerData>>> {
        match self.obj(v) {
            Some(Obj::Marker(m)) => Some(m.clone()),
            _ => None,
        }
    }
    /// `(markerp V)`.
    pub fn is_marker(&self, v: &Value) -> bool {
        matches!(self.obj(v), Some(Obj::Marker(_)))
    }
    /// `(marker-position M)` — 1-based position, or `None` for a detached marker.
    pub fn marker_position(&self, v: &Value) -> Option<usize> {
        let m = self.marker_rc(v)?;
        let md = m.borrow();
        md.buffer.map(|_| md.pos)
    }
    /// `(marker-buffer M)` — the buffer's object handle, or `None` when detached.
    pub fn marker_buffer(&self, v: &Value) -> Option<Value> {
        let m = self.marker_rc(v)?;
        let bi = m.borrow().buffer?;
        Some(self.buffers[bi].self_obj.clone())
    }
    /// `(marker-insertion-type M)`.
    pub fn marker_insertion_type(&self, v: &Value) -> Option<bool> {
        Some(self.marker_rc(v)?.borrow().insertion_type)
    }
    /// `(set-marker-insertion-type M TYPE)`.
    pub fn set_marker_insertion_type(&mut self, v: &Value, itype: bool) {
        if let Some(m) = self.marker_rc(v) {
            m.borrow_mut().insertion_type = itype;
        }
    }
    /// Coerce a value to a buffer position: an integer/float is itself; a marker
    /// yields its position (`None` when detached); anything else `None`.
    pub fn as_position(&self, v: &Value) -> Option<i64> {
        match v {
            Value::Int(n) => Some(*n),
            Value::Float(f) => Some(*f as i64),
            _ => self.marker_position(v).map(|p| p as i64),
        }
    }
    /// Point MARKER at `(buffer, pos)` — or detach it when `buffer` is `None` —
    /// moving it between buffer registries. `pos` is clamped to `[1, size+1]`.
    pub fn set_marker_to(
        &mut self,
        marker: &Value,
        buffer: Option<usize>,
        pos: usize,
    ) -> Result<(), String> {
        let m = self.marker_rc(marker).ok_or("set-marker: not a marker")?;
        let old_buf = m.borrow().buffer;
        if let Some(ob) = old_buf {
            if let Some(b) = self.buffers.get_mut(ob) {
                b.markers.retain(|x| !Rc::ptr_eq(x, &m));
            }
        }
        match buffer {
            None => {
                let mut md = m.borrow_mut();
                md.buffer = None;
                md.pos = 0;
            }
            Some(bi) => {
                let size = self.buffers[bi].text.len();
                let p = pos.clamp(1, size + 1);
                {
                    let mut md = m.borrow_mut();
                    md.buffer = Some(bi);
                    md.pos = p;
                }
                self.buffers[bi].markers.push(m.clone());
            }
        }
        Ok(())
    }
    /// Two markers are `equal` when they share a buffer and position (Emacs
    /// `Fequal` on markers).
    pub fn markers_equal(&self, a: &Value, b: &Value) -> bool {
        match (self.marker_rc(a), self.marker_rc(b)) {
            (Some(x), Some(y)) => {
                let (xa, xb) = (x.borrow(), y.borrow());
                xa.buffer == xb.buffer && xa.pos == xb.pos
            }
            _ => false,
        }
    }

    // ── text properties ──────────────────────────────────────────────────────
    /// `plist-get` with `eq` key comparison (the `get-text-property` default).
    pub fn plist_get_eq(&self, plist: &Value, prop: &Value) -> Value {
        let mut cur = plist.clone();
        while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
            let k = k.clone();
            let rest = d.clone();
            let (val, rest2) = match self.obj(&rest) {
                Some(Obj::Cons(v, d2)) => (v.clone(), d2.clone()),
                _ => return Value::Undef,
            };
            if self.values_eq(&k, prop) {
                return val;
            }
            cur = rest2;
        }
        Value::Undef
    }
    /// A fresh plist equal to PLIST but with PROP → VAL (`eq` key match; appended
    /// if absent). Never mutates the input.
    fn plist_put_copy(&mut self, plist: &Value, prop: &Value, val: &Value) -> Value {
        let mut flat: Vec<Value> = Vec::new();
        let mut replaced = false;
        let mut cur = plist.clone();
        while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
            let k = k.clone();
            let rest = d.clone();
            let (v, rest2) = match self.obj(&rest) {
                Some(Obj::Cons(v, d2)) => (v.clone(), d2.clone()),
                _ => break,
            };
            if self.values_eq(&k, prop) {
                flat.push(k);
                flat.push(val.clone());
                replaced = true;
            } else {
                flat.push(k);
                flat.push(v);
            }
            cur = rest2;
        }
        if !replaced {
            // Emacs prepends a newly-added property (existing keys keep their
            // position); `text-properties-at` returns most-recently-added first.
            let mut prepended = vec![prop.clone(), val.clone()];
            prepended.extend(flat);
            flat = prepended;
        }
        self.list_from(flat)
    }
    /// A fresh plist equal to PLIST with PROP removed (`eq` key match).
    fn plist_remove_copy(&mut self, plist: &Value, prop: &Value) -> Value {
        let mut flat: Vec<Value> = Vec::new();
        let mut cur = plist.clone();
        while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
            let k = k.clone();
            let rest = d.clone();
            let (v, rest2) = match self.obj(&rest) {
                Some(Obj::Cons(v, d2)) => (v.clone(), d2.clone()),
                _ => break,
            };
            if !self.values_eq(&k, prop) {
                flat.push(k);
                flat.push(v);
            }
            cur = rest2;
        }
        self.list_from(flat)
    }
    /// The property plist at absolute char index `idx0` in the current buffer.
    pub fn buffer_plist_at(&self, idx0: usize) -> Value {
        self.cur_buf_ref()
            .props
            .get(idx0)
            .cloned()
            .unwrap_or(Value::Undef)
    }
    /// Overwrite the property plist at absolute char index `idx0` in the current
    /// buffer (used by `insert` to carry an inserted string's text properties).
    pub fn buffer_set_plist_at(&mut self, idx0: usize, plist: Value) {
        if let Some(slot) = self.buffers[self.current].props.get_mut(idx0) {
            *slot = plist;
        }
    }
    /// The property plist at absolute char index `idx0` in buffer `bi`.
    pub fn buffer_plist_at_idx(&self, bi: usize, idx0: usize) -> Value {
        self.buffers[bi]
            .props
            .get(idx0)
            .cloned()
            .unwrap_or(Value::Undef)
    }
    /// The `(point-min, point-max)` bounds of buffer `bi`.
    pub fn buffer_begv_zv(&self, bi: usize) -> (usize, usize) {
        let b = &self.buffers[bi];
        (b.begv, b.zv)
    }
    /// `put-text-property` on the current buffer over char indices `[s0, e0)`.
    pub fn buffer_put_prop(&mut self, s0: usize, e0: usize, prop: &Value, val: &Value) {
        let n = self.cur_buf_ref().props.len();
        for idx in s0..e0.min(n) {
            let cur = self.buffers[self.current].props[idx].clone();
            let np = self.plist_put_copy(&cur, prop, val);
            self.buffers[self.current].props[idx] = np;
        }
    }
    /// `set-text-properties` on the current buffer: replace each char's plist over
    /// `[s0, e0)` with PLIST (shared — the slots are never mutated in place).
    pub fn buffer_set_props(&mut self, s0: usize, e0: usize, plist: &Value) {
        let n = self.cur_buf_ref().props.len();
        for idx in s0..e0.min(n) {
            self.buffers[self.current].props[idx] = plist.clone();
        }
    }
    /// `remove-text-properties` on the current buffer: drop PROP from each plist.
    pub fn buffer_remove_prop(&mut self, s0: usize, e0: usize, prop: &Value) {
        let n = self.cur_buf_ref().props.len();
        for idx in s0..e0.min(n) {
            let cur = self.buffers[self.current].props[idx].clone();
            let np = self.plist_remove_copy(&cur, prop);
            self.buffers[self.current].props[idx] = np;
        }
    }

    /// The per-char property plists registered for string S, or `None` when it has
    /// none (or a stale/reused pointer — the `Weak` guard rejects that).
    pub fn string_props_vec(&self, s: &Arc<String>) -> Option<Vec<Value>> {
        let key = Arc::as_ptr(s) as usize;
        let (weak, props) = self.string_props.get(&key)?;
        weak.upgrade().filter(|a| Arc::as_ptr(a) as usize == key)?;
        Some(props.clone())
    }
    /// The property plist at char index `idx0` of string S.
    pub fn string_plist_at(&self, s: &Arc<String>, idx0: usize) -> Value {
        self.string_props_vec(s)
            .and_then(|v| v.get(idx0).cloned())
            .unwrap_or(Value::Undef)
    }
    /// Install (replacing any existing) the per-char plists for string S.
    pub fn string_set_props_vec(&mut self, s: &Arc<String>, vec: Vec<Value>) {
        let key = Arc::as_ptr(s) as usize;
        self.string_props.insert(key, (Arc::downgrade(s), vec));
    }
    /// The property vec for S, creating an all-nil one of the right length if the
    /// string has none registered yet.
    fn string_props_or_new(&self, s: &Arc<String>) -> Vec<Value> {
        self.string_props_vec(s)
            .unwrap_or_else(|| vec![Value::Undef; s.chars().count()])
    }
    /// `put-text-property` on string S over char indices `[s0, e0)`.
    pub fn string_put_prop(
        &mut self,
        s: &Arc<String>,
        s0: usize,
        e0: usize,
        prop: &Value,
        val: &Value,
    ) {
        let mut vec = self.string_props_or_new(s);
        for idx in s0..e0.min(vec.len()) {
            let cur = vec[idx].clone();
            vec[idx] = self.plist_put_copy(&cur, prop, val);
        }
        self.string_set_props_vec(s, vec);
    }
    /// `set-text-properties` on string S over `[s0, e0)` (shared PLIST slots).
    pub fn string_set_props(&mut self, s: &Arc<String>, s0: usize, e0: usize, plist: &Value) {
        let mut vec = self.string_props_or_new(s);
        for idx in s0..e0.min(vec.len()) {
            vec[idx] = plist.clone();
        }
        self.string_set_props_vec(s, vec);
    }
    /// `remove-text-properties` on string S over `[s0, e0)`.
    pub fn string_remove_prop(&mut self, s: &Arc<String>, s0: usize, e0: usize, prop: &Value) {
        let mut vec = self.string_props_or_new(s);
        for idx in s0..e0.min(vec.len()) {
            let cur = vec[idx].clone();
            vec[idx] = self.plist_remove_copy(&cur, prop);
        }
        self.string_set_props_vec(s, vec);
    }
    /// Value comparison for merging text-property intervals: `eq` semantics, but
    /// strings also compare by content (adjacent cells that were given an
    /// `equal`-string property merge, matching Emacs's shared-string intervals).
    fn merge_val_eq(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Str(x), Value::Str(y)) => x == y,
            _ => self.values_eq(a, b),
        }
    }
    /// True if PLIST A ⊆ PLIST B: every `(key value)` of A has an equal value in B
    /// (a nil value counts as absent). Used only for interval merging on print.
    fn plist_subset(&self, a: &Value, b: &Value) -> bool {
        let mut cur = a.clone();
        while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
            let k = k.clone();
            let (v, rest2) = match self.obj(d) {
                Some(Obj::Cons(v, d2)) => (v.clone(), d2.clone()),
                _ => break,
            };
            let bv = self.plist_get_eq(b, &k);
            if !self.merge_val_eq(&bv, &v) {
                return false;
            }
            cur = rest2;
        }
        true
    }
    /// Structural plist equality (same key→value set, `eq` on values) — used to
    /// merge adjacent text-property intervals when printing a propertized string.
    fn plist_struct_eq(&self, a: &Value, b: &Value) -> bool {
        self.plist_subset(a, b) && self.plist_subset(b, a)
    }
    /// The `#(...)` interval tail for a propertized string: maximal runs of chars
    /// sharing a (non-nil) property list, as ` START END (plist)` segments. Empty
    /// when the string carries no properties.
    fn string_prop_intervals(&self, s: &Arc<String>, depth: usize) -> String {
        let Some(props) = self.string_props_vec(s) else {
            return String::new();
        };
        let mut out = String::new();
        let n = props.len();
        let mut i = 0;
        while i < n {
            let mut j = i + 1;
            while j < n && self.plist_struct_eq(&props[i], &props[j]) {
                j += 1;
            }
            if el_truthy(&props[i]) {
                out.push_str(&format!(
                    " {} {} {}",
                    i,
                    j,
                    self.print_inner(&props[i], true, depth + 1)
                ));
            }
            i = j;
        }
        out
    }

    /// Resolve char `c` in char-table `ct` with Emacs `char_table_ref` fallback:
    /// the table's own char value; if nil, its `default`; if that is also nil and
    /// `parent` is a char-table, recurse into the parent. `ct` must be a
    /// `Value::Obj` pointing at an `Obj::CharTable`.
    pub fn char_table_ref(&self, ct: &Value, c: u32) -> Value {
        let mut cur = ct.clone();
        // Iterate the parent chain instead of recursing.
        loop {
            let Some(Obj::CharTable(t)) = self.obj(&cur) else {
                return Value::Undef;
            };
            let v = t.raw_get(c);
            if el_truthy(&v) {
                return v;
            }
            if el_truthy(&t.default) {
                return t.default.clone();
            }
            if matches!(self.obj(&t.parent), Some(Obj::CharTable(_))) {
                cur = t.parent.clone();
            } else {
                return Value::Undef;
            }
        }
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
        // Conditions Emacs signals with an empty DATA list: the condition object
        // is just `(SYMBOL)` with no message datum (`arith-error`, `end-of-file`,
        // `beginning-of-buffer`, `end-of-buffer`). Their human-readable text lives
        // in the symbol's `error-message`, not in the data, so drop it here. The
        // generic `error`/`user-error` symbols keep the message as data.
        const NIL_DATA_ERRORS: &[&str] = &[
            "arith-error",
            "overflow-error",
            "end-of-file",
            "beginning-of-buffer",
            "end-of-buffer",
        ];
        let trimmed = e.trim();
        let sym_candidate = trimmed.split_once(':').map_or(trimmed, |(s, _)| s.trim());
        if NIL_DATA_ERRORS.contains(&sym_candidate) {
            let s = self.intern(sym_candidate);
            return self.list_from(vec![s]);
        }
        let (sym, msg) = match e.split_once(':') {
            Some((s, m)) => (s.trim().to_string(), m.trim().to_string()),
            None => ("error".to_string(), e.to_string()),
        };
        // These conditions carry a list of *values* as DATA in Emacs, not a
        // message string: `(wrong-type-argument PREDICATE VALUE)`,
        // `(args-out-of-range ARRAY START END)`, `(void-variable SYM)`,
        // `(void-function SYM)`. The Rust helpers render those values in
        // readable form, so re-read them into separate elements.
        if matches!(
            sym.as_str(),
            "wrong-type-argument"
                | "args-out-of-range"
                | "void-variable"
                | "void-function"
                | "wrong-number-of-arguments"
        ) {
            if let Some(data) = self.read_all_forms(&msg) {
                let s = self.intern(&sym);
                return self.cons(s, data);
            }
        }
        let s = self.intern(&sym);
        let m = Value::str(msg);
        self.list_from(vec![s, m])
    }

    /// Read every form in `src` into a proper list (used to reconstruct error
    /// DATA from a rendered message). None if nothing parses.
    fn read_all_forms(&mut self, src: &str) -> Option<Value> {
        let len = src.chars().count();
        let mut forms = Vec::new();
        let mut pos = 0;
        while pos < len {
            match crate::reader::read_one(self, src, pos) {
                Ok((v, next)) if next > pos => {
                    forms.push(v);
                    pos = next;
                }
                _ => break,
            }
        }
        if forms.is_empty() {
            None
        } else {
            Some(self.list_from(forms))
        }
    }
}

/// Print a finite float the way Emacs does: the shortest round-tripping form,
/// choosing exponential notation when the decimal exponent is ≤ -5, or ≥ 15 and
/// the exponential string is shorter (so `1e15` => `1e+15` but
/// `1234567890123456.0` stays decimal). Integer-valued floats keep a `.0`.
pub fn format_float(f: f64) -> String {
    let e_full = format!("{f:e}"); // "M[.MMM]eP"
    let (mantissa, exp_part) = e_full.rsplit_once('e').unwrap();
    let exp: i64 = exp_part.parse().unwrap_or(0);
    let dec = {
        let d = format!("{f}");
        if d.contains('.') {
            d
        } else {
            format!("{d}.0")
        }
    };
    let exp_str = {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:0>2}", exp.abs())
    };
    if exp <= -5 || (exp >= 15 && exp_str.len() < dec.len()) {
        exp_str
    } else {
        dec
    }
}

/// elisp truthiness: only `nil` (fusevm `Undef`) is false.
pub fn el_truthy(v: &Value) -> bool {
    !matches!(v, Value::Undef | Value::Bool(false))
}

/// Render a symbol name the way `prin1` does: with `\` escapes so it reads back
/// as the same symbol. The empty symbol prints as `##`.
fn print_symbol_readable(name: &str) -> String {
    if name.is_empty() {
        return "##".to_string();
    }
    // A name that would read as a number, or that starts with `?`/`.`, needs a
    // leading escape so it reads back as a symbol rather than a number/char/dot.
    let numeric = crate::reader::token_is_number(name);
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        let needs_escape = matches!(
            c,
            '"' | '\\' | '\'' | ';' | '#' | '(' | ')' | ',' | '`' | '[' | ']'
        ) || (c as u32) <= 0x20
            || (i == 0 && (numeric || c == '?' || c == '.'));
        if needs_escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
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
            "funcall" => {
                // `(funcall)` with no function designator: Emacs signals
                // `(wrong-number-of-arguments funcall 0)`, not a panic.
                if args.is_empty() {
                    return Err("wrong-number-of-arguments: funcall 0".to_string());
                }
                return call_function(&args[0], &args[1..]);
            }
            "apply" => {
                if args.is_empty() {
                    return Err("wrong-number-of-arguments: apply 0".to_string());
                }
                // apply spreads its LAST argument, which must be a list; with a
                // single argument that last IS `args[0]` (so `(apply '+)` fails
                // with `(wrong-type-argument listp +)`, matching Emacs).
                let spread = args.last().unwrap();
                let tail = with_host(|h| h.list_vec(spread)).ok_or_else(|| {
                    format!(
                        "wrong-type-argument: listp {}",
                        with_host(|h| h.print(spread, true))
                    )
                })?;
                let mut a: Vec<Value> = if args.len() >= 2 {
                    args[1..args.len() - 1].to_vec()
                } else {
                    Vec::new()
                };
                a.extend(tail);
                return call_function(&args[0], &a);
            }
            "mapcar" => {
                if args.len() < 2 {
                    return Err(format!("wrong-number-of-arguments: mapcar {}", args.len()));
                }
                let seq = with_host(|h| h.seq_vec(&args[1])).ok_or("mapcar: not a sequence")?;
                let mut out = Vec::with_capacity(seq.len());
                for e in seq {
                    out.push(call_function(&args[0], &[e])?);
                }
                return Ok(with_host(|h| h.list_from(out)));
            }
            "mapc" => {
                if args.len() < 2 {
                    return Err(format!("wrong-number-of-arguments: mapc {}", args.len()));
                }
                let seq = with_host(|h| h.seq_vec(&args[1])).ok_or("mapc: not a sequence")?;
                for e in seq {
                    call_function(&args[0], &[e])?;
                }
                return Ok(args[1].clone());
            }
            "sort" => {
                // Stable sort of a list/vector. Supports the classic
                // (sort SEQ PRED), the Emacs-30 keyword form
                // (sort SEQ &key :lessp :key :reverse), and (sort SEQ) which
                // falls back to the default `value<` ordering. Re-enters elisp
                // for PRED/:key so it lives here, not as a plain subr.
                if args.is_empty() {
                    return Err("wrong-number-of-arguments: sort 0".to_string());
                }
                let (items, was_vec) = with_host(|h| match h.obj(&args[0]) {
                    Some(Obj::Vector(v)) => (v.clone(), true),
                    _ => (h.list_vec(&args[0]).unwrap_or_default(), false),
                });
                let is_kw =
                    |v: &Value| with_host(|h| h.sym_name(v)).is_some_and(|n| n.starts_with(':'));
                let mut pred: Option<Value> = None;
                let mut key: Option<Value> = None;
                let mut reverse = false;
                // The classic `(sort SEQ PRED)` form sorts in place; the Emacs-30
                // keyword form is non-destructive unless `:in-place t`.
                let mut in_place;
                if args.len() == 2 && !is_kw(&args[1]) {
                    pred = Some(args[1].clone());
                    in_place = true;
                } else {
                    in_place = false;
                    let mut idx = 1;
                    while idx < args.len() {
                        let kw = with_host(|h| h.sym_name(&args[idx])).unwrap_or_default();
                        let val = args.get(idx + 1).cloned().unwrap_or(Value::Undef);
                        let truthy = !matches!(val, Value::Undef | Value::Bool(false));
                        match kw.as_str() {
                            ":lessp" | ":predicate" => pred = Some(val),
                            ":key" => {
                                if truthy {
                                    key = Some(val)
                                }
                            }
                            ":reverse" => reverse = truthy,
                            ":in-place" => in_place = truthy,
                            _ => {}
                        }
                        idx += 2;
                    }
                }
                let mut pairs: Vec<(Value, Value)> = Vec::with_capacity(items.len());
                for it in &items {
                    let k = match &key {
                        Some(kf) => call_function(kf, std::slice::from_ref(it))?,
                        None => it.clone(),
                    };
                    pairs.push((k, it.clone()));
                }
                merge_sort_by(&mut pairs, pred.as_ref())?;
                let mut sorted: Vec<Value> = pairs.into_iter().map(|(_, it)| it).collect();
                if reverse {
                    sorted.reverse();
                }
                // In-place forms write the sorted values back into the original
                // sequence and return it; otherwise build a fresh one.
                return Ok(with_host(|h| {
                    if !in_place {
                        return if was_vec {
                            h.alloc(Obj::Vector(sorted))
                        } else {
                            h.list_from(sorted)
                        };
                    }
                    if was_vec {
                        if let Value::Obj(id) = &args[0] {
                            if let Some(Obj::Vector(v)) = h.arena.get_mut(*id as usize) {
                                *v = sorted;
                                return args[0].clone();
                            }
                        }
                        h.alloc(Obj::Vector(sorted))
                    } else {
                        let mut cur = args[0].clone();
                        for val in sorted {
                            let next = match h.obj(&cur) {
                                Some(Obj::Cons(_, cdr)) => cdr.clone(),
                                _ => break,
                            };
                            if let Value::Obj(id) = cur {
                                if let Some(Obj::Cons(car, _)) = h.arena.get_mut(id as usize) {
                                    *car = val;
                                }
                            }
                            cur = next;
                        }
                        args[0].clone()
                    }
                }));
            }
            "maphash" => {
                if args.len() < 2 {
                    return Err(format!("wrong-number-of-arguments: maphash {}", args.len()));
                }
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
            "mapatoms" => {
                if args.is_empty() {
                    return Err(format!(
                        "wrong-number-of-arguments: mapatoms {}",
                        args.len()
                    ));
                }
                // The obarray defaults to the global one (the `obarray` variable).
                let ob = match args.get(1) {
                    Some(v) if !matches!(v, Value::Undef | Value::Bool(false)) => v.clone(),
                    _ => with_host(|h| {
                        let sym = h.find_symbol("obarray").unwrap_or(Value::Undef);
                        h.get_value(&sym).unwrap_or(Value::Undef)
                    }),
                };
                let syms = with_host(|h| h.obarray_symbols(&ob));
                for s in syms {
                    call_function(&args[0], &[s])?;
                }
                return Ok(Value::Undef);
            }
            // `load` reads a file's forms and evaluates them in the live host —
            // re-entrant (nested VM per form) and it dynamically rebinds
            // `load-file-name` &c, so it lives here, outside any host borrow.
            "load" => return intrinsic_load(args),
            // `eval` macroexpands, compiles, and runs a form — re-entrant, so it
            // lives here (outside any host borrow), like the other intrinsics.
            "eval" => {
                let form = args.first().ok_or("wrong-number-of-arguments: eval")?;
                // `t` is a self-evaluating constant symbol (Emacs `eval_sub`:
                // its value slot holds itself).  It is represented as
                // `Value::Bool(true)`, which `compile_top` would lower to the
                // integer 1, so short-circuit it here to return `t` itself.
                if matches!(form, Value::Bool(true)) {
                    return Ok(form.clone());
                }
                let expanded = macroexpand_all(form)?;
                let chunk = with_host(|h| crate::compiler::compile_top(h, &expanded))?;
                return run_chunk(chunk);
            }
            // The macro-expansion functions run macro expanders (re-entrant).
            "macroexpand-1" => {
                let form = args
                    .first()
                    .ok_or("wrong-number-of-arguments: macroexpand-1")?;
                return Ok(macroexpand_1(form)?.unwrap_or_else(|| form.clone()));
            }
            "macroexpand" => {
                // Expand the head to a fixpoint; don't recurse into sub-forms.
                let mut f = args
                    .first()
                    .ok_or("wrong-number-of-arguments: macroexpand")?
                    .clone();
                while let Some(e) = macroexpand_1(&f)? {
                    f = e;
                }
                return Ok(f);
            }
            "macroexpand-all" => {
                return macroexpand_all(
                    args.first()
                        .ok_or("wrong-number-of-arguments: macroexpand-all")?,
                )
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
        let Ok(caps) = caps else { break };
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
                from_buffer: false,
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
fn num_f(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// Default `value<` ordering used by `(sort SEQ)` with no predicate: numbers
/// compare numerically, strings and symbol names lexically.
fn value_lt(a: &Value, b: &Value) -> Result<bool, String> {
    if let (Some(x), Some(y)) = (num_f(a), num_f(b)) {
        return Ok(x < y);
    }
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        return Ok(x < y);
    }
    match (with_host(|h| h.sym_name(a)), with_host(|h| h.sym_name(b))) {
        (Some(x), Some(y)) => Ok(x < y),
        _ => Err("value<: unsupported comparison".into()),
    }
}

/// Stable merge sort of `(key, item)` pairs by `key`. With `pred` it re-enters
/// elisp `(pred key_j key_i)`; without, it falls back to `value_lt`.
fn merge_sort_by(items: &mut Vec<(Value, Value)>, pred: Option<&Value>) -> Result<(), String> {
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
        let rhs_first = match pred {
            Some(p) => {
                let r = call_function(p, &[right[j].0.clone(), left[i].0.clone()])?;
                !matches!(r, Value::Undef | Value::Bool(false))
            }
            None => value_lt(&right[j].0, &left[i].0)?,
        };
        if rhs_first {
            items.push(right[j].clone());
            j += 1;
        } else {
            items.push(left[i].clone());
            i += 1;
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
    let entry = with_host(|h| h.scope_depth());
    let setup = with_host(|h| {
        h.open_scope_in(env.clone());
        h.bind_params_into_scope(params, args)
    });
    if let Err(e) = setup {
        with_host(|h| h.unwind_scopes_to(entry));
        return Err(e);
    }
    let result = run_chunk((**body).clone());
    // Unwind to the entry depth (not just one scope): a `throw`/error out of an
    // inner `let` inside the body leaks scopes that this restores.
    with_host(|h| h.unwind_scopes_to(entry));
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
/// sub-forms), without descending into quoted data or into positions that are
/// not expression forms. Run before lowering.
///
/// Special forms with irregular shapes are handled explicitly so their
/// non-expression subparts are never mistaken for macro calls: a `let` binding
/// `(VAR INIT)` must not have `VAR` expanded, which matters because a symbol can
/// be *both* a special variable and a macro (e.g. `delay-mode-hooks`) — expanding
/// the binding head there loops forever.
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
    let head = with_host(|h| h.sym_name(&elems[0]));
    match head.as_deref() {
        // Quoted data is never expanded.
        Some("quote") | Some("function") => Ok(f),
        // Binding forms: expand each binding's INIT (never the VAR, which may name
        // a macro) and the body forms; keep the head and the binding names as-is.
        Some(kw @ ("let" | "let*")) => {
            let bindings = with_host(|h| h.list_vec(elems.get(1).unwrap_or(&Value::Undef)));
            let new_bindings = match bindings {
                Some(bs) => {
                    let mut out = Vec::with_capacity(bs.len());
                    for bd in &bs {
                        // A bare symbol binding stays as-is; a `(VAR INIT...)`
                        // list has only its INIT expressions expanded.
                        let parts = with_host(|h| {
                            if matches!(h.obj(bd), Some(Obj::Cons(..))) {
                                h.list_vec(bd)
                            } else {
                                None
                            }
                        });
                        match parts {
                            Some(parts) if !parts.is_empty() => {
                                let mut np = Vec::with_capacity(parts.len());
                                np.push(parts[0].clone()); // VAR, untouched
                                for p in &parts[1..] {
                                    np.push(macroexpand_all(p)?);
                                }
                                out.push(with_host(|h| h.list_from(np)));
                            }
                            _ => out.push(bd.clone()),
                        }
                    }
                    with_host(|h| h.list_from(out))
                }
                None => elems.get(1).cloned().unwrap_or(Value::Undef),
            };
            let mut out = Vec::with_capacity(elems.len());
            out.push(elems[0].clone());
            out.push(new_bindings);
            for e in &elems[2..] {
                out.push(macroexpand_all(e)?);
            }
            let _ = kw;
            Ok(with_host(|h| h.list_from(out)))
        }
        // `(lambda ARGLIST . BODY)`: the ARGLIST is a parameter list, not code —
        // a parameter named after a macro (e.g. `rx`) must NOT be macroexpanded.
        // Keep head + ARGLIST verbatim; expand only the body forms.
        Some("lambda") if elems.len() >= 2 => {
            let mut out = Vec::with_capacity(elems.len());
            out.push(elems[0].clone());
            out.push(elems[1].clone()); // ARGLIST, untouched
            for e in &elems[2..] {
                out.push(macroexpand_all(e)?);
            }
            Ok(with_host(|h| h.list_from(out)))
        }
        // `(defun|defmacro NAME ARGLIST . BODY)`: same protection for the ARGLIST
        // (and NAME); only the body forms are expression positions.
        Some(construct @ ("defun" | "defmacro")) if elems.len() >= 3 => {
            // Faithful byte-run.el `declare' handling: `defun'/`defmacro' are
            // macros in Emacs that process the `(declare ...)' specs (registering
            // gv-setters, obsolete/indent/doc-string props, …).  elisprs keeps
            // them as compiler special forms, so we delegate to the prelude bridge
            // `elisprs--expand-defun-declarations', which returns a rewritten
            // definition threading each spec's runtime side-effect form after it.
            // Guarded on fboundp so early-prelude defuns (compiled before the
            // bridge is defined) keep the pre-bridge behavior — no bootstrap cycle.
            let bridge_ready = with_host(|h| {
                let s = h.intern("elisprs--expand-defun-declarations");
                h.is_fbound(&s)
            });
            if bridge_ready {
                let (bridge, cons_sym, name, arglist, body_list) = with_host(|h| {
                    let bridge = h.intern("elisprs--expand-defun-declarations");
                    let cons_sym = h.intern(construct);
                    let body_list = h.list_from(elems[3..].to_vec());
                    (
                        bridge,
                        cons_sym,
                        elems[1].clone(),
                        elems[2].clone(),
                        body_list,
                    )
                });
                let replaced = call_function(&bridge, &[cons_sym, name, arglist, body_list])?;
                // Non-nil ⇒ BODY had a `declare'; expand the rewritten form (its
                // inner defun has the `declare' stripped, so this does not recurse).
                if el_truthy(&replaced) {
                    return macroexpand_all(&replaced);
                }
            }
            let mut out = Vec::with_capacity(elems.len());
            out.push(elems[0].clone());
            out.push(elems[1].clone()); // NAME, untouched
            out.push(elems[2].clone()); // ARGLIST, untouched
            for e in &elems[3..] {
                out.push(macroexpand_all(e)?);
            }
            Ok(with_host(|h| h.list_from(out)))
        }
        _ => {
            let mut out = Vec::with_capacity(elems.len());
            for e in &elems {
                out.push(macroexpand_all(e)?);
            }
            Ok(with_host(|h| h.list_from(out)))
        }
    }
}

/// `(catch TAG THUNK)` — run the thunk; if a `throw` to a matching tag unwinds
/// out of it, return the thrown value; otherwise re-propagate.
/// Resolve a load candidate to an absolute path string, expanding `~/` and
/// making relative paths absolute against the process cwd (which the elisp
/// `default-directory` mirrors). No path is required to exist here.
pub(crate) fn load_abspath(candidate: &str) -> std::path::PathBuf {
    if let Some(rest) = candidate.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    let p = std::path::PathBuf::from(candidate);
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p)
    }
}

/// `(load FILE &optional NOERROR NOMESSAGE NOSUFFIX MUST-SUFFIX)` — port of
/// Emacs's `Fload`/`openp` semantics (behavior, not line numbers; this repo has
/// no vendored C).
///
/// Resolution: if FILE has a directory component (or is absolute/`~`), it is
/// used as-is; otherwise each directory in `load-path` is tried (falling back to
/// cwd when `load-path` is empty). For each base, suffixes are tried in order:
/// `.el`, `.el.gz`, the exact name, then its `.gz` variant (Emacs would try `.elc`
/// first, but elisprs emits no bytecode so no `.elc` ever exists). The `.gz`
/// variants are jka-compr's `load-file-rep-suffixes`; a resolved `.gz` file is
/// gunzipped in memory. NOSUFFIX limits the search to the exact name (and its
/// `.gz`); MUST-SUFFIX requires a `load-suffixes` extension (`.el`/`.el.gz` here).
///
/// While the file's forms run, `load-file-name`, `load-true-file-name` and
/// `load-in-progress` are dynamically bound and restored afterward — even if a
/// form errors (the specstack is unwound to the pre-load depth).
fn intrinsic_load(args: &[Value]) -> Result<Value, String> {
    let file = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => {
            return Err(format!(
                "wrong-type-argument: stringp {}",
                other.as_str_cow()
            ))
        }
        None => return Err("wrong-number-of-arguments: load".to_string()),
    };
    let noerror = args.get(1).is_some_and(el_truthy);
    let nosuffix = args.get(3).is_some_and(el_truthy);
    let must_suffix = args.get(4).is_some_and(el_truthy);

    // Suffixes to append, in Emacs's search order. Each `load-suffixes` entry is
    // crossed with `load-file-rep-suffixes` = `("" ".gz")` (jka-compr's compressed
    // variant — the stock Emacs lisp tree ships as `*.el.gz`), then the bare
    // rep-suffixes are appended for the exact-name pass. `.elc` is skipped —
    // elisprs writes no bytecode files. NOSUFFIX drops the load-suffixes; MUST-SUFFIX
    // drops the bare exact-name pass. Mirrors `Fget_load_suffixes` + `openp` order.
    let suffixes: &[&str] = if nosuffix {
        &["", ".gz"]
    } else if must_suffix {
        &[".el", ".el.gz"]
    } else {
        &[".el", ".el.gz", "", ".gz"]
    };

    // Base names: FILE alone if it carries a directory component, else each
    // `load-path` entry joined with FILE (cwd when `load-path` is empty/unset).
    let has_dir = file.contains('/') || file.starts_with('~');
    let bases: Vec<String> = if has_dir {
        vec![file.clone()]
    } else {
        let lp = with_host(|h| {
            let sym = h.intern("load-path");
            h.get_value(&sym).ok().and_then(|v| h.list_vec(&v))
        });
        let dirs: Vec<String> = lp
            .unwrap_or_default()
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .collect();
        if dirs.is_empty() {
            vec![format!("./{file}")]
        } else {
            dirs.iter()
                .map(|d| {
                    if d.ends_with('/') {
                        format!("{d}{file}")
                    } else {
                        format!("{d}/{file}")
                    }
                })
                .collect()
        }
    };

    // First existing (base + suffix) wins.
    let mut resolved: Option<std::path::PathBuf> = None;
    'search: for base in &bases {
        for suf in suffixes {
            let cand = load_abspath(&format!("{base}{suf}"));
            if cand.is_file() {
                resolved = Some(cand);
                break 'search;
            }
        }
    }

    let path = match resolved {
        Some(p) => p,
        None => {
            if noerror {
                return Ok(Value::Undef);
            }
            // Emacs signals `file-missing` "Cannot open load file: FILE".
            return Err(format!(
                "file-missing: Cannot open load file: No such file or directory, {file}"
            ));
        }
    };

    // Read the resolved file. A `.gz` target is decompressed in memory (jka-compr
    // does the same via `load` -> `insert-file-contents` -> `jka-compr-insert`),
    // so a stock `*.el.gz` library evaluates identically to its `.el` form.
    let src = if path.extension().and_then(|e| e.to_str()) == Some("gz") {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("file-error: Cannot open load file: {}: {e}", path.display()))?;
        let mut dec = flate2::read::GzDecoder::new(&bytes[..]);
        let mut s = String::new();
        std::io::Read::read_to_string(&mut dec, &mut s)
            .map_err(|e| format!("file-error: uncompressing {}: {e}", path.display()))?;
        s
    } else {
        std::fs::read_to_string(&path)
            .map_err(|e| format!("file-error: Cannot open load file: {}: {e}", path.display()))?
    };
    let abs = Value::str(path.to_string_lossy().into_owned());

    // Dynamically bind the load vars, remembering the pre-load specstack depth
    // so we can unwind them even if a form errors.
    let depth = with_host(|h| {
        let d = h.specdepth();
        let lfn = h.intern("load-file-name");
        let ltn = h.intern("load-true-file-name");
        let lip = h.intern("load-in-progress");
        let _ = h.specbind(&lfn, abs.clone());
        let _ = h.specbind(&ltn, abs.clone());
        let _ = h.specbind(&lip, Value::Bool(true));
        d
    });

    let result = crate::run_top_forms(&src);
    with_host(|h| h.unbind_to(depth));

    result.map(|_| Value::Bool(true))
}

fn intrinsic_catch(args: &[Value]) -> Result<Value, String> {
    let tag = args.first().cloned().unwrap_or(Value::Undef);
    let thunk = args.get(1).cloned().unwrap_or(Value::Undef);
    with_host(|h| h.catch_tags.push(tag.clone()));
    let result = call_function(&thunk, &[]);
    with_host(|h| {
        h.catch_tags.pop();
    });
    match result {
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
    // Running the body forward: any leftover error object is stale.
    with_host(|h| h.pending_error = None);
    match call_function(&body, &[]) {
        Ok(v) => {
            // A `(:success BODY…)` handler runs on normal return, with VAR bound
            // to the body's value.
            let hlist = with_host(|h| h.list_vec(&handlers)).unwrap_or_default();
            for hp in hlist {
                let parts = with_host(|h| h.list_vec(&hp)).unwrap_or_default();
                if parts.len() < 2 {
                    continue;
                }
                let cname = with_host(|h| h.sym_name(&parts[0])).unwrap_or_default();
                if cname == ":success" {
                    let depth = with_host(|h| {
                        let d = h.specdepth();
                        if matches!(h.obj(&var), Some(Obj::Symbol(_))) {
                            let _ = h.specbind(&var, v.clone());
                        }
                        d
                    });
                    let hr = call_function(&parts[1], &[]);
                    with_host(|h| h.unbind_to(depth));
                    return hr;
                }
            }
            Ok(v)
        }
        Err(e) => {
            // A throw is not an error — let it keep unwinding to its catch.
            if with_host(|h| h.pending_throw.is_some()) {
                return Err(e);
            }
            // Prefer the structured error object's symbol over the message string.
            let esym: String = with_host(|h| {
                h.pending_error
                    .as_ref()
                    .and_then(|eo| h.obj(eo))
                    .and_then(|o| match o {
                        Obj::Cons(car, _) => h.sym_name(car),
                        _ => None,
                    })
            })
            .unwrap_or_else(|| e.split(':').next().unwrap_or("error").trim().to_string());
            let hlist = with_host(|h| h.list_vec(&handlers)).unwrap_or_default();
            // The signaled symbol's `error-conditions` (itself + parents, via
            // define-error); a handler matches any condition on this chain.
            let getfn = with_host(|h| h.intern("get"));
            let symv = with_host(|h| h.intern(&esym));
            let propv = with_host(|h| h.intern("error-conditions"));
            let mut signal_conditions: Vec<String> = call_function(&getfn, &[symv, propv])
                .ok()
                .and_then(|v| with_host(|h| h.list_vec(&v)))
                .map(|items| with_host(|h| items.iter().filter_map(|x| h.sym_name(x)).collect()))
                .unwrap_or_default();
            // `overflow-error`/`range-error` are signalled by float-rounding subrs
            // but their `define-error` chain lives in the elisp prelude, which may
            // not register them; supply Emacs's fixed parent chain so an
            // `arith-error`/`range-error` handler still catches an overflow.
            if signal_conditions.is_empty() {
                let chain: &[&str] = match esym.as_str() {
                    "overflow-error" => &["overflow-error", "range-error", "arith-error", "error"],
                    "range-error" => &["range-error", "arith-error", "error"],
                    _ => &[],
                };
                signal_conditions = chain.iter().map(|s| s.to_string()).collect();
            }
            for hp in hlist {
                let parts = with_host(|h| h.list_vec(&hp)).unwrap_or_default();
                if parts.len() < 2 {
                    continue;
                }
                // A handler condition is a symbol or a list of symbols; it matches
                // if any names `error`/`t`, the signaled condition, or a parent of
                // it (per the signal's error-conditions chain).
                let conds: Vec<String> = with_host(|h| match h.sym_name(&parts[0]) {
                    Some(name) => vec![name],
                    None => h
                        .list_vec(&parts[0])
                        .map(|items| items.iter().filter_map(|x| h.sym_name(x)).collect())
                        .unwrap_or_default(),
                });
                if conds.iter().any(|c| {
                    c == "error" || c == "t" || *c == esym || signal_conditions.contains(c)
                }) {
                    let depth = with_host(|h| {
                        let d = h.specdepth();
                        if matches!(h.obj(&var), Some(Obj::Symbol(_))) {
                            // Bind to the real (SYMBOL . DATA) object when we have
                            // it; otherwise reconstruct one from the message.
                            let eobj = h
                                .pending_error
                                .take()
                                .unwrap_or_else(|| h.make_error_object(&e));
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
