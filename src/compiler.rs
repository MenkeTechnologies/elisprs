//! The elisp → fusevm lowering. elisp does not run on a bespoke interpreter; it
//! compiles to `fusevm::Chunk` and executes on fusevm. Heap objects ride as
//! `Value::Obj` handles; the ElispHost (via fusevm's extension handler) supplies
//! the semantics and the (dynamic) binding environment.
//!
//! Lowering conventions:
//! - literals → `LoadInt`/`LoadFloat`/`LoadConst`/`LoadUndef`/`LoadTrue`
//! - `(quote X)` → `LoadConst(X)`
//! - call `(f a b)` → `LoadConst(f)`, args…, `Extended(CALL, argc)`
//! - elisp truthiness ≠ fusevm truthiness, so conditionals emit
//!   `Extended(TRUTHY)` before `JumpIfFalse` (the strykelang pattern)
//! - `lambda`/`defun` compile the body to a sub-chunk stored in a heap closure;
//!   calling it runs that chunk on a nested fusevm VM
//! - `let`/`let*` lower to dynamic bind/unbind ops around the body
//!
//! Not yet lowered (next milestone): macro expansion, backquote, and the
//! nonlocal-exit forms (catch/throw/condition-case/unwind-protect).

use crate::host;
use crate::host::{ops, ElispHost, FnKind, Obj};
use fusevm::{Chunk, ChunkBuilder, Op, Value};
use std::rc::Rc;

/// The primitives GNU Emacs 30.2's byte compiler turns into a bytecode op, which
/// calls the C function directly and never consults the symbol's function cell.
/// A call to one of these from *byte-compiled* Lisp therefore ignores any advice
/// on the symbol, while a call to anything else honours it — and Emacs ships its
/// preloaded Lisp byte-compiled, so this is the behaviour the prelude must have.
///
/// Not derived from bytecode.c (its opcode table is not in the installed tree)
/// but measured, one name at a time, on the 30.2 in `emacs-version`: byte-compile
/// `(lambda (a…) (NAME a…))`, `advice-add` NAME with an `:around` that records a
/// flag, call it, and read the flag. The 30 names it answered "advice fires" for
/// — `assoc` `eql` `safe-length` `symbol-function` `set` `append` `vectorp`
/// `natnump` `sequencep` `bufferp` `arrayp` `markerp` `functionp` `fboundp`
/// `boundp` `put` `buffer-string` `format` `message` `intern` `vector` `throw`
/// `signal` `error` `mapcar` `add-to-list` `assoc-default` `byte-code-function-p`
/// `called-interactively-p` `beginning-of-line` — are deliberately absent below.
///
/// Sorted; `is_open_coded` binary-searches it.
const OPEN_CODED: &[&str] = &[
    "%",
    "*",
    "+",
    "-",
    "/",
    "/=",
    "1+",
    "1-",
    "<",
    "<=",
    "=",
    ">",
    ">=",
    "aref",
    "aset",
    "assq",
    "bobp",
    "bolp",
    "buffer-substring",
    "car",
    "car-safe",
    "cdr",
    "cdr-safe",
    "char-after",
    "concat",
    "cons",
    "consp",
    "current-buffer",
    "current-column",
    "delete-region",
    "downcase",
    "elt",
    "end-of-line",
    "eobp",
    "eolp",
    "eq",
    "equal",
    "following-char",
    "forward-char",
    "forward-line",
    "fset",
    "funcall",
    "get",
    "goto-char",
    "indent-to",
    "insert",
    "integerp",
    "length",
    "list",
    "listp",
    "match-beginning",
    "match-end",
    "max",
    "member",
    "memq",
    "min",
    "narrow-to-region",
    "nconc",
    "not",
    "nreverse",
    "nth",
    "nthcdr",
    "null",
    "numberp",
    "point",
    "point-max",
    "point-min",
    "preceding-char",
    "set-buffer",
    "setcar",
    "setcdr",
    "skip-chars-forward",
    "string-equal",
    "string-lessp",
    "string<",
    "string=",
    "stringp",
    "substring",
    "symbol-value",
    "symbolp",
    "upcase",
    "widen",
];

