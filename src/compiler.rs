//! The elisp → fusevm lowering. This is the whole point of elisprs: elisp does
//! not run on a bespoke interpreter, it compiles to `fusevm::Chunk` and executes
//! on fusevm (with its JIT/AOT). Heap objects ride as `Value::Obj` handles; the
//! ElispHost (reached via fusevm's extension handler) supplies the semantics.
//!
//! Lowering conventions:
//! - self-evaluating literals → `LoadInt`/`LoadFloat`/`LoadConst`/`LoadUndef`/`LoadTrue`
//! - `(quote X)` → `LoadConst(X)` (X is a heap constant)
//! - a function call `(f a b)` → `LoadConst(f-symbol)`, args…, `Extended(CALL, argc)`
//! - elisp truthiness ≠ fusevm truthiness (only nil is false), so conditionals
//!   emit `Extended(TRUTHY)` before `JumpIfFalse` — the strykelang pattern.
//!
//! Supported special forms this milestone: quote, function, if, progn, when,
//! unless, and, or, setq. defun/let/lambda (the calling-convention milestone)
//! return a clear "not yet lowered" error.

use crate::host::{ops, ElispHost, Obj};
use fusevm::{Chunk, ChunkBuilder, Op, Value};

/// Compile one top-level form into a runnable chunk (value left on the stack).
pub fn compile_top(h: &mut ElispHost, form: &Value) -> Result<Chunk, String> {
    let mut b = ChunkBuilder::new();
    compile_form(h, &mut b, form)?;
    // No explicit terminator op: the VM halts when `ip` runs past the last op,
    // leaving the result on the stack (read by host::run_chunk).
    Ok(b.build())
}

/// Compile a whole program (forms run in sequence; last value remains).
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
                    load_const(b, form.clone()); // keywords self-evaluate
                } else {
                    // dynamic variable reference
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
        Some("progn") => compile_progn(h, b, &elems[1..])?,
        Some("if") => compile_if(h, b, &elems[1..])?,
        Some("when") => compile_when(h, b, &elems[1..], true)?,
        Some("unless") => compile_when(h, b, &elems[1..], false)?,
        Some("and") => compile_andor(h, b, &elems[1..], true)?,
        Some("or") => compile_andor(h, b, &elems[1..], false)?,
        Some("setq") => compile_setq(h, b, &elems[1..])?,
        Some(kw) if is_unsupported_special(kw) => {
            return Err(format!("special form `{kw}` not yet lowered (calling-convention milestone)"));
        }
        _ => {
            // function call: push symbol, push args, Extended(CALL, argc)
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
        "lambda" | "defun" | "defmacro" | "let" | "let*" | "defvar" | "defconst"
            | "while" | "cond" | "condition-case" | "unwind-protect" | "catch" | "throw"
    )
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
    // `unless` inverts by jumping on the opposite truth value.
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
    // Evaluate each; short-circuit keeping the deciding value on the stack.
    let mut end_jumps = Vec::new();
    for (i, f) in forms.iter().enumerate() {
        compile_form(h, b, f)?;
        if i + 1 < forms.len() {
            b.emit(Op::Dup, 0);
            b.emit(Op::Extended(ops::TRUTHY, 0), 0);
            // and: stop (jump to end) if false; or: stop if true. Keep value.
            let j = if is_and {
                b.emit(Op::JumpIfFalse(0), 0)
            } else {
                b.emit(Op::JumpIfTrue(0), 0)
            };
            end_jumps.push(j);
            b.emit(Op::Pop, 0); // discard the value we just tested; continue
        }
    }
    let end_pos = b.current_pos();
    for j in end_jumps {
        b.patch_jump(j, end_pos);
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
        load_const(b, sym); // push symbol
        compile_form(h, b, &parts[i + 1])?; // push value
        b.emit(Op::Extended(ops::SETVAR, 0), 0); // sets cell, leaves value
        i += 2;
        if i + 1 < parts.len() {
            b.emit(Op::Pop, 0); // discard all but the last value
        }
    }
    Ok(())
}

fn load_const(b: &mut ChunkBuilder, v: Value) {
    let c = b.add_constant(v);
    b.emit(Op::LoadConst(c), 0);
}
