//! Primitive subrs, written in Rust. Per the research inventory these are the
//! ~irreducible core; the large derived surface (caar.., seq-*, cl-*, alist
//! helpers) will be defined in an elisp prelude on top of these.

use crate::host::{ElispHost, MatchData, Obj};
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
fn as_int(v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        _ => Err(format!("wrong-type-argument: integerp {}", v.as_str_cow())),
    }
}
fn as_string(v: &Value) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s.to_string()),
        _ => Err(format!("wrong-type-argument: stringp {}", v.as_str_cow())),
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
// `mod` (vs `%`): the result takes the sign of the divisor, and either operand
// may be a float — (mod 13.5 4) => 1.5, (mod -1 3) => 2.
fn mod_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let (xi, xf, xisf) = as_num(&a[0])?;
    let (yi, yf, yisf) = as_num(&a[1])?;
    if xisf || yisf {
        if yf == 0.0 {
            return Err("arith-error: division by zero".to_string());
        }
        return Ok(Value::Float(xf - yf * (xf / yf).floor()));
    }
    if yi == 0 {
        return Err("arith-error: division by zero".to_string());
    }
    let mut r = xi % yi;
    if r != 0 && (r < 0) != (yi < 0) {
        r += yi;
    }
    Ok(Value::Int(r))
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
// `eq` is object identity. Fixnums and interned symbols/heap handles compare by
// value, but two distinct float *objects* are never `eq` (matching Emacs:
// `(eq 1.0 1.0)` => nil). `eql` adds by-value float comparison on top of `eq`.
fn el_eq(h: &ElispHost, a: &Value, b: &Value) -> bool {
    if is_nil(a) && is_nil(b) {
        return true;
    }
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Obj(x), Value::Obj(y)) => x == y,
        (Value::Bool(true), Value::Bool(true)) => true,
        _ => {
            let _ = h;
            false
        }
    }
}
fn el_eql(h: &ElispHost, a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        _ => el_eq(h, a, b),
    }
}
fn el_equal(h: &ElispHost, a: &Value, b: &Value) -> bool {
    if el_eql(h, a, b) {
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
fn eql_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(el_eql(h, &a[0], &a[1])))
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
        // Lists, vectors, and strings are all sequences `append` can flatten.
        match h.obj(v) {
            Some(Obj::Vector(items)) => out.extend(items.clone()),
            _ => match v {
                Value::Str(s) => out.extend(s.chars().map(|c| Value::Int(c as i64))),
                _ => match h.list_vec(v) {
                    Some(items) => out.extend(items),
                    None => return Err("wrong-type-argument: sequencep".to_string()),
                },
            },
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
fn make_symbol_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match &a[0] {
        Value::Str(s) => Ok(h.make_symbol(&s.to_string())),
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
fn terpri(_h: &mut ElispHost, _a: &[Value]) -> R {
    println!();
    Ok(Value::Bool(true))
}
fn print_fn(h: &mut ElispHost, a: &[Value]) -> R {
    println!("{}", h.print(&a[0], true));
    Ok(a[0].clone())
}
fn prin1_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::str(h.print(&a[0], true)))
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
/// A parsed `%`-directive: `%[-][0][width][.prec]CONV`.
struct FmtSpec {
    left: bool,
    zero: bool,
    width: usize,
    prec: Option<usize>,
    conv: char,
}

/// Pad `body` to `spec.width` honoring the `-` (left) and `0` (zero-fill) flags.
/// Zero-fill only applies to right-justified numerics and goes after any sign.
fn pad(body: String, spec: &FmtSpec) -> String {
    if body.chars().count() >= spec.width {
        return body;
    }
    let fill = spec.width - body.chars().count();
    if spec.left {
        format!("{body}{}", " ".repeat(fill))
    } else if spec.zero && matches!(spec.conv, 'd' | 'o' | 'x' | 'X' | 'e' | 'f' | 'g') {
        match body.strip_prefix('-') {
            Some(rest) => format!("-{}{rest}", "0".repeat(fill)),
            None => format!("{}{body}", "0".repeat(fill)),
        }
    } else {
        format!("{}{body}", " ".repeat(fill))
    }
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
        // %% is a literal percent and takes no flags/argument.
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }
        // flags
        let (mut left, mut zero) = (false, false);
        while let Some(&f) = chars.peek() {
            match f {
                '-' => left = true,
                '0' => zero = true,
                _ => break,
            }
            chars.next();
        }
        // width
        let mut width = 0usize;
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                width = width * 10 + (d as usize - '0' as usize);
                chars.next();
            } else {
                break;
            }
        }
        // .precision
        let mut prec = None;
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut p = 0usize;
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    p = p * 10 + (d as usize - '0' as usize);
                    chars.next();
                } else {
                    break;
                }
            }
            prec = Some(p);
        }
        let Some(conv) = chars.next() else {
            out.push('%');
            break;
        };
        let spec = FmtSpec {
            left,
            zero,
            width,
            prec,
            conv,
        };
        let arg = a.get(ai).unwrap_or(&Value::Undef);
        let body = match conv {
            's' => {
                let s = h.print(arg, false);
                match spec.prec {
                    Some(p) => s.chars().take(p).collect(),
                    None => s,
                }
            }
            'S' => h.print(arg, true),
            'd' => as_num(arg)?.0.to_string(),
            'o' => format!("{:o}", as_num(arg)?.0),
            'x' => format!("{:x}", as_num(arg)?.0),
            'X' => format!("{:X}", as_num(arg)?.0),
            'c' => char::from_u32(as_num(arg)?.0 as u32)
                .map(String::from)
                .unwrap_or_default(),
            'e' => match spec.prec {
                Some(p) => format!("{:.*e}", p, as_num(arg)?.1),
                None => format!("{:e}", as_num(arg)?.1),
            },
            'f' => format!("{:.*}", spec.prec.unwrap_or(6), as_num(arg)?.1),
            'g' => format!("{}", as_num(arg)?.1),
            other => {
                // Unknown directive: emit verbatim, consume no argument.
                out.push('%');
                out.push(other);
                continue;
            }
        };
        ai += 1;
        out.push_str(&pad(body, &spec));
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