/// Whether `name` is in [`OPEN_CODED`].
fn is_open_coded(name: &str) -> bool {
    OPEN_CODED.binary_search(&name).is_ok()
}

pub fn compile_top(h: &mut ElispHost, form: &Value) -> Result<Chunk, String> {
    let mut b = ChunkBuilder::new();
    compile_form(h, &mut b, form)?;
    Ok(b.build())
}

pub fn compile_program(h: &mut ElispHost, forms: &[Value]) -> Result<Chunk, String> {
    let mut b = ChunkBuilder::new();
    if forms.is_empty() {
        b.emit(Op::LoadUndef, 0);
    }
    for (i, form) in forms.iter().enumerate() {
        emit_line_marker(h, &mut b, form);
        compile_form(h, &mut b, form)?;
        if i + 1 < forms.len() {
            b.emit(Op::Pop, 0);
        }
    }
    Ok(b.build())
}

/// Emit a DAP statement marker for `form` when debug mode is on. The line rides
/// in the chunk's line table for the marker op; the `DBG_LINE` handler
/// (`host::ext_dispatch`) reads it back and calls `dap::check_line`. Stack-
/// neutral, and emitted only under `--dap` (zero bytes in an ordinary run).
fn emit_line_marker(h: &ElispHost, b: &mut ChunkBuilder, form: &Value) {
    if crate::host::debug_mode() {
        if let Some(line) = h.form_line(form) {
            b.emit(Op::Extended(ops::DBG_LINE, 0), line);
        }
    }
}

fn compile_form(h: &mut ElispHost, b: &mut ChunkBuilder, form: &Value) -> Result<(), String> {
    match form {
        Value::Int(n) => {
            b.emit(Op::LoadInt(*n), 0);
        }
        Value::Float(f) => {
            b.emit(Op::LoadFloat(*f), 0);
        }
        Value::Str(_) => load_const(b, form.clone()),
        Value::Undef | Value::Bool(false) => {
            b.emit(Op::LoadUndef, 0);
        }
        Value::Bool(true) => {
            b.emit(Op::LoadTrue, 0);
        }
        Value::Obj(_) => match h.obj(form) {
            Some(Obj::Symbol(s)) => {
                if s.name.starts_with(':') {
                    load_const(b, form.clone());
                } else {
                    load_const(b, form.clone());
                    b.emit(Op::Extended(ops::GETVAR, 0), 0);
                }
            }
            Some(Obj::Cons(..)) => compile_call(h, b, form)?,
            _ => load_const(b, form.clone()),
        },
        other => load_const(b, other.clone()),
    }
    Ok(())
}

