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

use fusevm::{Chunk, NumOp, VMResult, Value, VM};
use num_bigint::BigInt;
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
#[derive(Serialize, Deserialize, Clone)]
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
        /// Whether the global obarray maps `name` to *this* symbol.
        ///
        /// An uninterned symbol (a `make-symbol`/gensym result, a lambda
        /// parameter, a `let` binding in a macro body) has a name but no obarray
        /// entry. Without this flag the image cannot tell the two apart, and
        /// re-interning every symbol on import silently rebinds the global name
        /// to the uninterned copy: the prelude binds a local named `exp`
        /// (`prelude.rs`), so a cache hit used to shadow the `exp` builtin with a
        /// symbol that has no function cell — `(exp 1)` answered `void-function`
        /// on a warm cache and worked on a cold one.
        #[serde(default)]
        interned: bool,
    },
    Vector(Vec<Value>),
    Record(Vec<Value>),
    BoolVector(Vec<bool>),
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
        /// The captured lexical environment, innermost binding first, as
        /// `(symbol-handle, value)`. A closure without its captures is a closure
        /// whose body cannot run.
        #[serde(default)]
        env: Vec<(u32, Value)>,
        /// Dynamic-binding closure — see [`Obj::Closure::dynamic`].
        #[serde(default)]
        dynamic: bool,
        /// The closure's printable source: its arglist as written and its body
        /// forms ([`ClosureSrc`]). Both are ordinary heap `Value`s already in
        /// this image, so they cost handles, not a second encoding.
        ///
        /// Omitting them was a silent wrong answer, not a missing feature: the
        /// closure still *ran*, but printed as `#[nil () (t)]` because the
        /// importer rebuilt it with `ClosureSrc::default()`. That hit both
        /// non-interpreted paths — an `--aot-exe` binary, and the ordinary rkyv
        /// script cache from the second run onward:
        ///
        /// ```text
        /// (princ (prin1-to-string (lambda (x) (* x 2))))   ; lexical-binding: t
        ///   emacs 30.2   #[(x) ((* x 2)) (t)]
        ///   cold run     #[(x) ((* x 2)) (t)]
        ///   cache hit    #[nil () (t)]        <- source gone
        /// ```
        ///
        /// `#[serde(default)]` only helps the AOT image, which is serde_json and
        /// therefore self-describing. bincode — the script cache's encoding —
        /// reads fields positionally and ignores the attribute entirely, so a
        /// heap image written before these fields does NOT load: the decode runs
        /// off the end of the closure and into the next object. That is why
        /// `cache::SHARD_FORMAT_VERSION` is bumped to 6 alongside this field;
        /// the header check rejects a stale shard before any inner decode.
        #[serde(default)]
        arglist: Value,
        /// See `arglist`.
        #[serde(default)]
        src_body: Vec<Value>,
    },
    Bignum(BigInt),
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
    pub const DBG_LINE: u16 = 10; // DAP statement marker (debug only): fire dap::check_line
    pub const CHECK_ARITY: u16 = 11; // arg=argc; pop sym; signal if it names a subr rejecting argc
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
    /// A single binding, linked in front of `parent`.
    pub(crate) fn new(sym: u32, val: Value, parent: Lex) -> Self {
        Scope {
            sym,
            val: RefCell::new(val),
            parent,
        }
    }

    /// The bound symbol's arena handle.
    pub(crate) fn sym_handle(&self) -> u32 {
        self.sym
    }
    /// The binding's current value.
    pub(crate) fn value(&self) -> Value {
        self.val.borrow().clone()
    }
    /// The enclosing scope.
    pub(crate) fn parent_lex(&self) -> Lex {
        self.parent.clone()
    }

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
    /// An Emacs record (`record`/`make-record`, `#s(NAME …)`). Slot 0 holds the
    /// type symbol (the bare NAME, exactly as passed — `(aref rec 0)` returns it),
    /// slots 1.. hold the fields. A record is a *distinct* type from a vector:
    /// `recordp` is true and `vectorp` is nil, and — unlike a vector — a record is
    /// NOT a sequence (`vconcat`/`append`/`mapcar` signal `sequencep`), only
    /// `aref`/`aset`/`length`/`copy-sequence` apply. This is the storage for every
    /// `cl-defstruct` instance (and the cl-generic/EIEIO class descriptors built on
    /// them), whose slot 0 is the struct type symbol.
    Record(Vec<Value>),
    /// An Emacs bool-vector (`make-bool-vector`/`bool-vector`, `#&N"…"`). Each
    /// element is `t` or `nil`; stored one `bool` per element (the LSB-first byte
    /// packing is materialized only for the `#&N"…"` printed/read form). A
    /// bool-vector is an array and a sequence (so `aref`/`aset`/`length`/`elt`/
    /// `append`/`mapcar` apply) but NOT a vector (`vectorp` is nil).
    BoolVector(Vec<bool>),
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
        /// Dynamic-binding closure (`lexical-binding` nil): it captures nothing,
        /// binds its parameters on the specstack, and runs its body in dynamic
        /// mode, so every free variable resolves through the symbols' value cells
        /// at *call* time. Emacs prints such a function with a `nil` environment
        /// (`#[(y) (x) nil]`) where a lexical one with no captures prints `(t)`.
        dynamic: bool,
        /// The closure's *source*: its arglist and its body forms, kept so it can
        /// print the way Emacs prints an interpreted closure —
        /// `#[(x) ((list x x)) (t)]` — rather than as an opaque `#<closure>`.
        /// Emacs's interpreted closure is its source; elisprs lowers the body to a
        /// `Chunk`, so the forms would otherwise be gone. Shared (`Rc`) with every
        /// instance made from the same template, so capturing a closure in a loop
        /// does not copy the source.
        src: Rc<ClosureSrc>,
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
    /// An integer too large for a fixnum. Emacs has no fixed-width integers: an
    /// arithmetic result that leaves fixnum range (±2^61, see
    /// `most-positive-fixnum`) becomes a bignum, and stays exact. `type-of` still
    /// answers `integer` — bignum-ness is an implementation detail of the same
    /// type, which is why `integerp` accepts these and `fixnump` does not.
    ///
    /// Never in fixnum range: [`ElispHost::make_integer`] is the only constructor
    /// and it demotes a small value back to `Value::Int`, so `eql`/`equal` can
    /// compare a bignum against a fixnum by value without a cross-representation
    /// case, and two equal bignums always have the same printed form.
    Bignum(BigInt),
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
        /// Dynamic-binding function — see [`Obj::Closure::dynamic`].
        dynamic: bool,
        /// The closure object indirection landed on. `funcall_lambda` signals
        /// `(wrong-number-of-arguments FUN NARGS)` with `fun`, the *resolved*
        /// function — never the symbol the caller wrote — so a wrong-arity call
        /// to a `defun` names `#[(a) (a) (t)]`, not `f1`. (A subr is the other
        /// way round: `eval_sub` signals with `original_fun`, the symbol.)
        object: Value,
    },
}

/// What a designator's function cell names *right now*, for the pre-argument
/// arity guard. Deliberately cheaper than [`ElispHost::resolve_function`]: it
/// clones no closure body, parameter list or captured environment, because on
/// the shape it exists to wave through it must cost as little as possible.
pub enum FnKind {
    /// A subr, with its `(min, max)` arity (`max: None` is Emacs's `MANY`).
    Subr(usize, Option<usize>),
    /// A closure, a macro, or any other callable object.
    Other,
    /// Nothing callable: no function cell, or an alias chain that dead-ends.
    Vacant,
}

impl FnKind {
    /// Whether Emacs rejects a call of `argc` arguments *before* evaluating any
    /// of them. Only a subr's arity is checked that early: `eval_sub` reads
    /// `XSUBR (fun)->max_args` and signals straight from the argument-count
    /// switch, while a closure reaches `funcall_lambda` only after its arguments
    /// have already been evaluated into a vector.
    pub fn rejects_before_args(&self, argc: usize) -> bool {
        match self {
            FnKind::Subr(min, max) => argc < *min || max.is_some_and(|m| argc > m),
            _ => false,
        }
    }
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
    /// Whether the code now running was evaluated with `lexical-binding` nil.
    /// Emacs picks the binding mode per *evaluated form* — `eval`'s LEXICAL
    /// argument, or the file-local variable a file was loaded with — so this is a
    /// dynamic-extent flag, saved and restored around `eval` and around every call
    /// into a closure that remembers the mode it was made in.
    ///
    /// While set: `let`/`let*`/parameter bindings all go on the specstack (every
    /// symbol behaves as if `defvar`'d), and a `lambda` captures nothing. Default
    /// `false` — elisprs's own prelude and every file it loads are lexical.
    pub(crate) dynamic_binding: bool,
    /// Elisp call depth (`run_closure` nesting), bounded by `max-lisp-eval-depth`.
    pub(crate) eval_depth: usize,
    /// Arena handle of `max-lisp-eval-depth`, resolved once on first use so the
    /// per-call limit check is an array index rather than an obarray lookup.
    max_depth_sym: Option<u32>,
    /// `(macro . FUNCTION)` stand-ins for the macros the compiler lowers as
    /// intrinsics — see [`ElispHost::introspect_function_cell`].
    intrinsic_macro_cells: HashMap<u32, Value>,
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
    pub(crate) pending_error: Option<(String, Value)>,
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
    /// `print-circle` label table for ONE print call: arena id → print.c's status
    /// field, using the same encoding `Vprint_number_table` does. `0` is `Qt`
    /// ("candidate, seen once, no label"); `-N` is "label N assigned by
    /// [`ElispHost::print_preprocess`], not yet printed"; `N` is "already printed
    /// as `#N=`, so print `#N#`". Populated before printing when `print-circle` is
    /// non-nil, empty otherwise. `RefCell` because the printer runs on `&self`.
    pub(crate) print_labels: RefCell<HashMap<u32, i64>>,
    /// print.c `print_number_index`: the next unused `#N=` label. Assigned during
    /// [`ElispHost::print_preprocess`], not while printing.
    pub(crate) print_next_label: Cell<usize>,
    /// print.c `being_printed[PRINT_CIRCLE]`: the chain of objects currently open,
    /// indexed by print nesting depth. With `print-circle` nil, print.c scans
    /// `being_printed[0 .. print_depth)` for `BASE_EQ (obj, …)` and emits `#I`
    /// instead of recursing — that is how a self-referencing *car* (or vector,
    /// record or hash-table slot) terminates: `(let ((x (list 1))) (setcar x x) x)`
    /// prints `(#0)`. Only an aggregate can occupy an index below the current
    /// depth (print.c writes a scalar's slot and decrements `print_depth` again
    /// immediately), and every elisprs aggregate is a `Value::Obj`, so the slot
    /// holds an arena id and `None` stands for "not a heap object" — comparing ids
    /// is `BASE_EQ` restricted to the cases that can ever match.
    pub(crate) print_being: RefCell<Vec<Option<u32>>>,
    /// `Vprint_circle` sampled once at the top of a `print` call. print.c re-reads
    /// the global at every object; caching it keeps the per-object cost to a `Cell`
    /// read while preserving the branch it selects — the `being_printed` scan *and*
    /// the `PRINT_CIRCLE` depth ceiling live inside `if (NILP (Vprint_circle))`, so
    /// with the label table on a 250-deep nest prints instead of signalling.
    pub(crate) print_circle_on: Cell<bool>,
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
    /// Registered AOP pattern-intercepts (elisprs extension, ported from zshrs's
    /// `src/extensions/intercepts.rs`). This is the GLOB/pattern-matching advice
    /// layer — distinct from elisp's per-symbol nadvice (`advice-add`): one
    /// registration fires across every symbol whose name matches a glob such as
    /// `"forward-*"`, `"_*"`, or `"all"`, with before/after/around timing and a
    /// `proceed` protocol. Fired from [`call_function`]. Runtime-only (never
    /// serialized). See [`crate::intercepts`].
    pub(crate) intercepts: Vec<crate::intercepts::Intercept>,
    /// Symbols that elisp-level `fset`/`defalias` has pointed at a **subr**.
    ///
    /// Such a symbol has shown it can change what kind of function it names, so
    /// a later call to it gets the pre-argument arity guard even when the cell
    /// live at compile time would accept the count: the next `fset` can retarget
    /// it at a narrower subr before the call runs, and Emacs checks the cell
    /// that is live at the call. Only the `fset` subr records here — `defun`
    /// lowers to the `FSET` op and `defsubr` installs the builtins, so neither
    /// puts an ordinary definition in this set. Runtime-only (never serialized).
    pub(crate) subr_aliased: std::collections::HashSet<u32>,
    /// Re-entrancy guard for the intercept layer: set while an advice body (or a
    /// proceeded original) runs so nested function calls dispatch normally instead
    /// of re-triggering intercepts (prevents infinite recursion when advice calls
    /// a function its own pattern matches).
    pub(crate) intercept_active: bool,
    /// The callee + argument values of the function currently under an `around`
    /// intercept, so `(intercept-proceed)` can run the original. `None` outside an
    /// active around advice.
    pub(crate) intercept_current: Option<(Value, Vec<Value>)>,
    /// Set by `(intercept-proceed)` — records that an around advice ran the
    /// original command (mirrors zshrs's `__intercept_proceed` flag).
    pub(crate) intercept_proceeded: bool,
    /// Source line (1-based) of each list form, keyed by the head cons's arena
    /// handle. Populated by the reader; read by the compiler in debug mode to
    /// emit the DAP statement markers that drive line-level breakpoints/stepping.
    /// Session-local (never serialized); a fresh `reset_host` clears it.
    pub(crate) form_lines: HashMap<u32, u32>,
}

