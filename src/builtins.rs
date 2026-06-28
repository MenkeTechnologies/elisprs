//! The elisp subr (builtin) library for milestone 1.
//!
//! Each builtin takes already-evaluated arguments. They are registered into the
//! obarray's function cells by [`install`]. This is intentionally a useful core
//! (list/number/string/predicate/IO/functional ops), not the full ~1000-subr
//! GNU Emacs surface.

use crate::error::{ElError, ElResult};
use crate::interp::{is_nil, list_from, t_or_nil, to_vec, Interp};
use rust_lisp::model::{List, Symbol, Value};
use std::io::Write;

// ── argument coercion helpers ───────────────────────────────────────────────

enum Num {
    I(i64),
    F(f64),
}

fn as_num(v: &Value) -> ElResult<Num> {
    match v {
        Value::Int(i) => Ok(Num::I(*i)),
        Value::Float(f) => Ok(Num::F(*f)),
        _ => Err(ElError::wrong_type("number", type_name(v))),
    }
}
fn to_f(n: &Num) -> f64 {
    match n {
        Num::I(i) => *i as f64,
        Num::F(f) => *f,
    }
}
fn as_int(v: &Value) -> ElResult<i64> {
    match v {
        Value::Int(i) => Ok(*i),
        _ => Err(ElError::wrong_type("integer", type_name(v))),
    }
}
fn as_string(v: &Value) -> ElResult<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        _ => Err(ElError::wrong_type("string", type_name(v))),
    }
}
fn sym_name(v: &Value) -> ElResult<String> {
    match v {
        Value::Symbol(s) => Ok(s.0.clone()),
        Value::True => Ok("t".to_string()),
        v if is_nil(v) => Ok("nil".to_string()),
        _ => Err(ElError::wrong_type("symbol", type_name(v))),
    }
}
fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Int(_) => "integer",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Symbol(_) => "symbol",
        Value::List(_) => "list",
        Value::True | Value::False => "symbol",
        _ => "object",
    }
}

/// `eq`: identity for atoms (numbers/symbols/nil/t), false for compound values.
fn el_eq(a: &Value, b: &Value) -> bool {
    if is_nil(a) && is_nil(b) {
        return true;
    }
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::Symbol(x), Value::Symbol(y)) => x.0 == y.0,
        (Value::True, Value::True) => true,
        _ => false,
    }
}
/// `equal`: structural. nil and rust_lisp NIL unify; otherwise rely on Value's
/// structural `PartialEq`.
fn el_equal(a: &Value, b: &Value) -> bool {
    if is_nil(a) && is_nil(b) {
        return true;
    }
    a == b
}

// ── arithmetic ──────────────────────────────────────────────────────────────