fn compile_call(h: &mut ElispHost, b: &mut ChunkBuilder, form: &Value) -> Result<(), String> {
    let elems = h.list_vec(form).ok_or("malformed call form")?;
    let head = elems[0].clone();
    let name = match h.obj(&head) {
        Some(Obj::Symbol(s)) => Some(s.name.clone()),
        _ => None,
    };
    match name.as_deref() {
        Some("quote") => load_const(b, elems.get(1).cloned().unwrap_or(Value::Undef)),
        Some("function") => {
            // `#'(lambda ...)` is the closure, not the literal lambda form — so
            // compile a lambda argument like `lambda` does; otherwise (a symbol)
            // load it as a constant for the CALL handler to resolve.
            let arg = elems.get(1).cloned().unwrap_or(Value::Undef);
            let arg_elems = match h.obj(&arg) {
                Some(Obj::Cons(..)) => h.list_vec(&arg),
                _ => None,
            };
            let is_lambda = arg_elems
                .as_ref()
                .and_then(|e| e.first())
                .map(|f| h.sym_name(f).as_deref() == Some("lambda"))
                .unwrap_or(false);
            if is_lambda {
                compile_lambda(h, b, &arg_elems.unwrap(), false)?;
            } else {
                load_const(b, arg);
            }
        }
        Some("lambda") => compile_lambda(h, b, &elems, false)?,
        Some("progn") => compile_progn(h, b, &elems[1..])?,
        Some("prog1") => compile_prog1(h, b, &elems[1..])?,
        Some("if") => compile_if(h, b, &elems[1..])?,
        Some("when") => compile_when(h, b, &elems[1..], true)?,
        Some("unless") => compile_when(h, b, &elems[1..], false)?,
        Some("and") => compile_andor(h, b, &elems[1..], true)?,
        Some("or") => compile_andor(h, b, &elems[1..], false)?,
        Some("while") => compile_while(h, b, &elems[1..])?,
        Some("cond") => compile_cond(h, b, &elems[1..])?,
        Some("let") => compile_let(h, b, &elems[1..], false)?,
        Some("let*") => compile_let(h, b, &elems[1..], true)?,
        Some("setq") => compile_setq(h, b, &elems[1..])?,
        Some("defun") => compile_defun(h, b, &elems, false)?,
        Some("defmacro") => compile_defun(h, b, &elems, true)?,
        Some("defvar") => compile_defvar(h, b, &elems, false)?,
        Some("defconst") => compile_defvar(h, b, &elems, true)?,
        Some("catch") => compile_catch(h, b, &elems)?,
        Some("unwind-protect") => compile_unwind(h, b, &elems)?,
        Some("condition-case") => compile_condition_case(h, b, &elems)?,
        Some(kw) if is_unsupported_special(kw) => {
            return Err(format!(
                "special form `{kw}` not yet lowered (buffer milestone)"
            ));
        }
        _ => {
            // Fast path: lower core arithmetic/comparison on un-redefined
            // primitives to native fusevm ops, so hot loops are JIT/AOT-able
            // instead of dispatching to the host on every operation.
            if let Some(n) = &name {
                if h.is_primitive_fn(n) && try_native_op(h, b, n, &elems[1..])? {
                    return Ok(());
                }
            }
            // Push the operator. A `(lambda ...)` form in head position is
            // compiled to a closure (so `((lambda (x) x) 5)` works); any other
            // head is loaded as-is for the CALL handler to resolve.
            let head_elems = match h.obj(&head) {
                Some(Obj::Cons(..)) => h.list_vec(&head),
                _ => None,
            };
            let head_is_lambda = head_elems
                .as_ref()
                .and_then(|e| e.first())
                .map(|f| h.sym_name(f).as_deref() == Some("lambda"))
                .unwrap_or(false);
            let argc = elems.len() - 1;
            if argc > u8::MAX as usize {
                return Err("too many arguments".to_string());
            }
            // Emacs checks a subr's arity before it evaluates the argument
            // forms, so the guard has to precede the argument code.
            if !head_is_lambda && needs_arity_guard(h, &head, argc) {
                load_const(b, head.clone());
                b.emit(Op::Extended(ops::CHECK_ARITY, argc as u8), 0);
            }
            if head_is_lambda {
                compile_lambda(h, b, &head_elems.unwrap(), false)?;
            } else {
                // Inside the prelude, a call to one of the primitives Emacs's
                // byte compiler open-codes loads the subr itself, not the
                // symbol — see `OPEN_CODED`.
                let direct = name
                    .as_deref()
                    .filter(|n| host::prelude_compiling() && is_open_coded(n))
                    .and_then(|n| h.primitive_fn_value(n));
                load_const(b, direct.unwrap_or(head));
            }
            for arg in &elems[1..] {
                compile_form(h, b, arg)?;
            }
            b.emit(Op::Extended(ops::CALL, argc as u8), 0);
        }
    }
    Ok(())
}