/// A closure's printable source: the arglist as written and the body forms.
#[derive(Default)]
pub struct ClosureSrc {
    pub arglist: Value,
    pub body: Vec<Value>,
}

/// The cells of a symbol that the *running* file may mutate, snapshotted before
/// it runs so a cached heap image can roll them back (the cached chunks replay
/// every one of them on a hit). `special` is absent on purpose: the compiler sets
/// it, and a cache hit does not compile.
#[derive(Clone, Default)]
pub struct SymbolBaseline {
    pub value: Option<Value>,
    pub function: Option<Value>,
    pub buffer_local_auto: bool,
    pub alias_of: Option<u32>,
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
            dynamic_binding: false,
            eval_depth: 0,
            max_depth_sym: None,
            intrinsic_macro_cells: HashMap::new(),
            frame_stack: Vec::new(),
            error: None,
            pending_throw: None,
            catch_tags: Vec::new(),
            pending_error: None,
            match_data: None,
            output_capture: Vec::new(),
            print_overflow: Cell::new(false),
            print_labels: RefCell::new(HashMap::new()),
            print_next_label: Cell::new(1),
            print_being: RefCell::new(Vec::new()),
            print_circle_on: Cell::new(false),
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
            intercepts: Vec::new(),
            subr_aliased: std::collections::HashSet::new(),
            intercept_active: false,
            intercept_current: None,
            intercept_proceeded: false,
            form_lines: HashMap::new(),
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

    /// The only way to make an elisp integer: a value inside fixnum range is a
    /// `Value::Int`, anything else a heap `Obj::Bignum`.
    ///
    /// Normalizing here is what lets the rest of the interpreter stay simple —
    /// `eql`, `equal`, `sxhash` and the printer never have to consider a bignum
    /// that happens to hold a small value, because one cannot exist.
    pub fn make_integer(&mut self, n: BigInt) -> Value {
        use num_traits::ToPrimitive;
        match n.to_i64() {
            Some(i) if (MOST_NEGATIVE_FIXNUM..=MOST_POSITIVE_FIXNUM).contains(&i) => Value::Int(i),
            _ => self.alloc(Obj::Bignum(n)),
        }
    }