fn add(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let ns: Vec<Num> = args.iter().map(as_num).collect::<ElResult<_>>()?;
    if ns.iter().any(|n| matches!(n, Num::F(_))) {
        Ok(Value::Float(ns.iter().map(to_f).sum()))
    } else {
        Ok(Value::Int(ns.iter().map(|n| if let Num::I(i) = n { *i } else { 0 }).sum()))
    }
}
fn mul(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let ns: Vec<Num> = args.iter().map(as_num).collect::<ElResult<_>>()?;
    if ns.iter().any(|n| matches!(n, Num::F(_))) {
        Ok(Value::Float(ns.iter().map(to_f).product()))
    } else {
        Ok(Value::Int(ns.iter().map(|n| if let Num::I(i) = n { *i } else { 1 }).product()))
    }
}
fn sub(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let ns: Vec<Num> = args.iter().map(as_num).collect::<ElResult<_>>()?;
    if ns.is_empty() {
        return Ok(Value::Int(0));
    }
    let isf = ns.iter().any(|n| matches!(n, Num::F(_)));
    if ns.len() == 1 {
        return Ok(match &ns[0] {
            Num::I(i) => Value::Int(-i),
            Num::F(f) => Value::Float(-f),
        });
    }
    if isf {
        let mut acc = to_f(&ns[0]);
        for n in &ns[1..] {
            acc -= to_f(n);
        }
        Ok(Value::Float(acc))
    } else {
        let mut acc = if let Num::I(i) = ns[0] { i } else { 0 };
        for n in &ns[1..] {
            if let Num::I(i) = n {
                acc -= i;
            }
        }
        Ok(Value::Int(acc))
    }
}
fn div(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let ns: Vec<Num> = args.iter().map(as_num).collect::<ElResult<_>>()?;
    let isf = ns.iter().any(|n| matches!(n, Num::F(_)));
    if isf {
        let mut acc = to_f(&ns[0]);
        for n in &ns[1..] {
            acc /= to_f(n);
        }
        Ok(Value::Float(acc))
    } else {
        let mut acc = if let Num::I(i) = ns[0] { i } else { 0 };
        for n in &ns[1..] {
            let d = if let Num::I(i) = n { *i } else { 0 };
            if d == 0 {
                return Err(ElError::new("arith-error", "division by zero"));
            }
            acc /= d;
        }
        Ok(Value::Int(acc))
    }
}
fn modulo(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let a = as_int(&args[0])?;
    let b = as_int(&args[1])?;
    if b == 0 {
        return Err(ElError::new("arith-error", "division by zero"));
    }
    Ok(Value::Int(a.rem_euclid(b)))
}
fn one_plus(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    match as_num(&args[0])? {
        Num::I(i) => Ok(Value::Int(i + 1)),
        Num::F(f) => Ok(Value::Float(f + 1.0)),
    }
}
fn one_minus(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    match as_num(&args[0])? {
        Num::I(i) => Ok(Value::Int(i - 1)),
        Num::F(f) => Ok(Value::Float(f - 1.0)),
    }
}
fn abs(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    match as_num(&args[0])? {
        Num::I(i) => Ok(Value::Int(i.abs())),
        Num::F(f) => Ok(Value::Float(f.abs())),
    }
}
fn max_(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let mut best = args[0].clone();
    for a in &args[1..] {
        if to_f(&as_num(a)?) > to_f(&as_num(&best)?) {
            best = a.clone();
        }
    }
    Ok(best)
}
fn min_(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let mut best = args[0].clone();
    for a in &args[1..] {
        if to_f(&as_num(a)?) < to_f(&as_num(&best)?) {
            best = a.clone();
        }
    }
    Ok(best)
}

fn cmp_chain(args: &[Value], pred: impl Fn(f64, f64) -> bool) -> ElResult<Value> {
    for w in args.windows(2) {
        let a = to_f(&as_num(&w[0])?);
        let b = to_f(&as_num(&w[1])?);
        if !pred(a, b) {
            return Ok(Value::NIL);
        }
    }
    Ok(Value::True)
}
fn num_eq(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    cmp_chain(args, |a, b| a == b)
}
fn num_ne(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    cmp_chain(args, |a, b| a != b)
}
fn lt(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    cmp_chain(args, |a, b| a < b)
}
fn gt(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    cmp_chain(args, |a, b| a > b)
}
fn le(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    cmp_chain(args, |a, b| a <= b)
}
fn ge(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    cmp_chain(args, |a, b| a >= b)
}

// ── lists ───────────────────────────────────────────────────────────────────

