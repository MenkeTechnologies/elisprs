//! Primitive subrs, written in Rust. Per the research inventory these are the
//! ~irreducible core; the large derived surface (caar.., seq-*, cl-*, alist
//! helpers) will be defined in an elisp prelude on top of these.

use crate::host::{ElispHost, Obj};
use fusevm::Value;

type R = Result<Value, String>;

fn nil_or(b: bool) -> Value {
    if b {
        Value::Bool(true)
    } else {
        Value::Undef
    }
}
fn is_nil(v: &Value) -> bool {
    matches!(v, Value::Undef | Value::Bool(false))
}

// ── numeric helpers ──
fn as_num(v: &Value) -> Result<(i64, f64, bool), String> {
    match v {
        Value::Int(n) => Ok((*n, *n as f64, false)),
        Value::Float(f) => Ok((*f as i64, *f, true)),
        _ => Err(format!("wrong-type-argument: numberp {}", v.as_str_cow())),
    }
}
fn num_result(i: i64, f: f64, isf: bool) -> Value {
    if isf {
        Value::Float(f)
    } else {
        Value::Int(i)
    }
}

fn add(_h: &mut ElispHost, a: &[Value]) -> R {
    let (mut i, mut f, mut isf) = (0i64, 0f64, false);
    for v in a {
        let (vi, vf, vfl) = as_num(v)?;
        isf |= vfl;
        i += vi;
        f += vf;
    }
    Ok(num_result(i, f, isf))
}
fn mul(_h: &mut ElispHost, a: &[Value]) -> R {
    let (mut i, mut f, mut isf) = (1i64, 1f64, false);
    for v in a {
        let (vi, vf, vfl) = as_num(v)?;
        isf |= vfl;
        i *= vi;
        f *= vf;
    }
    Ok(num_result(i, f, isf))
}
fn sub(_h: &mut ElispHost, a: &[Value]) -> R {
    if a.is_empty() {
        return Ok(Value::Int(0));
    }
    let (i0, f0, mut isf) = as_num(&a[0])?;
    if a.len() == 1 {
        return Ok(if isf {
            Value::Float(-f0)
        } else {
            Value::Int(-i0)
        });
    }
    let (mut i, mut f) = (i0, f0);
    for v in &a[1..] {
        let (vi, vf, vfl) = as_num(v)?;
        isf |= vfl;
        i -= vi;
        f -= vf;
    }
    Ok(num_result(i, f, isf))
}
fn div(_h: &mut ElispHost, a: &[Value]) -> R {
    let (i0, f0, mut isf) = as_num(&a[0])?;
    let (mut i, mut f) = (i0, f0);
    for v in &a[1..] {
        let (vi, vf, vfl) = as_num(v)?;
        isf |= vfl;
        if !isf && vi == 0 {
            return Err("arith-error: division by zero".to_string());
        }
        if vi != 0 {
            i /= vi;
        }
        f /= vf;
    }
    Ok(num_result(i, f, isf))
}
fn modulo(_h: &mut ElispHost, a: &[Value]) -> R {
    let x = as_num(&a[0])?.0;
    let y = as_num(&a[1])?.0;
    if y == 0 {
        return Err("arith-error: division by zero".to_string());
    }
    Ok(Value::Int(x % y))
}
fn one_plus(_h: &mut ElispHost, a: &[Value]) -> R {
    let (i, f, isf) = as_num(&a[0])?;
    Ok(if isf {
        Value::Float(f + 1.0)
    } else {
        Value::Int(i + 1)
    })
}
fn one_minus(_h: &mut ElispHost, a: &[Value]) -> R {
    let (i, f, isf) = as_num(&a[0])?;
    Ok(if isf {
        Value::Float(f - 1.0)
    } else {
        Value::Int(i - 1)
    })
}

fn cmp(a: &[Value], pred: fn(f64, f64) -> bool) -> R {
    for w in a.windows(2) {
        if !pred(as_num(&w[0])?.1, as_num(&w[1])?.1) {
            return Ok(Value::Undef);
        }
    }
    Ok(Value::Bool(true))
}
fn num_eq(_h: &mut ElispHost, a: &[Value]) -> R {
    cmp(a, |x, y| x == y)
}
fn lt(_h: &mut ElispHost, a: &[Value]) -> R {
    cmp(a, |x, y| x < y)
}
fn gt(_h: &mut ElispHost, a: &[Value]) -> R {
    cmp(a, |x, y| x > y)
}
fn le(_h: &mut ElispHost, a: &[Value]) -> R {
    cmp(a, |x, y| x <= y)
}
fn ge(_h: &mut ElispHost, a: &[Value]) -> R {
    cmp(a, |x, y| x >= y)
}