// ── hash tables ──
fn hash_eq(h: &ElispHost, test: u8, a: &Value, b: &Value) -> bool {
    if test == 2 {
        el_equal(h, a, b)
    } else {
        el_eq(h, a, b)
    }
}
fn make_hash_table(h: &mut ElispHost, a: &[Value]) -> R {
    let mut test = 1u8; // eql default
    let mut i = 0;
    while i + 1 < a.len() {
        if h.sym_name(&a[i]).as_deref() == Some(":test") {
            test = match h.sym_name(&a[i + 1]).as_deref() {
                Some("eq") => 0,
                Some("equal") => 2,
                _ => 1,
            };
        }
        i += 2;
    }
    Ok(h.alloc(Obj::HashTable {
        test,
        entries: Vec::new(),
    }))
}
fn ht_view(h: &ElispHost, v: &Value) -> Result<(u8, Vec<(Value, Value)>), String> {
    match h.obj(v) {
        Some(Obj::HashTable { test, entries }) => Ok((*test, entries.clone())),
        _ => Err("wrong-type-argument: hash-table-p".to_string()),
    }
}
fn gethash(h: &mut ElispHost, a: &[Value]) -> R {
    let (test, entries) = ht_view(h, &a[1])?;
    for (k, v) in &entries {
        if hash_eq(h, test, &a[0], k) {
            return Ok(v.clone());
        }
    }
    Ok(a.get(2).cloned().unwrap_or(Value::Undef))
}
fn puthash(h: &mut ElispHost, a: &[Value]) -> R {
    let (test, entries) = ht_view(h, &a[2])?;
    let found = entries.iter().position(|(k, _)| hash_eq(h, test, &a[0], k));
    if let Value::Obj(id) = &a[2] {
        if let Some(Obj::HashTable { entries, .. }) = h.arena.get_mut(*id as usize) {
            match found {
                Some(i) => entries[i].1 = a[1].clone(),
                None => entries.push((a[0].clone(), a[1].clone())),
            }
        }
    }
    Ok(a[1].clone())
}
fn remhash(h: &mut ElispHost, a: &[Value]) -> R {
    let (test, entries) = ht_view(h, &a[1])?;
    let found = entries.iter().position(|(k, _)| hash_eq(h, test, &a[0], k));
    if let (Some(i), Value::Obj(id)) = (found, &a[1]) {
        if let Some(Obj::HashTable { entries, .. }) = h.arena.get_mut(*id as usize) {
            entries.remove(i);
        }
    }
    Ok(Value::Undef) // remhash always returns nil
}
fn clrhash(h: &mut ElispHost, a: &[Value]) -> R {
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::HashTable { entries, .. }) = h.arena.get_mut(*id as usize) {
            entries.clear();
        }
    }
    Ok(a[0].clone())
}
fn hash_table_count(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(ht_view(h, &a[0])?.1.len() as i64))
}
fn hash_table_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::HashTable { .. }))))
}
fn hash_table_keys(h: &mut ElispHost, a: &[Value]) -> R {
    let keys: Vec<Value> = ht_view(h, &a[0])?.1.into_iter().map(|(k, _)| k).collect();
    Ok(h.list_from(keys))
}
fn hash_table_values(h: &mut ElispHost, a: &[Value]) -> R {
    let vals: Vec<Value> = ht_view(h, &a[0])?.1.into_iter().map(|(_, v)| v).collect();
    Ok(h.list_from(vals))
}
fn copy_hash_table(h: &mut ElispHost, a: &[Value]) -> R {
    let (test, entries) = ht_view(h, &a[0])?;
    Ok(h.alloc(Obj::HashTable { test, entries }))
}