fn car(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    match &args[0] {
        Value::List(l) => Ok(l.car().unwrap_or(Value::NIL)),
        v if is_nil(v) => Ok(Value::NIL),
        _ => Err(ElError::wrong_type("listp", type_name(&args[0]))),
    }
}
fn cdr(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    match &args[0] {
        Value::List(l) => Ok(Value::List(l.cdr())),
        v if is_nil(v) => Ok(Value::NIL),
        _ => Err(ElError::wrong_type("listp", type_name(&args[0]))),
    }
}
fn cons(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let x = args[0].clone();
    match &args[1] {
        Value::List(l) => Ok(Value::List(l.cons(x))),
        _ => Err(ElError::err(
            "cons: dotted pairs are not supported in milestone 1 (cdr must be a list)",
        )),
    }
}
fn list(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(list_from(args.to_vec()))
}
fn append(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let mut out = Vec::new();
    for a in args {
        if is_nil(a) {
            continue;
        }
        match to_vec(a) {
            Some(v) => out.extend(v),
            None => return Err(ElError::wrong_type("listp", type_name(a))),
        }
    }
    Ok(list_from(out))
}
fn nth(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let n = as_int(&args[0])?;
    let v = to_vec(&args[1]).unwrap_or_default();
    Ok(if n < 0 { Value::NIL } else { v.get(n as usize).cloned().unwrap_or(Value::NIL) })
}
fn nthcdr(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let n = as_int(&args[0])?.max(0) as usize;
    let v = to_vec(&args[1]).unwrap_or_default();
    Ok(list_from(v.get(n..).map(|s| s.to_vec()).unwrap_or_default()))
}
fn reverse(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let mut v = to_vec(&args[0]).unwrap_or_default();
    v.reverse();
    Ok(list_from(v))
}
fn length(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    match &args[0] {
        Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::List(l) => Ok(Value::Int(List::into_iter(l).count() as i64)),
        v if is_nil(v) => Ok(Value::Int(0)),
        _ => Err(ElError::wrong_type("sequencep", type_name(&args[0]))),
    }
}
fn member(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let v = to_vec(&args[1]).unwrap_or_default();
    for (i, e) in v.iter().enumerate() {
        if el_equal(&args[0], e) {
            return Ok(list_from(v[i..].to_vec()));
        }
    }
    Ok(Value::NIL)
}
fn memq(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let v = to_vec(&args[1]).unwrap_or_default();
    for (i, e) in v.iter().enumerate() {
        if el_eq(&args[0], e) {
            return Ok(list_from(v[i..].to_vec()));
        }
    }
    Ok(Value::NIL)
}
fn assoc_impl(args: &[Value], eq: fn(&Value, &Value) -> bool) -> ElResult<Value> {
    let v = to_vec(&args[1]).unwrap_or_default();
    for e in &v {
        if let Some(first) = to_vec(e).and_then(|p| p.into_iter().next()) {
            if eq(&args[0], &first) {
                return Ok(e.clone());
            }
        }
    }
    Ok(Value::NIL)
}
fn assoc(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    assoc_impl(args, el_equal)
}
fn assq(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    assoc_impl(args, el_eq)
}

// ── predicates ──────────────────────────────────────────────────────────────

fn eq_fn(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(el_eq(&args[0], &args[1])))
}
fn equal_fn(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(el_equal(&args[0], &args[1])))
}
fn null_fn(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(is_nil(&args[0])))
}
fn numberp(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(matches!(args[0], Value::Int(_) | Value::Float(_))))
}
fn integerp(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(matches!(args[0], Value::Int(_))))
}
fn floatp(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(matches!(args[0], Value::Float(_))))
}
fn stringp(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(matches!(args[0], Value::String(_))))
}
fn symbolp(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(matches!(args[0], Value::Symbol(_) | Value::True) || is_nil(&args[0])))
}
fn consp(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(matches!(&args[0], Value::List(_)) && !is_nil(&args[0])))
}
fn listp(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(matches!(&args[0], Value::List(_)) || is_nil(&args[0])))
}
fn atom(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(!(matches!(&args[0], Value::List(_)) && !is_nil(&args[0]))))
}
fn functionp(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(it.resolve(&args[0]).is_ok()))
}

// ── symbols / cells ─────────────────────────────────────────────────────────

fn set_fn(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let name = sym_name(&args[0])?;
    it.set_value(&name, args[1].clone());
    Ok(args[1].clone())
}
fn symbol_value(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let name = sym_name(&args[0])?;
    it.get_value(&name).ok_or_else(|| ElError::void_variable(&name))
}
fn symbol_function(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let name = sym_name(&args[0])?;
    it.get_function(&name).ok_or_else(|| ElError::void_function(&name))
}
fn fset(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let name = sym_name(&args[0])?;
    it.set_function(&name, args[1].clone());
    Ok(args[1].clone())
}
fn boundp(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(it.is_bound(&sym_name(&args[0])?)))
}
fn fboundp(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(it.is_fbound(&sym_name(&args[0])?)))
}
fn symbol_name(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(Value::String(sym_name(&args[0])?))
}
fn intern(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(Value::Symbol(Symbol(as_string(&args[0])?)))
}

// ── strings ─────────────────────────────────────────────────────────────────