// ── equality ──
fn el_eq(h: &ElispHost, a: &Value, b: &Value) -> bool {
    if is_nil(a) && is_nil(b) {
        return true;
    }
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::Obj(x), Value::Obj(y)) => x == y,
        (Value::Bool(true), Value::Bool(true)) => true,
        _ => {
            let _ = h;
            false
        }
    }
}
fn el_equal(h: &ElispHost, a: &Value, b: &Value) -> bool {
    if el_eq(h, a, b) {
        return true;
    }
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Obj(_), Value::Obj(_)) => match (h.obj(a), h.obj(b)) {
            (Some(Obj::Cons(a1, a2)), Some(Obj::Cons(b1, b2))) => {
                el_equal(h, a1, b1) && el_equal(h, a2, b2)
            }
            (Some(Obj::Vector(va)), Some(Obj::Vector(vb))) => {
                va.len() == vb.len() && va.iter().zip(vb).all(|(x, y)| el_equal(h, x, y))
            }
            _ => false,
        },
        _ => false,
    }
}
fn eq_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(el_eq(h, &a[0], &a[1])))
}
fn equal_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(el_equal(h, &a[0], &a[1])))
}

// ── lists ──
fn cons_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(h.cons(a[0].clone(), a[1].clone()))
}
fn car(h: &mut ElispHost, a: &[Value]) -> R {
    match h.obj(&a[0]) {
        Some(Obj::Cons(x, _)) => Ok(x.clone()),
        _ if is_nil(&a[0]) => Ok(Value::Undef),
        _ => Err(format!(
            "wrong-type-argument: listp {}",
            h.print(&a[0], true)
        )),
    }
}
fn cdr(h: &mut ElispHost, a: &[Value]) -> R {
    match h.obj(&a[0]) {
        Some(Obj::Cons(_, y)) => Ok(y.clone()),
        _ if is_nil(&a[0]) => Ok(Value::Undef),
        _ => Err(format!(
            "wrong-type-argument: listp {}",
            h.print(&a[0], true)
        )),
    }
}
fn setcar(h: &mut ElispHost, a: &[Value]) -> R {
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::Cons(c, _)) = h.arena.get_mut(*id as usize) {
            *c = a[1].clone();
            return Ok(a[1].clone());
        }
    }
    Err("wrong-type-argument: consp".to_string())
}
fn setcdr(h: &mut ElispHost, a: &[Value]) -> R {
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::Cons(_, d)) = h.arena.get_mut(*id as usize) {
            *d = a[1].clone();
            return Ok(a[1].clone());
        }
    }
    Err("wrong-type-argument: consp".to_string())
}
fn list_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(h.list_from(a.to_vec()))
}
fn append_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut out = Vec::new();
    for v in a {
        if is_nil(v) {
            continue;
        }
        match h.list_vec(v) {
            Some(items) => out.extend(items),
            None => return Err("wrong-type-argument: listp".to_string()),
        }
    }
    Ok(h.list_from(out))
}
fn reverse_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut v = h.list_vec(&a[0]).ok_or("wrong-type-argument: listp")?;
    v.reverse();
    Ok(h.list_from(v))
}
fn length_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match &a[0] {
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::Undef => Ok(Value::Int(0)),
        Value::Obj(_) => match h.obj(&a[0]) {
            Some(Obj::Vector(items)) => Ok(Value::Int(items.len() as i64)),
            _ => Ok(Value::Int(
                h.list_vec(&a[0]).map(|v| v.len()).unwrap_or(0) as i64
            )),
        },
        _ => Err("wrong-type-argument: sequencep".to_string()),
    }
}
fn nth_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_num(&a[0])?.0;
    let v = h.list_vec(&a[1]).unwrap_or_default();
    Ok(if n < 0 {
        Value::Undef
    } else {
        v.get(n as usize).cloned().unwrap_or(Value::Undef)
    })
}

// ── predicates ──
fn null_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(is_nil(&a[0])))
}
fn consp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Cons(..)))))
}
fn listp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        is_nil(&a[0]) || matches!(h.obj(&a[0]), Some(Obj::Cons(..))),
    ))
}
fn atom(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(!matches!(h.obj(&a[0]), Some(Obj::Cons(..)))))
}
fn symbolp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        matches!(a[0], Value::Bool(true) | Value::Undef)
            || matches!(h.obj(&a[0]), Some(Obj::Symbol(_))),
    ))
}
fn stringp(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(a[0], Value::Str(_))))
}
fn numberp(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(a[0], Value::Int(_) | Value::Float(_))))
}
fn integerp(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(a[0], Value::Int(_))))
}
fn floatp(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(a[0], Value::Float(_))))
}
fn vectorp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Vector(_)))))
}
fn zerop(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        matches!(a[0], Value::Int(0)) || matches!(a[0], Value::Float(f) if f == 0.0),
    ))
}