// ── strings ──
fn substring(_h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;
    let norm = |i: i64| -> usize { (if i < 0 { (len + i).max(0) } else { i.min(len) }) as usize };
    let start = match a.get(1) {
        Some(v) if !is_nil(v) => norm(as_int(v)?),
        _ => 0,
    };
    let end = match a.get(2) {
        Some(v) if !is_nil(v) => norm(as_int(v)?),
        _ => len as usize,
    };
    if start > end {
        return Err("args-out-of-range".to_string());
    }
    Ok(Value::str(chars[start..end].iter().collect::<String>()))
}
fn split_string(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let parts: Vec<Value> = if a.len() < 2 || is_nil(&a[1]) {
        s.split_whitespace().map(Value::str).collect()
    } else {
        let sep = as_string(&a[1])?;
        if sep.is_empty() {
            s.chars().map(|c| Value::str(c.to_string())).collect()
        } else {
            s.split(sep.as_str()).map(Value::str).collect()
        }
    };
    Ok(h.list_from(parts))
}
fn string_prefix_p(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(as_string(&a[1])?.starts_with(&as_string(&a[0])?)))
}
fn string_suffix_p(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(as_string(&a[1])?.ends_with(&as_string(&a[0])?)))
}
fn string_empty_p(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(as_string(&a[0])?.is_empty()))
}
fn string_join(h: &mut ElispHost, a: &[Value]) -> R {
    let items = h.list_vec(&a[0]).ok_or("string-join: not a list")?;
    let sep = match a.get(1) {
        Some(v) if !is_nil(v) => as_string(v)?,
        _ => String::new(),
    };
    let strs: Result<Vec<String>, String> = items.iter().map(as_string).collect();
    Ok(Value::str(strs?.join(&sep)))
}
fn char_to_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_int(&a[0])?;
    Ok(Value::str(
        char::from_u32(n as u32)
            .map(|c| c.to_string())
            .unwrap_or_default(),
    ))
}
fn string_to_char(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(
        as_string(&a[0])?
            .chars()
            .next()
            .map(|c| c as i64)
            .unwrap_or(0),
    ))
}
fn make_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_int(&a[0])?.max(0) as usize;
    let c = char::from_u32(as_int(&a[1])? as u32).unwrap_or(' ');
    Ok(Value::str(c.to_string().repeat(n)))
}
fn string_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let mut s = String::new();
    for v in a {
        if let Some(c) = char::from_u32(as_int(v)? as u32) {
            s.push(c);
        }
    }
    Ok(Value::str(s))
}
fn string_to_list(h: &mut ElispHost, a: &[Value]) -> R {
    let items: Vec<Value> = as_string(&a[0])?
        .chars()
        .map(|c| Value::Int(c as i64))
        .collect();
    Ok(h.list_from(items))
}
fn string_search(_h: &mut ElispHost, a: &[Value]) -> R {
    let needle = as_string(&a[0])?;
    let hay = as_string(&a[1])?;
    Ok(match hay.find(&needle) {
        Some(byte_idx) => Value::Int(hay[..byte_idx].chars().count() as i64),
        None => Value::Undef,
    })
}

// ── regexp ──

/// Char index ↔ byte offset on a UTF-8 string. elisp counts characters; the
/// `regex` crate reports bytes, so every boundary crosses this conversion.
fn byte_of_char(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}
fn char_of_byte(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx].chars().count()
}