fn concat(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let mut out = String::new();
    for a in args {
        out.push_str(&as_string(a)?);
    }
    Ok(Value::String(out))
}
fn string_eq(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(sym_or_string(&args[0])? == sym_or_string(&args[1])?))
}
fn string_lt(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(t_or_nil(sym_or_string(&args[0])? < sym_or_string(&args[1])?))
}
fn sym_or_string(v: &Value) -> ElResult<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Symbol(s) => Ok(s.0.clone()),
        _ => Err(ElError::wrong_type("string", type_name(v))),
    }
}
fn upcase(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(Value::String(as_string(&args[0])?.to_uppercase()))
}
fn downcase(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(Value::String(as_string(&args[0])?.to_lowercase()))
}
fn number_to_string(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(Value::String(it.print(&args[0], false)))
}
fn string_to_number(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let s = as_string(&args[0])?;
    let t = s.trim();
    if let Ok(i) = t.parse::<i64>() {
        Ok(Value::Int(i))
    } else if let Ok(f) = t.parse::<f64>() {
        Ok(Value::Float(f))
    } else {
        Ok(Value::Int(0))
    }
}

// ── format / IO ─────────────────────────────────────────────────────────────

fn el_format(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let fmt = as_string(&args[0])?;
    let mut out = String::new();
    let mut ai = 1usize;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('s') => {
                out.push_str(&it.print(arg(args, ai)?, false));
                ai += 1;
            }
            Some('S') => {
                out.push_str(&it.print(arg(args, ai)?, true));
                ai += 1;
            }
            Some('d') => {
                out.push_str(&as_int(arg(args, ai)?)?.to_string());
                ai += 1;
            }
            Some('x') => {
                out.push_str(&format!("{:x}", as_int(arg(args, ai)?)?));
                ai += 1;
            }
            Some('o') => {
                out.push_str(&format!("{:o}", as_int(arg(args, ai)?)?));
                ai += 1;
            }
            Some('c') => {
                let n = as_int(arg(args, ai)?)?;
                if let Some(ch) = char::from_u32(n as u32) {
                    out.push(ch);
                }
                ai += 1;
            }
            Some('f') => {
                out.push_str(&format!("{}", to_f(&as_num(arg(args, ai)?)?)));
                ai += 1;
            }
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    Ok(Value::String(out))
}
fn arg<'a>(args: &'a [Value], i: usize) -> ElResult<&'a Value> {
    args.get(i).ok_or_else(|| ElError::err("format: not enough arguments"))
}
fn message(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let s = el_format(it, args)?;
    if let Value::String(text) = &s {
        eprintln!("{text}");
    }
    Ok(s)
}
fn princ(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    print!("{}", it.print(&args[0], false));
    let _ = std::io::stdout().flush();
    Ok(args[0].clone())
}
fn prin1(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    print!("{}", it.print(&args[0], true));
    let _ = std::io::stdout().flush();
    Ok(args[0].clone())
}
fn print_fn(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    println!("{}", it.print(&args[0], true));
    Ok(args[0].clone())
}
fn terpri(_it: &mut Interp, _args: &[Value]) -> ElResult<Value> {
    println!();
    Ok(Value::True)
}

// ── functional ──────────────────────────────────────────────────────────────

fn funcall(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let call = it.resolve(&args[0])?;
    it.apply(call, args[1..].to_vec())
}
fn apply(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let call = it.resolve(&args[0])?;
    let mut rest = args[1..args.len() - 1].to_vec();
    let last = &args[args.len() - 1];
    rest.extend(to_vec(last).ok_or_else(|| ElError::wrong_type("listp", type_name(last)))?);
    it.apply(call, rest)
}
fn mapcar(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let call = it.resolve(&args[0])?;
    let seq = to_vec(&args[1]).ok_or_else(|| ElError::wrong_type("listp", type_name(&args[1])))?;
    let mut out = Vec::with_capacity(seq.len());
    for e in seq {
        out.push(it.apply(call.clone(), vec![e])?);
    }
    Ok(list_from(out))
}
fn mapc(it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    let call = it.resolve(&args[0])?;
    let seq = to_vec(&args[1]).ok_or_else(|| ElError::wrong_type("listp", type_name(&args[1])))?;
    for e in seq {
        it.apply(call.clone(), vec![e])?;
    }
    Ok(args[1].clone())
}
fn identity(_it: &mut Interp, args: &[Value]) -> ElResult<Value> {
    Ok(args[0].clone())
}

// ── registration ────────────────────────────────────────────────────────────