// ── vectors ──
fn vector_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(h.alloc(Obj::Vector(a.to_vec())))
}
fn make_vector(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_num(&a[0])?.0.max(0) as usize;
    Ok(h.alloc(Obj::Vector(vec![a[1].clone(); n])))
}
fn aref(h: &mut ElispHost, a: &[Value]) -> R {
    let i = as_num(&a[1])?.0.max(0) as usize;
    match h.obj(&a[0]) {
        Some(Obj::Vector(items)) => items.get(i).cloned().ok_or("args-out-of-range".to_string()),
        _ => match &a[0] {
            Value::Str(s) => s
                .chars()
                .nth(i)
                .map(|c| Value::Int(c as i64))
                .ok_or("args-out-of-range".to_string()),
            _ => Err("wrong-type-argument: arrayp".to_string()),
        },
    }
}
fn aset(h: &mut ElispHost, a: &[Value]) -> R {
    let i = as_num(&a[1])?.0.max(0) as usize;
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::Vector(items)) = h.arena.get_mut(*id as usize) {
            if i < items.len() {
                items[i] = a[2].clone();
                return Ok(a[2].clone());
            }
            return Err("args-out-of-range".to_string());
        }
    }
    Err("wrong-type-argument: arrayp".to_string())
}

// ── symbols / cells ──
fn symbol_name(h: &mut ElispHost, a: &[Value]) -> R {
    h.sym_name(&a[0])
        .map(Value::str)
        .ok_or("wrong-type-argument: symbolp".to_string())
}
fn intern_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match &a[0] {
        Value::Str(s) => Ok(h.intern(&s.to_string())),
        _ => Err("wrong-type-argument: stringp".to_string()),
    }
}
fn set_fn(h: &mut ElispHost, a: &[Value]) -> R {
    h.set_value(&a[0], a[1].clone())?;
    Ok(a[1].clone())
}
fn symbol_value(h: &mut ElispHost, a: &[Value]) -> R {
    h.get_value(&a[0])
}

// ── functional ──
// `funcall`/`apply`/`mapcar`/`mapc` are intercepted in `host::call_function`
// (they re-enter elisp, so they can't run inside a host borrow) — they are not
// plain subrs here.
fn identity(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(a[0].clone())
}

// ── nonlocal exits ──
// `throw` records the (tag, value) and aborts via the error channel; `catch`
// (an intrinsic in host::call_function) intercepts it.
fn throw_fn(h: &mut ElispHost, a: &[Value]) -> R {
    h.pending_throw = Some((a[0].clone(), a.get(1).cloned().unwrap_or(Value::Undef)));
    Err("--throw--".to_string())
}
fn error_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let msg = el_format(h, a)?;
    Err(format!("error: {msg}"))
}
fn signal_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let sym = h.sym_name(&a[0]).unwrap_or_else(|| "error".to_string());
    let data = h.print(a.get(1).unwrap_or(&Value::Undef), true);
    Err(format!("{sym}: {data}"))
}