/// Compile an elisp regexp to a `regex::Regex`, surfacing both translation and
/// compilation failures as elisp-style `invalid-regexp` errors.
fn compile(pat: &str) -> Result<regex::Regex, String> {
    let translated = crate::regexp::translate(pat)?;
    regex::Regex::new(&translated).map_err(|e| format!("invalid-regexp: {e}"))
}

/// Run `re` against `subject` starting at char index `start`, returning the
/// capture spans in *char* positions (group 0 = whole match).
fn run_match(
    re: &regex::Regex,
    subject: &str,
    start: usize,
) -> Option<Vec<Option<(usize, usize)>>> {
    let start_byte = byte_of_char(subject, start);
    let caps = re.captures_at(subject, start_byte)?;
    let spans = (0..caps.len())
        .map(|i| {
            caps.get(i).map(|m| {
                (
                    char_of_byte(subject, m.start()),
                    char_of_byte(subject, m.end()),
                )
            })
        })
        .collect();
    Some(spans)
}

/// `(string-match REGEXP STRING &optional START)` — search STRING for REGEXP,
/// set the match data, and return the char index where the match begins (nil if
/// no match).
fn string_match(h: &mut ElispHost, a: &[Value]) -> R {
    let pat = as_string(&a[0])?;
    let subject = as_string(&a[1])?;
    let start = match a.get(2) {
        Some(Value::Undef) | Some(Value::Bool(false)) | None => 0,
        Some(v) => as_int(v)?.max(0) as usize,
    };
    let re = compile(&pat)?;
    match run_match(&re, &subject, start) {
        Some(spans) => {
            let begin = spans[0].map(|(b, _)| b as i64).unwrap_or(0);
            h.match_data = Some(MatchData { subject, spans });
            Ok(Value::Int(begin))
        }
        None => Ok(Value::Undef),
    }
}

/// `(string-match-p REGEXP STRING &optional START)` — like `string-match` but
/// preserves the existing match data.
fn string_match_p(h: &mut ElispHost, a: &[Value]) -> R {
    let saved = h.match_data.take();
    let result = string_match(h, a);
    h.match_data = saved;
    result
}

/// `(match-beginning N)` / `(match-end N)` — the char position of the start/end
/// of the Nth subexpression of the last match, or nil.
fn match_edge(h: &mut ElispHost, a: &[Value], end: bool) -> R {
    let n = as_int(&a[0])?.max(0) as usize;
    let edge = h
        .match_data
        .as_ref()
        .and_then(|m| m.spans.get(n).copied().flatten())
        .map(|(b, e)| if end { e } else { b });
    Ok(match edge {
        Some(pos) => Value::Int(pos as i64),
        None => Value::Undef,
    })
}
fn match_beginning(h: &mut ElispHost, a: &[Value]) -> R {
    match_edge(h, a, false)
}
fn match_end(h: &mut ElispHost, a: &[Value]) -> R {
    match_edge(h, a, true)
}

/// `(match-string N &optional STRING)` — the text matched by the Nth
/// subexpression. STRING defaults to the subject of the last `string-match`.
fn match_string(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_int(&a[0])?.max(0) as usize;
    let Some(md) = h.match_data.as_ref() else {
        return Ok(Value::Undef);
    };
    let subject = match a.get(1) {
        Some(Value::Str(s)) => s.to_string(),
        _ => md.subject.clone(),
    };
    match md.spans.get(n).copied().flatten() {
        Some((b, e)) => {
            let bb = byte_of_char(&subject, b);
            let eb = byte_of_char(&subject, e);
            Ok(Value::str(subject.get(bb..eb).unwrap_or("").to_string()))
        }
        None => Ok(Value::Undef),
    }
}

/// `(match-data)` — the last match's positions as a flat list
/// `(beg0 end0 beg1 end1 …)`, with `nil nil` for groups that did not match.
/// Pairs with `set-match-data` to save/restore around inner searches.
fn match_data_fn(h: &mut ElispHost, _a: &[Value]) -> R {
    let spans = match &h.match_data {
        Some(md) => md.spans.clone(),
        None => return Ok(Value::Undef),
    };
    let mut items = Vec::with_capacity(spans.len() * 2);
    for span in spans {
        match span {
            Some((b, e)) => {
                items.push(Value::Int(b as i64));
                items.push(Value::Int(e as i64));
            }
            None => {
                items.push(Value::Undef);
                items.push(Value::Undef);
            }
        }
    }
    Ok(h.list_from(items))
}