/// Whether a call to `head` with `argc` arguments should carry the
/// pre-argument arity guard (`CHECK_ARITY`).
///
/// The guard's *verdict* is always retaken at run time — `fset`/`defalias` can
/// retarget the symbol after this compiles, and Emacs honours the cell that is
/// live at the call, not the one that was live at compile time. All that is
/// decided here is whether emitting it can ever pay, which keeps it off the two
/// shapes that make up almost every call:
///
/// - the symbol already names a subr that accepts `argc` — there is nothing to
///   signal unless it is later retargeted at a *narrower* subr;
/// - the symbol already names a closure — Emacs evaluates a closure's arguments
///   before checking its arity too, so the guard would never fire.
///
/// It is emitted in three cases: when the call is already wrong against the live
/// cell; when the symbol names nothing yet — a forward reference, or a symbol
/// that `fset` will point at a subr; and when `fset` has already pointed the
/// symbol at a subr once, since it can be pointed at a *narrower* one before the
/// call runs.
///
/// Neither of the last two is hypothetical. After
/// `(fset 'f (symbol-function 'car))`, Emacs signals on `(f 1 2)` without
/// evaluating either argument, and `f` is unbound when the call compiles; and a
/// second `(fset 'f (symbol-function 'cdr))` narrows a cell that was wide enough
/// when it did.
fn needs_arity_guard(h: &ElispHost, head: &Value, argc: usize) -> bool {
    if !matches!(h.obj(head), Some(Obj::Symbol(_))) {
        return false;
    }
    let kind = h.fn_kind(head);
    matches!(kind, FnKind::Vacant) || kind.rejects_before_args(argc) || h.is_subr_aliased(head)
}