// ── strings / format / IO ──
fn concat_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut out = String::new();
    for v in a {
        match v {
            Value::Str(s) => out.push_str(s),
            Value::Undef => {}
            _ => {
                // concat over a list of chars
                if let Some(items) = h.list_vec(v) {
                    for it in items {
                        if let Value::Int(c) = it {
                            if let Some(ch) = char::from_u32(c as u32) {
                                out.push(ch);
                            }
                        }
                    }
                } else {
                    return Err("wrong-type-argument: sequencep".to_string());
                }
            }
        }
    }
    Ok(Value::str(out))
}
fn el_format(h: &ElispHost, a: &[Value]) -> Result<String, String> {
    let fmt = match &a[0] {
        Value::Str(s) => s.to_string(),
        _ => return Err("format: not a string".to_string()),
    };
    let mut out = String::new();
    let mut ai = 1;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('s') => {
                out.push_str(&h.print(a.get(ai).unwrap_or(&Value::Undef), false));
                ai += 1;
            }
            Some('S') => {
                out.push_str(&h.print(a.get(ai).unwrap_or(&Value::Undef), true));
                ai += 1;
            }
            Some('d') => {
                out.push_str(&as_num(a.get(ai).unwrap_or(&Value::Undef))?.0.to_string());
                ai += 1;
            }
            Some('x') => {
                out.push_str(&format!(
                    "{:x}",
                    as_num(a.get(ai).unwrap_or(&Value::Undef))?.0
                ));
                ai += 1;
            }
            Some('c') => {
                if let Some(ch) =
                    char::from_u32(as_num(a.get(ai).unwrap_or(&Value::Undef))?.0 as u32)
                {
                    out.push(ch);
                }
                ai += 1;
            }
            Some('f') => {
                out.push_str(&format!(
                    "{}",
                    as_num(a.get(ai).unwrap_or(&Value::Undef))?.1
                ));
                ai += 1;
            }
            Some(o) => {
                out.push('%');
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    Ok(out)
}
fn format_fn(h: &mut ElispHost, a: &[Value]) -> R {
    el_format(h, a).map(Value::str)
}
fn message_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let s = el_format(h, a)?;
    eprintln!("{s}");
    Ok(Value::str(s))
}
fn princ_fn(h: &mut ElispHost, a: &[Value]) -> R {
    print!("{}", h.print(&a[0], false));
    use std::io::Write;
    let _ = std::io::stdout().flush();
    Ok(a[0].clone())
}
fn prin1_fn(h: &mut ElispHost, a: &[Value]) -> R {
    print!("{}", h.print(&a[0], true));
    use std::io::Write;
    let _ = std::io::stdout().flush();
    Ok(a[0].clone())
}
fn number_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::str(h.print(&a[0], false)))
}

/// Install the primitive subr set.
pub fn install(h: &mut ElispHost) {
    let mut s = |n: &str, min: usize, max: Option<usize>, f: crate::host::SubrFn| {
        h.defsubr(n, min, max, f);
    };
    // arithmetic
    s("+", 0, None, add);
    s("-", 0, None, sub);
    s("*", 0, None, mul);
    s("/", 1, None, div);
    s("%", 2, Some(2), modulo);
    s("1+", 1, Some(1), one_plus);
    s("1-", 1, Some(1), one_minus);
    s("=", 1, None, num_eq);
    s("<", 1, None, lt);
    s(">", 1, None, gt);
    s("<=", 1, None, le);
    s(">=", 1, None, ge);
    // equality / predicates
    s("eq", 2, Some(2), eq_fn);
    s("eql", 2, Some(2), eq_fn);
    s("equal", 2, Some(2), equal_fn);
    s("null", 1, Some(1), null_fn);
    s("not", 1, Some(1), null_fn);
    s("consp", 1, Some(1), consp);
    s("listp", 1, Some(1), listp);
    s("atom", 1, Some(1), atom);
    s("symbolp", 1, Some(1), symbolp);
    s("stringp", 1, Some(1), stringp);
    s("numberp", 1, Some(1), numberp);
    s("integerp", 1, Some(1), integerp);
    s("floatp", 1, Some(1), floatp);
    s("vectorp", 1, Some(1), vectorp);
    s("zerop", 1, Some(1), zerop);
    // lists
    s("cons", 2, Some(2), cons_fn);
    s("car", 1, Some(1), car);
    s("cdr", 1, Some(1), cdr);
    s("setcar", 2, Some(2), setcar);
    s("setcdr", 2, Some(2), setcdr);
    s("list", 0, None, list_fn);
    s("append", 0, None, append_fn);
    s("reverse", 1, Some(1), reverse_fn);
    s("length", 1, Some(1), length_fn);
    s("nth", 2, Some(2), nth_fn);
    // vectors
    s("vector", 0, None, vector_fn);
    s("make-vector", 2, Some(2), make_vector);
    s("aref", 2, Some(2), aref);
    s("aset", 3, Some(3), aset);
    // symbols
    s("symbol-name", 1, Some(1), symbol_name);
    s("intern", 1, Some(2), intern_fn);
    s("make-symbol", 1, Some(1), intern_fn);
    s("set", 2, Some(2), set_fn);
    s("symbol-value", 1, Some(1), symbol_value);
    // functional (funcall/apply/mapcar/mapc are handled in host::call_function)
    s("identity", 1, Some(1), identity);
    // nonlocal exits (catch/unwind-protect/condition-case are compiler intrinsics)
    s("throw", 2, Some(2), throw_fn);
    s("error", 1, None, error_fn);
    s("user-error", 1, None, error_fn);
    s("signal", 2, Some(2), signal_fn);
    // strings / IO
    s("concat", 0, None, concat_fn);
    s("format", 1, None, format_fn);
    s("message", 1, None, message_fn);
    s("princ", 1, Some(2), princ_fn);
    s("prin1", 1, Some(2), prin1_fn);
    s("number-to-string", 1, Some(1), number_to_string);
}