    /// The integer value of `v`, if it is one (fixnum or bignum).
    pub fn as_bigint(&self, v: &Value) -> Option<BigInt> {
        match v {
            Value::Int(n) => Some(BigInt::from(*n)),
            Value::Obj(_) => match self.obj(v) {
                Some(Obj::Bignum(b)) => Some(b.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `v` is an integer (fixnum or bignum) — elisp `integerp`.
    pub fn is_integer(&self, v: &Value) -> bool {
        matches!(v, Value::Int(_)) || matches!(self.obj(v), Some(Obj::Bignum(_)))
    }

    /// Whether `v` is a number — elisp `numberp`.
    pub fn is_number(&self, v: &Value) -> bool {
        matches!(v, Value::Int(_) | Value::Float(_)) || self.is_bignum(v)
    }

    /// Whether `v` is specifically a bignum — elisp `bignump`.
    pub fn is_bignum(&self, v: &Value) -> bool {
        matches!(self.obj(v), Some(Obj::Bignum(_)))
    }

    /// Apply a fusevm arithmetic/comparison op to two elisp numbers.
    ///
    /// Reached only from the numeric hook, i.e. only for the operands fusevm
    /// could not compute natively. Integer arithmetic is exact (promoting to a
    /// bignum as needed); any float operand makes the result a float, as in
    /// Emacs.
    pub fn apply_num_op(&mut self, op: NumOp, a: Num, b: Num) -> Result<Value, String> {
        use NumOp::*;
        // Comparisons first: they answer a bool for any pair, and comparing two
        // exact integers must not detour through `f64` (which would call
        // 2^62 and 2^62+1 equal).
        if matches!(op, Lt | Gt | Le | Ge | Eq | Ne) {
            let ord = match (&a, &b) {
                (Num::Int(x), Num::Int(y)) => x.cmp(y),
                _ => match a.to_f64().partial_cmp(&b.to_f64()) {
                    Some(o) => o,
                    // A NaN operand: every comparison is false, `/=` is true.
                    None => return Ok(Value::Bool(matches!(op, Ne))),
                },
            };
            let yes = match op {
                Lt => ord.is_lt(),
                Gt => ord.is_gt(),
                Le => ord.is_le(),
                Ge => ord.is_ge(),
                Eq => ord.is_eq(),
                _ => ord.is_ne(),
            };
            return Ok(Value::Bool(yes));
        }

        match (a, b) {
            (Num::Int(x), Num::Int(y)) => {
                let r = match op {
                    Add => x + y,
                    Sub => x - y,
                    Mul => x * y,
                    Neg => -x,
                    Mod => {
                        if y == BigInt::from(0) {
                            return Err("arith-error".to_string());
                        }
                        x % y
                    }
                    // `/` and `expt` are elisp builtins, not lowered to VM ops, so
                    // the VM never asks us for them with integer operands.
                    Div | Pow => return Ok(Value::Float(bigint_to_f64(&x) / bigint_to_f64(&y))),
                    Lt | Gt | Le | Ge | Eq | Ne => unreachable!("handled above"),
                };
                Ok(self.make_integer(r))
            }
            // Float-contagious, exactly like Emacs.
            (a, b) => {
                let (x, y) = (a.to_f64(), b.to_f64());
                Ok(Value::Float(match op {
                    Add => x + y,
                    Sub => x - y,
                    Mul => x * y,
                    Neg => -x,
                    Div => x / y,
                    Mod => x % y,
                    Pow => x.powf(y),
                    Lt | Gt | Le | Ge | Eq | Ne => unreachable!("handled above"),
                }))
            }
        }
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
                _ if el_nil(v) => Some("nil".to_string()),
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
            if el_nil(&cur) {
                return Some(out);
            }
            match &cur {
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
                // A bool-vector's elements are `t`/`nil`.
                Some(Obj::BoolVector(bits)) => Some(
                    bits.iter()
                        .map(|&b| if b { Value::Bool(true) } else { Value::Undef })
                        .collect(),
                ),
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
        // `nil` and `t` are self-evaluating symbols; `nil` arrives either as
        // `Undef` (a literal) or as `Bool(false)` (a comparison that answered
        // false), and both are the same symbol.
        match v {
            Value::Bool(true) => Ok(Value::Bool(true)),
            _ if el_nil(v) => Ok(Value::Undef),
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
        // `nil` and `t` are self-evaluating symbols; `nil` arrives either as
        // `Undef` (a literal) or as `Bool(false)` (a comparison that answered
        // false), and both are the same symbol.
        match v {
            Value::Bool(true) => Ok(Value::Bool(true)),
            _ if el_nil(v) => Ok(Value::Undef),
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
    /// The function cell **as introspection sees it**: the real cell, or — for a
    /// compiler intrinsic that Emacs implements as an ordinary macro — the
    /// registered `(macro . FUNCTION)` stand-in.
    ///
    /// `when`/`unless` are lowered by name in `compiler.rs`, so they have no
    /// function cell at all and `(type-of (symbol-function 'when))` answered
    /// `symbol` where Emacs answers `cons`. The stand-in is deliberately kept
    /// OUT of the symbol's real function cell: putting it there would make
    /// `resolve_function` — and therefore `macroexpand_1` on the compile path —
    /// treat every `when` in the prelude as a macro call to run, losing the
    /// dedicated `compile_when` lowering. The expansion the stand-in produces is
    /// the same `subr.el` one [`expand_intrinsic_macro`] already reproduces.
    pub fn introspect_function_cell(&self, sym: &Value) -> Option<Value> {
        if let Some(cell) = self.function_cell(sym) {
            return Some(cell);
        }
        self.sym_handle(sym)
            .and_then(|id| self.intrinsic_macro_cells.get(&id))
            .cloned()
    }
    /// Register the `(macro . FUNCTION)` stand-in for a compiler intrinsic.
    pub fn set_intrinsic_macro_cell(&mut self, sym: &Value, cell: Value) {
        if let Some(id) = self.sym_handle(sym) {
            self.intrinsic_macro_cells.insert(id, cell);
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
    /// Record the source line of a list form (keyed by its head-cons handle).
    /// Called by the reader as it builds each `(...)`. No-op for non-`Obj` forms.
    pub fn record_form_line(&mut self, form: &Value, line: u32) {
        if let Value::Obj(id) = form {
            self.form_lines.insert(*id, line);
        }
    }
    /// The recorded source line of a form, if the reader saw it as a list.
    pub fn form_line(&self, form: &Value) -> Option<u32> {
        match form {
            Value::Obj(id) => self.form_lines.get(id).copied(),
            _ => None,
        }
    }

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
    ///
    /// Under `lexical-binding` nil ([`Self::dynamic_binding`]) *every* symbol takes
    /// the dynamic path, which is what makes a `lambda` made in that mode see the
    /// caller's bindings and not its birthplace's.
    pub fn bind_here(&mut self, id: u32, val: Value) {
        if self.dynamic_binding || self.is_special(id) {
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
    /// Enter one elisp call frame, or `Err` with Emacs's `excessive-lisp-nesting`
    /// when `max-lisp-eval-depth` is already reached.
    ///
    /// Emacs's eval.c raises this *before* the native stack runs out, so runaway
    /// recursion is a catchable signal — `(condition-case e … (error e))` yields
    /// `(excessive-lisp-nesting 1601)`. elisprs previously had no limit at all
    /// and simply aborted the process with `fatal runtime error: stack
    /// overflow`, which nothing can catch and which loses the rest of the
    /// session. The limit is read from the variable on every call so a `let`
    /// around it takes effect (it is `defvar`'d, so the binding is on the
    /// specstack and lives in the global value cell — an array index, not a
    /// scope walk).
    pub fn enter_eval_frame(&mut self) -> Result<(), String> {
        let sym = match self.max_depth_sym {
            Some(s) => s,
            None => {
                let s = self.intern("max-lisp-eval-depth");
                let h = self.sym_handle(&s);
                self.max_depth_sym = h;
                match h {
                    Some(h) => h,
                    None => {
                        self.eval_depth += 1;
                        return Ok(());
                    }
                }
            }
        };
        let limit = match &self.arena[sym as usize] {
            Obj::Symbol(s) => match s.value {
                // eval.c clamps the variable up to a floor of 100, so a smaller
                // value cannot lock the session out of running anything:
                // `(let ((max-lisp-eval-depth 5)) …)` still signals at 101.
                Some(Value::Int(n)) => (n.max(100)) as usize,
                _ => usize::MAX,
            },
            _ => usize::MAX,
        };
        if self.eval_depth >= limit {
            return Err(format!("excessive-lisp-nesting: {}", self.eval_depth + 1));
        }
        self.eval_depth += 1;
        Ok(())
    }
    /// Leave one elisp call frame (paired with [`Self::enter_eval_frame`]).
    pub fn leave_eval_frame(&mut self) {
        self.eval_depth = self.eval_depth.saturating_sub(1);
    }

    /// Instantiate a closure from a compile-time template, capturing the current
    /// lexical environment. Templates are stored with `env: None`.
    ///
    /// Under dynamic binding (`lexical-binding` nil — see [`Self::dynamic_binding`])
    /// nothing is captured: Emacs's `eval` returns the `lambda` form itself, whose
    /// free variables are looked up in the symbols' value cells when it is *called*.
    /// The instance records the mode so its body and parameters run dynamically too,
    /// however far from the `eval` it is finally funcalled.
    pub fn instantiate_closure(&mut self, template: &Value) -> Value {
        if let Some(Obj::Closure {
            params,
            body,
            is_macro,
            src,
            ..
        }) = self.obj(template)
        {
            let (params, body, is_macro, src) =
                (params.clone(), body.clone(), *is_macro, src.clone());
            let dynamic = self.dynamic_binding;
            let env = if dynamic { None } else { self.lex.clone() };
            return self.alloc(Obj::Closure {
                params,
                body,
                is_macro,
                env,
                dynamic,
                src,
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
        let (params, body, is_macro, base_env, csrc, dynamic) = match self.obj(src) {
            Some(Obj::Closure {
                params,
                body,
                is_macro,
                env,
                src: csrc,
                dynamic,
            }) => (
                params.clone(),
                body.clone(),
                *is_macro,
                env.clone(),
                csrc.clone(),
                *dynamic,
            ),
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
            dynamic,
            src: csrc,
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
            .enumerate()
            .map(|(off, o)| match o {
                Obj::Cons(a, b) => SerObj::Cons(a.clone(), b.clone()),
                Obj::Symbol(s) => SerObj::Symbol {
                    name: s.name.clone(),
                    value: s.value.clone(),
                    function: s.function.clone(),
                    special: s.special,
                    buffer_local_auto: s.buffer_local_auto,
                    alias_of: s.alias_of,
                    interned: self.symbol_is_interned(s, (self.builtin_count + off) as u32),
                },
                Obj::Vector(v) => SerObj::Vector(v.clone()),
                Obj::Record(v) => SerObj::Record(v.clone()),
                Obj::BoolVector(v) => SerObj::BoolVector(v.clone()),
                Obj::Bignum(b) => SerObj::Bignum(b.clone()),
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
                    interned: false,
                },
                Obj::Closure {
                    params,
                    body,
                    is_macro,
                    env,
                    dynamic,
                    src,
                } => SerObj::Closure {
                    required: params.required.clone(),
                    optional: params.optional.clone(),
                    rest: params.rest,
                    body: (**body).clone(),
                    is_macro: *is_macro,
                    env: self.flatten_lex(env),
                    dynamic: *dynamic,
                    arglist: src.arglist.clone(),
                    src_body: src.body.clone(),
                },
                // No Subr ever lives in the user range (only `install` makes them).
                Obj::Subr { .. } => SerObj::Symbol {
                    name: "--unexpected-subr--".to_string(),
                    value: None,
                    function: None,
                    special: false,
                    buffer_local_auto: false,
                    alias_of: None,
                    interned: false,
                },
            })
            .collect()
    }
    pub fn builtin_count(&self) -> usize {
        self.builtin_count
    }
    /// The captured lexical environment as Emacs prints it in a closure: an alist
    /// of the captured bindings, newest first, or `(t)` when nothing is captured
    /// (`t` is Emacs's marker that the closure is lexically bound).
    fn captured_alist(&self, env: &Lex, readable: bool, depth: usize) -> String {
        let mut cells = Vec::new();
        let mut cur = env.clone();
        while let Some(scope) = cur {
            let name = match self.arena.get(scope.sym_handle() as usize) {
                Some(Obj::Symbol(s)) => s.name.clone(),
                _ => break,
            };
            let val = self.print_inner(&scope.value(), readable, depth + 1);
            cells.push(format!("({name} . {val})"));
            cur = scope.parent_lex();
        }
        if cells.is_empty() {
            "(t)".to_string()
        } else {
            format!("({})", cells.join(" "))
        }
    }

    /// Whether the global obarray maps this symbol's name to *this* handle — i.e.
    /// the symbol is interned, not a `make-symbol` result, a lambda parameter, or
    /// a macro-local binding that merely shares a name with something interned.
    fn symbol_is_interned(&self, s: &SymbolData, id: u32) -> bool {
        self.obarray.get(&s.name) == Some(&id)
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
    /// A scope chain as `(symbol-handle, value)` pairs, innermost first.
    pub fn flatten_lex(&self, env: &Lex) -> Vec<(u32, Value)> {
        let mut out = Vec::new();
        let mut cur = env.clone();
        while let Some(scope) = cur {
            out.push((scope.sym_handle(), scope.value()));
            cur = scope.parent_lex();
        }
        out
    }

    /// Rebuild a scope chain from [`Self::flatten_lex`]'s output.
    pub fn rebuild_lex(&self, pairs: Vec<(u32, Value)>) -> Lex {
        let mut env: Lex = None;
        // Innermost first on the way out, so re-link from the outermost in.
        for (sym, val) in pairs.into_iter().rev() {
            env = Some(Rc::new(Scope::new(sym, val, env)));
        }
        env
    }

    /// One heap object in its serializable form. `id` is its arena handle (needed
    /// to decide whether a symbol owns its name in the obarray).
    fn ser_obj(&self, o: &Obj, id: u32) -> SerObj {
        match o {
            Obj::Bignum(b) => SerObj::Bignum(b.clone()),
            Obj::Symbol(s) => SerObj::Symbol {
                name: s.name.clone(),
                value: s.value.clone(),
                function: s.function.clone(),
                special: s.special,
                buffer_local_auto: s.buffer_local_auto,
                alias_of: s.alias_of,
                interned: self.symbol_is_interned(s, id),
            },
            Obj::Cons(a, b) => SerObj::Cons(a.clone(), b.clone()),
            Obj::Vector(v) => SerObj::Vector(v.clone()),
            Obj::Record(v) => SerObj::Record(v.clone()),
            Obj::BoolVector(v) => SerObj::BoolVector(v.clone()),
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
            Obj::Closure {
                params,
                body,
                is_macro,
                env,
                dynamic,
                src,
            } => SerObj::Closure {
                required: params.required.clone(),
                optional: params.optional.clone(),
                rest: params.rest,
                body: (**body).clone(),
                is_macro: *is_macro,
                env: self.flatten_lex(env),
                dynamic: *dynamic,
                arglist: src.arglist.clone(),
                src_body: src.body.clone(),
            },
            // Runtime-only objects (and subrs, which `install` recreates): a
            // placeholder keeps the arena indices aligned.
            Obj::Subr { .. } | Obj::Buffer(_) | Obj::Marker(_) | Obj::Obarray(_) => {
                SerObj::Symbol {
                    name: "--unexpected-runtime-obj--".to_string(),
                    value: None,
                    function: None,
                    special: false,
                    buffer_local_auto: false,
                    alias_of: None,
                    interned: false,
                }
            }
        }
    }

    /// Serialize an arena range exactly as it stands. Taken right after the
    /// prelude, this is the clean state the cached chunks expect to replay onto.
    pub fn export_heap_range(&self, start: usize, end: usize) -> Vec<SerObj> {
        self.arena[start..end]
            .iter()
            .enumerate()
            .map(|(off, o)| self.ser_obj(o, (start + off) as u32))
            .collect()
    }

    pub fn snapshot_values(&self, start: usize, end: usize) -> Vec<Option<SymbolBaseline>> {
        (start..end)
            .map(|i| match self.arena.get(i) {
                Some(Obj::Symbol(s)) => Some(SymbolBaseline {
                    value: s.value.clone(),
                    function: s.function.clone(),
                    buffer_local_auto: s.buffer_local_auto,
                    alias_of: s.alias_of,
                }),
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
        clean_prelude: &[SerObj],
    ) -> Vec<SerObj> {
        // The image a cache hit replays onto must be the heap as it stood BEFORE
        // this file ran, because the cached chunks re-apply every effect the file
        // had. Exporting the post-run heap double-applies them:
        //
        //   - a prelude object the file mutated comes back already mutated. The
        //     symbol-plist table is the visible case — `(get 'g 'custom-group)`
        //     returned the previous run's entries, and the replay appended to
        //     them again.
        //   - an order-dependent symbol flag changes the result outright: a
        //     `make-variable-buffer-local` left `buffer_local_auto` set, so
        //     replaying the file's own `(defvar bl-y nil)` created a buffer-local
        //     binding the cold run never had.
        //
        // So: prelude objects come from the snapshot taken before the run, and
        // objects the file itself created keep only what the COMPILER gave them
        // (`special` — a cache hit does not compile, so nothing would set it
        // again); everything the chunks set at run time is cleared.
        let mut out: Vec<SerObj> = clean_prelude.to_vec();
        for (off, o) in self.arena[prelude_end..].iter().enumerate() {
            let id = (prelude_end + off) as u32;
            let ser = match o {
                Obj::Symbol(s) => SerObj::Symbol {
                    name: s.name.clone(),
                    value: None,
                    function: None,
                    special: s.special,
                    buffer_local_auto: false,
                    alias_of: None,
                    interned: self.symbol_is_interned(s, id),
                },
                other => self.ser_obj(other, id),
            };
            out.push(ser);
        }
        out
    }
    /// Rebuild the user/prelude heap from an image. Must be called on a fresh
    /// host (arena == builtins only) so handles line up with compile time.
    pub fn import_heap_image(&mut self, image: Vec<SerObj>) {
        for ser in image {
            let id = self.arena.len() as u32;
            let obj = match ser {
                SerObj::Cons(a, b) => Obj::Cons(a, b),
                SerObj::Bignum(b) => Obj::Bignum(b),
                SerObj::Symbol {
                    name,
                    value,
                    function,
                    special,
                    buffer_local_auto,
                    alias_of,
                    interned,
                } => {
                    // Only a symbol that *was* the global binding for its name may
                    // claim that name again. Re-interning an uninterned symbol
                    // here would shadow a builtin of the same name with a copy
                    // that has no function cell.
                    if interned {
                        self.obarray.insert(name.clone(), id);
                    }
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
                SerObj::Record(v) => Obj::Record(v),
                SerObj::BoolVector(v) => Obj::BoolVector(v),
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
                    env,
                    dynamic,
                    arglist,
                    src_body,
                } => Obj::Closure {
                    params: Rc::new(Params {
                        required,
                        optional,
                        rest,
                    }),
                    src: Rc::new(ClosureSrc {
                        arglist,
                        body: src_body,
                    }),
                    body: Rc::new(body),
                    is_macro,
                    env: self.rebuild_lex(env),
                    dynamic,
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

    /// What `f` currently names, following the same alias chain as
    /// [`Self::resolve_function`] but building none of the callable — see
    /// [`FnKind`]. Used by the pre-argument arity guard at both compile and run
    /// time, so the two always agree on what counts as a subr.
    pub fn fn_kind(&self, f: &Value) -> FnKind {
        let mut cur = f.clone();
        for _ in 0..64 {
            let next = match self.obj(&cur) {
                Some(Obj::Subr { min, max, .. }) => return FnKind::Subr(*min, *max),
                Some(Obj::Symbol(s)) => s.function.clone(),
                Some(_) => return FnKind::Other,
                None => return FnKind::Vacant,
            };
            match next {
                Some(v) => cur = v,
                None => return FnKind::Vacant,
            }
        }
        FnKind::Other
    }

    /// Record that elisp-level `fset`/`defalias` pointed `sym` at a subr. See
    /// [`Self::subr_aliased`].
    pub fn note_subr_alias(&mut self, sym: &Value, def: &Value) {
        if matches!(self.fn_kind(def), FnKind::Subr(..)) {
            if let Some(id) = self.sym_handle(sym) {
                self.subr_aliased.insert(id);
            }
        }
    }

    /// Whether `sym` has ever been `fset` to a subr. See [`Self::subr_aliased`].
    pub fn is_subr_aliased(&self, sym: &Value) -> bool {
        self.sym_handle(sym)
            .is_some_and(|id| self.subr_aliased.contains(&id))
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
                    dynamic,
                    ..
                }) => {
                    return Ok(Resolved::Closure {
                        params: params.clone(),
                        body: body.clone(),
                        is_macro: *is_macro,
                        env: env.clone(),
                        dynamic: *dynamic,
                        object: cur.clone(),
                    })
                }
                Some(Obj::Symbol(s)) => match &s.function {
                    Some(def) => cur = def.clone(),
                    None => return Err(format!("void-function: {}", s.name)),
                },
                // NOTE: no `pending_error` here. This function is also called
                // speculatively (`macroexpand_1` asks whether a head names a
                // macro), so a side effect would outlive the failed probe and be
                // mistaken for the next real error.
                //
                // `t` and `nil` ARE symbols in Emacs — they just have no function
                // cell — so calling one is `void-function`, not `invalid-function`.
                // elisprs represents them as `Value::Bool`/`Value::Undef` rather
                // than heap symbols, so they need naming here.
                None if matches!(cur, Value::Bool(true)) => {
                    return Err("void-function: t".to_string())
                }
                None if matches!(cur, Value::Undef | Value::Bool(false)) => {
                    return Err("void-function: nil".to_string())
                }
                _ => return Err(format!("invalid-function: {}", self.print(&cur, true))),
            }
        }
        Err("function indirection too deep".to_string())
    }

    // ── printing ──
    pub fn print(&self, v: &Value, readable: bool) -> String {
        self.print_overflow.set(false);
        self.print_labels.borrow_mut().clear();
        self.print_being.borrow_mut().clear();
        self.print_next_label.set(1);
        // `print-circle` is opt-in: only then does the printer pay for the
        // reference-counting pre-pass that finds the objects needing `#N=` labels.
        self.print_circle_on.set(self.print_flag("print-circle"));
        if self.print_circle_on.get() {
            self.print_next_label.set(1);
            self.print_preprocess(v);
        }
        self.print_inner(v, readable, 0)
    }

    /// print.c `print_preprocess`: fill the label table from the structure of V.
    ///
    /// The label NUMBER is assigned here, at the moment an object is met for the
    /// SECOND time in this traversal — not when it is finally printed. The two
    /// orders differ, and the difference is observable: in
    /// `(let ((print-circle t)) (prin1-to-string ROOT))` over a graph where the
    /// vector element printed first is met-twice later than a cons printed after
    /// it, Emacs answers `([#2=#s(r …) [#1=(9 . 9) …]] #1# (#2# 3))` — `#2` before
    /// `#1` in the output. Numbering at print time gets the labels backwards.
    ///
    /// Traversal is print.c's explicit-stack DFS: for a cons, push the CDR (unless
    /// nil) and continue into the CAR; for a vector-like, push every element and
    /// take them left to right. An object met again is neither renumbered nor
    /// re-descended, so a cycle terminates.
    ///
    /// The table doubles as print.c's status field: `0` is `Qt` ("seen once, no
    /// label"), `-N` is "label N assigned, not yet printed", `N` is "already
    /// printed as `#N=`".
    ///
    /// Candidates are the containers `PRINT_CIRCLE_CANDIDATE_P` accepts that
    /// elisprs's printer actually recurses into — cons, vector, record,
    /// char-table, closure, hash-table. That is also what makes a cycle safe under
    /// `print-circle` t: the depth ceiling does NOT run in that mode (Emacs prints
    /// a 250-deep nest fine), so termination rests entirely on every container
    /// that can close a cycle being labellable. Strings are candidates in print.c
    /// but not here: an elisprs string is a `Value::Str(Arc<String>)` with no
    /// object identity to share, the same constraint `aset`-on-a-string records.
    fn print_preprocess(&self, v: &Value) {
        let mut stack: Vec<Value> = Vec::new();
        let mut obj = v.clone();
        loop {
            if let Value::Obj(id) = obj {
                // Children in print order; `None` for a non-candidate.
                let children: Option<Vec<Value>> = match self.arena.get(id as usize) {
                    Some(Obj::Cons(car, cdr)) => {
                        // print.c: `if (!NILP (XCDR (obj))) push (XCDR (obj));
                        //           obj = XCAR (obj); continue;`
                        let mut kids = vec![car.clone()];
                        if el_truthy(cdr) {
                            kids.push(cdr.clone());
                        }
                        Some(kids)
                    }
                    Some(Obj::Vector(items)) | Some(Obj::Record(items)) => Some(items.clone()),
                    Some(Obj::CharTable(t)) => {
                        Some(vec![t.default.clone(), t.parent.clone(), t.subtype.clone()])
                    }
                    Some(Obj::Closure { src, .. }) => {
                        let mut kids = vec![src.arglist.clone()];
                        kids.extend(src.body.iter().cloned());
                        Some(kids)
                    }
                    Some(Obj::HashTable { entries, .. }) => {
                        let mut kids = Vec::with_capacity(entries.len() * 2);
                        for (k, val) in entries {
                            kids.push(k.clone());
                            kids.push(val.clone());
                        }
                        Some(kids)
                    }
                    _ => None,
                };
                if let Some(kids) = children {
                    let mut labels = self.print_labels.borrow_mut();
                    match labels.get(&id).copied() {
                        // `Qt`: OBJ appears more than once. Number it now.
                        Some(0) => {
                            let n = self.print_next_label.get();
                            self.print_next_label.set(n + 1);
                            labels.insert(id, -(n as i64));
                        }
                        // Already numbered — print.c's `if (SYMBOLP (num))` is false,
                        // so no new index and no descent.
                        Some(_) => {}
                        None => {
                            labels.insert(id, 0);
                            drop(labels);
                            // Pushed in reverse so `pop` yields them left to right,
                            // which is what print.c's array entry does; anything a
                            // child pushes lands above its siblings, so each subtree
                            // completes before the next sibling — a plain DFS.
                            stack.extend(kids.into_iter().rev());
                        }
                    }
                }
            }
            match stack.pop() {
                Some(next) => obj = next,
                None => break,
            }
        }
    }

    /// The `#N=` / `#N#` prefix for OBJ, or None when it needs no label — print.c
    /// `print_object`'s `PRINT_CIRCLE_CANDIDATE_P` arm.
    ///
    /// Returns `Some(Err(text))` when the object was already printed — the caller
    /// emits `text` (`#N#`) INSTEAD of the object — and `Some(Ok(text))` on the
    /// first visit, where `text` (`#N=`) is a prefix and the object still prints
    /// in full. A `0` slot is print.c's `Qt`: a candidate that turned out not to be
    /// shared, which prints with no label at all.
    #[allow(clippy::result_large_err)]
    fn circle_label(&self, v: &Value) -> Option<Result<String, String>> {
        let Value::Obj(id) = v else { return None };
        let mut labels = self.print_labels.borrow_mut();
        let slot = labels.get_mut(id)?;
        match (*slot).cmp(&0) {
            std::cmp::Ordering::Equal => None,
            // Negative: "hasn't been printed yet" — emit `#N=` and flip the sign.
            std::cmp::Ordering::Less => {
                let n = -*slot;
                *slot = n;
                Some(Ok(format!("#{n}=")))
            }
            std::cmp::Ordering::Greater => Some(Err(format!("#{slot}#"))),
        }
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
        // print.c `print_object`'s prologue, in its own order: the whole
        // `being_printed` mechanism (and the `PRINT_CIRCLE` ceiling that guards it)
        // is the `NILP (Vprint_circle)` arm, and the `#N=`/`#N#` label table is the
        // other. They are alternatives, never both.
        if self.print_circle_on.get() {
            match self.circle_label(v) {
                // Already printed once: emit the back-reference INSTEAD of
                // re-printing (this is what terminates a circular structure).
                Some(Err(backref)) => return backref,
                Some(Ok(prefix)) => {
                    return format!("{prefix}{}", self.print_body(v, readable, depth))
                }
                None => {}
            }
        } else {
            // `if (print_depth >= PRINT_CIRCLE) error ("Apparently circular
            // structure being printed");` — stop recursing (both to match Emacs and
            // to keep the Rust call stack bounded) and flag it for `print_checked`.
            if depth >= PRINT_CIRCLE {
                self.print_overflow.set(true);
                return String::new();
            }
            // `for (int i = 0; i < print_depth; i++) if (BASE_EQ (obj,
            // being_printed[i])) → "#i"`. An object that is its own ancestor prints
            // as the index of the enclosing copy rather than recursing forever.
            let id = match v {
                Value::Obj(id) => Some(*id),
                _ => None,
            };
            if id.is_some() {
                let being = self.print_being.borrow();
                for (i, slot) in being.iter().take(depth).enumerate() {
                    if *slot == id {
                        return format!("#{i}");
                    }
                }
            }
            // `being_printed[print_depth] = obj;`
            let mut being = self.print_being.borrow_mut();
            if being.len() <= depth {
                being.resize(depth + 1, None);
            }
            being[depth] = id;
        }
        self.print_body(v, readable, depth)
    }

    /// `print_inner` minus the depth guard and the `print-circle` labelling — the
    /// actual per-type rendering. Split out so a labelled object can emit its
    /// `#N=` prefix and then print its body without re-entering the label check
    /// (which would see the label already assigned and emit `#N#` forever).
    fn print_body(&self, v: &Value, readable: bool, depth: usize) -> String {
        match v {
            Value::Undef => "nil".to_string(),
            Value::Bool(true) => "t".to_string(),
            Value::Bool(false) => "nil".to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => {
                // Emacs's read syntax for the non-finite floats.
                if f.is_nan() {
                    // print.c prints the NaN's significand, not a fixed 0:
                    // `(read "3.7e+NaN")' prints back as `3.0e+NaN'. See
                    // `reader::NAN_PAYLOAD_MASK'.
                    let payload = f.to_bits() & crate::reader::NAN_PAYLOAD_MASK;
                    let sign = if f.is_sign_negative() { "-" } else { "" };
                    format!("{sign}{payload}.0e+NaN")
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
                    // print-escape-control-characters: every remaining control
                    // character prints as a backslash + *octal* escape (Emacs
                    // `print_object`), so a tab reads back as `\11`.
                    if self.print_flag("print-escape-control-characters") {
                        let mut esc = String::with_capacity(t.len());
                        for c in t.chars() {
                            if (c as u32) < 0x20 || c as u32 == 0x7f {
                                esc.push_str(&format!("\\{:o}", c as u32));
                            } else {
                                esc.push(c);
                            }
                        }
                        t = esc;
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
                    // NO `print-level` check: print.c tests `Vprint_level` in exactly
                    // one place — `case Lisp_Cons` — so only a LIST is ever replaced
                    // by `...`. A vector still costs a level (`print_depth++` runs
                    // for every object), it just cannot be truncated itself:
                    // `(let ((print-level 2)) [[[[1]]]])` prints in full, while the
                    // list one level down in `(([[(1)]]))` becomes `...`.
                    let parts = self.print_seq(items, readable, depth + 1);
                    format!("[{}]", parts.join(" "))
                }
                Some(Obj::Record(items)) => {
                    // No `print-level` check, for the same reason as `Obj::Vector`
                    // above: print.c truncates only conses.
                    // Emacs record syntax `#s(SLOT0 SLOT1 …)` — slot 0 is the type
                    // symbol, printed like any other slot (so a cl-defstruct
                    // instance reads back as `#s(NAME …)`).
                    let parts = self.print_seq(items, readable, depth + 1);
                    format!("#s({})", parts.join(" "))
                }
                Some(Obj::BoolVector(bits)) => {
                    // Emacs prints a bool-vector as `#&LEN"PACKED"`, where PACKED is
                    // the bits LSB-first in `ceil(LEN/8)` bytes. The byte string
                    // uses print.c's rules: `"`/`\` are backslash-escaped, bytes
                    // >= 128 are `\OOO` octal, all others (incl. controls) are raw.
                    let nbytes = bits.len().div_ceil(8);
                    let mut packed = vec![0u8; nbytes];
                    for (i, &b) in bits.iter().enumerate() {
                        if b {
                            packed[i / 8] |= 1 << (i % 8);
                        }
                    }
                    let mut inner = String::new();
                    for &byte in &packed {
                        match byte {
                            b'"' => inner.push_str("\\\""),
                            b'\\' => inner.push_str("\\\\"),
                            0..=127 => inner.push(byte as char),
                            _ => inner.push_str(&format!("\\{byte:o}")),
                        }
                    }
                    format!("#&{}\"{}\"", bits.len(), inner)
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
                // A bignum prints exactly like a fixnum — same type in elisp.
                Some(Obj::Bignum(b)) => b.to_string(),
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
                Some(Obj::Closure {
                    is_macro,
                    env,
                    src,
                    dynamic,
                    ..
                }) => {
                    // Emacs: `#[ARGLIST BODY ENV]` for an interpreted closure, where
                    // ENV is the captured lexical alist — newest binding first — or
                    // `(t)` when nothing is captured. A macro is that, consed onto
                    // `macro`. A dynamically-bound function captures nothing at all
                    // and prints `nil` there — `(eval '(let ((x 1)) (lambda (y) x))
                    // nil)` is `#[(y) (x) nil]`, never `#[(y) (x) (t)]`.
                    let arglist = self.print_inner(&src.arglist, readable, depth + 1);
                    let body: Vec<String> = src
                        .body
                        .iter()
                        .map(|f| self.print_inner(f, readable, depth + 1))
                        .collect();
                    let captures = if *dynamic {
                        "nil".to_string()
                    } else {
                        self.captured_alist(env, readable, depth + 1)
                    };
                    let closure = format!("#[{arglist} ({}) {captures}]", body.join(" "));
                    if *is_macro {
                        format!("(macro . {closure})")
                    } else {
                        closure
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
        // print.c pushes a `PE_list` continuation carrying `last`, `maxlen` and a
        // BRENT cycle detector (`tortoise`, step countdown `n`, step period `m`,
        // `tortoise_idx`), then prints the head's car and re-enters the loop. The
        // state is per-list, so a nested list gets its own detector, exactly as a
        // separate stack entry does in C.
        let print_length = self.print_limit("print-length");
        // `if (print_length == 0) print_c_string ("...)")` — before the car.
        if print_length == Some(0) {
            return "(...)".to_string();
        }
        let mut maxlen: i64 = print_length.map_or(i64::MAX, |n| n as i64);
        let mut out = String::from("(");
        let Some(Obj::Cons(car0, _)) = self.obj(v) else {
            return "()".to_string();
        };
        out.push_str(&self.print_inner(car0, readable, nd));
        let mut last = v.clone();
        let mut tortoise = match v {
            Value::Obj(id) => *id,
            _ => return "()".to_string(),
        };
        let (mut n, mut m, mut tortoise_idx): (i64, i64, i64) = (2, 2, 0);
        // `Lisp_Object next = XCDR (e->u.list.last);` — `last` always holds the
        // cons whose cdr the continuation is about to inspect.
        while let Some(Obj::Cons(_, cdr)) = self.obj(&last) {
            let next = cdr.clone();
            match &next {
                // Both nil representations end the list (a `(1 . nil)` cdr is the
                // one-element list `(1)`, never a dotted pair).
                Value::Undef | Value::Bool(false) => break,
                Value::Obj(id) if matches!(self.arena.get(*id as usize), Some(Obj::Cons(..))) => {
                    // A shared/circular tail prints as a dotted `#N#` back-reference
                    // (`#1=(1 2 . #1#)`) — the tail is a labelled object, so it must
                    // go through `print_inner` rather than continuing this loop.
                    // print.c checks this BEFORE the space, and before the tortoise.
                    // print.c: `if (!(NILP (num) || EQ (num, Qt)))` — only a
                    // NUMBERED tail takes the dotted branch. A `0` slot is `Qt`, a
                    // candidate that turned out unshared, and continues the list.
                    if self.print_circle_on.get()
                        && self
                            .print_labels
                            .borrow()
                            .get(id)
                            .is_some_and(|slot| *slot != 0)
                    {
                        out.push_str(" . ");
                        out.push_str(&self.print_inner(&next, readable, nd));
                        break;
                    }
                    out.push(' ');
                    maxlen -= 1;
                    if maxlen <= 0 {
                        out.push_str("...");
                        break;
                    }
                    last = next.clone();
                    n -= 1;
                    if n == 0 {
                        // "Double tortoise update period and teleport it." The
                        // teleport TAKES PRECEDENCE over the equality test, which is
                        // why the reported index only ever takes the values
                        // 0, 2, 6, 14, 30, … (2^k - 2) — it is the tortoise's
                        // position at the last teleport, not the cycle's period.
                        tortoise_idx += m;
                        m <<= 1;
                        n = m;
                        tortoise = *id;
                    } else if tortoise == *id {
                        // print.c's own comment: "This #N tail index is somewhat
                        // ambiguous; see bug#55395." The `)` is part of C's format
                        // string; here the shared `out.push(')')` below supplies it.
                        out.push_str(". #");
                        out.push_str(&tortoise_idx.to_string());
                        break;
                    }
                    let Some(Obj::Cons(a, _)) = self.obj(&last) else {
                        break;
                    };
                    out.push_str(&self.print_inner(a, readable, nd));
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
    /// The slot a buffer *object* names, live or killed. `resolve_buffer` filters
    /// killed slots out; the three callers that follow Emacs's `Fget_buffer`
    /// (`get-buffer`, `set-buffer`, `buffer-name`) must be able to see them,
    /// because a killed buffer object stays a buffer object in Emacs and only
    /// `BUFFER_LIVE_P` distinguishes it.
    fn buffer_slot(&self, v: &Value) -> Option<usize> {
        match self.obj(v) {
            Some(Obj::Buffer(idx)) if *idx < self.buffers.len() => Some(*idx),
            _ => None,
        }
    }
    /// Port of `Fget_buffer` (`src/buffer.c`):
    ///
    /// ```c
    ///   if (BUFFERP (buffer_or_name)) return buffer_or_name;
    ///   CHECK_STRING (buffer_or_name);
    ///   return Fcdr (Fassoc (buffer_or_name, Vbuffer_alist, Qnil));
    /// ```
    ///
    /// A buffer object is returned unchanged whether or not it is live — only
    /// the name lookup can answer nil — and anything that is neither a buffer
    /// nor a string signals `(wrong-type-argument stringp X)` before any lookup
    /// happens. `Ok(None)` is Emacs's nil.
    pub fn get_buffer(&self, v: &Value) -> Result<Option<usize>, String> {
        if let Some(idx) = self.buffer_slot(v) {
            return Ok(Some(idx));
        }
        match v {
            Value::Str(s) => Ok(self.find_buffer_by_name(s)),
            _ => Err(format!(
                "wrong-type-argument: stringp {}",
                self.print(v, true)
            )),
        }
    }
    /// Port of `nsberror` (`src/buffer.c`) — the "no such buffer" signal shared
    /// by every buffer-taking subr:
    ///
    /// ```c
    ///   if (STRINGP (spec)) error ("No buffer named %s", SDATA (spec));
    ///   error ("Invalid buffer argument");
    /// ```
    ///
    /// The name goes in unquoted (`SDATA`, not `prin1`).
    pub fn nsberror(&self, v: &Value) -> String {
        match v {
            Value::Str(s) => format!("error: No buffer named {s}"),
            _ => "error: Invalid buffer argument".to_string(),
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
    /// Port of `Fset_buffer` (`src/buffer.c`):
    ///
    /// ```c
    ///   buffer = Fget_buffer (buffer_or_name);
    ///   if (NILP (buffer)) nsberror (buffer_or_name);
    ///   if (!BUFFER_LIVE_P (XBUFFER (buffer))) error ("Selecting deleted buffer");
    ///   set_buffer_internal (XBUFFER (buffer));
    /// ```
    ///
    /// The three failures are distinct and were one message here before: a
    /// non-string non-buffer is `(wrong-type-argument stringp X)` from
    /// `get_buffer`, an unknown *name* is `No buffer named NAME`, and a killed
    /// buffer *object* is `Selecting deleted buffer`.
    pub fn set_buffer(&mut self, v: &Value) -> Result<Value, String> {
        let idx = self.get_buffer(v)?.ok_or_else(|| self.nsberror(v))?;
        if self.buffers[idx].name.is_none() {
            return Err("error: Selecting deleted buffer".to_string());
        }
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
        // The text is gone, so every position field has to go back to BEG with
        // it — `reset_buffer` does exactly this in Emacs:
        //
        // ```c
        //   b->pt = BEG;  b->begv = BEG;  b->zv = BEG;
        // ```
        //
        // Leaving them stale broke `EditBuffer::point`'s documented invariant
        // ("always kept within `[begv, zv]`"): after `(insert "hello")
        // (kill-buffer)` point was 6 in a zero-length buffer, and the next
        // `buffer-substring` sliced `text[..5]` and aborted the process.
        b.point = 1;
        b.begv = 1;
        b.zv = 1;
        b.mark = None;
        // Detach every marker that pointed into the killed buffer.
        for mk in b.markers.drain(..) {
            let mut md = mk.borrow_mut();
            md.buffer = None;
            md.pos = 0;
        }
        if self.current == idx {
            // `Fkill_buffer` re-selects with `Fset_buffer (Fother_buffer (…))`,
            // and `other-buffer` ends in `get-scratch-buffer-create` — "If no
            // other buffer exists, return the buffer `*scratch*' (creating it if
            // necessary)". So a killed buffer can never stay current; the old
            // `.unwrap_or(0)` left slot 0 current even when slot 0 was the
            // buffer just killed.
            self.current = match self.buffers.iter().position(|b| b.name.is_some()) {
                Some(i) => i,
                None => {
                    let handle = self.new_buffer("*scratch*".to_string());
                    self.buffer_slot(&handle).expect("fresh buffer slot")
                }
            };
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
    /// Drop any per-char plists registered for string S, making it a plain
    /// unpropertized string — what `substring-no-properties` returns.
    pub fn string_clear_props(&mut self, s: &Arc<String>) {
        self.string_props.remove(&(Arc::as_ptr(s) as usize));
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
    /// Carry text properties onto a freshly-built string.
    ///
    /// `pieces` names, for each run of the result in order, the source string it
    /// came from and the char index it starts at inside that source. A run whose
    /// source has no properties contributes nils, and a result with no propertized
    /// piece at all registers nothing — an unpropertized string must stay one, or
    /// every `concat` would allocate a plist vector.
    ///
    /// This is what makes `(concat (propertize "a" 'face 'bold) "b")` keep the
    /// face on its first character, the way Emacs's interval trees do: the
    /// properties are per-character, so they follow the characters wherever a
    /// builtin copies them.
    pub fn string_carry_props(
        &mut self,
        out: &Arc<String>,
        pieces: &[(Option<Arc<String>>, usize, usize)],
    ) {
        let mut vec: Vec<Value> = Vec::new();
        let mut any = false;
        for (src, start, len) in pieces {
            match src.as_ref().and_then(|s| self.string_props_vec(s)) {
                Some(props) => {
                    any = true;
                    for i in 0..*len {
                        vec.push(props.get(start + i).cloned().unwrap_or(Value::Undef));
                    }
                }
                None => vec.extend(std::iter::repeat_n(Value::Undef, *len)),
            }
        }
        if any {
            self.string_set_props_vec(out, vec);
        }
    }

    /// The whole of `src` as one piece, for a builtin that transforms a string
    /// character-for-character (`upcase`, `capitalize`) or copies it entire.
    pub fn string_carry_all(&mut self, out: &Arc<String>, src: &Arc<String>) {
        let len = src.chars().count();
        self.string_carry_props(out, &[(Some(Arc::clone(src)), 0, len)]);
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
        // An empty plist is not the same interval as one that names a property
        // whose value happens to be nil: `(propertize "ab" 'p nil)` followed by
        // an unpropertized character prints as two runs, not one. The subset
        // walk cannot see the difference — a key absent from a plist reads as
        // nil — so the emptiness is compared first.
        if el_truthy(a) != el_truthy(b) {
            return false;
        }
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
            // Chars that share the SAME plist object (one `propertize`/
            // `set-text-properties` call covers a range with one plist) are one
            // Emacs interval no matter what the plist holds — `values_eq` catches
            // that even when the plist's keys (e.g. a string key) would defeat
            // the structural walk below.
            while j < n
                && (self.values_eq(&props[i], &props[j])
                    || self.plist_struct_eq(&props[i], &props[j]))
            {
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

    /// The designator characters of the syntax codes, in code order — Emacs's
    /// `syntax_code_spec` (`src/syntax.c`) and the prelude's
    /// `--syntax-code-spec--`, which must stay the same string.
    const SYNTAX_CODE_SPEC: &'static [u8] = b" .w_()'\"$\\/<>@!|";

    /// The syntax table `(syntax-table)` would return: the current buffer's, or
    /// the standard one when the buffer has none. Read straight out of the two
    /// prelude variables rather than by calling elisp, because the caller is a
    /// regexp compile that can happen underneath any evaluation.
    fn current_syntax_table(&self) -> Value {
        let local = self
            .find_symbol("--current-syntax-table--")
            .and_then(|s| self.get_value(&s).ok());
        match local {
            Some(v) if el_truthy(&v) => v,
            _ => self
                .find_symbol("--standard-syntax-table--")
                .and_then(|s| self.get_value(&s).ok())
                .unwrap_or(Value::Undef),
        }
    }

    /// Every character range whose syntax class in the current table is `class`
    /// (a `modify-syntax-entry` designator: `w`, `_`, `.`, `<`, `>`, …).
    ///
    /// This is what `\sC` needs. Emacs's regexp engine asks `SYNTAX (c) ==
    /// class` per character as it matches; elisprs translates to a `fancy_regex`
    /// pattern up front, so the same question has to be answered for the whole
    /// character space at compile time. That is cheap here because a `CharTable`
    /// stores runs (`ranges: Vec<(u32, Value)>`), not 4M slots: the breakpoints
    /// of the table and of every table in its parent chain bound every place the
    /// answer can change.
    ///
    /// An entry that is not a `(CODE . MATCH)` cons counts as class 0
    /// (whitespace), which is what `SYNTAX` yields for an unset slot.
    pub fn syntax_class_ranges(&self, class: char) -> Vec<(u32, u32)> {
        let table = self.current_syntax_table();
        let mut breaks: Vec<u32> = vec![0];
        let mut cur = table.clone();
        for _ in 0..64 {
            let Some(Obj::CharTable(t)) = self.obj(&cur) else {
                break;
            };
            breaks.extend(t.ranges.iter().map(|(s, _)| *s));
            cur = t.parent.clone();
        }
        breaks.sort_unstable();
        breaks.dedup();

        let class_at = |c: u32| -> char {
            let v = self.char_table_ref(&table, c);
            let code = match self.obj(&v) {
                Some(Obj::Cons(car, _)) => match car {
                    Value::Int(n) => (*n as u64 & 0xFFFF) as usize,
                    _ => 0,
                },
                _ => 0,
            };
            *Self::SYNTAX_CODE_SPEC.get(code).unwrap_or(&b' ') as char
        };

        let mut out: Vec<(u32, u32)> = Vec::new();
        for (i, &lo) in breaks.iter().enumerate() {
            let hi = breaks.get(i + 1).map_or(MAX_CHAR, |n| n - 1);
            if class_at(lo) != class {
                continue;
            }
            // Runs are produced in ascending order, so a run abutting the
            // previous kept one just extends it.
            match out.last_mut() {
                Some(last) if last.1 + 1 == lo => last.1 = hi,
                _ => out.push((lo, hi)),
            }
        }
        out
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
    /// Detach the current lexical scope chain, returning it. Used by `eval`,
    /// whose FORM runs in the environment its LEXICAL argument names rather than
    /// in the caller's.
    pub fn take_lex(&mut self) -> Lex {
        self.lex.take()
    }

    /// Restore a scope chain taken by [`Self::take_lex`].
    pub fn restore_lex(&mut self, lex: Lex) {
        self.lex = lex;
    }

    /// The OClosure side table, flattened for the cache: `(closure-handle, type,
    /// slot-handles)`.
    ///
    /// It is NOT reconstructible from the heap image: `oclosure--fix-type` runs
    /// when the *prelude* runs (`oclosure--accessor-prototype` is a prelude
    /// `defconst`), and a cache hit skips the prelude. Without it, every prelude
    /// OClosure came back as a plain closure and `oclosure--copy` answered
    /// "not an OClosure" on the second run of any script.
    pub fn export_oclosure_meta(&self) -> Vec<(u32, u32, Vec<u32>)> {
        self.oclosure_meta
            .iter()
            .map(|(id, m)| (*id, m.ty, m.slots.clone()))
            .collect()
    }

    /// Restore the table exported by [`Self::export_oclosure_meta`]. Handles line
    /// up because the image is imported into a host whose arena is at exactly the
    /// length it had when the image was taken.
    pub fn import_oclosure_meta(&mut self, meta: Vec<(u32, u32, Vec<u32>)>) {
        for (id, ty, slots) in meta {
            self.oclosure_meta.insert(id, OClosureMeta { ty, slots });
        }
    }

    /// A sequence's elements, or Emacs's error for what it actually is.
    ///
    /// An improper list names its offending TAIL (`(reverse (cons 1 2))` is
    /// `(wrong-type-argument listp 2)`), while a non-sequence names itself.
    pub fn seq_vec_checked(&self, v: &Value) -> Result<Vec<Value>, String> {
        if let Some(items) = self.seq_vec(v) {
            return Ok(items);
        }
        // A cons that `seq_vec` rejected is an improper list: walk to the tail.
        if matches!(self.obj(v), Some(Obj::Cons(..))) {
            let mut cur = v.clone();
            while let Some(Obj::Cons(_, cdr)) = self.obj(&cur) {
                cur = cdr.clone();
            }
            return Err(format!(
                "wrong-type-argument: listp {}",
                self.print(&cur, true)
            ));
        }
        Err(format!(
            "wrong-type-argument: sequencep {}",
            self.print(v, true)
        ))
    }

    /// The function a designator names: a symbol resolves through its function
    /// cell (following aliases), anything else is already a function.
    ///
    /// `funcall`/`apply` resolve before calling, which is why Emacs's arity error
    /// from them names `#<subr char-to-string>` where a direct `(char-to-string)`
    /// names the symbol.
    pub fn function_designator(&mut self, f: &Value) -> Value {
        let mut cur = f.clone();
        for _ in 0..16 {
            match self.obj(&cur) {
                Some(Obj::Symbol(s)) => match s.function.clone() {
                    Some(next) => cur = next,
                    None => return cur,
                },
                _ => return cur,
            }
        }
        cur
    }

    /// Signal Emacs's `(wrong-number-of-arguments FUNCTION COUNT)`.
    ///
    /// The data holds the function *object*. Rendering it into the message and
    /// re-reading it (which is how the other error helpers build their data) cannot
    /// work here: a closure prints as `#[(x) (x) (t)]`, and no reader can turn that
    /// back into the closure it came from.
    pub fn signal_wrong_nargs(&mut self, callee: &Value, argc: usize) -> String {
        let sym = self.intern("wrong-number-of-arguments");
        let count = Value::Int(argc as i64);
        let data = self.list_from(vec![callee.clone(), count]);
        let display = format!("{} {}", self.print(callee, true), argc);
        let obj = self.cons(sym, data);
        let msg = format!("wrong-number-of-arguments: {display}");
        self.set_pending_error(&msg, obj);
        msg
    }

    /// Record the error object that belongs to `msg` (the string the failing call
    /// returns as its `Err`). See [`Self::take_pending_error`].
    pub fn set_pending_error(&mut self, msg: &str, obj: Value) {
        self.pending_error = Some((msg.to_string(), obj));
    }

    /// The error object recorded for `msg`, if any. An object recorded for a
    /// *different* message belongs to an error that has already been dealt with —
    /// it must not stand in for this one.
    pub fn take_pending_error(&mut self, msg: &str) -> Option<Value> {
        match &self.pending_error {
            Some((m, _)) if m == msg => self.pending_error.take().map(|(_, v)| v),
            _ => None,
        }
    }

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
                | "wrong-length-argument"
                | "invalid-function"
                // `(excessive-lisp-nesting DEPTH)` — DEPTH is the integer, not "1601".
                | "excessive-lisp-nesting"
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
    // Emacs prints a float as the shortest string that reads back as the same
    // float (`float_to_string` → gnulib `dtoastr`), formatted like C's `%g`: it
    // tries precision 15, 16, then 17 and takes the first that round-trips. That
    // is why `(float most-positive-fixnum)` prints as `2.305843009213694e+18`
    // rather than `2305843009213694000.0` — %g goes exponential once the decimal
    // exponent reaches the precision.
    // gnulib's `ftoastr` starts at precision 1 for a subnormal (where a single
    // significant digit already round-trips: 5e-324) and at DBL_DIG = 15
    // otherwise (so 1e10 prints as 10000000000.0, not 1e+10).
    let start = if f != 0.0 && f.abs() < f64::MIN_POSITIVE {
        1
    } else {
        15
    };
    for prec in start..=17 {
        let s = format_g(f, prec);
        if s.parse::<f64>() == Ok(f) {
            return ensure_float_syntax(s);
        }
    }
    ensure_float_syntax(format_g(f, 17))
}

/// C's `%g` with the given precision: fixed notation while the decimal exponent
/// is in `-4..PREC`, exponential outside it, trailing zeros trimmed either way.
fn format_g(x: f64, prec: usize) -> String {
    let prec = prec.max(1);
    let e_form = format!("{:.*e}", prec - 1, x);
    let (mantissa, exp_str) = e_form.rsplit_once('e').unwrap_or((e_form.as_str(), "0"));
    let exp: i32 = exp_str.parse().unwrap_or(0);
    if exp < -4 || exp >= prec as i32 {
        let sign = if exp < 0 { '-' } else { '+' };
        // C pads the exponent to at least two digits: `1e-10`, `1e+300`.
        format!("{}e{}{:02}", trim_zeros(mantissa), sign, exp.abs())
    } else {
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        trim_zeros(&format!("{x:.decimals$}"))
    }
}

/// Drop a fraction's trailing zeros (and a bare trailing point), as `%g` does.
fn trim_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0');
    t.strip_suffix('.').unwrap_or(t).to_string()
}

/// A printed float must read back as a float: Emacs appends `.0` when neither a
/// decimal point nor an exponent is present (`100` → `100.0`).
fn ensure_float_syntax(s: String) -> String {
    if s.contains('.') || s.contains('e') || s.contains("INF") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// True when V is elisp `nil`.
///
/// `nil` has two spellings on the VM. A literal `nil` compiles to fusevm
/// `Undef`, but every op that answers a boolean — `Op::NumLt` and the rest of
/// the comparisons the compiler lowers `<`/`=`/`>` to — answers
/// `Value::Bool(false)`. Both are elisp's one `nil`, so every place that accepts
/// `nil` has to accept both spellings: `(length (= 5 42))` is `0` in Emacs, and
/// treating only `Undef` as the empty list made it `(wrong-type-argument
/// sequencep nil)` — an error naming the very value it refused to recognise.
pub fn el_nil(v: &Value) -> bool {
    matches!(v, Value::Undef | Value::Bool(false))
}

/// elisp truthiness: only `nil` is false.
pub fn el_truthy(v: &Value) -> bool {
    !el_nil(v)
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
    /// DAP debug execution: when on, the compiler emits `DBG_LINE` statement
    /// markers and `run_chunk` skips the tracing JIT so every marker fires
    /// through the interpreter. Off = zero overhead (no markers emitted at all).
    static DEBUG_MODE: Cell<bool> = const { Cell::new(false) };
}

/// Enable/disable DAP debug execution (statement markers + JIT-off). Set by the
/// `--dap` server around the program compile+run; a single `Cell` load per
/// `run_chunk` when off.
pub fn set_debug_mode(on: bool) {
    DEBUG_MODE.with(|d| d.set(on));
}
/// Is DAP debug execution active? Read by the compiler (marker emission) and
/// `run_chunk` (JIT gating).
pub fn debug_mode() -> bool {
    DEBUG_MODE.with(|d| d.get())
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
                let f = with_host(|h| h.function_designator(&args[0]));
                return call_function(&f, &args[1..]);
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
                    // Fapply sizes SPREAD with `list_length`, whose
                    // CHECK_LIST_END names the loop variable: the improper
                    // TAIL of a dotted list -- (apply #'+ '(1 . 2)) =>
                    // (wrong-type-argument listp 2) -- or SPREAD itself when
                    // it is not a cons at all.
                    with_host(|h| {
                        let mut t = spread.clone();
                        while let Some(Obj::Cons(_, cdr)) = h.obj(&t) {
                            t = cdr.clone();
                        }
                        format!("wrong-type-argument: listp {}", h.print(&t, true))
                    })
                })?;
                let mut a: Vec<Value> = if args.len() >= 2 {
                    args[1..args.len() - 1].to_vec()
                } else {
                    Vec::new()
                };
                a.extend(tail);
                let f = with_host(|h| h.function_designator(&args[0]));
                return call_function(&f, &a);
            }
            "mapcar" => {
                if args.len() < 2 {
                    return Err(format!("wrong-number-of-arguments: mapcar {}", args.len()));
                }
                // An improper list names its tail; a non-sequence names itself.
                let seq = with_host(|h| h.seq_vec_checked(&args[1]))?;
                let f = with_host(|h| h.function_designator(&args[0]));
                let mut out = Vec::with_capacity(seq.len());
                for e in seq {
                    out.push(call_function(&f, &[e])?);
                }
                return Ok(with_host(|h| h.list_from(out)));
            }
            "mapc" => {
                if args.len() < 2 {
                    return Err(format!("wrong-number-of-arguments: mapc {}", args.len()));
                }
                let seq = with_host(|h| h.seq_vec_checked(&args[1]))?;
                let f = with_host(|h| h.function_designator(&args[0]));
                for e in seq {
                    call_function(&f, &[e])?;
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
                let (items, was_vec) = match with_host(|h| match h.obj(&args[0]) {
                    Some(Obj::Vector(v)) => Some((v.clone(), true)),
                    _ => h.list_vec(&args[0]).map(|l| (l, false)),
                }) {
                    Some(pair) => pair,
                    // A cons that is not a proper list: Emacs's list walk names the
                    // offending tail. Anything else is not a sortable container at
                    // all.
                    None if with_host(|h| matches!(h.obj(&args[0]), Some(Obj::Cons(..)))) => {
                        return Err(with_host(|h| h.seq_vec_checked(&args[0]))
                            .expect_err("a cons that is not a proper list must fail"))
                    }
                    None => {
                        return Err(format!(
                            "wrong-type-argument: list-or-vector-p {}",
                            with_host(|h| h.print(&args[0], true))
                        ))
                    }
                };
                let is_kw =
                    |v: &Value| with_host(|h| h.sym_name(v)).is_some_and(|n| n.starts_with(':'));
                let mut pred: Option<Value> = None;
                let mut key: Option<Value> = None;
                let mut reverse = false;
                // The classic `(sort SEQ PRED)` form sorts in place; the Emacs-30
                // keyword form is non-destructive unless `:in-place t`.
                let mut in_place;
                if args.len() == 2 && !is_kw(&args[1]) {
                    // A nil PRED is not a function to call — Emacs 30's `sort`
                    // documents PREDICATE as defaulting to `value<`, and passing
                    // nil explicitly takes that default: `(sort '(3 1 2) nil)` is
                    // `(1 2 3)`, and `(sort '(t "010") nil)` is value<'s
                    // `(type-mismatch "010" t)`, never `void-function nil`.
                    if !el_truthy(&args[1]) {
                        pred = None;
                    } else {
                        pred = Some(with_host(|h| h.function_designator(&args[1])));
                    }
                    in_place = true;
                } else {
                    in_place = false;
                    let mut idx = 1;
                    while idx < args.len() {
                        let kw = with_host(|h| h.sym_name(&args[idx])).unwrap_or_default();
                        let val = args.get(idx + 1).cloned().unwrap_or(Value::Undef);
                        let truthy = !matches!(val, Value::Undef | Value::Bool(false));
                        match kw.as_str() {
                            // As above: `:lessp nil` selects the `value<` default.
                            ":lessp" | ":predicate" => {
                                pred = if truthy {
                                    Some(with_host(|h| h.function_designator(&val)))
                                } else {
                                    None
                                }
                            }
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
                // FORM is evaluated in the lexical environment given by LEXICAL —
                // NOT in the caller's. `(let ((x 5)) (eval 'x t))` signals
                // `void-variable x` in Emacs: `t` means "lexical binding, empty
                // environment". Running the chunk in the live scope chain instead
                // leaked every binding the caller happened to hold, and any closure
                // FORM created captured (and printed) them.
                //
                // A nil (or omitted) LEXICAL selects *dynamic* binding for FORM:
                // `let` binds value cells, and a `lambda` captures nothing, so
                // `(eval '(funcall (let ((x 1)) (lambda () x))) nil)` is
                // `void-variable x` — the binding is gone by the time it is called.
                let lexical = args.get(1).is_some_and(el_truthy);
                let saved = with_host(|h| h.take_lex());
                let prev_mode = with_host(|h| std::mem::replace(&mut h.dynamic_binding, !lexical));
                let out = run_chunk(chunk);
                with_host(|h| {
                    h.restore_lex(saved);
                    h.dynamic_binding = prev_mode;
                });
                return out;
            }
            // The macro-expansion functions run macro expanders (re-entrant).
            "macroexpand-1" => {
                let form = args
                    .first()
                    .ok_or("wrong-number-of-arguments: macroexpand-1")?;
                // A user macro (if any) wins; otherwise fall back to the intrinsic
                // `when`/`unless` expansions the compiler lowers as special forms.
                if let Some(e) = macroexpand_1(form)? {
                    return Ok(e);
                }
                return Ok(expand_intrinsic_macro(form).unwrap_or_else(|| form.clone()));
            }
            "macroexpand" => {
                // Expand the head to a fixpoint; don't recurse into sub-forms.
                let mut f = args
                    .first()
                    .ok_or("wrong-number-of-arguments: macroexpand")?
                    .clone();
                loop {
                    if let Some(e) = macroexpand_1(&f)? {
                        f = e;
                        continue;
                    }
                    if let Some(e) = expand_intrinsic_macro(&f) {
                        f = e;
                        continue;
                    }
                    break;
                }
                return Ok(f);
            }
            "macroexpand-all" => {
                return macroexpand_all_builtin(
                    args.first()
                        .ok_or("wrong-number-of-arguments: macroexpand-all")?,
                )
            }
            // (`replace-regexp-in-string` needs no interception: it is a Lisp
            // function here as in Emacs — the prelude port funcalls a function
            // REP itself.)
            // Nonlocal-exit intrinsics (the compiler rewrites catch/unwind-protect/
            // condition-case into these, passing lambda thunks).
            "--catch--" => return intrinsic_catch(args),
            "--unwind--" => return intrinsic_unwind(args),
            "--condition-case--" => return intrinsic_condition_case(args),
            // AOP pattern-intercept proceed protocol (elisprs extension). Runs the
            // original command from inside an `around` advice — re-entrant, so it
            // lives here (outside any host borrow), like the other intrinsics.
            "intercept-proceed" => return crate::intercepts::intrinsic_intercept_proceed(),
            // Inline Rust FFI: the `rust { ... }` desugar (src/rust_ffi.rs) emits
            // `(__rust-compile B64 LINE)`; compile the block to a cached cdylib
            // and register its `pub extern "C"` exports (which become callable by
            // bareword). Shells out to rustc, so it must run outside any host
            // borrow — hence here with the other intrinsics. Returns nil.
            "__rust-compile" => {
                let b64 = match args.first() {
                    Some(Value::Str(s)) => s.to_string(),
                    _ => String::new(),
                };
                return fusevm::ffi::compile_and_register(&b64).map(|_| Value::Undef);
            }
            _ => {}
        }
    }

    // ── zshrs-original AOP pattern-intercept layer (elisprs extension) ──
    // Distinct from elisp nadvice (`advice-add`, per-symbol): this fires on a
    // GLOB/pattern match across many symbol names at once (`"forward-*"`, `"_*"`,
    // `"all"`) with before/after/around advice + a timing/proceed protocol. Only a
    // named (symbol) callee participates — an anonymous closure has no name to
    // match. The zero-intercept common case is a single cheap bool load; the
    // `intercept_active` guard makes advice bodies (and the proceeded original)
    // dispatch normally without re-triggering (recursion guard). See
    // [`crate::intercepts`].
    if with_host(|h| !h.intercepts.is_empty() && !h.intercept_active) {
        if let Some(name) = with_host(|h| h.sym_name(f)) {
            if let Some(result) = crate::intercepts::run_intercepts(f, &name, args)? {
                return Ok(result);
            }
        }
    }

    // The callee as written by the caller — a symbol, or the function object when
    // applied. It goes into the `wrong-number-of-arguments` data, so keep it
    // before the match binds `f` to the resolved fn pointer.
    let callee = f.clone();
    let resolved = match with_host(|h| h.resolve_function(f)) {
        Ok(r) => r,
        Err(e) => {
            // Inline Rust FFI fallback: a `rust { ... }` exported function is
            // callable by bareword when no elisp function shadows it. A user
            // `defun` still wins — it resolves above; only a `void-function` miss
            // (no function cell) reaches here, and the cheap membership check
            // keeps this off the hot path. elisp Values ARE fusevm Values, so the
            // args (ints/floats/strings) marshal straight through `try_call`.
            if let Some(name) = with_host(|h| h.sym_name(f)) {
                if fusevm::ffi::is_registered(&name) {
                    if let Some(r) = fusevm::ffi::try_call(&name, args) {
                        return r;
                    }
                }
            }
            return Err(e);
        }
    };
    match resolved {
        Resolved::Subr { f, min, max, name } => {
            if args.len() < min || max.is_some_and(|m| args.len() > m) {
                let _ = &name;
                // Emacs's error data is (FUNC N): the callee as the caller wrote
                // it (a symbol, or the subr object when applied) and the argument
                // count it was given.
                return Err(with_host(|h| h.signal_wrong_nargs(&callee, args.len())));
            }
            with_host(|h| f(h, args))
        }
        Resolved::Closure {
            params,
            body,
            is_macro,
            env,
            dynamic,
            object,
        } => {
            if is_macro {
                return Err("macro called as a function (use it in a macro position)".to_string());
            }
            // `funcall_lambda`'s signal names the resolved closure, not the
            // designator: `xsignal2 (Qwrong_number_of_arguments, … fun, …)`.
            // Passing `callee` here made `(f1 1 2)` report `f1` where Emacs
            // reports `#[(a) (a) (t)]`, and `defalias`ing a second name onto the
            // same function reported that second name.
            let _ = &callee;
            let max = params.required.len() + params.optional.len();
            if args.len() < params.required.len() || (params.rest.is_none() && args.len() > max) {
                return Err(with_host(|h| h.signal_wrong_nargs(&object, args.len())));
            }
            // The one place a user function body is entered: every compiled
            // `CALL`, `funcall` and `apply` funnels through here, so the DAP
            // function-breakpoint check is hooked once rather than per caller.
            crate::dap::enter_function(&callee);
            run_closure(&params, &body, env, dynamic, args)
        }
    }
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
        // Everything else — lists, vectors, and every cross-type pair — is the
        // prelude's `value<`, which already carries Emacs 30's class rules and
        // its `(type-mismatch A B)` signal. Duplicating them here would be a
        // second implementation of the same order, and it was the reason a
        // default-predicate `sort` reported "value<: unsupported comparison"
        // where Emacs signals `(type-mismatch "010" t)`.
        _ => {
            let f = with_host(|h| h.intern("value<"));
            Ok(el_truthy(&call_function(&f, &[a.clone(), b.clone()])?))
        }
    }
}

/// Stable merge sort of `(key, item)` pairs by `key`. With `pred` it re-enters
/// elisp `(pred key_j key_i)`; without, it falls back to `value_lt`.
fn merge_sort_by(items: &mut Vec<(Value, Value)>, pred: Option<&Value>) -> Result<(), String> {
    let n = items.len();
    if n < 2 {
        return Ok(());
    }
    let lt = |x: &Value, y: &Value| -> Result<bool, String> {
        match pred {
            Some(p) => {
                let r = call_function(p, &[x.clone(), y.clone()])?;
                Ok(!matches!(r, Value::Undef | Value::Bool(false)))
            }
            None => value_lt(x, y),
        }
    };
    // Emacs 30 sorts with `tim_sort` (src/sort.c, the CPython listsort port),
    // and its COMPARISON ORDER is observable through a side-effecting or
    // throwing predicate: `count_run` first calls pred(a[1], a[0]) — so
    // (sort '(t 1.0 nil) #'string-to-number) dies on 1.0, not nil. For inputs
    // shorter than MAX_MINRUN (64) — every list differential fuzzing produces —
    // tim_sort is exactly count_run + binary insertion, reproduced verbatim
    // here. Longer inputs keep the stable merge sort below (same result;
    // Emacs's merge machinery would call the predicate in a different order).
    if n < 64 {
        // count_run: pred(a[1], a[0]) picks ascending (extend while NOT less)
        // vs strictly-descending (extend while less, then reverse — strictness
        // is what keeps the reversal stable).
        let mut run = 2;
        if lt(&items[1].0, &items[0].0)? {
            while run < n && lt(&items[run].0, &items[run - 1].0)? {
                run += 1;
            }
            items[..run].reverse();
        } else {
            while run < n && !lt(&items[run].0, &items[run - 1].0)? {
                run += 1;
            }
        }
        // binarysort: insert a[start] into the sorted prefix a[0..start],
        // probing pred(pivot, a[mid]) and landing AFTER equals (stable).
        for start in run..n {
            let (mut l, mut r) = (0usize, start);
            while l < r {
                let p = l + ((r - l) >> 1);
                if lt(&items[start].0, &items[p].0)? {
                    r = p;
                } else {
                    l = p + 1;
                }
            }
            items[l..=start].rotate_right(1);
        }
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
/// Call a closure: open a scope over its captured environment, bind the
/// parameters, run the body, then unwind.
///
/// `dynamic` is the closure's binding mode ([`Obj::Closure::dynamic`]). It is
/// installed for the duration of the call, so a function made under
/// `lexical-binding` nil binds its parameters — and every `let` its body runs —
/// on the specstack, and any `lambda` that body creates is dynamic in turn. The
/// previous mode is restored on every exit path, including a signalled error.
fn run_closure(
    params: &Rc<Params>,
    body: &Rc<Chunk>,
    env: Lex,
    dynamic: bool,
    args: &[Value],
) -> Result<Value, String> {
    with_host(|h| h.enter_eval_frame())?;
    let entry = with_host(|h| h.scope_depth());
    let prev_mode = with_host(|h| std::mem::replace(&mut h.dynamic_binding, dynamic));
    let setup = with_host(|h| {
        h.open_scope_in(env.clone());
        h.bind_params_into_scope(params, args)
    });
    if let Err(e) = setup {
        with_host(|h| {
            h.unwind_scopes_to(entry);
            h.dynamic_binding = prev_mode;
            h.leave_eval_frame();
        });
        return Err(e);
    }
    let result = run_chunk((**body).clone());
    // Unwind to the entry depth (not just one scope): a `throw`/error out of an
    // inner `let` inside the body leaks scopes that this restores.
    with_host(|h| {
        h.unwind_scopes_to(entry);
        h.dynamic_binding = prev_mode;
        h.leave_eval_frame();
    });
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
                dynamic,
                ..
            }) => Some((params, body, env, dynamic, elems[1..].to_vec())),
            _ => None,
        }
    });
    match info {
        Some((params, body, env, dynamic, args)) => {
            Ok(Some(run_closure(&params, &body, env, dynamic, &args)?))
        }
        None => Ok(None),
    }
}

/// Faithful `subr.el` expansions for the two intrinsic macros elisprs lowers as
/// compiler special forms (`when`/`unless`). Because the compiler intercepts
/// them by name they have no closure for [`macroexpand_1`] to find, so the
/// `macroexpand`/`macroexpand-1`/`macroexpand-all` builtins consult this to
/// reproduce Emacs's macro output. Returns `None` for any other head, and for a
/// head the user has shadowed with a real macro (that closure wins in the
/// callers, which try [`macroexpand_1`] first).
///
/// Emacs 30.2 `subr.el`:
/// ```elisp
/// (defmacro when   (cond &rest body) (list 'if cond (cons 'progn body)))
/// (defmacro unless (cond &rest body) (cons 'if (cons cond (cons nil body))))
/// ```
pub fn expand_intrinsic_macro(form: &Value) -> Option<Value> {
    with_host(|h| {
        let elems = h.list_vec(form)?;
        if elems.is_empty() {
            return None;
        }
        let name = h.sym_name(&elems[0])?;
        let body = &elems[2.min(elems.len())..];
        let cond = elems.get(1).cloned().unwrap_or(Value::Undef);
        match name.as_str() {
            // (if COND (progn BODY...))
            "when" => {
                let if_sym = h.intern("if");
                let mut progn = vec![h.intern("progn")];
                progn.extend_from_slice(body);
                let progn = h.list_from(progn);
                Some(h.list_from(vec![if_sym, cond, progn]))
            }
            // (if COND nil BODY...) — nil is `Value::Undef`.
            "unless" => {
                let mut out = vec![h.intern("if"), cond, Value::Undef];
                out.extend_from_slice(body);
                Some(h.list_from(out))
            }
            _ => None,
        }
    })
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
    macroexpand_all_impl(form, false)
}

/// `macroexpand-all` as the elisp builtin exposes it: identical to
/// [`macroexpand_all`] but also unfolds the intrinsic `when`/`unless` macros
/// (see [`expand_intrinsic_macro`]). Kept off the compile pipeline so the
/// compiler's dedicated `when`/`unless` lowering (`compile_when`) still fires.
pub fn macroexpand_all_builtin(form: &Value) -> Result<Value, String> {
    macroexpand_all_impl(form, true)
}

fn macroexpand_all_impl(form: &Value, expand_intrinsics: bool) -> Result<Value, String> {
    let mut f = form.clone();
    loop {
        if let Some(e) = macroexpand_1(&f)? {
            f = e;
            continue;
        }
        if expand_intrinsics {
            if let Some(e) = expand_intrinsic_macro(&f) {
                f = e;
                continue;
            }
        }
        break;
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
                                    np.push(macroexpand_all_impl(p, expand_intrinsics)?);
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
                out.push(macroexpand_all_impl(e, expand_intrinsics)?);
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
                out.push(macroexpand_all_impl(e, expand_intrinsics)?);
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
                    return macroexpand_all_impl(&replaced, expand_intrinsics);
                }
            }
            let mut out = Vec::with_capacity(elems.len());
            out.push(elems[0].clone());
            out.push(elems[1].clone()); // NAME, untouched
            out.push(elems[2].clone()); // ARGLIST, untouched
            for e in &elems[3..] {
                out.push(macroexpand_all_impl(e, expand_intrinsics)?);
            }
            Ok(with_host(|h| h.list_from(out)))
        }
        _ => {
            let mut out = Vec::with_capacity(elems.len());
            for e in &elems {
                let expanded = macroexpand_all_impl(e, expand_intrinsics)?;
                // A `defmacro' among sibling forms has to take effect BEFORE its
                // siblings are expanded, or a macro defined and used in the same
                // enclosing form is unusable: expansion of `(progn (defmacro m …)
                // (m 1))' reached `(m 1)' while `m' still had no function cell, so
                // it compiled as a call and failed at run time with "macro called
                // as a function". Emacs's interpreter never sees this because it
                // evaluates `progn' forms one at a time, and its byte-compiler
                // evaluates `defmacro' at compile time for exactly this reason.
                //
                // The form is already fully expanded, so compiling it here cannot
                // re-enter expansion. Running it only writes the macro's function
                // cell — the enclosing form still contains the `defmacro', which
                // installs the identical definition again when it runs.
                if is_defmacro_form(e) {
                    if let Ok(chunk) = with_host(|h| crate::compiler::compile_top(h, &expanded)) {
                        let _ = run_chunk(chunk);
                    }
                }
                out.push(expanded);
            }
            Ok(with_host(|h| h.list_from(out)))
        }
    }
}

/// True when FORM is literally `(defmacro NAME ARGLIST …)`.
fn is_defmacro_form(form: &Value) -> bool {
    with_host(|h| {
        let Some(Obj::Cons(head, _)) = h.obj(form) else {
            return false;
        };
        let head = head.clone();
        h.sym_name(&head).as_deref() == Some("defmacro")
    })
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
    let cleanup_r = call_function(&cleanup, &[]);
    with_host(|h| {
        if h.pending_throw.is_none() {
            h.pending_throw = saved;
        }
    });
    // A cleanup that itself signals SUPERSEDES whatever the body was doing —
    // eval.c runs the unwind handler outside the body's protection, so its error
    // propagates and the body's in-flight error is dropped. Discarding it here
    // (`let _ =`) made `(condition-case e (unwind-protect (error "in")
    // (error "cleanup")) (error (cadr e)))` answer "in" where Emacs answers
    // "cleanup", and silently swallowed every failure inside a cleanup form.
    cleanup_r?;
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
                let obj = h
                    .pending_error
                    .as_ref()
                    .filter(|(m, _)| *m == e)
                    .map(|(_, eo)| eo.clone())?;
                match h.obj(&obj) {
                    Some(Obj::Cons(car, _)) => {
                        let car = car.clone();
                        h.sym_name(&car)
                    }
                    _ => None,
                }
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
                // An `error' handler is NOT a catch-all: it matches only signals
                // whose `error-conditions' chain actually contains `error'. `quit'
                // is the case that proves it — data.c seeds it with conditions
                // (quit), so `(condition-case nil (signal 'quit nil) (error 'c)
                // (t 'top))' answers `top' in Emacs. Treating `error' as
                // unconditional answered `c'. Only `t' is the real catch-all.
                //
                // When the chain is empty the signal came from an internal Rust
                // error string with no `define-error' registration; those are all
                // ordinary errors, so `error' still matches them.
                let derives_from_error =
                    signal_conditions.is_empty() || signal_conditions.iter().any(|c| c == "error");
                if conds.iter().any(|c| {
                    (c == "error" && derives_from_error)
                        || c == "t"
                        || *c == esym
                        || signal_conditions.contains(c)
                }) {
                    let depth = with_host(|h| {
                        let d = h.specdepth();
                        if matches!(h.obj(&var), Some(Obj::Symbol(_))) {
                            // Bind to the real (SYMBOL . DATA) object when we have
                            // it; otherwise reconstruct one from the message.
                            let eobj = h
                                .take_pending_error(&e)
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
        ops::CHECK_ARITY => {
            // Emacs's `eval_sub` resolves the function cell BEFORE it evaluates
            // the argument forms, so `(car (setq x 1) (setq y 2))` signals
            // `(wrong-number-of-arguments car 2)` with x and y still 0. The
            // verdict is taken here rather than at compile time because
            // `fset`/`defalias` can retarget the symbol in between, and Emacs
            // honours the cell that is live at the call.
            let argc = arg as usize;
            let symv = vm.pop();
            let bad = with_host(|h| {
                // An AOP intercept fronts the callee, so the underlying subr's
                // arity is not the arity being called — leave those to `CALL`.
                h.intercepts.is_empty() && h.fn_kind(&symv).rejects_before_args(argc)
            });
            if bad {
                let e = with_host(|h| h.signal_wrong_nargs(&symv, argc));
                abort(vm, e);
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
        ops::DBG_LINE => {
            // A DAP statement marker (emitted only in debug mode). The line rides
            // in the chunk's line table for this op; `vm.ip` has already advanced
            // past the marker, so the marker's slot is `ip - 1`. Stack-neutral.
            let line = *vm.chunk.lines.get(vm.ip.saturating_sub(1)).unwrap_or(&0);
            if line != 0 {
                crate::dap::check_line(line);
            }
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

/// Emacs `most-positive-fixnum` (2^61-1) and `most-negative-fixnum` (-2^61).
/// An integer outside this range is a bignum, even though it still fits an
/// `i64` — matching Emacs, where the two low bits are the type tag.
pub const MOST_POSITIVE_FIXNUM: i64 = 2305843009213693951;
/// See [`MOST_POSITIVE_FIXNUM`].
pub const MOST_NEGATIVE_FIXNUM: i64 = -2305843009213693952;

/// An elisp number in the one place elisp arithmetic has to be exact: an integer
/// is arbitrary-precision, a float is an `f64`. Mixed arithmetic is
/// float-contagious, as in Emacs.
pub enum Num {
    Int(BigInt),
    Float(f64),
}

impl Num {
    pub fn to_f64(&self) -> f64 {
        match self {
            Num::Int(i) => bigint_to_f64(i),
            Num::Float(f) => *f,
        }
    }
}

/// `BigInt` → `f64`, the conversion Emacs does when an integer meets a float.
/// Saturates to ±inf beyond `f64` range, which is what Emacs's `(float (expt 10
/// 400))` yields.
pub fn bigint_to_f64(i: &BigInt) -> f64 {
    use num_traits::ToPrimitive;
    i.to_f64()
        .unwrap_or(if i.sign() == num_bigint::Sign::Minus {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        })
}

/// elisp's numeric semantics, installed into fusevm as a [`fusevm::NumericHook`].
///
/// fusevm's arithmetic ops are awk-flavoured by default — a non-numeric operand
/// coerces (`"a"` → `0.0`) and integer overflow wraps — and the compiler lowers
/// `+`/`-`/`*`/`1+`/`1-` and the numeric comparisons straight to them
/// (`compiler.rs`, `try_native_op`), which is what makes elisp arithmetic
/// JIT-compilable. Strict mode keeps that lowering and hands back only the cases
/// fusevm cannot compute natively:
///
/// - a non-number (string, cons, symbol, `t`, `nil`): Emacs signals
///   `(wrong-type-argument number-or-marker-p X)` — except for a marker, which is
///   its buffer position;
/// - a bignum operand, which rides through the VM as a `Value::Obj` handle;
/// - an integer result that overflowed `i64` *or* left elisp's fixnum range:
///   Emacs's integers are unbounded, so it becomes a bignum.
///
/// Everything else — fixnum arithmetic in range, all float arithmetic — never
/// reaches here and stays on the VM's (and the JIT's) fast path.
pub(crate) fn numeric_hook(op: NumOp, a: &Value, b: &Value) -> Result<Value, String> {
    with_host(|h| {
        // `(+ 1 (point-marker))` — a marker is a number in arithmetic position.
        let coerce = |h: &ElispHost, v: &Value| -> Option<Num> {
            match v {
                Value::Int(n) => Some(Num::Int(BigInt::from(*n))),
                Value::Float(f) => Some(Num::Float(*f)),
                Value::Obj(_) => match h.obj(v) {
                    Some(Obj::Bignum(b)) => Some(Num::Int(b.clone())),
                    _ => h.marker_position(v).map(|p| Num::Int(BigInt::from(p))),
                },
                _ => None,
            }
        };
        let unary = matches!(op, NumOp::Neg);
        let an = coerce(h, a).ok_or_else(|| {
            format!(
                "wrong-type-argument: number-or-marker-p {}",
                h.print(a, true)
            )
        })?;
        let bn = if unary {
            Num::Int(BigInt::from(0))
        } else {
            coerce(h, b).ok_or_else(|| {
                format!(
                    "wrong-type-argument: number-or-marker-p {}",
                    h.print(b, true)
                )
            })?
        };
        h.apply_num_op(op, an, bn)
    })
}

/// Run a compiled chunk on a fresh fusevm VM, returning the elisp result.
pub fn run_chunk(chunk: Chunk) -> Result<Value, String> {
    with_host(|h| h.error = None);
    let mut vm = VM::new(chunk);
    vm.set_extension_handler(Box::new(ext_dispatch));
    vm.set_extension_wide_handler(Box::new(ext_dispatch_wide));
    // elisp is not awk: arithmetic signals on a non-number and promotes past
    // fixnum range instead of coercing and wrapping. The hook is what the VM
    // (and its JIT) call for the cases they cannot compute natively.
    vm.set_numeric_hook(std::sync::Arc::new(numeric_hook));
    vm.set_fixnum_range(MOST_NEGATIVE_FIXNUM, MOST_POSITIVE_FIXNUM);
    // Hot loops trace-compile through fusevm's Cranelift JIT; with the
    // `jit-disk-cache` feature, compiled native code is persisted across runs.
    // Under the DAP debugger the JIT is disabled so every `DBG_LINE` statement
    // marker fires through the interpreter (a JIT-compiled trace would elide the
    // extension-op callback and the debugger could never pause).
    if !debug_mode() {
        vm.enable_tracing_jit();
    }
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
