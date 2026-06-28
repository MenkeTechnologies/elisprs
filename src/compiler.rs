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

use crate::host::{ops, ElispHost, Obj};
use fusevm::{Chunk, ChunkBuilder, Op, Value};
use std::rc::Rc;

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
        compile_form(h, &mut b, form)?;
        if i + 1 < forms.len() {
            b.emit(Op::Pop, 0);
        }
    }
    Ok(b.build())
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
        Some("function") => load_const(b, elems.get(1).cloned().unwrap_or(Value::Undef)),
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
        Some("defvar") | Some("defconst") => compile_defvar(h, b, &elems)?,
        Some("catch") => compile_catch(h, b, &elems)?,
        Some("unwind-protect") => compile_unwind(h, b, &elems)?,
        Some("condition-case") => compile_condition_case(h, b, &elems)?,
        Some(kw) if is_unsupported_special(kw) => {
            return Err(format!(
                "special form `{kw}` not yet lowered (buffer milestone)"
            ));
        }
        _ => {
            // function call
            load_const(b, head);
            let argc = elems.len() - 1;
            for arg in &elems[1..] {
                compile_form(h, b, arg)?;
            }
            if argc > u8::MAX as usize {
                return Err("too many arguments".to_string());
            }
            b.emit(Op::Extended(ops::CALL, argc as u8), 0);
        }
    }
    Ok(())
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

fn compile_lambda(
    h: &mut ElispHost,
    b: &mut ChunkBuilder,
    elems: &[Value],
    is_macro: bool,
) -> Result<(), String> {
    let arglist = elems.get(1).cloned().unwrap_or(Value::Undef);
    let params = h.parse_params(&arglist)?;
    let body = compile_body_chunk(h, elems.get(2..).unwrap_or(&[]))?;
    let clo = h.alloc(Obj::Closure {
        params: Rc::new(params),
        body: Rc::new(body),
        is_macro,
    });
    load_const(b, clo);
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
    let clo = h.alloc(Obj::Closure {
        params: Rc::new(params),
        body: Rc::new(body),
        is_macro,
    });
    load_const(b, name); // symbol
    load_const(b, clo); // definition
    b.emit(Op::Extended(ops::FSET, 0), 0); // sets function cell, leaves the symbol
    Ok(())
}

fn compile_defvar(h: &mut ElispHost, b: &mut ChunkBuilder, elems: &[Value]) -> Result<(), String> {
    let name = elems.get(1).cloned().ok_or("defvar: missing name")?;
    // Milestone simplification: set the value (always) and return the symbol.
    if let Some(init) = elems.get(2) {
        load_const(b, name.clone());
        compile_form(h, b, init)?;
        b.emit(Op::Extended(ops::SETVAR, 0), 0);
        b.emit(Op::Pop, 0);
    }
    load_const(b, name);
    Ok(())
}

fn parse_binding(h: &ElispHost, bd: &Value) -> Result<(Value, Value), String> {
    if matches!(h.obj(bd), Some(Obj::Symbol(_))) {
        return Ok((bd.clone(), Value::Undef));
    }
    let parts = h.list_vec(bd).ok_or("let: malformed binding")?;
    let sym = parts.first().cloned().ok_or("let: empty binding")?;
    if !matches!(h.obj(&sym), Some(Obj::Symbol(_))) {
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
        // let*: bind each before evaluating the next init
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