/// Install the milestone-1 subr set into `it`'s function cells.
pub fn install(it: &mut Interp) {
    // arithmetic
    it.defsubr("+", 0, None, add);
    it.defsubr("-", 0, None, sub);
    it.defsubr("*", 0, None, mul);
    it.defsubr("/", 1, None, div);
    it.defsubr("%", 2, Some(2), modulo);
    it.defsubr("mod", 2, Some(2), modulo);
    it.defsubr("1+", 1, Some(1), one_plus);
    it.defsubr("1-", 1, Some(1), one_minus);
    it.defsubr("abs", 1, Some(1), abs);
    it.defsubr("max", 1, None, max_);
    it.defsubr("min", 1, None, min_);
    it.defsubr("=", 1, None, num_eq);
    it.defsubr("/=", 2, Some(2), num_ne);
    it.defsubr("<", 1, None, lt);
    it.defsubr(">", 1, None, gt);
    it.defsubr("<=", 1, None, le);
    it.defsubr(">=", 1, None, ge);

    // lists
    it.defsubr("car", 1, Some(1), car);
    it.defsubr("cdr", 1, Some(1), cdr);
    it.defsubr("cons", 2, Some(2), cons);
    it.defsubr("list", 0, None, list);
    it.defsubr("append", 0, None, append);
    it.defsubr("nth", 2, Some(2), nth);
    it.defsubr("nthcdr", 2, Some(2), nthcdr);
    it.defsubr("reverse", 1, Some(1), reverse);
    it.defsubr("length", 1, Some(1), length);
    it.defsubr("member", 2, Some(2), member);
    it.defsubr("memq", 2, Some(2), memq);
    it.defsubr("assoc", 2, Some(2), assoc);
    it.defsubr("assq", 2, Some(2), assq);

    // predicates
    it.defsubr("eq", 2, Some(2), eq_fn);
    it.defsubr("eql", 2, Some(2), eq_fn);
    it.defsubr("equal", 2, Some(2), equal_fn);
    it.defsubr("null", 1, Some(1), null_fn);
    it.defsubr("not", 1, Some(1), null_fn);
    it.defsubr("numberp", 1, Some(1), numberp);
    it.defsubr("integerp", 1, Some(1), integerp);
    it.defsubr("floatp", 1, Some(1), floatp);
    it.defsubr("stringp", 1, Some(1), stringp);
    it.defsubr("symbolp", 1, Some(1), symbolp);
    it.defsubr("consp", 1, Some(1), consp);
    it.defsubr("listp", 1, Some(1), listp);
    it.defsubr("atom", 1, Some(1), atom);
    it.defsubr("functionp", 1, Some(1), functionp);

    // symbols / cells
    it.defsubr("set", 2, Some(2), set_fn);
    it.defsubr("symbol-value", 1, Some(1), symbol_value);
    it.defsubr("symbol-function", 1, Some(1), symbol_function);
    it.defsubr("fset", 2, Some(2), fset);
    it.defsubr("boundp", 1, Some(1), boundp);
    it.defsubr("fboundp", 1, Some(1), fboundp);
    it.defsubr("symbol-name", 1, Some(1), symbol_name);
    it.defsubr("intern", 1, Some(1), intern);
    it.defsubr("make-symbol", 1, Some(1), intern);

    // strings
    it.defsubr("concat", 0, None, concat);
    it.defsubr("string=", 2, Some(2), string_eq);
    it.defsubr("string-equal", 2, Some(2), string_eq);
    it.defsubr("string<", 2, Some(2), string_lt);
    it.defsubr("string-lessp", 2, Some(2), string_lt);
    it.defsubr("upcase", 1, Some(1), upcase);
    it.defsubr("downcase", 1, Some(1), downcase);
    it.defsubr("number-to-string", 1, Some(1), number_to_string);
    it.defsubr("string-to-number", 1, Some(2), string_to_number);

    // format / IO
    it.defsubr("format", 1, None, el_format);
    it.defsubr("message", 1, None, message);
    it.defsubr("princ", 1, Some(2), princ);
    it.defsubr("prin1", 1, Some(2), prin1);
    it.defsubr("print", 1, Some(2), print_fn);
    it.defsubr("terpri", 0, Some(1), terpri);

    // functional
    it.defsubr("funcall", 1, None, funcall);
    it.defsubr("apply", 2, None, apply);
    it.defsubr("mapcar", 2, Some(2), mapcar);
    it.defsubr("mapc", 2, Some(2), mapc);
    it.defsubr("identity", 1, Some(1), identity);
}