/// `(set-match-data LIST)` — restore match positions from a `match-data` list.
/// Integer positions carry no subject, so a later `match-string` must be given
/// its STRING argument (matching Emacs's behaviour for integer match data).
fn set_match_data(h: &mut ElispHost, a: &[Value]) -> R {
    if is_nil(&a[0]) {
        h.match_data = None;
        return Ok(Value::Undef);
    }
    let flat = h.list_vec(&a[0]).ok_or("set-match-data: not a list")?;
    let subject = h
        .match_data
        .as_ref()
        .map(|m| m.subject.clone())
        .unwrap_or_default();
    let mut spans = Vec::with_capacity(flat.len() / 2);
    for pair in flat.chunks(2) {
        match (pair.first(), pair.get(1)) {
            (Some(Value::Int(b)), Some(Value::Int(e))) => {
                spans.push(Some((*b.max(&0) as usize, *e.max(&0) as usize)))
            }
            _ => spans.push(None),
        }
    }
    h.match_data = Some(MatchData { subject, spans });
    Ok(Value::Undef)
}

/// `(regexp-quote STRING)` — STRING with every regexp-special character escaped
/// so it matches literally under elisp regexp rules.
fn regexp_quote(_h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        // The set Emacs's own `regexp-quote` escapes.
        if matches!(c, '.' | '*' | '+' | '?' | '[' | ']' | '^' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    Ok(Value::str(out))
}

/// Expand a `replace-regexp-in-string` template against one match: `\&` is the
/// whole match, `\1`..`\9` are capture groups, `\\` is a literal backslash.
fn expand_replacement(rep: &str, caps: &regex::Captures, subject: &str) -> String {
    let mut out = String::with_capacity(rep.len());
    let mut it = rep.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('&') => {
                if let Some(m) = caps.get(0) {
                    out.push_str(&subject[m.start()..m.end()]);
                }
            }
            Some(d @ '0'..='9') => {
                let idx = d as usize - '0' as usize;
                if let Some(m) = caps.get(idx) {
                    out.push_str(&subject[m.start()..m.end()]);
                }
            }
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `(replace-regexp-in-string REGEXP REP STRING &optional FIXEDCASE LITERAL)` —
/// replace every match of REGEXP in STRING with REP. REP is a template (`\&`,
/// `\N` backrefs) unless LITERAL is non-nil. Function-valued REP is not yet
/// supported (string templates cover the common case without re-entering the VM).
fn replace_regexp_in_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let pat = as_string(&a[0])?;
    let rep = as_string(&a[1])?;
    let subject = as_string(&a[2])?;
    let literal = !matches!(
        a.get(4),
        Some(Value::Undef) | Some(Value::Bool(false)) | None
    );
    let re = compile(&pat)?;
    let mut out = String::with_capacity(subject.len());
    let mut last = 0usize;
    for caps in re.captures_iter(&subject) {
        let m = caps.get(0).unwrap();
        out.push_str(&subject[last..m.start()]);
        if literal {
            out.push_str(&rep);
        } else {
            out.push_str(&expand_replacement(&rep, &caps, &subject));
        }
        last = m.end();
        // Avoid an infinite loop on a zero-width match.
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

// ── numeric: float → integer rounding, and integer bit ops ──
fn floor_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let (i, f, isf) = as_num(&a[0])?;
    Ok(Value::Int(if isf { f.floor() as i64 } else { i }))
}
fn ceiling_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let (i, f, isf) = as_num(&a[0])?;
    Ok(Value::Int(if isf { f.ceil() as i64 } else { i }))
}
fn round_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    // Emacs rounds half to even (banker's rounding): (round 2.5) => 2, (round 0.5) => 0.
    let (i, f, isf) = as_num(&a[0])?;
    Ok(Value::Int(if isf { f.round_ties_even() as i64 } else { i }))
}
fn truncate_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let (i, f, isf) = as_num(&a[0])?;
    Ok(Value::Int(if isf { f.trunc() as i64 } else { i }))
}
fn float_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let (_i, f, _isf) = as_num(&a[0])?;
    Ok(Value::Float(f))
}
fn logand_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let mut r: i64 = -1;
    for v in a {
        r &= as_int(v)?;
    }
    Ok(Value::Int(r))
}
fn logior_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let mut r: i64 = 0;
    for v in a {
        r |= as_int(v)?;
    }
    Ok(Value::Int(r))
}
fn logxor_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let mut r: i64 = 0;
    for v in a {
        r ^= as_int(v)?;
    }
    Ok(Value::Int(r))
}
fn lognot_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(!as_int(&a[0])?))
}
fn ash_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_int(&a[0])?;
    let c = as_int(&a[1])?;
    Ok(Value::Int(if c >= 0 {
        n.wrapping_shl(c as u32)
    } else {
        n >> (-c) as u32
    }))
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
    s("mod", 2, Some(2), mod_fn);
    s("1+", 1, Some(1), one_plus);
    s("1-", 1, Some(1), one_minus);
    s("=", 1, None, num_eq);
    s("<", 1, None, lt);
    s(">", 1, None, gt);
    s("<=", 1, None, le);
    s(">=", 1, None, ge);
    // equality / predicates
    s("eq", 2, Some(2), eq_fn);
    s("eql", 2, Some(2), eql_fn);
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
    s("make-symbol", 1, Some(1), make_symbol_fn);
    s("set", 2, Some(2), set_fn);
    s("symbol-value", 1, Some(1), symbol_value);
    // functional (funcall/apply/mapcar/mapc are handled in host::call_function)
    s("identity", 1, Some(1), identity);
    s("terpri", 0, Some(1), terpri);
    s("print", 1, Some(2), print_fn);
    s("prin1-to-string", 1, Some(1), prin1_to_string);
    // nonlocal exits (catch/unwind-protect/condition-case are compiler intrinsics)
    s("throw", 2, Some(2), throw_fn);
    s("error", 1, None, error_fn);
    s("user-error", 1, None, error_fn);
    s("signal", 2, Some(2), signal_fn);
    // hash tables (maphash is intercepted in host::call_function)
    s("make-hash-table", 0, None, make_hash_table);
    s("gethash", 2, Some(3), gethash);
    s("puthash", 3, Some(3), puthash);
    s("remhash", 2, Some(2), remhash);
    s("clrhash", 1, Some(1), clrhash);
    s("hash-table-count", 1, Some(1), hash_table_count);
    s("hash-table-p", 1, Some(1), hash_table_p);
    s("hash-table-keys", 1, Some(1), hash_table_keys);
    s("hash-table-values", 1, Some(1), hash_table_values);
    s("copy-hash-table", 1, Some(1), copy_hash_table);
    // strings
    s("substring", 1, Some(3), substring);
    s("split-string", 1, Some(4), split_string);
    s("string-prefix-p", 2, Some(3), string_prefix_p);
    s("string-suffix-p", 2, Some(3), string_suffix_p);
    s("string-empty-p", 1, Some(1), string_empty_p);
    s("string-join", 1, Some(2), string_join);
    s("char-to-string", 1, Some(1), char_to_string);
    s("string-to-char", 1, Some(1), string_to_char);
    s("make-string", 2, Some(3), make_string);
    s("string", 0, None, string_fn);
    s("string-to-list", 1, Some(1), string_to_list);
    s("string-search", 2, Some(3), string_search);
    // regexp
    s("string-match", 2, Some(3), string_match);
    s("string-match-p", 2, Some(3), string_match_p);
    s("match-beginning", 1, Some(1), match_beginning);
    s("match-end", 1, Some(1), match_end);
    s("match-string", 1, Some(2), match_string);
    s("match-data", 0, Some(3), match_data_fn);
    s("set-match-data", 1, Some(2), set_match_data);
    s("regexp-quote", 1, Some(1), regexp_quote);
    s(
        "replace-regexp-in-string",
        3,
        Some(6),
        replace_regexp_in_string,
    );
    // strings / IO
    s("concat", 0, None, concat_fn);
    s("format", 1, None, format_fn);
    s("message", 1, None, message_fn);
    s("princ", 1, Some(2), princ_fn);
    s("prin1", 1, Some(2), prin1_fn);
    s("number-to-string", 1, Some(1), number_to_string);
    // numeric: float→int rounding + integer bit ops
    s("floor", 1, Some(2), floor_fn);
    s("ceiling", 1, Some(2), ceiling_fn);
    s("round", 1, Some(2), round_fn);
    s("truncate", 1, Some(2), truncate_fn);
    s("float", 1, Some(1), float_fn);
    s("logand", 0, None, logand_fn);
    s("logior", 0, None, logior_fn);
    s("logxor", 0, None, logxor_fn);
    s("lognot", 1, Some(1), lognot_fn);
    s("ash", 2, Some(2), ash_fn);
    s("lsh", 2, Some(2), ash_fn);
}