/// Lower a call to a native fusevm op sequence when the operator is a core
/// numeric primitive with a compatible arity. Returns `Ok(true)` if lowered.
/// (`/`, `%`, `mod` stay on the host to preserve elisp integer-division and
/// remainder semantics; native arithmetic also skips the host's wrong-type
/// signaling — an accepted fast-path trade-off for numbers.)
fn try_native_op(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    name: &str,
    args: &[Value],
) -> Result<bool, String> {
    let binop = |h: &mut ElispHost, b: &mut ChunkBuilder, op: Op| -> Result<(), String> {
        compile_form(h, b, &args[0])?;
        compile_form(h, b, &args[1])?;
        b.emit(op, 0);
        Ok(())
    };
    match name {
        "+" | "*" | "-" => {
            let (ident, op) = match name {
                "+" => (0, Op::Add),
                "*" => (1, Op::Mul),
                _ => (0, Op::Sub),
            };
            // Which shapes may use the native opcodes:
            //
            //   0 args      the identity constant, no operand to check.
            //   (- X)       `Op::Negate' type-checks X itself.
            //   2 args      both operands are evaluated before the op runs.
            //
            // Everything else goes to the n-ary builtin, which the VM calls with
            // all arguments already evaluated:
            //
            //   (+ X)       a lone operand emitted bare skips the type check
            //               altogether, so `(+ t)' quietly returned `t' instead
            //               of signalling `wrong-type-argument'.
            //   3+ args     a chain of binary ops interleaves evaluation with
            //               folding, so a type error in argument N aborts before
            //               argument N+1 is ever evaluated. Emacs evaluates
            //               *every* argument first and folds afterwards, so
            //               `(* 1 t (setq n 9))' must still leave `n' at 9.
            let native = match (name, args.len()) {
                (_, 0) | (_, 2) => true,
                ("-", 1) => true,
                _ => false,
            };
            if !native {
                return Ok(false);
            }
            if args.is_empty() {
                b.emit(Op::LoadInt(ident), 0);
            } else if name == "-" && args.len() == 1 {
                compile_form(h, b, &args[0])?;
                b.emit(Op::Negate, 0);
            } else {
                compile_form(h, b, &args[0])?;
                for a in &args[1..] {
                    compile_form(h, b, a)?;
                    b.emit(op.clone(), 0);
                }
            }
        }
        // Lower to native Add/Sub with a constant 1 (not Inc/Dec): Add/Sub are
        // float-contagious like elisp `+`/`-`, so (1+ 1.0) => 2.0, whereas the
        // integer Inc/Dec opcodes would truncate the float.
        "1+" if args.len() == 1 => {
            compile_form(h, b, &args[0])?;
            b.emit(Op::LoadInt(1), 0);
            b.emit(Op::Add, 0);
        }
        "1-" if args.len() == 1 => {
            compile_form(h, b, &args[0])?;
            b.emit(Op::LoadInt(1), 0);
            b.emit(Op::Sub, 0);
        }
        "<" if args.len() == 2 => binop(h, b, Op::NumLt)?,
        ">" if args.len() == 2 => binop(h, b, Op::NumGt)?,
        "<=" if args.len() == 2 => binop(h, b, Op::NumLe)?,
        ">=" if args.len() == 2 => binop(h, b, Op::NumGe)?,
        "=" if args.len() == 2 => binop(h, b, Op::NumEq)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn is_unsupported_special(kw: &str) -> bool {
    matches!(
        kw,
        "save-excursion" | "save-current-buffer" | "save-restriction"
    )
}

// ── nonlocal-exit lowering: rewrite to intrinsic calls with lambda thunks ──

fn lambda_of(h: &mut ElispHost, body: &[Value]) -> Value {
    let mut items = vec![h.intern("lambda"), Value::Undef]; // (lambda () body...)
    items.extend_from_slice(body);
    h.list_from(items)
}
fn call_of(h: &mut ElispHost, name: &str, args: Vec<Value>) -> Value {
    let mut items = vec![h.intern(name)];
    items.extend(args);
    h.list_from(items)
}
fn quote_of(h: &mut ElispHost, v: Value) -> Value {
    let q = h.intern("quote");
    h.list_from(vec![q, v])
}

fn compile_catch(h: &mut ElispHost, b: &mut ChunkBuilder, elems: &[Value]) -> Result<(), String> {
    let tag = elems.get(1).cloned().unwrap_or(Value::Undef);
    let thunk = lambda_of(h, elems.get(2..).unwrap_or(&[]));
    let form = call_of(h, "--catch--", vec![tag, thunk]);
    compile_form(h, b, &form)
}

fn compile_unwind(h: &mut ElispHost, b: &mut ChunkBuilder, elems: &[Value]) -> Result<(), String> {
    let body_form = elems.get(1).cloned().unwrap_or(Value::Undef);
    let body = lambda_of(h, &[body_form]);
    let cleanup = lambda_of(h, elems.get(2..).unwrap_or(&[]));
    let form = call_of(h, "--unwind--", vec![body, cleanup]);
    compile_form(h, b, &form)
}

fn compile_condition_case(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    elems: &[Value],
) -> Result<(), String> {
    let var = elems.get(1).cloned().unwrap_or(Value::Undef);
    let body_form = elems.get(2).cloned().unwrap_or(Value::Undef);
    let body = lambda_of(h, &[body_form]);
    let mut pairs = Vec::new();
    for hc in elems.get(3..).unwrap_or(&[]) {
        let parts = h.list_vec(hc).ok_or("condition-case: malformed handler")?;
        let cond = quote_of(h, parts.first().cloned().unwrap_or(Value::Undef));
        let hthunk = lambda_of(h, parts.get(1..).unwrap_or(&[]));
        pairs.push(call_of(h, "list", vec![cond, hthunk]));
    }
    let handlers_form = call_of(h, "list", pairs);
    let qvar = quote_of(h, var);
    let form = call_of(h, "--condition-case--", vec![qvar, body, handlers_form]);
    compile_form(h, b, &form)
}

fn compile_body_chunk(h: &mut ElispHost, forms: &[Value]) -> Result<Chunk, String> {
    let mut bb = ChunkBuilder::new();
    compile_progn(h, &mut bb, forms)?;
    Ok(bb.build())
}

/// The body forms a closure *prints*, with Emacs's normalization of the empty
/// body: `(lambda ())` prints as `#[nil (nil) (t)]`, never `#[nil () (t)]`, and
/// the same holds for `defun` and `defmacro`. Only the printed source is
/// affected — an empty compiled body already evaluates to nil.
fn printable_body(forms: &[Value]) -> Vec<Value> {
    if forms.is_empty() {
        vec![Value::Undef]
    } else {
        forms.to_vec()
    }
}

fn compile_lambda(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    elems: &[Value],
    is_macro: bool,
) -> Result<(), String> {
    let arglist = elems.get(1).cloned().unwrap_or(Value::Undef);
    let params = h.parse_params(&arglist)?;
    let body = compile_body_chunk(h, elems.get(2..).unwrap_or(&[]))?;
    // Keep the source: Emacs's interpreted closure prints as `#[ARGLIST BODY ENV]`,
    // and the compiled `Chunk` cannot be turned back into forms.
    let src = Rc::new(crate::host::ClosureSrc {
        arglist: arglist.clone(),
        body: printable_body(elems.get(2..).unwrap_or(&[])),
    });
    let template = h.alloc(Obj::Closure {
        params: Rc::new(params),
        body: Rc::new(body),
        is_macro,
        env: None,
        // A template carries no mode; `instantiate_closure` stamps the mode
        // in force where the `lambda` is evaluated.
        dynamic: false,
        src,
    });
    load_const(b, template);
    // Capture the current lexical environment into a fresh closure at runtime.
    b.emit(Op::Extended(ops::MAKE_CLOSURE, 0), 0);
    Ok(())
}

fn compile_defun(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    elems: &[Value],
    is_macro: bool,
) -> Result<(), String> {
    let name = elems.get(1).cloned().ok_or("defun: missing name")?;
    if !matches!(h.obj(&name), Some(Obj::Symbol(_))) {
        return Err("defun: name must be a symbol".to_string());
    }
    let arglist = elems.get(2).cloned().unwrap_or(Value::Undef);
    let params = h.parse_params(&arglist)?;
    let body = compile_body_chunk(h, elems.get(3..).unwrap_or(&[]))?;
    let src = Rc::new(crate::host::ClosureSrc {
        arglist: arglist.clone(),
        body: printable_body(elems.get(3..).unwrap_or(&[])),
    });
    let template = h.alloc(Obj::Closure {
        params: Rc::new(params),
        body: Rc::new(body),
        is_macro,
        env: None,
        // A template carries no mode; `instantiate_closure` stamps the mode
        // in force where the `lambda` is evaluated.
        dynamic: false,
        src,
    });
    load_const(b, name); // symbol
    load_const(b, template); // definition template
    if !is_macro {
        // A defun captures its defining lexical env; a macro does not.
        b.emit(Op::Extended(ops::MAKE_CLOSURE, 0), 0);
    }
    b.emit(Op::Extended(ops::FSET, 0), 0); // sets function cell, leaves the symbol
    Ok(())
}

/// `defvar` and `defconst` differ in exactly one way, and it is not the
/// declaration: both mark the variable special, but `defvar`'s initializer runs
/// only when the variable is still void, while `defconst` always assigns. A
/// user's `(setq foo 5)` therefore survives a later `(defvar foo 9)` — which is
/// what makes it safe to `setq` a library's variable before loading it — and
/// does not survive `(defconst foo 9)`.
fn compile_defvar(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    elems: &[Value],
    constant: bool,
) -> Result<(), String> {
    let name = elems.get(1).cloned().ok_or("defvar: missing name")?;
    // defvar/defconst declare a dynamically-scoped (special) variable.
    h.set_special(&name);
    if let Some(init) = elems.get(2) {
        load_const(b, name.clone());
        compile_form(h, b, init)?;
        let op = if constant {
            ops::SETVAR
        } else {
            ops::DEFVAR_INIT
        };
        b.emit(Op::Extended(op, 0), 0);
        b.emit(Op::Pop, 0);
    }
    load_const(b, name);
    Ok(())
}

fn parse_binding(h: &ElispHost, bd: &Value) -> Result<(Value, Value), String> {
    // A constant in binding position is accepted here and rejected by the
    // runtime binder. Emacs evaluates every init before it attempts the first
    // write — `(let ((a (setq x 1)) (nil 2)) …)` sets x and *then* signals
    // `setting-constant` — and rejecting at compile time would additionally put
    // the error outside any enclosing `condition-case`, which catches it.
    if h.constant_symbol_name(bd).is_some() {
        return Ok((bd.clone(), Value::Undef));
    }
    if matches!(h.obj(bd), Some(Obj::Symbol(_))) {
        return Ok((bd.clone(), Value::Undef));
    }
    let parts = h.list_vec(bd).ok_or("let: malformed binding")?;
    let sym = parts.first().cloned().ok_or("let: empty binding")?;
    if !matches!(h.obj(&sym), Some(Obj::Symbol(_))) && h.constant_symbol_name(&sym).is_none() {
        return Err("let: binding name must be a symbol".to_string());
    }
    Ok((sym, parts.get(1).cloned().unwrap_or(Value::Undef)))
}

fn compile_let(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    elems: &[Value],
    sequential: bool,
) -> Result<(), String> {
    let bindings = h
        .list_vec(elems.first().unwrap_or(&Value::Undef))
        .unwrap_or_default();
    let parsed: Vec<(Value, Value)> = bindings
        .iter()
        .map(|bd| parse_binding(h, bd))
        .collect::<Result<_, _>>()?;
    let n = parsed.len();
    if sequential {
        // let*: open one scope, then bind each var before the next init is evaluated
        b.emit(Op::Extended(ops::SCOPE_OPEN, 0), 0);
        for (sym, init) in &parsed {
            compile_form(h, b, init)?;
            load_const(b, sym.clone());
            b.emit(Op::Extended(ops::SPECBIND, 0), 0);
        }
    } else {
        // let: evaluate all inits in the outer scope, then bind together
        for (sym, init) in &parsed {
            compile_form(h, b, init)?;
            load_const(b, sym.clone());
        }
        b.emit(Op::ExtendedWide(ops::LETBIND, n), 0);
    }
    compile_progn(h, b, elems.get(1..).unwrap_or(&[]))?;
    b.emit(Op::ExtendedWide(ops::UNBIND, n), 0);
    Ok(())
}

fn compile_progn(h: &mut ElispHost, b: &mut ChunkBuilder, forms: &[Value]) -> Result<(), String> {
    if forms.is_empty() {
        b.emit(Op::LoadUndef, 0);
        return Ok(());
    }
    for (i, f) in forms.iter().enumerate() {
        // Statement marker so breakpoints/stepping fire inside function bodies,
        // `progn`, `let` bodies, etc. — not just at top-level forms.
        emit_line_marker(h, b, f);
        compile_form(h, b, f)?;
        if i + 1 < forms.len() {
            b.emit(Op::Pop, 0);
        }
    }
    Ok(())
}

fn compile_prog1(h: &mut ElispHost, b: &mut ChunkBuilder, forms: &[Value]) -> Result<(), String> {
    if forms.is_empty() {
        b.emit(Op::LoadUndef, 0);
        return Ok(());
    }
    compile_form(h, b, &forms[0])?; // value kept
    for f in &forms[1..] {
        compile_form(h, b, f)?;
        b.emit(Op::Pop, 0);
    }
    Ok(())
}

fn compile_if(h: &mut ElispHost, b: &mut ChunkBuilder, parts: &[Value]) -> Result<(), String> {
    let cond = parts.first().cloned().unwrap_or(Value::Undef);
    let then = parts.get(1).cloned().unwrap_or(Value::Undef);
    compile_form(h, b, &cond)?;
    b.emit(Op::Extended(ops::TRUTHY, 0), 0);
    let jf = b.emit(Op::JumpIfFalse(0), 0);
    compile_form(h, b, &then)?;
    let jend = b.emit(Op::Jump(0), 0);
    let else_pos = b.current_pos();
    b.patch_jump(jf, else_pos);
    compile_progn(h, b, parts.get(2..).unwrap_or(&[]))?;
    let end_pos = b.current_pos();
    b.patch_jump(jend, end_pos);
    Ok(())
}

fn compile_when(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    parts: &[Value],
    polarity: bool,
) -> Result<(), String> {
    let cond = parts.first().cloned().unwrap_or(Value::Undef);
    compile_form(h, b, &cond)?;
    b.emit(Op::Extended(ops::TRUTHY, 0), 0);
    let jmp = if polarity {
        b.emit(Op::JumpIfFalse(0), 0)
    } else {
        b.emit(Op::JumpIfTrue(0), 0)
    };
    compile_progn(h, b, parts.get(1..).unwrap_or(&[]))?;
    let jend = b.emit(Op::Jump(0), 0);
    let skip_pos = b.current_pos();
    b.patch_jump(jmp, skip_pos);
    b.emit(Op::LoadUndef, 0);
    let end_pos = b.current_pos();
    b.patch_jump(jend, end_pos);
    Ok(())
}

fn compile_andor(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    forms: &[Value],
    is_and: bool,
) -> Result<(), String> {
    if forms.is_empty() {
        b.emit(if is_and { Op::LoadTrue } else { Op::LoadUndef }, 0);
        return Ok(());
    }
    let mut end_jumps = Vec::new();
    for (i, f) in forms.iter().enumerate() {
        compile_form(h, b, f)?;
        if i + 1 < forms.len() {
            b.emit(Op::Dup, 0);
            b.emit(Op::Extended(ops::TRUTHY, 0), 0);
            let j = if is_and {
                b.emit(Op::JumpIfFalse(0), 0)
            } else {
                b.emit(Op::JumpIfTrue(0), 0)
            };
            end_jumps.push(j);
            b.emit(Op::Pop, 0);
        }
    }
    let end_pos = b.current_pos();
    for j in end_jumps {
        b.patch_jump(j, end_pos);
    }
    Ok(())
}

fn compile_while(h: &mut ElispHost, b: &mut ChunkBuilder, parts: &[Value]) -> Result<(), String> {
    let start = b.current_pos();
    compile_form(h, b, parts.first().unwrap_or(&Value::Undef))?;
    b.emit(Op::Extended(ops::TRUTHY, 0), 0);
    let jexit = b.emit(Op::JumpIfFalse(0), 0);
    compile_progn(h, b, parts.get(1..).unwrap_or(&[]))?;
    b.emit(Op::Pop, 0); // discard each iteration's body value
    b.emit(Op::Jump(start), 0);
    let exit = b.current_pos();
    b.patch_jump(jexit, exit);
    b.emit(Op::LoadUndef, 0); // while returns nil
    Ok(())
}

fn compile_cond(h: &mut ElispHost, b: &mut ChunkBuilder, clauses: &[Value]) -> Result<(), String> {
    let mut end_jumps = Vec::new();
    for clause in clauses {
        let parts = h.list_vec(clause).ok_or("cond: malformed clause")?;
        if parts.is_empty() {
            continue;
        }
        compile_form(h, b, &parts[0])?; // test value on stack
        if parts.len() == 1 {
            // no body: value is the test value if non-nil
            b.emit(Op::Dup, 0);
            b.emit(Op::Extended(ops::TRUTHY, 0), 0);
            let jnext = b.emit(Op::JumpIfFalse(0), 0);
            let jend = b.emit(Op::Jump(0), 0); // truthy: keep test value
            end_jumps.push(jend);
            let next = b.current_pos();
            b.patch_jump(jnext, next);
            b.emit(Op::Pop, 0); // falsy: drop test value, continue
        } else {
            b.emit(Op::Extended(ops::TRUTHY, 0), 0);
            let jnext = b.emit(Op::JumpIfFalse(0), 0);
            compile_progn(h, b, &parts[1..])?;
            let jend = b.emit(Op::Jump(0), 0);
            end_jumps.push(jend);
            let next = b.current_pos();
            b.patch_jump(jnext, next);
        }
    }
    b.emit(Op::LoadUndef, 0); // no clause matched
    let end = b.current_pos();
    for j in end_jumps {
        b.patch_jump(j, end);
    }
    Ok(())
}

fn compile_setq(h: &mut ElispHost, b: &mut ChunkBuilder, parts: &[Value]) -> Result<(), String> {
    if parts.is_empty() {
        b.emit(Op::LoadUndef, 0);
        return Ok(());
    }
    let mut i = 0;
    while i + 1 < parts.len() {
        let sym = parts[i].clone();
        if !matches!(h.obj(&sym), Some(Obj::Symbol(_))) {
            return Err("setq: expected a symbol".to_string());
        }
        load_const(b, sym);
        compile_form(h, b, &parts[i + 1])?;
        b.emit(Op::Extended(ops::SETVAR, 0), 0);
        i += 2;
        if i + 1 < parts.len() {
            b.emit(Op::Pop, 0);
        }
    }
    Ok(())
}

fn load_const(b: &mut ChunkBuilder, v: Value) {
    let c = b.add_constant(v);
    b.emit(Op::LoadConst(c), 0);
}
