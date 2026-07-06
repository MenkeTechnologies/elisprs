//! Primitive subrs, written in Rust. Per the research inventory these are the
//! ~irreducible core; the large derived surface (caar.., seq-*, cl-*, alist
//! helpers) will be defined in an elisp prelude on top of these.

use crate::host::{CharTable, ElispHost, MatchData, Obj, Resolved};
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
fn as_num(h: &ElispHost, v: &Value) -> Result<(i64, f64, bool), String> {
    match v {
        Value::Int(n) => Ok((*n, *n as f64, false)),
        Value::Float(f) => Ok((*f as i64, *f, true)),
        // A marker coerces to its (integer) buffer position in arithmetic.
        _ => match h.marker_position(v) {
            Some(p) => Ok((p as i64, p as f64, false)),
            None => Err(format!("wrong-type-argument: numberp {}", v.as_str_cow())),
        },
    }
}
fn as_int(h: &ElispHost, v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        _ => match h.marker_position(v) {
            Some(p) => Ok(p as i64),
            None => Err(format!("wrong-type-argument: integerp {}", v.as_str_cow())),
        },
    }
}
fn as_string(v: &Value) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s.to_string()),
        _ => Err(format!("wrong-type-argument: stringp {}", v.as_str_cow())),
    }
}
/// Strict integer accessor signalling `integer-or-marker-p` on non-integers —
/// Emacs's bitwise (`logand`/`logior`/`logxor`) and `%` argument checks reject
/// floats rather than truncating them.
fn as_int_om(h: &ElispHost, v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(format!(
            "wrong-type-argument: integer-or-marker-p {}",
            h.print(v, true)
        )),
    }
}
/// Strict integer accessor signalling `integerp` (for `ash`/`lsh`/`lognot`/`logcount`).
fn as_integer(h: &ElispHost, v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(format!(
            "wrong-type-argument: integerp {}",
            h.print(v, true)
        )),
    }
}
/// Character-code accessor: a valid character is an integer in [0, #x3FFFFF];
/// anything else signals `wrong-type-argument characterp VALUE` (a negative code
/// or one past the upper bound, as `char-to-string`/`make-string` do).
fn as_char(h: &ElispHost, v: &Value) -> Result<u32, String> {
    match v {
        Value::Int(n) if (0..=0x3F_FFFF).contains(n) => Ok(*n as u32),
        _ => Err(format!(
            "wrong-type-argument: characterp {}",
            h.print(v, true)
        )),
    }
}
fn num_result(i: i64, f: f64, isf: bool) -> Value {
    if isf {
        Value::Float(f)
    } else {
        Value::Int(i)
    }
}

fn add(h: &mut ElispHost, a: &[Value]) -> R {
    let (mut i, mut f, mut isf) = (0i64, 0f64, false);
    for v in a {
        let (vi, vf, vfl) = as_num(h, v)?;
        isf |= vfl;
        i += vi;
        f += vf;
    }
    Ok(num_result(i, f, isf))
}
fn mul(h: &mut ElispHost, a: &[Value]) -> R {
    let (mut i, mut f, mut isf) = (1i64, 1f64, false);
    for v in a {
        let (vi, vf, vfl) = as_num(h, v)?;
        isf |= vfl;
        i *= vi;
        f *= vf;
    }
    Ok(num_result(i, f, isf))
}
fn sub(h: &mut ElispHost, a: &[Value]) -> R {
    if a.is_empty() {
        return Ok(Value::Int(0));
    }
    let (i0, f0, mut isf) = as_num(h, &a[0])?;
    if a.len() == 1 {
        return Ok(if isf {
            Value::Float(-f0)
        } else {
            Value::Int(-i0)
        });
    }
    let (mut i, mut f) = (i0, f0);
    for v in &a[1..] {
        let (vi, vf, vfl) = as_num(h, v)?;
        isf |= vfl;
        i -= vi;
        f -= vf;
    }
    Ok(num_result(i, f, isf))
}
fn div(h: &mut ElispHost, a: &[Value]) -> R {
    let (i0, f0, mut isf) = as_num(h, &a[0])?;
    let (mut i, mut f) = (i0, f0);
    for v in &a[1..] {
        let (vi, vf, vfl) = as_num(h, v)?;
        isf |= vfl;
        if !isf && vi == 0 {
            return Err("arith-error: division by zero".to_string());
        }
        if vi != 0 {
            i /= vi;
        }
        f /= vf;
    }
    // A float division that yields NaN (e.g. (/ 0.0 0.0)) gets a hardware-dependent
    // sign bit: x86-64 produces a sign-negative NaN, ARM a positive one. Emacs prints
    // such a result as "0.0e+NaN" (positive), so canonicalize the sign here to keep
    // output platform-independent. A later negation still flips it to "-0.0e+NaN".
    if isf && f.is_nan() {
        f = f.abs();
    }
    Ok(num_result(i, f, isf))
}
fn modulo(h: &mut ElispHost, a: &[Value]) -> R {
    let x = as_int_om(h, &a[0])?;
    let y = as_int_om(h, &a[1])?;
    if y == 0 {
        return Err("arith-error: division by zero".to_string());
    }
    Ok(Value::Int(x % y))
}
// `mod` (vs `%`): the result takes the sign of the divisor, and either operand
// may be a float — (mod 13.5 4) => 1.5, (mod -1 3) => 2.
fn mod_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (xi, xf, xisf) = as_num(h, &a[0])?;
    let (yi, yf, yisf) = as_num(h, &a[1])?;
    if xisf || yisf {
        // Faithful port of Emacs `Fmod` (data.c): fmod, then fix the sign so the
        // result matches the divisor. `%` on f64 is fmod (the remainder keeps the
        // dividend's sign, so `(mod -0.0 5)` stays `-0.0`, as Emacs returns). A
        // zero float divisor yields NaN — Emacs does NOT signal arith-error for
        // float mod-by-zero (only integer mod-by-zero, handled below).
        let mut r = xf % yf;
        if if yf < 0.0 { r > 0.0 } else { r < 0.0 } {
            r += yf;
        }
        // Canonicalize the hardware-dependent NaN sign to positive, matching
        // Emacs's "0.0e+NaN" printer (mirrors the `div` handling above).
        if r.is_nan() {
            r = r.abs();
        }
        return Ok(Value::Float(r));
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
fn one_plus(h: &mut ElispHost, a: &[Value]) -> R {
    let (i, f, isf) = as_num(h, &a[0])?;
    Ok(if isf {
        Value::Float(f + 1.0)
    } else {
        Value::Int(i + 1)
    })
}
fn one_minus(h: &mut ElispHost, a: &[Value]) -> R {
    let (i, f, isf) = as_num(h, &a[0])?;
    Ok(if isf {
        Value::Float(f - 1.0)
    } else {
        Value::Int(i - 1)
    })
}

fn cmp(h: &ElispHost, a: &[Value], pred: fn(f64, f64) -> bool) -> R {
    for w in a.windows(2) {
        if !pred(as_num(h, &w[0])?.1, as_num(h, &w[1])?.1) {
            return Ok(Value::Undef);
        }
    }
    Ok(Value::Bool(true))
}
fn num_eq(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |x, y| x == y)
}
fn lt(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |x, y| x < y)
}
fn gt(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |x, y| x > y)
}
fn le(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |x, y| x <= y)
}
fn ge(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |x, y| x >= y)
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
            // Two markers are `equal` when they share a buffer and position.
            (Some(Obj::Marker(_)), Some(Obj::Marker(_))) => h.markers_equal(a, b),
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
    if a.is_empty() {
        return Ok(Value::Undef);
    }
    // The final argument becomes the tail as-is (shared, any type) — so a
    // non-list last arg yields a dotted result: (append '(1 2) 3) => (1 2 . 3).
    // Every preceding argument must be a sequence and is flattened.
    let mut out = Vec::new();
    for v in &a[..a.len() - 1] {
        if is_nil(v) {
            continue;
        }
        match h.obj(v) {
            Some(Obj::Vector(items)) => out.extend(items.clone()),
            _ => match v {
                Value::Str(s) => out.extend(s.chars().map(|c| Value::Int(c as i64))),
                _ => match h.list_vec(v) {
                    Some(items) => out.extend(items),
                    None => {
                        return Err(format!(
                            "wrong-type-argument: sequencep {}",
                            h.print(v, true)
                        ))
                    }
                },
            },
        }
    }
    let mut tail = a[a.len() - 1].clone();
    for item in out.into_iter().rev() {
        tail = h.cons(item, tail);
    }
    Ok(tail)
}
fn reverse_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // `reverse` works on any sequence: list, string, or vector.
    if let Value::Str(s) = &a[0] {
        return Ok(Value::str(s.chars().rev().collect::<String>()));
    }
    let vec_items = match h.obj(&a[0]) {
        Some(Obj::Vector(items)) => Some(items.clone()),
        _ => None,
    };
    if let Some(mut items) = vec_items {
        items.reverse();
        return Ok(h.alloc(Obj::Vector(items)));
    }
    let mut v = h
        .list_vec(&a[0])
        .ok_or_else(|| format!("wrong-type-argument: sequencep {}", h.print(&a[0], true)))?;
    v.reverse();
    Ok(h.list_from(v))
}
/// `(downcase OBJ)` / `(upcase OBJ)` — case-fold a string, or a single character
/// (an integer), returning the same kind. Unicode-aware via Rust's case mapping.
/// Single-character (simple) case mapping, matching Emacs's `upcase`/`downcase`
/// on a *character*. Rust's `char::to_uppercase`/`to_lowercase` are the Unicode
/// *full* mappings, which can expand to several chars (ß→"SS", ﬁ→"FI"); Emacs's
/// char case folds to exactly one char. For the multi-expansion chars Emacs
/// returns the char unchanged, except this enumerated set where the simple /
/// titlecase mapping is a distinct single char (German sharp s and the Greek
/// iota-subscript forms). Verified against emacs 30.2 `upcase` over 0..#x110000.
fn simple_case(cp: i64, upper: bool) -> i64 {
    if upper {
        match cp {
            223 => return 7838,                                       // ß → ẞ
            8064..=8071 | 8080..=8087 | 8096..=8103 => return cp + 8, // ᾀ.. → ᾈ..
            8115 | 8131 | 8179 => return cp + 9,                      // ῃ ῳ ᾳ → titlecase
            _ => {}
        }
    }
    let Some(ch) = u32::try_from(cp).ok().and_then(char::from_u32) else {
        return cp;
    };
    let mut mapped: [char; 3] = ['\0'; 3];
    let mut n = 0;
    if upper {
        for m in ch.to_uppercase() {
            if n < 3 {
                mapped[n] = m;
            }
            n += 1;
        }
    } else {
        for m in ch.to_lowercase() {
            if n < 3 {
                mapped[n] = m;
            }
            n += 1;
        }
    }
    // A multi-char full mapping has no single-char simple mapping → unchanged.
    if n == 1 {
        mapped[0] as i64
    } else {
        cp
    }
}
fn case_fold(a: &[Value], upper: bool) -> R {
    match &a[0] {
        Value::Int(c) => Ok(Value::Int(simple_case(*c, upper))),
        Value::Str(s) => Ok(Value::str(if upper {
            s.to_uppercase()
        } else {
            s.to_lowercase()
        })),
        _ => Err("wrong-type-argument: char-or-string-p".to_string()),
    }
}
fn downcase_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    case_fold(a, false)
}
fn upcase_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    case_fold(a, true)
}
fn length_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match &a[0] {
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::Undef => Ok(Value::Int(0)),
        Value::Obj(_) => match h.obj(&a[0]) {
            Some(Obj::Vector(items)) => Ok(Value::Int(items.len() as i64)),
            Some(Obj::Cons(..)) => {
                // Walk the cons spine, Floyd-detecting cycles. A non-nil, non-cons
                // tail is an improper list -> `wrong-type-argument listp TAIL`
                // (e.g. `(length '(1 2 . 3))` signals with 3); a circular list
                // signals `circular-list`.
                let mut fast = a[0].clone();
                let mut slow = a[0].clone();
                let mut n: i64 = 0;
                loop {
                    for _ in 0..2 {
                        match h.obj(&fast) {
                            Some(Obj::Cons(_, d)) => {
                                fast = d.clone();
                                n += 1;
                            }
                            _ if is_nil(&fast) => return Ok(Value::Int(n)),
                            _ => {
                                return Err(format!(
                                    "wrong-type-argument: listp {}",
                                    h.print(&fast, true)
                                ))
                            }
                        }
                    }
                    if let Some(Obj::Cons(_, d)) = h.obj(&slow) {
                        slow = d.clone();
                    }
                    if h.values_eq(&slow, &fast) {
                        // Faithful error symbol; the exact DATA payload (the list
                        // itself) needs cycle-aware printing (host.rs), so we omit
                        // it here rather than infinite-loop trying to render it.
                        return Err("circular-list: circular list".to_string());
                    }
                }
            }
            _ => Ok(Value::Int(0)),
        },
        _ => Err(format!(
            "wrong-type-argument: sequencep {}",
            h.print(&a[0], true)
        )),
    }
}
fn nth_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // `(nth n list)` = `(car (nthcdr n list))`: walk the cons spine n times, then
    // take the car. Improper lists are fine (`(nth 0 '(a . 1))` => a); only when
    // we actually need the car/cdr of a non-list does Emacs signal listp. N must
    // be an integer — a float or other type signals `integerp`.
    let n = as_integer(h, &a[0])?;
    let mut cur = a[1].clone();
    let mut i = 0;
    while i < n {
        let next = match h.obj(&cur) {
            Some(Obj::Cons(_, d)) => d.clone(),
            _ if is_nil(&cur) => return Ok(Value::Undef),
            _ => {
                return Err(format!(
                    "wrong-type-argument: listp {}",
                    h.print(&cur, true)
                ))
            }
        };
        cur = next;
        i += 1;
    }
    match h.obj(&cur) {
        Some(Obj::Cons(car, _)) => Ok(car.clone()),
        _ if is_nil(&cur) => Ok(Value::Undef),
        _ => Err(format!(
            "wrong-type-argument: listp {}",
            h.print(&cur, true)
        )),
    }
}

// ── c[ad]+r combinators ──
// Each composes `car`/`cdr`, inheriting their exact edge semantics: car/cdr of
// nil yield nil (so short lists return nil), while car/cdr of a non-nil non-cons
// signals `wrong-type-argument listp`. Read the letters right-to-left as the
// order of operations (e.g. `caadr` = (car (car (cdr X)))).
fn caadr(h: &mut ElispHost, a: &[Value]) -> R {
    let v = cdr(h, a)?;
    let v = car(h, &[v])?;
    car(h, &[v])
}
fn cadar(h: &mut ElispHost, a: &[Value]) -> R {
    let v = car(h, a)?;
    let v = cdr(h, &[v])?;
    car(h, &[v])
}
fn cdaar(h: &mut ElispHost, a: &[Value]) -> R {
    let v = car(h, a)?;
    let v = car(h, &[v])?;
    cdr(h, &[v])
}
fn cdadr(h: &mut ElispHost, a: &[Value]) -> R {
    let v = cdr(h, a)?;
    let v = car(h, &[v])?;
    cdr(h, &[v])
}
fn cddar(h: &mut ElispHost, a: &[Value]) -> R {
    let v = car(h, a)?;
    let v = cdr(h, &[v])?;
    cdr(h, &[v])
}
// cl-lib 2-level aliases (cl-caar/cl-cadr/cl-cdar/cl-cddr), identical to the
// non-prefixed forms.
fn cl_caar(h: &mut ElispHost, a: &[Value]) -> R {
    let v = car(h, a)?;
    car(h, &[v])
}
fn cl_cadr(h: &mut ElispHost, a: &[Value]) -> R {
    let v = cdr(h, a)?;
    car(h, &[v])
}
fn cl_cdar(h: &mut ElispHost, a: &[Value]) -> R {
    let v = car(h, a)?;
    cdr(h, &[v])
}
fn cl_cddr(h: &mut ElispHost, a: &[Value]) -> R {
    let v = cdr(h, a)?;
    cdr(h, &[v])
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
    let n = as_num(h, &a[0])?.0;
    if n < 0 {
        return Err(format!("wrong-type-argument: wholenump {n}"));
    }
    Ok(h.alloc(Obj::Vector(vec![a[1].clone(); n as usize])))
}
fn aref(h: &mut ElispHost, a: &[Value]) -> R {
    // A char-table indexes by character (0..=MAX_CHAR) with parent/default
    // fallback, unlike a plain vector's positional index.
    if matches!(h.obj(&a[0]), Some(Obj::CharTable(_))) {
        let c = as_char(h, &a[1])?;
        return Ok(h.char_table_ref(&a[0], c));
    }
    let idx = as_num(h, &a[1])?.0;
    let oor = |h: &ElispHost| format!("args-out-of-range: {} {idx}", h.print(&a[0], true));
    if idx < 0 {
        return Err(oor(h));
    }
    let i = idx as usize;
    match h.obj(&a[0]) {
        Some(Obj::Vector(items)) => items.get(i).cloned().ok_or_else(|| oor(h)),
        _ => match &a[0] {
            Value::Str(s) => s
                .chars()
                .nth(i)
                .map(|c| Value::Int(c as i64))
                .ok_or_else(|| oor(h)),
            _ => Err(format!(
                "wrong-type-argument: arrayp {}",
                h.print(&a[0], true)
            )),
        },
    }
}
fn aset(h: &mut ElispHost, a: &[Value]) -> R {
    // A char-table indexes by character; `aset` sets that single char.
    if matches!(h.obj(&a[0]), Some(Obj::CharTable(_))) {
        let c = as_char(h, &a[1])?;
        if let Value::Obj(id) = &a[0] {
            if let Some(Obj::CharTable(t)) = h.arena.get_mut(*id as usize) {
                t.set_range(c, c, a[2].clone());
            }
        }
        return Ok(a[2].clone());
    }
    let idx = as_num(h, &a[1])?.0;
    if idx < 0 {
        return Err(format!("args-out-of-range: {} {idx}", h.print(&a[0], true)));
    }
    let i = idx as usize;
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::Vector(items)) = h.arena.get_mut(*id as usize) {
            if i < items.len() {
                items[i] = a[2].clone();
                return Ok(a[2].clone());
            }
            return Err(format!("args-out-of-range: {} {idx}", h.print(&a[0], true)));
        }
    }
    Err("wrong-type-argument: arrayp".to_string())
}
/// `(fillarray ARRAY ITEM)` — set every element of vector ARRAY to ITEM, in
/// place. (Strings are immutable here, so only vectors are supported.)
fn fillarray(h: &mut ElispHost, a: &[Value]) -> R {
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::Vector(items)) = h.arena.get_mut(*id as usize) {
            for x in items.iter_mut() {
                *x = a[1].clone();
            }
            return Ok(a[0].clone());
        }
    }
    Err(format!(
        "wrong-type-argument: arrayp {}",
        h.print(&a[0], true)
    ))
}

// ── char-tables ──
/// `(make-char-table--new SUBTYPE INIT N-EXTRA)` — low-level allocator. The
/// public `make-char-table` (prelude) reads N-EXTRA from SUBTYPE's
/// `char-table-extra-slots' property before calling this. INIT fills every char
/// slot; the `default` slot starts nil (Emacs `Fmake_char_table`).
fn make_char_table_new(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_num(h, &a[2])?.0;
    let n = if n < 0 { 0 } else { n as usize };
    Ok(h.alloc(Obj::CharTable(CharTable::new(
        a[0].clone(),
        a[1].clone(),
        n,
    ))))
}
fn char_table_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::CharTable(_)))))
}
fn char_table_subtype(h: &mut ElispHost, a: &[Value]) -> R {
    match h.obj(&a[0]) {
        Some(Obj::CharTable(t)) => Ok(t.subtype.clone()),
        _ => Err(wrong_char_table(h, &a[0])),
    }
}
fn char_table_parent(h: &mut ElispHost, a: &[Value]) -> R {
    match h.obj(&a[0]) {
        Some(Obj::CharTable(t)) => Ok(t.parent.clone()),
        _ => Err(wrong_char_table(h, &a[0])),
    }
}
fn set_char_table_parent(h: &mut ElispHost, a: &[Value]) -> R {
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::CharTable(t)) = h.arena.get_mut(*id as usize) {
            t.parent = a[1].clone();
            return Ok(a[1].clone());
        }
    }
    Err(wrong_char_table(h, &a[0]))
}
fn char_table_extra_slot(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_num(h, &a[1])?.0;
    match h.obj(&a[0]) {
        Some(Obj::CharTable(t)) => t
            .extra
            .get(n as usize)
            .filter(|_| n >= 0)
            .cloned()
            .ok_or_else(|| format!("args-out-of-range: {} {n}", h.print(&a[0], true))),
        _ => Err(wrong_char_table(h, &a[0])),
    }
}
fn set_char_table_extra_slot(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_num(h, &a[1])?.0;
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::CharTable(t)) = h.arena.get_mut(*id as usize) {
            if n >= 0 && (n as usize) < t.extra.len() {
                t.extra[n as usize] = a[2].clone();
                return Ok(a[2].clone());
            }
            return Err(format!("args-out-of-range: {} {n}", h.print(&a[0], true)));
        }
    }
    Err(wrong_char_table(h, &a[0]))
}
/// `(char-table-range CHAR-TABLE RANGE)` — RANGE is nil (the default slot), a
/// character, or a cons `(FROM . TO)` (value at FROM). `t` is invalid.
fn char_table_range(h: &mut ElispHost, a: &[Value]) -> R {
    if !matches!(h.obj(&a[0]), Some(Obj::CharTable(_))) {
        return Err(wrong_char_table(h, &a[0]));
    }
    match &a[1] {
        Value::Undef | Value::Bool(false) => match h.obj(&a[0]) {
            Some(Obj::CharTable(t)) => Ok(t.default.clone()),
            _ => unreachable!(),
        },
        Value::Bool(true) => Err("Invalid RANGE argument to 'char-table-range'".to_string()),
        Value::Int(_) => {
            let c = as_char(h, &a[1])?;
            Ok(h.char_table_ref(&a[0], c))
        }
        _ => {
            // A cons (FROM . TO): value at FROM (with fallback), like Emacs.
            if let Some(Obj::Cons(from, _)) = h.obj(&a[1]) {
                let from = from.clone();
                let c = as_char(h, &from)?;
                Ok(h.char_table_ref(&a[0], c))
            } else {
                Err("Invalid RANGE argument to 'char-table-range'".to_string())
            }
        }
    }
}
/// `(set-char-table-range CHAR-TABLE RANGE VALUE)` — RANGE is nil (set default),
/// `t` (set every char), a character, or a cons `(FROM . TO)`.
fn set_char_table_range(h: &mut ElispHost, a: &[Value]) -> R {
    let id = match &a[0] {
        Value::Obj(id) if matches!(h.obj(&a[0]), Some(Obj::CharTable(_))) => *id,
        _ => return Err(wrong_char_table(h, &a[0])),
    };
    let val = a[2].clone();
    match &a[1] {
        Value::Undef | Value::Bool(false) => {
            if let Some(Obj::CharTable(t)) = h.arena.get_mut(id as usize) {
                t.default = val;
            }
        }
        Value::Bool(true) => {
            if let Some(Obj::CharTable(t)) = h.arena.get_mut(id as usize) {
                t.set_range(0, MAX_CHAR as u32, val);
            }
        }
        Value::Int(_) => {
            let c = as_char(h, &a[1])?;
            if let Some(Obj::CharTable(t)) = h.arena.get_mut(id as usize) {
                t.set_range(c, c, val);
            }
        }
        _ => {
            let (from, to) = match h.obj(&a[1]) {
                Some(Obj::Cons(f, t)) => (f.clone(), t.clone()),
                _ => return Err("Invalid RANGE argument to 'set-char-table-range'".to_string()),
            };
            let from = as_char(h, &from)?;
            let to = as_char(h, &to)?;
            if let Some(Obj::CharTable(t)) = h.arena.get_mut(id as usize) {
                if from <= to {
                    t.set_range(from, to, val);
                }
            }
        }
    }
    Ok(a[2].clone())
}
fn wrong_char_table(h: &ElispHost, v: &Value) -> String {
    format!("wrong-type-argument: char-table-p {}", h.print(v, true))
}

// ── symbols / cells ──
fn symbol_name(h: &mut ElispHost, a: &[Value]) -> R {
    match h.sym_name(&a[0]) {
        Some(s) => Ok(Value::str(s)),
        None => Err(format!(
            "wrong-type-argument: symbolp {}",
            h.print(&a[0], true)
        )),
    }
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
/// `(makunbound SYMBOL)` — clear SYMBOL's value cell, returning SYMBOL.
fn makunbound(h: &mut ElispHost, a: &[Value]) -> R {
    h.unset_value(&a[0])?;
    Ok(a[0].clone())
}
/// `(fset SYMBOL DEFINITION)` — set SYMBOL's function cell, returning DEFINITION.
fn fset(h: &mut ElispHost, a: &[Value]) -> R {
    h.set_function_value(&a[0], a[1].clone())?;
    Ok(a[1].clone())
}
/// `(fboundp SYMBOL)` — non-nil if SYMBOL has a function definition.
fn fboundp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.resolve_function(&a[0]).is_ok()))
}
/// `(indirect-function OBJECT)` — follow symbol→function-cell aliases to the final
/// function object (a subr/closure), or nil if undefined.
fn indirect_function(h: &mut ElispHost, a: &[Value]) -> R {
    let mut cur = a[0].clone();
    for _ in 0..64 {
        match h.obj(&cur) {
            Some(Obj::Symbol(s)) => match &s.function {
                Some(def) => cur = def.clone(),
                None => return Ok(Value::Undef),
            },
            _ => return Ok(cur),
        }
    }
    Ok(cur)
}
/// `(boundp SYMBOL)` — non-nil if SYMBOL currently has a value.
fn boundp(h: &mut ElispHost, a: &[Value]) -> R {
    // nil and t are always bound; otherwise the value cell must resolve.
    let bound = is_nil(&a[0]) || matches!(a[0], Value::Bool(true)) || h.get_value(&a[0]).is_ok();
    Ok(nil_or(bound))
}

// ── functional ──
// `funcall`/`apply`/`mapcar`/`mapc` are intercepted in `host::call_function`
// (they re-enter elisp, so they can't run inside a host borrow) — they are not
// plain subrs here.
fn identity(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(a[0].clone())
}
fn terpri(h: &mut ElispHost, _a: &[Value]) -> R {
    h.emit("\n");
    Ok(Value::Bool(true))
}
fn print_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let s = h.print(&a[0], true);
    h.emit(&s);
    h.emit("\n");
    Ok(a[0].clone())
}
fn push_output_capture(h: &mut ElispHost, _a: &[Value]) -> R {
    h.output_capture.push(String::new());
    Ok(Value::Undef)
}
fn pop_output_capture(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::str(h.output_capture.pop().unwrap_or_default()))
}
fn prin1_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::str(h.print(&a[0], true)))
}

// ── nonlocal exits ──
// `throw` records the (tag, value) and aborts via the error channel; `catch`
// (an intrinsic in host::call_function) intercepts it.
fn throw_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let tag = a[0].clone();
    let val = a.get(1).cloned().unwrap_or(Value::Undef);
    let tags = h.catch_tags.clone();
    if tags.iter().any(|t| h.values_eq(t, &tag)) {
        h.pending_throw = Some((tag, val));
        Err("--throw--".to_string())
    } else {
        // No matching catch on the stack: signal (no-catch TAG VALUE).
        let sym = h.intern("no-catch");
        let data = h.list_from(vec![tag, val]);
        let display = h.print(&data, true);
        h.pending_error = Some(h.cons(sym, data));
        Err(format!("no-catch: {display}"))
    }
}
fn error_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // Error object: (error "MESSAGE"). Keep it for condition-case.
    let msg = el_format(h, a)?;
    let esym = h.intern("error");
    let mstr = Value::str(msg.clone());
    let data = h.list_from(vec![mstr]);
    h.pending_error = Some(h.cons(esym, data));
    Err(format!("error: {msg}"))
}
fn user_error_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // Like `error`, but signals the `user-error` condition.
    let msg = el_format(h, a)?;
    let esym = h.intern("user-error");
    let mstr = Value::str(msg.clone());
    let data = h.list_from(vec![mstr]);
    h.pending_error = Some(h.cons(esym, data));
    Err(format!("user-error: {msg}"))
}
fn signal_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // Error object: (ERROR-SYMBOL . DATA) — preserve the actual data list.
    let sym = h.sym_name(&a[0]).unwrap_or_else(|| "error".to_string());
    let display = h.print(a.get(1).unwrap_or(&Value::Undef), true);
    let symv = h.intern(&sym);
    let data = a.get(1).cloned().unwrap_or(Value::Undef);
    h.pending_error = Some(h.cons(symv, data));
    Err(format!("{sym}: {display}"))
}

// ── strings / format / IO ──
fn concat_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut out = String::new();
    for v in a {
        match v {
            Value::Str(s) => out.push_str(s),
            Value::Undef => {}
            _ => {
                // concat over a list OR vector of character codes.
                let items = h.list_vec(v).or_else(|| match h.obj(v) {
                    Some(Obj::Vector(items)) => Some(items.clone()),
                    _ => None,
                });
                if let Some(items) = items {
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
    plus: bool,
    space: bool,
    alt: bool,
    width: usize,
    prec: Option<usize>,
    conv: char,
}

/// Format an integer in `radix` (8/16) the way Emacs's `%o`/`%x`/`%X` do: a
/// leading `-` and the magnitude's digits (not two's complement), with an
/// optional `0`/`0x`/`0X` prefix when the `#` flag is set.
/// Zero-pad a magnitude digit string to at least `prec` digits (integer-precision
/// semantics). `Some(0)` with value "0" yields an empty string, like C/Emacs.
fn pad_digits(mag: &str, prec: Option<usize>) -> String {
    match prec {
        None => mag.to_string(),
        Some(0) if mag == "0" => String::new(),
        Some(p) if mag.len() < p => format!("{}{mag}", "0".repeat(p - mag.len())),
        Some(_) => mag.to_string(),
    }
}
fn format_radix(n: i64, radix: u32, upper: bool, alt: bool, prec: Option<usize>) -> String {
    let (sign, mag) = if n < 0 {
        ("-", n.unsigned_abs())
    } else {
        ("", n as u64)
    };
    let body = match (radix, upper) {
        (16, true) => format!("{mag:X}"),
        (16, false) => format!("{mag:x}"),
        _ => format!("{mag:o}"),
    };
    let body = pad_digits(&body, prec);
    let prefix = if alt && mag != 0 {
        match (radix, upper) {
            (16, true) => "0X",
            (16, false) => "0x",
            _ => "0",
        }
    } else {
        ""
    };
    format!("{sign}{prefix}{body}")
}

/// C-style `%e`: a `prec`-digit mantissa, then `e`, a sign, and a ≥2-digit
/// exponent (`1000.0` => `1.000000e+03`). Rust's `{:e}` omits the padding/sign.
/// C-printf `%g`: pick `%e` or `%f` by the decimal exponent, with PREC
/// significant digits; trailing zeros are trimmed unless `alt` (the `#` flag).
fn format_g(v: f64, prec: usize, alt: bool) -> String {
    let p = prec.max(1);
    let strip = |mant: &str| -> String {
        if mant.contains('.') {
            mant.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            mant.to_string()
        }
    };
    // Decimal exponent X from an %e rendering (0 for a zero value).
    let x: i32 = if v == 0.0 {
        0
    } else {
        let es = format!("{:.*e}", p - 1, v);
        es[es.find('e').unwrap() + 1..].parse().unwrap_or(0)
    };
    if x >= -4 && x < p as i32 {
        let prec_f = (p as i32 - 1 - x).max(0) as usize;
        let s = format!("{:.*}", prec_f, v);
        if alt {
            s
        } else {
            strip(&s)
        }
    } else {
        let body = format_e(v, p - 1);
        if alt {
            body
        } else {
            match body.find('e') {
                Some(ep) => format!("{}{}", strip(&body[..ep]), &body[ep..]),
                None => strip(&body),
            }
        }
    }
}
fn format_e(v: f64, prec: usize) -> String {
    let s = format!("{:.*e}", prec, v);
    match s.find('e') {
        Some(epos) => {
            let (mant, rest) = s.split_at(epos);
            let exp = &rest[1..];
            let (sign, digits) = match exp.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', exp.strip_prefix('+').unwrap_or(exp)),
            };
            format!("{mant}e{sign}{digits:0>2}")
        }
        None => s,
    }
}

/// Prefix an explicit sign on a non-negative numeric body per the `+`/space
/// flags (`+` wins over space). A leading `-` already carries the sign.
fn apply_sign(body: String, spec: &FmtSpec) -> String {
    if body.starts_with('-') {
        body
    } else if spec.plus {
        format!("+{body}")
    } else if spec.space {
        format!(" {body}")
    } else {
        body
    }
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
        // Keep any leading sign (-, +, space) and `0x`/`0X` radix prefix ahead of
        // the zero fill: `%#010x` of 255 => `0x000000ff`.
        let mut p = 0;
        if matches!(body.chars().next(), Some('-' | '+' | ' ')) {
            p = 1;
        }
        if body[p..].starts_with("0x") || body[p..].starts_with("0X") {
            p += 2;
        }
        format!("{}{}{}", &body[..p], "0".repeat(fill), &body[p..])
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
        // Optional argument field `N$` (N starts 1-9, so it can't be confused
        // with a `0` flag) — selects the Nth argument: `%2$s` uses arg 2.
        let mut field: Option<usize> = None;
        let mut width = 0usize;
        let mut width_done = false;
        if matches!(chars.peek(), Some('1'..='9')) {
            let mut num = 0usize;
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    num = num * 10 + (d as usize - '0' as usize);
                    chars.next();
                } else {
                    break;
                }
            }
            if chars.peek() == Some(&'$') {
                chars.next();
                field = Some(num);
            } else {
                // Not a field — those digits were the width.
                width = num;
                width_done = true;
            }
        }
        // flags
        let (mut left, mut zero, mut plus, mut space, mut alt) =
            (false, false, false, false, false);
        if !width_done {
            while let Some(&f) = chars.peek() {
                match f {
                    '-' => left = true,
                    '0' => zero = true,
                    '+' => plus = true,
                    ' ' => space = true,
                    '#' => alt = true,
                    _ => break,
                }
                chars.next();
            }
        }
        // width
        if !width_done {
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    width = width * 10 + (d as usize - '0' as usize);
                    chars.next();
                } else {
                    break;
                }
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
        let mut spec = FmtSpec {
            left,
            zero,
            plus,
            space,
            alt,
            width,
            prec,
            conv,
        };
        // Emacs error messages for the argument step. The curly apostrophe in
        // BAD_TYPE matches `emacs -Q` (default text-quoting-style).
        const NOT_ENOUGH: &str = "Not enough arguments for format string";
        const BAD_TYPE: &str = "Format specifier doesn\u{2019}t match argument type";
        // A field number selects an explicit (1-based) argument; otherwise take
        // the next one in sequence.
        let idx = field.unwrap_or(ai);
        // The numeric/char conversions need a number-valued argument; a missing
        // one is "Not enough arguments", a non-number is a type mismatch.
        let numf = |idx: usize| -> Result<(i64, f64, bool), String> {
            let arg = a.get(idx).ok_or_else(|| NOT_ENOUGH.to_string())?;
            as_num(h, arg).map_err(|_| BAD_TYPE.to_string())
        };
        let body = match conv {
            's' => {
                let arg = a.get(idx).ok_or_else(|| NOT_ENOUGH.to_string())?;
                let s = h.print(arg, false);
                match spec.prec {
                    Some(p) => s.chars().take(p).collect(),
                    None => s,
                }
            }
            'S' => {
                let arg = a.get(idx).ok_or_else(|| NOT_ENOUGH.to_string())?;
                h.print(arg, true)
            }
            // The `+`/space sign flags apply to the signed conversions (d/e/f/g).
            // `%i` is an accepted alias for `%d` (as in C printf).
            'd' | 'i' => {
                let n = numf(idx)?.0;
                let mag = pad_digits(&n.unsigned_abs().to_string(), spec.prec);
                apply_sign(if n < 0 { format!("-{mag}") } else { mag }, &spec)
            }
            'o' => format_radix(numf(idx)?.0, 8, false, spec.alt, spec.prec),
            'x' => format_radix(numf(idx)?.0, 16, false, spec.alt, spec.prec),
            'X' => format_radix(numf(idx)?.0, 16, true, spec.alt, spec.prec),
            'c' => char::from_u32(numf(idx)?.0 as u32)
                .map(String::from)
                .unwrap_or_default(),
            'e' | 'f' | 'g' => {
                let v = numf(idx)?.1;
                if v.is_finite() {
                    let raw = match conv {
                        'e' => format_e(v, spec.prec.unwrap_or(6)),
                        'f' => format!("{:.*}", spec.prec.unwrap_or(6), v),
                        _ => format_g(v, spec.prec.unwrap_or(6), spec.alt),
                    };
                    apply_sign(raw, &spec)
                } else {
                    // inf/nan: Emacs renders "inf"/"-inf"/"nan", ignoring precision
                    // and the `0` flag (space-padded to width). `+`/space signs
                    // apply to infinities but never to NaN.
                    spec.zero = false;
                    if v.is_nan() {
                        "nan".to_string()
                    } else if v < 0.0 {
                        "-inf".to_string()
                    } else {
                        apply_sign("inf".to_string(), &spec)
                    }
                }
            }
            other => {
                // Unknown conversion. Emacs still validates argument
                // availability first — `(format "%b")` is "Not enough
                // arguments", but `(format "%b" 1)` is "Invalid format
                // operation %b" — so check the arg before signalling.
                if a.get(idx).is_none() {
                    return Err(NOT_ENOUGH.to_string());
                }
                return Err(format!("Invalid format operation %{other}"));
            }
        };
        if field.is_none() {
            ai += 1;
        }
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
    let s = h.print(&a[0], false);
    h.emit(&s);
    Ok(a[0].clone())
}
fn prin1_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let s = h.print(&a[0], true);
    h.emit(&s);
    Ok(a[0].clone())
}
fn number_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::str(h.print(&a[0], false)))
}

// ── hash tables ──
fn hash_eq(h: &ElispHost, test: u8, a: &Value, b: &Value) -> bool {
    match test {
        // `equal` test: deep structural equality.
        2 => el_equal(h, a, b),
        // `eql` test (the make-hash-table default): like `eq` but equal floats
        // (and, in real Emacs, equal bignums) compare the same, so a float key
        // put with `puthash` is found again by `gethash`.
        1 => el_eql(h, a, b),
        // `eq` test: identity.
        _ => el_eq(h, a, b),
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
/// `(hash-table-test TABLE)` — the symbol naming TABLE's comparison test.
fn hash_table_test(h: &mut ElispHost, a: &[Value]) -> R {
    let test = ht_view(h, &a[0])?.0;
    let name = match test {
        0 => "eq",
        1 => "eql",
        _ => "equal",
    };
    Ok(h.intern(name))
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
fn substring(h: &mut ElispHost, a: &[Value]) -> R {
    // Emacs `substring` works on both strings and vectors (arrays).
    enum Seq {
        Str(Vec<char>),
        Vec(Vec<Value>),
    }
    let seq = match &a[0] {
        Value::Str(s) => Seq::Str(s.chars().collect()),
        _ => match h.obj(&a[0]) {
            Some(Obj::Vector(items)) => Seq::Vec(items.clone()),
            _ => {
                return Err(format!(
                    "wrong-type-argument: arrayp {}",
                    h.print(&a[0], true)
                ));
            }
        },
    };
    let len = match &seq {
        Seq::Str(c) => c.len() as i64,
        Seq::Vec(v) => v.len() as i64,
    };
    // Negative indices count from the end; Emacs then bounds-checks rather than
    // clamping, signalling args-out-of-range for anything outside [0, len].
    let adj = |i: i64| -> i64 {
        if i < 0 {
            len + i
        } else {
            i
        }
    };
    let start = match a.get(1) {
        Some(v) if !is_nil(v) => adj(as_int(h, v)?),
        _ => 0,
    };
    let end = match a.get(2) {
        Some(v) if !is_nil(v) => adj(as_int(h, v)?),
        _ => len,
    };
    if start < 0 || end > len || start > end {
        // Emacs reports the *original* FROM/TO arguments (nil for an omitted
        // TO), not the negative-resolved or defaulted values.
        let from = a
            .get(1)
            .map(|v| h.print(v, true))
            .unwrap_or_else(|| "nil".to_string());
        let to = a
            .get(2)
            .map(|v| h.print(v, true))
            .unwrap_or_else(|| "nil".to_string());
        return Err(format!(
            "args-out-of-range: {} {from} {to}",
            h.print(&a[0], true)
        ));
    }
    match seq {
        Seq::Str(c) => Ok(Value::str(
            c[start as usize..end as usize].iter().collect::<String>(),
        )),
        Seq::Vec(v) => Ok(h.alloc(Obj::Vector(v[start as usize..end as usize].to_vec()))),
    }
}
fn split_string(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    // With the default separators (whitespace) OMIT-NULLS is implicitly on; with
    // an explicit SEPARATORS it defaults off unless the 3rd arg is non-nil.
    let default_seps = a.len() < 2 || is_nil(&a[1]);
    let omit_nulls = default_seps || a.get(2).is_some_and(|v| !is_nil(v));
    let mut parts: Vec<String> = if default_seps {
        s.split_whitespace().map(|w| w.to_string()).collect()
    } else {
        let sep = as_string(&a[1])?;
        if sep.is_empty() {
            s.chars().map(|c| c.to_string()).collect()
        } else {
            // SEPARATORS is a regexp in Emacs, not a literal string.
            let re = compile_cf(&sep, false)?;
            re.split(&s)
                .filter_map(|w| w.ok())
                .map(|w| w.to_string())
                .collect()
        }
    };
    if omit_nulls {
        parts.retain(|w| !w.is_empty());
    }
    Ok(h.list_from(parts.into_iter().map(Value::str).collect()))
}
fn string_prefix_p(_h: &mut ElispHost, a: &[Value]) -> R {
    let (pre, s) = (as_string(&a[0])?, as_string(&a[1])?);
    let ignore_case = a.get(2).is_some_and(|v| !is_nil(v));
    Ok(nil_or(if ignore_case {
        s.to_lowercase().starts_with(&pre.to_lowercase())
    } else {
        s.starts_with(&pre)
    }))
}
fn string_suffix_p(_h: &mut ElispHost, a: &[Value]) -> R {
    let (suf, s) = (as_string(&a[0])?, as_string(&a[1])?);
    let ignore_case = a.get(2).is_some_and(|v| !is_nil(v));
    Ok(nil_or(if ignore_case {
        s.to_lowercase().ends_with(&suf.to_lowercase())
    } else {
        s.ends_with(&suf)
    }))
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
fn char_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_char(h, &a[0])?;
    Ok(Value::str(
        char::from_u32(n).map(|c| c.to_string()).unwrap_or_default(),
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
fn make_string(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_int(h, &a[0])?;
    if n < 0 {
        return Err(format!("wrong-type-argument: wholenump {n}"));
    }
    let c = char::from_u32(as_char(h, &a[1])?).unwrap_or(' ');
    Ok(Value::str(c.to_string().repeat(n as usize)))
}
fn string_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut s = String::new();
    for v in a {
        if let Some(c) = char::from_u32(as_int(h, v)? as u32) {
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
fn string_search(h: &mut ElispHost, a: &[Value]) -> R {
    let needle = as_string(&a[0])?;
    let hay = as_string(&a[1])?;
    // Optional START is a char index; search only the tail from there, then map
    // the byte offset back to an absolute char index. Emacs bounds-checks START
    // against [0, len], signalling args-out-of-range with the raw START value.
    let hay_len = hay.chars().count() as i64;
    let start_char = match a.get(2) {
        Some(v) if !is_nil(v) => {
            let n = as_int(h, v)?;
            if n < 0 || n > hay_len {
                return Err(format!("args-out-of-range: {n}"));
            }
            n as usize
        }
        _ => 0,
    };
    let start_byte = hay
        .char_indices()
        .nth(start_char)
        .map(|(b, _)| b)
        .unwrap_or(hay.len());
    Ok(match hay[start_byte..].find(&needle) {
        Some(off) => Value::Int(hay[..start_byte + off].chars().count() as i64),
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
pub(crate) fn char_of_byte(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx].chars().count()
}

/// Compile an elisp regexp to a `fancy_regex::Regex` (optionally case-insensitively,
/// for `case-fold-search`), surfacing translation and compilation failures as
/// elisp-style `invalid-regexp` errors.
pub(crate) fn compile_cf(pat: &str, case_insensitive: bool) -> Result<fancy_regex::Regex, String> {
    let translated = crate::regexp::translate(pat)?;
    // Elisp `^`/`$` always match line boundaries, so compile in multiline mode;
    // `\``/`\'` (translated to \A/\z) keep matching the absolute start/end.
    let flags = if case_insensitive { "(?mi)" } else { "(?m)" };
    let pat = format!("{flags}{translated}");
    fancy_regex::Regex::new(&pat).map_err(|e| format!("invalid-regexp: {e}"))
}
/// Read the dynamic `case-fold-search` (default t) — string matching folds case
/// unless it is bound to nil.
pub(crate) fn case_fold_search(h: &ElispHost) -> bool {
    match h.find_symbol("case-fold-search") {
        Some(sym) => h
            .get_value(&sym)
            .map(|v| !matches!(v, Value::Undef | Value::Bool(false)))
            .unwrap_or(true),
        None => true,
    }
}

/// Run `re` against `subject` starting at char index `start`, returning the
/// capture spans in *char* positions (group 0 = whole match).
fn run_match(
    re: &fancy_regex::Regex,
    subject: &str,
    start: usize,
) -> Option<Vec<Option<(usize, usize)>>> {
    let start_byte = byte_of_char(subject, start);
    let caps = re.captures_from_pos(subject, start_byte).ok().flatten()?;
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
        Some(v) => {
            // Emacs: a negative START counts from the end (`len + START`); any
            // START outside `[0, len]` after that adjustment is args-out-of-range,
            // whose DATA is `(STRING RAW-START)`.
            let raw = as_int(h, v)?;
            let len = subject.chars().count() as i64;
            let pos = if raw < 0 { len + raw } else { raw };
            if pos < 0 || pos > len {
                return Err(format!("args-out-of-range: {} {raw}", h.print(&a[1], true)));
            }
            pos as usize
        }
    };
    let re = compile_cf(&pat, case_fold_search(h))?;
    match run_match(&re, &subject, start) {
        Some(spans) => {
            let begin = spans[0].map(|(b, _)| b as i64).unwrap_or(0);
            h.match_data = Some(MatchData {
                subject,
                spans,
                from_buffer: false,
            });
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
    let n = as_int(h, &a[0])?.max(0) as usize;
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
    let n = as_int(h, &a[0])?.max(0) as usize;
    let Some(md) = h.match_data.as_ref() else {
        return Ok(Value::Undef);
    };
    let span = md.spans.get(n).copied().flatten();
    // Buffer matches store 1-based buffer positions; read the current buffer
    // text by char (unless an explicit STRING argument is given).
    if md.from_buffer && !matches!(a.get(1), Some(Value::Str(_))) {
        return Ok(match span {
            Some((b, e)) => {
                let t = &h.cur_buf().text;
                let (lo, hi) = ((b - 1).min(t.len()), (e - 1).min(t.len()));
                Value::str(t[lo..hi].iter().collect::<String>())
            }
            None => Value::Undef,
        });
    }
    let subject = match a.get(1) {
        Some(Value::Str(s)) => s.to_string(),
        _ => md.subject.clone(),
    };
    match span {
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
    h.match_data = Some(MatchData {
        subject,
        spans,
        from_buffer: false,
    });
    Ok(Value::Undef)
}

/// `(regexp-quote STRING)` — STRING with every regexp-special character escaped
/// so it matches literally under elisp regexp rules.
fn regexp_quote(_h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        // The set Emacs's own `regexp-quote` escapes (search.c Fregexp_quote:
        // `*.\?+[^$`). Notably `]` is NOT escaped.
        if matches!(c, '.' | '*' | '+' | '?' | '[' | '^' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    Ok(Value::str(out))
}

/// Expand a `replace-regexp-in-string` template against one match: `\&` is the
/// whole match, `\1`..`\9` are capture groups, `\\` is a literal backslash.
fn expand_replacement(rep: &str, caps: &fancy_regex::Captures, subject: &str) -> String {
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

/// Adapt REP's case to MATCHED's (Emacs FIXEDCASE-nil behavior): an all-uppercase
/// match upcases REP; a capitalized match (first letter upper, some lowercase)
/// upcases the first letter of each word in REP; otherwise REP is unchanged.
fn adapt_replacement_case(matched: &str, rep: String) -> String {
    let (mut upper, mut lower, mut first_upper) = (0u32, 0u32, None);
    for c in matched.chars() {
        if c.is_alphabetic() {
            if c.is_uppercase() {
                upper += 1;
            } else {
                lower += 1;
            }
            if first_upper.is_none() {
                first_upper = Some(c.is_uppercase());
            }
        }
    }
    if upper > 0 && lower == 0 {
        rep.to_uppercase()
    } else if first_upper == Some(true) {
        // Upcase the first letter of each word (run of alphanumerics), keep the rest.
        let mut out = String::with_capacity(rep.len());
        let mut prev_word = false;
        for c in rep.chars() {
            if c.is_alphabetic() && !prev_word {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            prev_word = c.is_alphanumeric();
        }
        out
    } else {
        rep
    }
}

/// `(replace-regexp-in-string REGEXP REP STRING &optional FIXEDCASE LITERAL)` —
/// replace every match of REGEXP in STRING with REP. REP is a template (`\&`,
/// `\N` backrefs) unless LITERAL is non-nil. Function-valued REP is not yet
/// supported (string templates cover the common case without re-entering the VM).
fn replace_regexp_in_string(h: &mut ElispHost, a: &[Value]) -> R {
    let pat = as_string(&a[0])?;
    let rep = as_string(&a[1])?;
    let subject = as_string(&a[2])?;
    // FIXEDCASE (4th arg) nil → adapt the replacement's case to the match's.
    let fixedcase = !matches!(
        a.get(3),
        Some(Value::Undef) | Some(Value::Bool(false)) | None
    );
    let literal = !matches!(
        a.get(4),
        Some(Value::Undef) | Some(Value::Bool(false)) | None
    );
    let re = compile_cf(&pat, case_fold_search(h))?;
    let mut out = String::with_capacity(subject.len());
    let mut last = 0usize;
    for caps in re.captures_iter(&subject) {
        let Ok(caps) = caps else { break };
        let m = caps.get(0).unwrap();
        out.push_str(&subject[last..m.start()]);
        let piece = if literal {
            rep.clone()
        } else {
            expand_replacement(&rep, &caps, &subject)
        };
        out.push_str(&if fixedcase {
            piece
        } else {
            adapt_replacement_case(m.as_str(), piece)
        });
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
/// Rounding mode for `floor`/`ceiling`/`round`/`truncate`.
#[derive(Clone, Copy)]
enum Rm {
    Floor,
    Ceil,
    Trunc,
    Round,
}
/// `(OP NUMBER &optional DIVISOR)` — round NUMBER (or NUMBER/DIVISOR) to an
/// integer under `rm`. Integer operands use exact integer division so large
/// magnitudes don't lose precision; a float operand routes through `f64`.
fn quotient(h: &ElispHost, a: &[Value], rm: Rm) -> R {
    let (xi, xf, xisf) = as_num(h, &a[0])?;
    match a.get(1) {
        Some(d) if !is_nil(d) => {
            let (di, df, disf) = as_num(h, d)?;
            if !xisf && !disf {
                if di == 0 {
                    return Err("arith-error: division by zero".to_string());
                }
                return Ok(Value::Int(int_div(xi, di, rm)));
            }
            if df == 0.0 {
                return Err("arith-error: division by zero".to_string());
            }
            let q = xf / df;
            if !q.is_finite() {
                // Emacs signals `overflow-error` when a float rounding op has no
                // finite integer result (infinite or NaN quotient), rather than
                // saturating. (truncate 1.0e+INF 2.0) => (overflow-error).
                return Err("overflow-error".to_string());
            }
            Ok(Value::Int(apply_rm(q, rm) as i64))
        }
        _ => {
            if xisf {
                if !xf.is_finite() {
                    // (truncate 1.0e+INF), (round (/ 0.0 0.0)) => (overflow-error).
                    return Err("overflow-error".to_string());
                }
                Ok(Value::Int(apply_rm(xf, rm) as i64))
            } else {
                Ok(Value::Int(xi))
            }
        }
    }
}
fn apply_rm(f: f64, rm: Rm) -> f64 {
    match rm {
        Rm::Floor => f.floor(),
        Rm::Ceil => f.ceil(),
        Rm::Trunc => f.trunc(),
        Rm::Round => f.round_ties_even(),
    }
}
fn int_div(x: i64, y: i64, rm: Rm) -> i64 {
    let q = x / y;
    let r = x % y;
    match rm {
        Rm::Trunc => q,
        Rm::Floor if r != 0 && (r < 0) != (y < 0) => q - 1,
        Rm::Ceil if r != 0 && (r < 0) == (y < 0) => q + 1,
        Rm::Round => (x as f64 / y as f64).round_ties_even() as i64,
        _ => q,
    }
}
fn floor_fn(h: &mut ElispHost, a: &[Value]) -> R {
    quotient(h, a, Rm::Floor)
}
fn ceiling_fn(h: &mut ElispHost, a: &[Value]) -> R {
    quotient(h, a, Rm::Ceil)
}
fn round_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // Emacs rounds half to even (banker's rounding): (round 2.5) => 2, (round 0.5) => 0.
    quotient(h, a, Rm::Round)
}
fn truncate_fn(h: &mut ElispHost, a: &[Value]) -> R {
    quotient(h, a, Rm::Trunc)
}
fn float_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (_i, f, _isf) = as_num(h, &a[0])?;
    Ok(Value::Float(f))
}
fn logand_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut r: i64 = -1;
    for v in a {
        r &= as_int_om(h, v)?;
    }
    Ok(Value::Int(r))
}
fn logior_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut r: i64 = 0;
    for v in a {
        r |= as_int_om(h, v)?;
    }
    Ok(Value::Int(r))
}
fn logxor_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut r: i64 = 0;
    for v in a {
        r ^= as_int_om(h, v)?;
    }
    Ok(Value::Int(r))
}
fn lognot_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(!as_integer(h, &a[0])?))
}
fn ash_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_integer(h, &a[0])?;
    let c = as_integer(h, &a[1])?;
    Ok(Value::Int(if c >= 0 {
        n.wrapping_shl(c as u32)
    } else {
        // Arithmetic right shift; a shift ≥ 64 fills with the sign bit, so a
        // positive value collapses to 0 and a negative one to -1 (Rust's `>>`
        // would panic on an out-of-range count).
        let sh = (-c) as u32;
        if sh >= 64 {
            if n < 0 {
                -1
            } else {
                0
            }
        } else {
            n >> sh
        }
    }))
}

/// `(lsh VALUE COUNT)` — *logical* shift, unlike `ash`'s arithmetic shift.
/// Left shift matches `ash`. For a right shift (negative COUNT) Emacs treats the
/// fixnum as an *unsigned* value of the fixnum bit width, so vacated high bits
/// fill with zeros rather than the sign bit: `(lsh -1 -1)` => 2305843009213693951
/// (`(2^62-1) >> 1`), not -1. Fixnums are 62-bit here (`most-positive-fixnum`
/// = 2^61-1), so mask to 62 bits before the unsigned shift.
fn lsh_fn(h: &mut ElispHost, a: &[Value]) -> R {
    const FIXNUM_MASK: u64 = (1u64 << 62) - 1;
    let n = as_integer(h, &a[0])?;
    let c = as_integer(h, &a[1])?;
    Ok(Value::Int(if c >= 0 {
        n.wrapping_shl(c as u32)
    } else {
        let sh = (-c) as u32;
        // Masked value is < 2^62, so any shift ≥ 62 yields 0 (and this also
        // avoids an out-of-range `>>` on the u64).
        if sh >= 62 {
            0
        } else {
            (((n as u64) & FIXNUM_MASK) >> sh) as i64
        }
    }))
}

// ── parity: float math / numeric parsing / introspection ──

/// `(expt BASE EXP)` — integer power when BASE is an integer and EXP a
/// non-negative integer; otherwise float `BASE**EXP` (covers negative and
/// fractional exponents). `(expt 2 10)`=>1024, `(expt 2 -1)`=>0.5, `(expt 2.0 0.5)`.
fn expt_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (bi, bf, bisf) = as_num(h, &a[0])?;
    let (ei, ef, eisf) = as_num(h, &a[1])?;
    if !bisf && !eisf && ei >= 0 {
        Ok(Value::Int(bi.wrapping_pow(ei as u32)))
    } else {
        Ok(Value::Float(bf.powf(ef)))
    }
}
fn sqrt_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.sqrt()))
}
fn exp_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.exp()))
}
fn log_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let x = as_num(h, &a[0])?.1;
    Ok(Value::Float(match a.get(1) {
        // Emacs uses `log10`/`log2` for base 10/2 (exact for powers of the base:
        // (log 1000 10) => 3.0), falling back to ln(x)/ln(base) otherwise.
        Some(b) => {
            let base = as_num(h, b)?.1;
            if base == 10.0 {
                x.log10()
            } else if base == 2.0 {
                x.log2()
            } else {
                x.ln() / base.ln()
            }
        }
        None => x.ln(),
    }))
}
fn sin_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.sin()))
}
fn cos_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.cos()))
}
fn tan_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.tan()))
}
fn asin_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.asin()))
}
fn acos_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.acos()))
}
fn atan_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let y = as_num(h, &a[0])?.1;
    Ok(Value::Float(match a.get(1) {
        Some(x) => y.atan2(as_num(h, x)?.1),
        None => y.atan(),
    }))
}
/// `ldexp(x, n)` = `x * 2^n`, computed like C's `scalbn` (musl) so subnormal
/// results are preserved instead of flushing to 0. A naive `x * 2f64.powi(n)`
/// overflows `2^n` to infinity for very negative `n` and returns 0.0, whereas
/// Emacs (via C `ldexp`) yields the smallest subnormal (e.g. `(ldexp 1.0 -1074)`
/// => 5e-324). The staged scaling keeps every intermediate in range.
fn scalbn(x: f64, mut n: i64) -> f64 {
    let two_1023 = 2f64.powi(1023);
    let two_m1022 = f64::MIN_POSITIVE; // 2^-1022
    let two_53 = (1u64 << 53) as f64; // 2^53
    let mut y = x;
    if n > 1023 {
        y *= two_1023;
        n -= 1023;
        if n > 1023 {
            y *= two_1023;
            n -= 1023;
            if n > 1023 {
                n = 1023;
            }
        }
    } else if n < -1022 {
        // Keep the final n < -53 to avoid double rounding in the subnormal range.
        y *= two_m1022 * two_53;
        n += 1022 - 53;
        if n < -1022 {
            y *= two_m1022 * two_53;
            n += 1022 - 53;
            if n < -1022 {
                n = -1022;
            }
        }
    }
    y * f64::from_bits(((0x3ff + n) as u64) << 52)
}
fn ldexp_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let m = as_num(h, &a[0])?.1;
    let e = as_num(h, &a[1])?.0;
    Ok(Value::Float(scalbn(m, e)))
}
fn copysign_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(
        as_num(h, &a[0])?.1.copysign(as_num(h, &a[1])?.1),
    ))
}
/// Decompose V into (SIGNIFICAND . EXPONENT) with the significand in [0.5,1).
/// Bit-level port of C `frexp` (musl): exact for all values including subnormals
/// and huge magnitudes. The old `log2`-based formula divided by `2^e`, which
/// overflowed/underflowed to give `(0.0 . 1024)` for 1e308 and `(inf . -1073)`
/// for the smallest subnormal instead of Emacs's `(0.5562… . 1024)` / `(0.5 . -1073)`.
fn frexp_parts(x: f64) -> (f64, i64) {
    let bits = x.to_bits();
    let ee = (bits >> 52) & 0x7ff;
    if ee == 0 {
        // Subnormal or zero.
        if x == 0.0 {
            (x, 0) // preserves -0.0
        } else {
            // Normalize by scaling up 2^64, then correct the exponent.
            let (m, e) = frexp_parts(x * 2f64.powi(64));
            (m, e - 64)
        }
    } else if ee == 0x7ff {
        (x, 0) // inf or NaN
    } else {
        let e = ee as i64 - 0x3fe;
        let m = f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000);
        (m, e)
    }
}
fn frexp_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (m, e) = frexp_parts(as_num(h, &a[0])?.1);
    Ok(h.cons(Value::Float(m), Value::Int(e)))
}
fn isnan_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(a[0], Value::Float(f) if f.is_nan())))
}
fn fround_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.round_ties_even()))
}
fn ffloor_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.floor()))
}
fn fceiling_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.ceil()))
}
fn ftruncate_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_num(h, &a[0])?.1.trunc()))
}

/// `(string-to-number STRING &optional BASE)` — parse a leading number. With
/// BASE (2–16) parse an integer in that radix; otherwise parse an int, or a
/// float when a `.` or exponent is present. Non-numeric input yields 0.
fn string_to_number(h: &mut ElispHost, a: &[Value]) -> R {
    let raw = as_string(&a[0])?;
    let s = raw.trim_start();
    if let Some(bv) = a.get(1) {
        // Base 10 (and nil) use the float-capable default parser below; only a
        // non-decimal base forces integer-only parsing.
        if !is_nil(bv) {
            let base_i = as_int(h, bv)?;
            // Emacs restricts BASE to 2..16 and signals args-out-of-range
            // otherwise (checked before the base==10 fast path).
            if !(2..=16).contains(&base_i) {
                return Err(format!("args-out-of-range: {base_i}"));
            }
            if base_i != 10 {
                let base = base_i as u32;
                let mut chars = s.chars().peekable();
                let mut sign = 1i64;
                match chars.peek() {
                    Some('+') => {
                        chars.next();
                    }
                    Some('-') => {
                        sign = -1;
                        chars.next();
                    }
                    _ => {}
                }
                let (mut n, mut seen) = (0i64, false);
                for c in chars {
                    match c.to_digit(base) {
                        Some(d) => {
                            n = n.wrapping_mul(base as i64) + d as i64;
                            seen = true;
                        }
                        None => break,
                    }
                }
                return Ok(Value::Int(if seen { sign * n } else { 0 }));
            }
        }
    }
    let b: Vec<char> = s.chars().collect();
    let (mut i, n) = (0usize, b.len());
    let start = i;
    if i < n && (b[i] == '+' || b[i] == '-') {
        i += 1;
    }
    let (mut has_digit, mut is_float) = (false, false);
    while i < n && b[i].is_ascii_digit() {
        i += 1;
        has_digit = true;
    }
    // A bare trailing dot (no fractional digit, no exponent) keeps the number an
    // integer in Emacs: `(string-to-number "1.")` => 1, not 1.0. Only a digit
    // after the dot — or an exponent below — makes it a float.
    let mut dot_pos = None;
    if i < n && b[i] == '.' {
        dot_pos = Some(i);
        i += 1;
        while i < n && b[i].is_ascii_digit() {
            i += 1;
            has_digit = true;
            is_float = true;
        }
    }
    if has_digit && i < n && (b[i] == 'e' || b[i] == 'E') {
        let mut j = i + 1;
        if j < n && (b[j] == '+' || b[j] == '-') {
            j += 1;
        }
        if j < n && b[j].is_ascii_digit() {
            is_float = true;
            i = j;
            while i < n && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    if !has_digit {
        return Ok(Value::Int(0));
    }
    if is_float {
        let tok: String = b[start..i].iter().collect();
        Ok(Value::Float(tok.parse().unwrap_or(0.0)))
    } else {
        // Integer parse must exclude a trailing dot ("1." => 1), so stop at the
        // dot position when one was consumed without becoming a float.
        let end = dot_pos.unwrap_or(i);
        let tok: String = b[start..end].iter().collect();
        Ok(Value::Int(tok.parse().unwrap_or(0)))
    }
}

// ── sxhash ──
// Hash values are NOT bit-compatible with GNU Emacs (impl-specific), but they are
// self-consistent: `equal`/`eq`/`eql` objects hash equally, within bounded depth.
fn hash_mix(acc: u64, x: u64) -> u64 {
    (acc ^ x)
        .wrapping_mul(0x100000001b3)
        .wrapping_add(0x9e3779b97f4a7c15)
}
fn hash_bytes(s: &str) -> u64 {
    let mut acc = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        acc = hash_mix(acc, b as u64);
    }
    acc
}
/// Structural hash (for `sxhash-equal`), depth-bounded like Emacs.
fn sxhash_equal(h: &ElispHost, v: &Value, depth: u32) -> u64 {
    match v {
        Value::Int(n) => *n as u64,
        Value::Float(f) => f.to_bits(),
        Value::Str(s) => hash_bytes(s),
        Value::Bool(false) | Value::Undef => 0,
        Value::Bool(true) => 1,
        Value::Obj(_) => match h.obj(v) {
            Some(Obj::Symbol(s)) => hash_mix(0x5111, hash_bytes(&s.name)),
            Some(Obj::Cons(car, cdr)) => {
                if depth >= 5 {
                    0x3
                } else {
                    let (car, cdr) = (car.clone(), cdr.clone());
                    hash_mix(
                        hash_mix(0xc0, sxhash_equal(h, &car, depth + 1)),
                        sxhash_equal(h, &cdr, depth + 1),
                    )
                }
            }
            Some(Obj::Vector(items)) => {
                if depth >= 5 {
                    0x4
                } else {
                    let items = items.clone();
                    let mut acc = 0x7e;
                    for it in &items {
                        acc = hash_mix(acc, sxhash_equal(h, it, depth + 1));
                    }
                    acc
                }
            }
            _ => 0x6,
        },
        _ => 0x8,
    }
}
/// Identity-ish hash (for `sxhash-eq`): heap objects by arena id, numbers by value.
fn sxhash_eq(v: &Value) -> u64 {
    match v {
        Value::Int(n) => *n as u64,
        Value::Float(f) => f.to_bits(),
        Value::Bool(false) | Value::Undef => 0,
        Value::Bool(true) => 1,
        Value::Str(s) => hash_bytes(s),
        Value::Obj(id) => hash_mix(0xab, *id as u64),
        _ => 0x8,
    }
}
/// Mask a raw hash to a non-negative fixnum, as Emacs's sxhash returns.
fn sxhash_fixnum(x: u64) -> Value {
    Value::Int((x & 0x1FFF_FFFF_FFFF_FFFF) as i64)
}
fn sxhash_equal_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(sxhash_fixnum(sxhash_equal(h, &a[0], 0)))
}
fn sxhash_eq_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(sxhash_fixnum(sxhash_eq(&a[0])))
}
fn sxhash_eql_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // eql: numbers by value, everything else by identity.
    let x = match &a[0] {
        Value::Int(_) | Value::Float(_) => sxhash_equal(h, &a[0], 0),
        other => sxhash_eq(other),
    };
    Ok(sxhash_fixnum(x))
}

/// `(type-of OBJECT)` — the symbol naming OBJECT's primitive type.
fn type_of(h: &mut ElispHost, a: &[Value]) -> R {
    // A cl-defstruct instance reports its struct name (like an Emacs record).
    if let Some(Obj::Vector(items)) = h.obj(&a[0]) {
        if let Some(name) = h.struct_tag_name(&items.clone()) {
            return Ok(h.intern(&name));
        }
    }
    let name = match &a[0] {
        Value::Int(_) => "integer",
        Value::Float(_) => "float",
        Value::Str(_) => "string",
        Value::Bool(_) | Value::Undef => "symbol",
        Value::Obj(_) => match h.obj(&a[0]) {
            Some(Obj::Cons(..)) => "cons",
            Some(Obj::Symbol(_)) => "symbol",
            Some(Obj::Vector(_)) => "vector",
            Some(Obj::Subr { .. }) => "subr",
            Some(Obj::Closure { .. }) => "function",
            Some(Obj::HashTable { .. }) => "hash-table",
            Some(Obj::CharTable(_)) => "char-table",
            Some(Obj::Buffer(_)) => "buffer",
            Some(Obj::Marker(_)) => "marker",
            None => "symbol",
        },
        _ => "symbol",
    };
    Ok(h.intern(name))
}
/// `(recordp OBJECT)` / `(cl-struct-p OBJECT)` — non-nil for a cl-defstruct
/// instance (a `cl-struct-NAME`-tagged vector, in this model).
fn recordp(h: &mut ElispHost, a: &[Value]) -> R {
    let ok = matches!(h.obj(&a[0]), Some(Obj::Vector(items)) if h.struct_tag_name(&items.clone()).is_some());
    Ok(nil_or(ok))
}

// ── OClosure C primitives (oclosure.el) ──
// These implement the host-specific seam `oclosure.el` builds on. The rest of
// oclosure.el is ported faithfully into the prelude. Because elisprs closures
// are compiled (not aref-indexable interpreted-functions), the type + slot
// layout is attached via a side table and slot values live in the closure's
// captured env — see `ElispHost::oclosure_*`.

/// `(closurep OBJECT)` — t if OBJECT is a closure.
fn closurep_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.is_closure(&a[0])))
}

/// `(oclosure--fix-type TYPE SLOTS MUTABLES CLOSURE)` — mark CLOSURE as an
/// OClosure of TYPE with the given ordered SLOTS; returns CLOSURE. In Emacs this
/// is a 2-arg cconv marker whose type rides in the lambda's `:documentation`;
/// elisprs passes the type + slot names explicitly (its closures have no
/// aref-addressable docstring slot) — a NAMED divergence of the internal seam,
/// not of the observable API.
fn oclosure_fix_type(h: &mut ElispHost, a: &[Value]) -> R {
    let closure = a[3].clone();
    if !h.is_closure(&closure) {
        return Err("Wrong type argument: closurep".to_string());
    }
    let ty = h
        .as_sym_handle(&a[0])
        .ok_or("oclosure--fix-type: type is not a symbol")?;
    let slot_vals = h
        .list_vec(&a[1])
        .ok_or("oclosure--fix-type: slots is not a proper list")?;
    let mut slots = Vec::with_capacity(slot_vals.len());
    for s in &slot_vals {
        slots.push(
            h.as_sym_handle(s)
                .ok_or("oclosure--fix-type: slot name is not a symbol")?,
        );
    }
    h.oclosure_set_meta(&closure, ty, slots);
    Ok(closure)
}

/// `(oclosure-type OCLOSURE)` — the type symbol of OCLOSURE, or nil.
fn oclosure_type_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match h.oclosure_type_of(&a[0]) {
        Some(id) => Ok(Value::Obj(id)),
        None => Ok(Value::Undef),
    }
}

/// `(oclosure--get OCLOSURE INDEX MUTABLE)` — value of slot INDEX. MUTABLE is
/// accepted for signature compatibility but unused (slot values always live in a
/// mutable env cell; mutability is enforced by the class in `oclosure--set-slot-value`).
fn oclosure_get_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if !h.is_closure(&a[0]) {
        return Err("Wrong type argument: closurep".to_string());
    }
    let idx = as_int(h, &a[1])? as usize;
    h.oclosure_get(&a[0], idx)
        .ok_or_else(|| "oclosure--get: slot index out of range".to_string())
}

/// `(oclosure--set V OCLOSURE INDEX)` — set slot INDEX to V; returns V.
fn oclosure_set_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if !h.is_closure(&a[1]) {
        return Err("Wrong type argument: closurep".to_string());
    }
    let idx = as_int(h, &a[2])? as usize;
    if h.oclosure_set(&a[1], idx, &a[0]) {
        Ok(a[0].clone())
    } else {
        Err("oclosure--set: slot index out of range".to_string())
    }
}

/// `(oclosure--copy OCLOSURE MUTLIST &rest ARGS)` — functional copy of OCLOSURE
/// with the first `(length ARGS)` slots replaced. MUTLIST (bytecode mutable-cell
/// wrapping) is irrelevant to elisprs's env-cell slots and ignored.
fn oclosure_copy_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if !h.is_closure(&a[0]) {
        return Err("Wrong type argument: closurep".to_string());
    }
    let args: Vec<Value> = a[2..].to_vec();
    h.oclosure_copy(&a[0], &args)
        .ok_or_else(|| "oclosure--copy: not an OClosure".to_string())
}

/// `(functionp OBJECT)` — non-nil if OBJECT can be called as a function (a subr,
/// a non-macro closure, or a symbol whose function cell resolves to one).
fn functionp(h: &mut ElispHost, a: &[Value]) -> R {
    let ok = match h.resolve_function(&a[0]) {
        Ok(Resolved::Subr { .. }) => true,
        Ok(Resolved::Closure { is_macro, .. }) => !is_macro,
        Err(_) => false,
    };
    Ok(nil_or(ok))
}
fn char_or_string_p(_h: &mut ElispHost, a: &[Value]) -> R {
    // A "character" is an integer in [0, #x3FFFFF]; strings always qualify.
    let ok = match &a[0] {
        Value::Int(n) => (0..=0x3F_FFFF).contains(n),
        Value::Str(_) => true,
        _ => false,
    };
    Ok(nil_or(ok))
}
fn char_equal(h: &mut ElispHost, a: &[Value]) -> R {
    let (c1, c2) = (as_int(h, &a[0])?, as_int(h, &a[1])?);
    if c1 == c2 {
        return Ok(Value::Bool(true));
    }
    // With case-fold-search (default t), compare case-insensitively.
    if case_fold_search(h) {
        if let (Some(x), Some(y)) = (char::from_u32(c1 as u32), char::from_u32(c2 as u32)) {
            let eq = x.to_lowercase().eq(y.to_lowercase());
            return Ok(nil_or(eq));
        }
    }
    Ok(Value::Bool(false))
}
/// `(symbol-function SYMBOL)` — the symbol's function-cell value, or nil.
fn symbol_function(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(h.function_cell(&a[0]).unwrap_or(Value::Undef))
}
/// `(intern-soft NAME)` — the interned symbol named NAME, or nil if none exists.
fn intern_soft(h: &mut ElispHost, a: &[Value]) -> R {
    let name = match &a[0] {
        Value::Str(s) => s.to_string(),
        _ => h.sym_name(&a[0]).ok_or("wrong-type-argument: stringp")?,
    };
    Ok(h.find_symbol(&name).unwrap_or(Value::Undef))
}
fn subrp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Subr { .. }))))
}
// Forms elisprs lowers as compiler intrinsics but which Emacs classifies as
// *macros* (`lambda`/`when`/… are macros there, not special forms). Each carries
// the minimum arity of its Emacs `subr.el` lambda-list (max is always `many`),
// used by `func-arity`.
const INTRINSIC_MACROS: &[(&str, i64)] = &[
    ("lambda", 0),
    ("when", 1),
    ("unless", 1),
    ("defun", 2),
    ("defmacro", 2),
];
// The genuine special forms, matching Emacs's `special-form-p`. Each carries the
// C-level minimum arity Emacs's `func-arity` reports (max is always `unevalled`).
// `prog2` is a macro in Emacs, not a special form; `interactive`/`inline` are.
const SPECIAL_FORMS: &[(&str, i64)] = &[
    ("and", 0),
    ("catch", 1),
    ("cond", 0),
    ("condition-case", 2),
    ("defconst", 2),
    ("defvar", 1),
    ("function", 1),
    ("if", 2),
    ("inline", 0),
    ("interactive", 0),
    ("let", 1),
    ("let*", 1),
    ("or", 0),
    ("prog1", 1),
    ("progn", 0),
    ("quote", 1),
    ("save-current-buffer", 0),
    ("save-excursion", 0),
    ("save-restriction", 0),
    ("setq", 0),
    ("unwind-protect", 1),
    ("while", 1),
];
/// `(macrop OBJECT)` — non-nil if OBJECT is (or names) a macro.
fn macrop(h: &mut ElispHost, a: &[Value]) -> R {
    if matches!(
        h.resolve_function(&a[0]),
        Ok(Resolved::Closure { is_macro: true, .. })
    ) {
        return Ok(Value::Bool(true));
    }
    // The intrinsic forms have no closure to resolve, but are macros in Emacs.
    let ok = h
        .sym_name(&a[0])
        .is_some_and(|n| INTRINSIC_MACROS.iter().any(|(m, _)| *m == n.as_str()));
    Ok(nil_or(ok))
}
/// `(special-form-p OBJECT)` — non-nil if OBJECT names a special form (per
/// Emacs's classification, not elisprs's internal lowering).
fn special_form_p(h: &mut ElispHost, a: &[Value]) -> R {
    let ok = h
        .sym_name(&a[0])
        .map(|n| SPECIAL_FORMS.iter().any(|(sf, _)| *sf == n.as_str()))
        .unwrap_or(false);
    Ok(nil_or(ok))
}
fn char_uppercase_p(h: &mut ElispHost, a: &[Value]) -> R {
    let c = char::from_u32(as_int(h, &a[0])? as u32);
    Ok(nil_or(c.is_some_and(|c| c.is_uppercase())))
}
/// `(string-distance S1 S2 &optional BYTECOMPARE)` — Levenshtein edit distance.
/// With BYTECOMPARE non-nil, Emacs measures over UTF-8 bytes, not characters
/// (editfns.c Fstring_distance).
fn string_distance(_h: &mut ElispHost, a: &[Value]) -> R {
    let a0 = as_string(&a[0])?;
    let a1 = as_string(&a[1])?;
    let bytewise = a.len() > 2 && !is_nil(&a[2]);
    let (s1, s2): (Vec<u32>, Vec<u32>) = if bytewise {
        (
            a0.bytes().map(|b| b as u32).collect(),
            a1.bytes().map(|b| b as u32).collect(),
        )
    } else {
        (
            a0.chars().map(|c| c as u32).collect(),
            a1.chars().map(|c| c as u32).collect(),
        )
    };
    let m = s2.len();
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for (i, c1) in s1.iter().enumerate() {
        cur[0] = i + 1;
        for (j, c2) in s2.iter().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    Ok(Value::Int(prev[m] as i64))
}
/// `(vconcat &rest SEQUENCES)` — concatenate any sequences (lists, vectors,
/// strings) into a new vector. `(vconcat [1 2] "a")` => `[1 2 97]`.
fn vconcat_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut out = Vec::new();
    for v in a {
        if is_nil(v) {
            continue;
        }
        match h.obj(v) {
            Some(Obj::Vector(items)) => out.extend(items.clone()),
            _ => match v {
                Value::Str(s) => out.extend(s.chars().map(|c| Value::Int(c as i64))),
                _ => match h.list_vec(v) {
                    Some(items) => out.extend(items),
                    None => {
                        return Err(format!(
                            "wrong-type-argument: sequencep {}",
                            h.print(v, true)
                        ))
                    }
                },
            },
        }
    }
    Ok(h.alloc(Obj::Vector(out)))
}
/// `(abs NUMBER)` — absolute value, keeping the int/float type (and turning
/// `-0.0` into `0.0`, which a `(< x 0)` test would miss).
fn abs_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    match &a[0] {
        Value::Int(n) => Ok(Value::Int(n.wrapping_abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        _ => Err(format!(
            "wrong-type-argument: numberp {}",
            a[0].as_str_cow()
        )),
    }
}
/// `(logcount N)` — count of set bits for N≥0, or of clear bits for N<0 (i.e.
/// bits differing from the sign bit), matching Emacs.
fn logcount_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_integer(h, &a[0])?;
    let bits = if n >= 0 {
        n.count_ones()
    } else {
        (!n).count_ones()
    };
    Ok(Value::Int(bits as i64))
}
// ── secure-hash / sha1 / md5 (self-contained, no crates) ──
fn sha1_bytes(msg: &[u8]) -> Vec<u8> {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let ml = (msg.len() as u64).wrapping_mul(8);
    let mut data = msg.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&ml.to_be_bytes());
    for chunk in data.chunks(64) {
        let mut w = [0u32; 80];
        for (i, wi) in w.iter_mut().enumerate().take(16) {
            *wi = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    h.iter().flat_map(|x| x.to_be_bytes()).collect()
}
fn sha256_bytes(msg: &[u8]) -> Vec<u8> {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let ml = (msg.len() as u64).wrapping_mul(8);
    let mut data = msg.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&ml.to_be_bytes());
    for chunk in data.chunks(64) {
        let mut w = [0u32; 64];
        for (i, wi) in w.iter_mut().enumerate().take(16) {
            *wi = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (hi, vi) in h.iter_mut().zip(v.iter()) {
            *hi = hi.wrapping_add(*vi);
        }
    }
    h.iter().flat_map(|x| x.to_be_bytes()).collect()
}
fn md5_bytes(msg: &[u8]) -> Vec<u8> {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let (mut a0, mut b0, mut c0, mut d0) =
        (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    let ml = (msg.len() as u64).wrapping_mul(8);
    let mut data = msg.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&ml.to_le_bytes());
    for chunk in data.chunks(64) {
        let mut m = [0u32; 16];
        for (i, mi) in m.iter_mut().enumerate() {
            *mi = u32::from_le_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    [a0, b0, c0, d0]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect()
}
fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
/// The string argument's bytes between optional char START/END.
fn hash_input(h: &ElispHost, a: &[Value], obj_idx: usize) -> Result<Vec<u8>, String> {
    let s = as_string(&a[obj_idx])?;
    let chars: Vec<char> = s.chars().collect();
    let start = match a.get(obj_idx + 1) {
        Some(v) if !is_nil(v) => as_int(h, v)?.max(0) as usize,
        _ => 0,
    };
    let end = match a.get(obj_idx + 2) {
        Some(v) if !is_nil(v) => (as_int(h, v)?.max(0) as usize).min(chars.len()),
        _ => chars.len(),
    };
    let sub: String = chars[start.min(end)..end].iter().collect();
    Ok(sub.into_bytes())
}
fn sha1_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::str(to_hex(&sha1_bytes(&hash_input(h, a, 0)?))))
}
fn md5_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::str(to_hex(&md5_bytes(&hash_input(h, a, 0)?))))
}
/// `(secure-hash ALGORITHM OBJECT &optional START END BINARY)`.
fn secure_hash(h: &mut ElispHost, a: &[Value]) -> R {
    // ALGORITHM is a symbol (md5/sha1/sha256/…); accept a string too.
    let algo = h
        .sym_name(&a[0])
        .or_else(|| as_string(&a[0]).ok())
        .unwrap_or_default();
    let bytes = hash_input(h, a, 1)?;
    let digest = match algo.as_str() {
        "md5" => md5_bytes(&bytes),
        "sha1" => sha1_bytes(&bytes),
        "sha256" => sha256_bytes(&bytes),
        other => return Err(format!("error: unsupported secure-hash algorithm {other}")),
    };
    // BINARY (4th optional, index 4): return the raw bytes as a string.
    if a.get(4).is_some_and(|v| !is_nil(v)) {
        Ok(Value::str(
            digest.iter().map(|&b| b as char).collect::<String>(),
        ))
    } else {
        Ok(Value::str(to_hex(&digest)))
    }
}
// ── base64 / url encoding ──
const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
fn b64_encode(input: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(alphabet[((n >> 18) & 63) as usize] as char);
        out.push(alphabet[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(alphabet[((n >> 6) & 63) as usize] as char);
        } else if pad {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(alphabet[(n & 63) as usize] as char);
        } else if pad {
            out.push('=');
        }
    }
    out
}
/// Insert a newline every 76 output characters (Emacs base64 default wrapping).
fn b64_wrap(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 76);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && i % 76 == 0 {
            out.push('\n');
        }
        out.push(c);
    }
    out
}
fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let (mut bits, mut nbits, mut out) = (0u32, 0u32, Vec::new());
    for c in input.bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = val(c).ok_or("error: invalid base64 data")?;
        bits = (bits << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Ok(out)
}
/// Render decoded bytes as a string with each byte a char 0–255 (unibyte-ish).
fn bytes_to_str(bytes: &[u8]) -> Value {
    Value::str(bytes.iter().map(|&b| b as char).collect::<String>())
}
fn base64_encode_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let raw = b64_encode(as_string(&a[0])?.as_bytes(), B64_STD, true);
    let no_break = a.get(1).is_some_and(|v| !is_nil(v));
    Ok(Value::str(if no_break { raw } else { b64_wrap(&raw) }))
}
fn base64_decode_string(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(bytes_to_str(&b64_decode(&as_string(&a[0])?)?))
}
fn base64url_encode_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let no_pad = a.get(1).is_some_and(|v| !is_nil(v));
    Ok(Value::str(b64_encode(
        as_string(&a[0])?.as_bytes(),
        B64_URL,
        !no_pad,
    )))
}
fn base64url_decode_string(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(bytes_to_str(&b64_decode(&as_string(&a[0])?)?))
}
/// `(url-hexify-string STRING)` — percent-encode all but `[A-Za-z0-9-._~]`.
fn url_hexify_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let mut out = String::new();
    for b in as_string(&a[0])?.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(Value::str(out))
}
/// `(url-unhex-string STRING)` — decode `%XX` escapes.
fn url_unhex_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Ok(Value::str(String::from_utf8(out).unwrap_or_else(|e| {
        e.into_bytes().iter().map(|&b| b as char).collect()
    })))
}
/// `(string-to-vector STRING)` — a vector of STRING's character codes.
fn string_to_vector(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let items: Vec<Value> = s.chars().map(|c| Value::Int(c as i64)).collect();
    Ok(h.alloc(Obj::Vector(items)))
}
/// `(logb X)` — the binary exponent of |X|: floor(log2(|X|)).
///
/// Faithful to Emacs 30 `Flogb` (floatfns.c): a finite nonzero argument yields
/// the integer `frexp` exponent minus one; every other case (zero, ±infinity,
/// NaN) falls through to C `logb`, which returns a *float* — `-inf` for zero,
/// `+inf` for either infinity, and NaN for NaN.
fn logb_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let f = as_num(h, &a[0])?.1;
    if f.is_finite() && f != 0.0 {
        return Ok(Value::Int(f.abs().log2().floor() as i64));
    }
    let val = if f.is_nan() {
        f64::NAN
    } else if f == 0.0 {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    Ok(Value::Float(val))
}
/// `(max-char &optional UNICODE)` — the largest character code. With non-nil
/// UNICODE the max Unicode scalar (`#x10FFFF`); otherwise the max Emacs char
/// code (`#x3FFFFF`), which spans the raw-byte / eight-bit range too.
fn max_char(_h: &mut ElispHost, a: &[Value]) -> R {
    let unicode = a.first().map(|v| !is_nil(v)).unwrap_or(false);
    Ok(Value::Int(if unicode { 0x10_FFFF } else { 0x3F_FFFF }))
}
/// `(byteorder)` — `?l` (108) on a little-endian host, `?B` (66) on big-endian.
fn byteorder(_h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Int(if cfg!(target_endian = "little") {
        108
    } else {
        66
    }))
}
// ── character modifiers / descriptions (faithful C ports) ──
// Modifier bit values from src/lisp.h; base-character mask is CHARACTERBITS=22.
const CHAR_ALT: i64 = 0x0400000;
const CHAR_SUPER: i64 = 0x0800000;
const CHAR_HYPER: i64 = 0x1000000;
const CHAR_SHIFT: i64 = 0x2000000;
const CHAR_CTL: i64 = 0x4000000;
const CHAR_META: i64 = 0x8000000;
const CHAR_MODIFIER_MASK: i64 =
    CHAR_ALT | CHAR_SUPER | CHAR_HYPER | CHAR_SHIFT | CHAR_CTL | CHAR_META;
const MAX_CHAR: i64 = 0x3F_FFFF;
const MAX_5_BYTE_CHAR: i64 = 0x3F_FF7F;

/// `ASCII_CHAR_P(c)` (src/character.h): `0 <= c && c < 0x80`.
fn ascii_char_p(c: i64) -> bool {
    (0..0x80).contains(&c)
}
/// `CHECK_CHARACTER`: a character is a fixnum in `0..=MAX_CHAR` (`0x3FFFFF`).
fn check_character(h: &ElispHost, v: &Value) -> Result<i64, String> {
    let c = as_int(h, v)?;
    if (0..=MAX_CHAR).contains(&c) {
        Ok(c)
    } else {
        Err(format!("wrong-type-argument: characterp {c}"))
    }
}

/// `(char-resolve-modifiers CHAR)` — port of `char_resolve_modifier_mask`
/// (src/character.c). Fold the Shift and Control modifier bits of an ASCII base
/// character into the code; Meta and other modifiers are left in place. CHAR is
/// any integer (`CHECK_FIXNUM`), not just a valid character.
fn char_resolve_modifiers(h: &mut ElispHost, a: &[Value]) -> R {
    let mut c = as_int(h, &a[0])?;
    // A non-ASCII base character can't reflect modifier bits into the code.
    if !ascii_char_p(c & !CHAR_MODIFIER_MASK) {
        return Ok(Value::Int(c));
    }
    if c & CHAR_SHIFT != 0 {
        let base = c & 0o377;
        // Shift is valid only with [A-Za-z]; on control chars / SPC it's dropped.
        if (b'A' as i64..=b'Z' as i64).contains(&base) {
            c &= !CHAR_SHIFT;
        } else if (b'a' as i64..=b'z' as i64).contains(&base) {
            c = (c & !CHAR_SHIFT) - (b'a' as i64 - b'A' as i64);
        } else if (c & !CHAR_MODIFIER_MASK) <= 0x20 {
            c &= !CHAR_SHIFT;
        }
    }
    if c & CHAR_CTL != 0 {
        let base = c & 0o377;
        // Allow `\C- ` and `\C-?`; otherwise make ASCII control chars from
        // letters (both cases) and the non-letters within 0100..0137.
        if base == b' ' as i64 {
            c &= !0o177 & !CHAR_CTL;
        } else if base == b'?' as i64 {
            c = 0o177 | (c & !0o177 & !CHAR_CTL);
        } else if (0o101..=0o132).contains(&(c & 0o137)) || (0o100..=0o137).contains(&(c & 0o177)) {
            c &= 0o37 | (!0o177 & !CHAR_CTL);
        }
    }
    Ok(Value::Int(c))
}

/// `(text-char-description CHARACTER)` — port of `Ftext_char_description`
/// (src/keymap.c) + `push_text_char_description`. ASCII control chars become
/// `^X`, DEL becomes `^?`, everything else renders as itself. Modifier bits
/// (Meta etc.) fail the `characterp` check. Characters outside Unicode
/// (eight-bit / raw internal codes) can't be held in a UTF-8 string here, so
/// they yield the empty string.
fn text_char_description(h: &mut ElispHost, a: &[Value]) -> R {
    let c = check_character(h, &a[0])?;
    if ascii_char_p(c) {
        let s = if c < 0o40 {
            format!("^{}", (c as u8 + 64) as char)
        } else if c == 0o177 {
            "^?".to_string()
        } else {
            (c as u8 as char).to_string()
        };
        Ok(Value::str(s))
    } else {
        Ok(Value::str(
            char::from_u32(c as u32)
                .map(|c| c.to_string())
                .unwrap_or_default(),
        ))
    }
}

/// `(unibyte-char-to-multibyte BYTE)` — port of `Funibyte_char_to_multibyte`
/// (src/charset.c). ASCII bytes map to themselves; bytes `0x80..=0xFF` become
/// the eight-bit character `0x3FFF00 + byte`. BYTE above 255 is not unibyte.
fn unibyte_char_to_multibyte(h: &mut ElispHost, a: &[Value]) -> R {
    let c = check_character(h, &a[0])?;
    if c >= 256 {
        return Err(format!("error: Not a unibyte character: {c}"));
    }
    Ok(Value::Int(if c < 0x80 { c } else { c + 0x3F_FF00 }))
}

/// `(multibyte-char-to-unibyte CHAR)` — port of `Fmultibyte_char_to_unibyte`
/// (src/charset.c). Characters below 256 map to themselves, eight-bit chars
/// (above `MAX_5_BYTE_CHAR`) map to their raw byte, all others map to -1.
fn multibyte_char_to_unibyte(h: &mut ElispHost, a: &[Value]) -> R {
    let c = check_character(h, &a[0])?;
    let byte = if c < 256 {
        c
    } else if c > MAX_5_BYTE_CHAR {
        c - 0x3F_FF00
    } else {
        -1
    };
    Ok(Value::Int(byte))
}

/// `(emacs-pid)` — the process id of the running interpreter.
fn emacs_pid(_h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Int(std::process::id() as i64))
}

/// `(load-average &optional USE-FLOATS)` — port of `Fload_average` (src/fns.c).
/// The 1/5/15-minute system load averages: each `load` (a float) when
/// USE-FLOATS is non-nil, else `trunc(100 * load)` as an integer.
fn load_average(h: &mut ElispHost, a: &[Value]) -> R {
    let use_floats = a.first().map(|v| !is_nil(v)).unwrap_or(false);
    let mut loads = [0f64; 3];
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if n < 0 {
        return Err("error: load-average not implemented for this operating system".to_string());
    }
    let items = loads[..n as usize]
        .iter()
        .map(|&l| {
            if use_floats {
                Value::Float(l)
            } else {
                Value::Int((100.0 * l) as i64)
            }
        })
        .collect();
    Ok(h.list_from(items))
}

/// `(bare-symbol-p OBJECT)` — non-nil if OBJECT is a symbol without position.
/// elisprs has no symbol-with-position type, so every symbol is bare — this is
/// exactly `symbolp` (nil and t count as symbols).
fn bare_symbol_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        matches!(a[0], Value::Bool(true) | Value::Undef)
            || matches!(h.obj(&a[0]), Some(Obj::Symbol(_))),
    ))
}
/// `(car-less-than-car A B)` — `(< (car A) (car B))`, the standard comparator
/// for sorting alists (Emacs `car-less-than-car`).
fn car_less_than_car(h: &mut ElispHost, a: &[Value]) -> R {
    let car_of = |h: &ElispHost, v: &Value| -> Result<Value, String> {
        match h.obj(v) {
            Some(Obj::Cons(x, _)) => Ok(x.clone()),
            _ if is_nil(v) => Ok(Value::Undef),
            _ => Err(format!("wrong-type-argument: listp {}", h.print(v, true))),
        }
    };
    let a0 = car_of(h, &a[0])?;
    let b0 = car_of(h, &a[1])?;
    Ok(nil_or(as_num(h, &a0)?.1 < as_num(h, &b0)?.1))
}
/// `(subr-name SUBR)` — the name of a primitive SUBR as a string. Signals
/// `wrong-type-argument` when SUBR is not a subr (e.g. a plain symbol).
fn subr_name(h: &mut ElispHost, a: &[Value]) -> R {
    match h.obj(&a[0]) {
        Some(Obj::Subr { name, .. }) => Ok(Value::str(name.clone())),
        _ => Err(format!(
            "wrong-type-argument: subrp {}",
            h.print(&a[0], true)
        )),
    }
}
/// `(default-boundp SYMBOL)` — non-nil if SYMBOL has a default value, i.e. its
/// global value cell is bound (ignoring any buffer-local binding).
fn default_boundp(h: &mut ElispHost, a: &[Value]) -> R {
    let bound = is_nil(&a[0]) || matches!(a[0], Value::Bool(true)) || h.default_boundp_raw(&a[0]);
    Ok(nil_or(bound))
}
/// `(default-toplevel-value SYMBOL)` — SYMBOL's default (toplevel) value, ignoring
/// any buffer-local binding; signals `void-variable` when the default is unbound.
fn default_toplevel_value(h: &mut ElispHost, a: &[Value]) -> R {
    h.raw_global_value(&a[0])
}
/// `(read STRING)` — read the first Lisp form from STRING.
fn read_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let forms = crate::reader::read_all(h, &s)?;
    forms
        .into_iter()
        .next()
        .ok_or_else(|| "end-of-file".to_string())
}
/// `(read-from-string STRING &optional START END)` — read the first object from
/// STRING (from char index START), returning `(OBJECT . END-INDEX)`.
fn read_from_string(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(&a[0])?;
    let size = s.chars().count() as i64;
    // START/END follow Emacs `validate_subarray`: a negative index counts from
    // the end (`i + len`), nil defaults to 0 / len, and the checked range is
    // `0 <= from <= to <= len`; a violation is args-out-of-range whose DATA is
    // `(STRING RAW-START RAW-END)` built from the *original* (unadjusted) args.
    let adj = |v: Option<&Value>, default: i64| -> i64 {
        match v {
            Some(Value::Int(n)) => {
                if *n < 0 {
                    n + size
                } else {
                    *n
                }
            }
            _ => default,
        }
    };
    let from = adj(a.get(1), 0);
    let to = adj(a.get(2), size);
    if !(0 <= from && from <= to && to <= size) {
        let disp = |v: Option<&Value>| match v {
            None | Some(Value::Undef) | Some(Value::Bool(false)) => "nil".to_string(),
            Some(v) => h.print(v, true),
        };
        return Err(format!(
            "args-out-of-range: {} {} {}",
            h.print(&a[0], true),
            disp(a.get(1)),
            disp(a.get(2))
        ));
    }
    // Limit the reader to the first `to` characters. Char indices in the prefix
    // are identical to the original, so the returned END position stays valid.
    let limited: String = s.chars().take(to as usize).collect();
    let (form, end) = crate::reader::read_one(h, &limited, from as usize)?;
    Ok(h.cons(form, Value::Int(end as i64)))
}
/// `(compare-strings S1 START1 END1 S2 START2 END2 &optional IGNORE-CASE)` —
/// `t` if the substrings are equal, else a signed 1-based index of the first
/// mismatch (negative when S1 sorts before S2), per Emacs.
fn compare_strings(_h: &mut ElispHost, a: &[Value]) -> R {
    let s1: Vec<char> = as_string(&a[0])?.chars().collect();
    let s2: Vec<char> = as_string(&a[3])?.chars().collect();
    let bound = |v: &Value, default: usize| -> usize {
        match v {
            Value::Int(n) => (*n).max(0) as usize,
            _ => default,
        }
    };
    let (start1, end1) = (
        bound(&a[1], 0),
        bound(a.get(2).unwrap_or(&Value::Undef), s1.len()).min(s1.len()),
    );
    let (start2, end2) = (
        bound(&a[4], 0),
        bound(a.get(5).unwrap_or(&Value::Undef), s2.len()).min(s2.len()),
    );
    let ignore_case = a.get(6).is_some_and(|v| !is_nil(v));
    let sub1 = &s1[start1.min(end1)..end1];
    let sub2 = &s2[start2.min(end2)..end2];
    let fold = |c: char| {
        if ignore_case {
            c.to_lowercase().next().unwrap_or(c)
        } else {
            c
        }
    };
    let n = sub1.len().min(sub2.len());
    for i in 0..n {
        let (x, y) = (fold(sub1[i]), fold(sub2[i]));
        if x != y {
            let idx = (i + 1) as i64;
            return Ok(Value::Int(if x < y { -idx } else { idx }));
        }
    }
    match sub1.len().cmp(&sub2.len()) {
        std::cmp::Ordering::Less => Ok(Value::Int(-((n + 1) as i64))),
        std::cmp::Ordering::Greater => Ok(Value::Int((n + 1) as i64)),
        std::cmp::Ordering::Equal => Ok(Value::Bool(true)),
    }
}

/// `(member-ignore-case ELT LIST)` — like `member`, but the comparison is
/// case-insensitive and ELT is compared only against the *string* elements of
/// LIST (non-strings are skipped, never match). Returns the tail of LIST that
/// begins with the first matching element, else nil. Mirrors GNU Emacs subr.el:
/// each candidate is tested via `(compare-strings ELT 0 nil CAND 0 nil t)` which
/// signals `wrong-type-argument stringp` if ELT is not a string and a string
/// candidate is reached; an all-non-string / empty LIST returns nil silently.
fn member_ignore_case(h: &mut ElispHost, a: &[Value]) -> R {
    let elt = a[0].clone();
    let mut cur = a[1].clone();
    loop {
        // Pull car/cdr out into owned values so the immutable arena borrow ends
        // before the mutable `compare_strings` call below.
        let (car, cdr) = match h.obj(&cur) {
            Some(Obj::Cons(car, cdr)) => (car.clone(), cdr.clone()),
            _ => return Ok(Value::Undef),
        };
        if let Value::Str(_) = &car {
            let cmp = compare_strings(
                h,
                &[
                    elt.clone(),
                    Value::Int(0),
                    Value::Undef,
                    car,
                    Value::Int(0),
                    Value::Undef,
                    Value::Bool(true),
                ],
            )?;
            if matches!(cmp, Value::Bool(true)) {
                return Ok(cur);
            }
        }
        cur = cdr;
    }
}

// ── time ──
fn now_secs() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

// ── random ──
thread_local! {
    static RNG_STATE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
/// Seed the PRNG from the system clock (xorshift never starts from 0).
fn rng_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15);
    nanos | 1
}
fn rng_next() -> u64 {
    RNG_STATE.with(|s| {
        let mut x = s.get();
        if x == 0 {
            x = rng_seed();
        }
        // xorshift64
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        x
    })
}
/// `(random &optional LIMIT)`. With a positive integer LIMIT, return an integer in
/// [0, LIMIT); with t, reseed and return a random integer; otherwise a random
/// fixnum (may be negative).
fn random_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match a.first() {
        Some(Value::Bool(true)) => {
            RNG_STATE.with(|s| s.set(rng_seed()));
            Ok(Value::Int((rng_next() >> 1) as i64))
        }
        Some(v) if !is_nil(v) => {
            let n = as_int(h, v)?;
            if n <= 0 {
                return Err("args-out-of-range: random limit must be positive".to_string());
            }
            Ok(Value::Int((rng_next() % n as u64) as i64))
        }
        _ => Ok(Value::Int((rng_next() >> 1) as i64)),
    }
}

/// Convert an elisp TIME value to epoch seconds (float). Accepts nil (= now), an
/// integer/float of seconds, a `(TICKS . HZ)` pair, or a `(HIGH LOW [USEC ...])`
/// legacy list.
fn time_arg_secs(h: &ElispHost, v: Option<&Value>) -> Result<f64, String> {
    match v {
        None => Ok(now_secs()),
        Some(t) if is_nil(t) => Ok(now_secs()),
        Some(Value::Int(n)) => Ok(*n as f64),
        Some(Value::Float(f)) => Ok(*f),
        Some(t) => {
            // (TICKS . HZ): a cons whose cdr is a number.
            if let Some(Obj::Cons(car, Value::Int(hz))) = h.obj(t) {
                if *hz != 0 {
                    return Ok(as_num(h, car)?.1 / (*hz as f64));
                }
            }
            // (HIGH LOW [USEC [PSEC]]).
            let parts = h.list_vec(t).ok_or("invalid time value")?;
            let get = |i: usize| parts.get(i).and_then(|v| as_num(h, v).ok()).map(|x| x.1);
            let high = get(0).unwrap_or(0.0);
            let low = get(1).unwrap_or(0.0);
            let usec = get(2).unwrap_or(0.0);
            Ok(high * 65536.0 + low + usec / 1.0e6)
        }
    }
}

/// Decompose epoch seconds into a `struct tm` for the given ZONE (nil = local,
/// non-nil non-number = UTC, integer = fixed offset seconds east of UTC).
fn time_decompose(secs: f64, zone: Option<&Value>) -> libc::tm {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    match zone {
        None | Some(Value::Undef) | Some(Value::Bool(false)) => {
            let t = secs.floor() as libc::time_t;
            unsafe { libc::localtime_r(&t, &mut tm) };
        }
        Some(Value::Int(off)) => {
            // Fixed offset: read as UTC at secs+off, then stamp the offset.
            let t = (secs.floor() as libc::time_t) + *off as libc::time_t;
            unsafe { libc::gmtime_r(&t, &mut tm) };
            tm.tm_gmtoff = *off as libc::c_long;
        }
        _ => {
            let t = secs.floor() as libc::time_t;
            unsafe { libc::gmtime_r(&t, &mut tm) };
        }
    }
    tm
}

const WD_ABBR: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WD_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MON_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MON_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn fmt_time_string(fmt: &str, tm: &libc::tm, secs: f64) -> String {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= chars.len() {
            out.push('%');
            break;
        }
        // Optional flags (-_0^#) then optional field width.
        let mut flag: Option<char> = None;
        while i < chars.len() && matches!(chars[i], '-' | '_' | '0' | '^' | '#') {
            if matches!(chars[i], '-' | '_' | '0') {
                flag = Some(chars[i]);
            }
            i += 1;
        }
        let mut wbuf = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            wbuf.push(chars[i]);
            i += 1;
        }
        let user_w: Option<usize> = wbuf.parse().ok();
        if i >= chars.len() {
            break;
        }
        let d = chars[i];
        i += 1;
        // Numeric field with default width/pad, honoring flags.
        let numpad = |val: i64, deftw: usize, defpad: char| -> String {
            if flag == Some('-') {
                return val.to_string();
            }
            let width = user_w.unwrap_or(deftw);
            let pad = match flag {
                Some('_') => ' ',
                Some('0') => '0',
                _ => defpad,
            };
            let s = val.abs().to_string();
            let body = if s.len() < width {
                format!("{}{}", pad.to_string().repeat(width - s.len()), s)
            } else {
                s
            };
            if val < 0 {
                format!("-{body}")
            } else {
                body
            }
        };
        let year = tm.tm_year as i64 + 1900;
        match d {
            'Y' => out.push_str(&numpad(year, 1, '0')),
            'y' => out.push_str(&numpad(year.rem_euclid(100), 2, '0')),
            'm' => out.push_str(&numpad(tm.tm_mon as i64 + 1, 2, '0')),
            'd' => out.push_str(&numpad(tm.tm_mday as i64, 2, '0')),
            'e' => out.push_str(&numpad(tm.tm_mday as i64, 2, ' ')),
            'H' => out.push_str(&numpad(tm.tm_hour as i64, 2, '0')),
            'k' => out.push_str(&numpad(tm.tm_hour as i64, 2, ' ')),
            'I' => out.push_str(&numpad(((tm.tm_hour as i64 + 11) % 12) + 1, 2, '0')),
            'l' => out.push_str(&numpad(((tm.tm_hour as i64 + 11) % 12) + 1, 2, ' ')),
            'M' => out.push_str(&numpad(tm.tm_min as i64, 2, '0')),
            'S' => out.push_str(&numpad(tm.tm_sec as i64, 2, '0')),
            'j' => out.push_str(&numpad(tm.tm_yday as i64 + 1, 3, '0')),
            'w' => out.push_str(&numpad(tm.tm_wday as i64, 1, '0')),
            'u' => out.push_str(&numpad(
                if tm.tm_wday == 0 {
                    7
                } else {
                    tm.tm_wday as i64
                },
                1,
                '0',
            )),
            's' => out.push_str(&(secs.floor() as i64).to_string()),
            // Subsecond field: nanoseconds as a fixed 9-digit number. A field
            // width ≤ 9 keeps that many leading digits (%3N = milliseconds,
            // %6N = microseconds); a width > 9 right-pads with zeros.
            'N' => {
                let frac = secs - secs.floor();
                let nanos = (frac * 1.0e9).round().clamp(0.0, 999_999_999.0) as i64;
                let full = format!("{nanos:09}");
                let w = user_w.unwrap_or(9);
                if w <= 9 {
                    out.push_str(&full[..w]);
                } else {
                    out.push_str(&full);
                    out.push_str(&"0".repeat(w - 9));
                }
            }
            'p' => out.push_str(if tm.tm_hour < 12 { "AM" } else { "PM" }),
            'P' => out.push_str(if tm.tm_hour < 12 { "am" } else { "pm" }),
            'a' => out.push_str(WD_ABBR[(tm.tm_wday as usize) % 7]),
            'A' => out.push_str(WD_FULL[(tm.tm_wday as usize) % 7]),
            'b' | 'h' => out.push_str(MON_ABBR[(tm.tm_mon as usize) % 12]),
            'B' => out.push_str(MON_FULL[(tm.tm_mon as usize) % 12]),
            'Z' => {
                if !tm.tm_zone.is_null() {
                    let cs = unsafe { std::ffi::CStr::from_ptr(tm.tm_zone) };
                    out.push_str(&cs.to_string_lossy());
                }
            }
            'z' => {
                let off = tm.tm_gmtoff;
                let sign = if off < 0 { '-' } else { '+' };
                let a = off.unsigned_abs();
                out.push_str(&format!("{sign}{:02}{:02}", a / 3600, (a % 3600) / 60));
            }
            'F' => out.push_str(&fmt_time_string("%Y-%m-%d", tm, secs)),
            'T' => out.push_str(&fmt_time_string("%H:%M:%S", tm, secs)),
            'R' => out.push_str(&fmt_time_string("%H:%M", tm, secs)),
            'D' => out.push_str(&fmt_time_string("%m/%d/%y", tm, secs)),
            'c' => out.push_str(&fmt_time_string("%a %b %e %H:%M:%S %Y", tm, secs)),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            '%' => out.push('%'),
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

fn float_time(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(time_arg_secs(h, a.first())?))
}

fn current_time(_h: &mut ElispHost, _a: &[Value]) -> R {
    let secs = now_secs();
    let isec = secs.floor() as i64;
    let usec = ((secs - secs.floor()) * 1.0e6) as i64;
    Ok(_h.list_from(vec![
        Value::Int(isec >> 16),
        Value::Int(isec & 0xffff),
        Value::Int(usec),
        Value::Int(0),
    ]))
}

fn format_time_string(h: &mut ElispHost, a: &[Value]) -> R {
    let fmt = as_string(&a[0])?;
    let secs = time_arg_secs(h, a.get(1))?;
    let tm = time_decompose(secs, a.get(2));
    Ok(Value::str(fmt_time_string(&fmt, &tm, secs)))
}

fn current_time_string(h: &mut ElispHost, a: &[Value]) -> R {
    let secs = time_arg_secs(h, a.first())?;
    let tm = time_decompose(secs, a.get(1));
    Ok(Value::str(fmt_time_string(
        "%a %b %e %H:%M:%S %Y",
        &tm,
        secs,
    )))
}

// `tm_gmtoff` is `c_long`; `i64::from` is needed on 32-bit but a no-op here.
#[allow(clippy::useless_conversion)]
fn decode_time(h: &mut ElispHost, a: &[Value]) -> R {
    let secs = time_arg_secs(h, a.first())?;
    let tm = time_decompose(secs, a.get(1));
    let dst = match tm.tm_isdst {
        0 => Value::Undef,
        n if n > 0 => Value::Bool(true),
        _ => Value::Int(-1),
    };
    Ok(h.list_from(vec![
        Value::Int(tm.tm_sec as i64),
        Value::Int(tm.tm_min as i64),
        Value::Int(tm.tm_hour as i64),
        Value::Int(tm.tm_mday as i64),
        Value::Int(tm.tm_mon as i64 + 1),
        Value::Int(tm.tm_year as i64 + 1900),
        Value::Int(tm.tm_wday as i64),
        dst,
        Value::Int(i64::from(tm.tm_gmtoff)),
    ]))
}

fn encode_time(h: &mut ElispHost, a: &[Value]) -> R {
    // Two conventions: (encode-time DECODED-LIST) where the list is
    // (SEC MIN HOUR DAY MON YEAR [DOW] [DST] [ZONE]); or the spread form
    // (encode-time SEC MIN HOUR DAY MON YEAR &optional ZONE).
    let single_list = a.len() == 1 && h.list_vec(&a[0]).is_some();
    let (parts, zone) = if single_list {
        let p = h.list_vec(&a[0]).unwrap();
        let z = p.get(8).cloned();
        (p, z)
    } else {
        (a.to_vec(), a.get(6).cloned())
    };
    let g = |i: usize| {
        parts
            .get(i)
            .and_then(|v| as_num(h, v).ok())
            .map(|x| x.0)
            .unwrap_or(0)
    };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_sec = g(0) as libc::c_int;
    tm.tm_min = g(1) as libc::c_int;
    tm.tm_hour = g(2) as libc::c_int;
    tm.tm_mday = g(3) as libc::c_int;
    tm.tm_mon = (g(4) - 1) as libc::c_int;
    tm.tm_year = (g(5) - 1900) as libc::c_int;
    tm.tm_isdst = -1;
    let secs: i64 = match zone.as_ref() {
        None | Some(Value::Undef) | Some(Value::Bool(false)) => unsafe {
            libc::mktime(&mut tm) as i64
        },
        // Components are stated in a fixed offset east of UTC: read as UTC, then back out the offset.
        Some(Value::Int(off)) => unsafe { libc::timegm(&mut tm) as i64 - *off },
        _ => unsafe { libc::timegm(&mut tm) as i64 },
    };
    Ok(h.list_from(vec![
        Value::Int(secs.div_euclid(65536)),
        Value::Int(secs.rem_euclid(65536)),
    ]))
}

// ── environment / working directory ──
fn getenv_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let name = as_string(&a[0])?;
    Ok(std::env::var(&name).map(Value::str).unwrap_or(Value::Undef))
}
fn setenv_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    let name = as_string(&a[0])?;
    match a.get(1) {
        Some(v) if !is_nil(v) => {
            let val = as_string(v)?;
            std::env::set_var(&name, &val);
            Ok(Value::str(val))
        }
        _ => {
            std::env::remove_var(&name);
            Ok(Value::Undef)
        }
    }
}
fn special_variable_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.symbol_special(&a[0])))
}
fn func_arity(h: &mut ElispHost, a: &[Value]) -> R {
    // Special forms and the intrinsic macros have no resolvable function cell,
    // so `resolve_function` would signal `void-function`. Emacs instead reports
    // their C-level arity: special forms as `(MIN . unevalled)`, macros as
    // `(MIN . many)`.
    if let Some(name) = h.sym_name(&a[0]) {
        let name = name.as_str().to_owned();
        if let Some((_, min)) = SPECIAL_FORMS.iter().find(|(sf, _)| *sf == name) {
            let unevalled = h.intern("unevalled");
            return Ok(h.cons(Value::Int(*min), unevalled));
        }
        if let Some((_, min)) = INTRINSIC_MACROS.iter().find(|(m, _)| *m == name) {
            let many = h.intern("many");
            return Ok(h.cons(Value::Int(*min), many));
        }
    }
    let (min, max) = {
        match h.resolve_function(&a[0])? {
            Resolved::Subr { min, max, .. } => (min as i64, max.map(|m| m as i64)),
            Resolved::Closure { params, .. } => {
                let mn = params.required.len() as i64;
                if params.rest.is_some() {
                    (mn, None)
                } else {
                    (mn, Some(mn + params.optional.len() as i64))
                }
            }
        }
    };
    let maxv = match max {
        Some(m) => Value::Int(m),
        None => h.intern("many"),
    };
    Ok(h.cons(Value::Int(min), maxv))
}
fn current_directory(_h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::str(
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "/".to_string()),
    ))
}

// ── filesystem (read-only queries) ──
/// Expand a leading `~/` against $HOME; relative paths resolve against the
/// process cwd (= `default-directory`), as elisp expects.
fn fs_expand(s: &str) -> std::path::PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(s)
}
fn fs_access(p: &std::path::Path, mode: libc::c_int) -> bool {
    use std::os::unix::ffi::OsStrExt;
    match std::ffi::CString::new(p.as_os_str().as_bytes()) {
        Ok(c) => unsafe { libc::access(c.as_ptr(), mode) == 0 },
        Err(_) => false,
    }
}
fn file_exists_p(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_expand(&as_string(&a[0])?).exists()))
}
fn file_directory_p(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_expand(&as_string(&a[0])?).is_dir()))
}
fn file_regular_p(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_expand(&as_string(&a[0])?).is_file()))
}
fn file_readable_p(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_access(
        &fs_expand(&as_string(&a[0])?),
        libc::R_OK,
    )))
}
fn file_writable_p(_h: &mut ElispHost, a: &[Value]) -> R {
    let p = fs_expand(&as_string(&a[0])?);
    // For a non-existent file, writability is the parent directory's.
    let target = if p.exists() {
        p.clone()
    } else {
        p.parent().map(|x| x.to_path_buf()).unwrap_or(p)
    };
    Ok(nil_or(fs_access(&target, libc::W_OK)))
}
fn file_symlink_p(_h: &mut ElispHost, a: &[Value]) -> R {
    match std::fs::read_link(fs_expand(&as_string(&a[0])?)) {
        Ok(t) => Ok(Value::str(t.to_string_lossy().into_owned())),
        Err(_) => Ok(Value::Undef),
    }
}
fn directory_files_raw(h: &mut ElispHost, a: &[Value]) -> R {
    let raw = as_string(&a[0])?;
    let match_re = match a.get(1) {
        Some(v) if !is_nil(v) => Some(compile_cf(&as_string(v)?, false)?),
        _ => None,
    };
    let nosort = a.get(2).is_some_and(|v| !is_nil(v));
    let rd = std::fs::read_dir(fs_expand(&raw))
        .map_err(|_| format!("file-missing: Opening directory: No such file: {raw}"))?;
    let mut names: Vec<String> = vec![".".into(), "..".into()];
    for e in rd.flatten() {
        names.push(e.file_name().to_string_lossy().into_owned());
    }
    if let Some(re) = match_re {
        names.retain(|n| re.is_match(n).unwrap_or(false));
    }
    if !nosort {
        names.sort();
    }
    Ok(h.list_from(names.into_iter().map(Value::str).collect()))
}

// ── buffer registry (named, live buffers over a global registry) ──
fn bufferp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Buffer(_)))))
}
fn current_buffer_fn(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.current_buffer())
}
fn set_buffer_fn(h: &mut ElispHost, a: &[Value]) -> R {
    h.set_buffer(&a[0])
}
fn get_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    // A buffer object returns itself if live; a name looks the buffer up.
    match h.resolve_buffer(&a[0]) {
        Some(idx) => Ok(h.buffers[idx].self_obj.clone()),
        None => Ok(Value::Undef),
    }
}
fn get_buffer_create(h: &mut ElispHost, a: &[Value]) -> R {
    let name = as_string(&a[0])?;
    Ok(h.get_buffer_create(&name))
}
fn generate_new_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let base = as_string(&a[0])?;
    let name = h.generate_new_buffer_name(&base);
    // generate-new-buffer always makes a fresh buffer (the unique name is free).
    Ok(h.get_buffer_create(&name))
}
fn generate_new_buffer_name(h: &mut ElispHost, a: &[Value]) -> R {
    let base = as_string(&a[0])?;
    Ok(Value::str(h.generate_new_buffer_name(&base)))
}
fn buffer_name(h: &mut ElispHost, a: &[Value]) -> R {
    let idx = match a.first() {
        Some(v) if !is_nil(v) => match h.resolve_buffer(v) {
            Some(i) => i,
            None => return Ok(Value::Undef),
        },
        _ => h.current,
    };
    Ok(match &h.buffers[idx].name {
        Some(n) => Value::str(n.clone()),
        None => Value::Undef,
    })
}
fn buffer_live_p(h: &mut ElispHost, a: &[Value]) -> R {
    let ok = matches!(h.obj(&a[0]), Some(Obj::Buffer(idx))
        if h.buffers.get(*idx).is_some_and(|b| b.name.is_some()));
    Ok(nil_or(ok))
}
fn kill_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(h.kill_buffer(a.first()))
}
fn rename_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let newname = as_string(&a[0])?;
    h.rename_buffer(&newname)
}
fn buffer_list(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.buffer_list())
}

// ── mark (a bare position; active-region / mark-ring not modeled) ──
fn set_mark_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mark = match a.first() {
        Some(v) if !is_nil(v) => Some(as_int(h, v)?.max(1) as usize),
        _ => None,
    };
    h.cur_buf().mark = mark;
    Ok(a.first().cloned().unwrap_or(Value::Undef))
}
fn mark_fn(h: &mut ElispHost, _a: &[Value]) -> R {
    // The optional FORCE argument is accepted but irrelevant here: with no active
    // region tracking, `mark` simply reports the mark position (or nil).
    Ok(match h.cur_buf().mark {
        Some(m) => Value::Int(m as i64),
        None => Value::Undef,
    })
}
fn region_beginning(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    let m = buf
        .mark
        .ok_or("error: The mark is not set now, so there is no region")?;
    Ok(Value::Int(buf.point.min(m) as i64))
}
fn region_end(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    let m = buf
        .mark
        .ok_or("error: The mark is not set now, so there is no region")?;
    Ok(Value::Int(buf.point.max(m) as i64))
}

// ── narrowing ──
fn narrow_to_region(h: &mut ElispHost, a: &[Value]) -> R {
    let beg = as_int(h, &a[0])?.max(1) as usize;
    let end = as_int(h, &a[1])?.max(1) as usize;
    h.narrow(beg, end);
    Ok(Value::Undef)
}
fn widen_fn(h: &mut ElispHost, _a: &[Value]) -> R {
    h.widen();
    Ok(Value::Undef)
}
/// `--se-push--`: record point as a save-excursion marker on the current buffer.
fn se_push(h: &mut ElispHost, _a: &[Value]) -> R {
    let p = h.cur_buf().point;
    h.cur_buf().se_markers.push(p);
    Ok(Value::Undef)
}
/// `--se-pop--`: restore point from (and drop) the current buffer's top
/// save-excursion marker.
fn se_pop(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    if let Some(m) = buf.se_markers.pop() {
        buf.point = m.clamp(buf.begv, buf.zv);
    }
    Ok(Value::Undef)
}
/// `--save-restriction--`: push the current `(begv, zv)` so it tracks edits.
fn save_restriction_push(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    let (lo, hi) = (buf.begv, buf.zv);
    buf.restrict_stack.push((lo, hi));
    Ok(Value::Undef)
}
/// `--restore-restriction--`: pop and reinstate the saved narrowing.
fn restore_restriction(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    if let Some((lo, hi)) = buf.restrict_stack.pop() {
        let maxzv = buf.text.len() + 1;
        buf.begv = lo.clamp(1, maxzv);
        buf.zv = hi.clamp(1, maxzv);
        buf.point = buf.point.clamp(buf.begv, buf.zv);
    }
    Ok(Value::Undef)
}
// ── buffer-local variables ──
fn make_local_variable(h: &mut ElispHost, a: &[Value]) -> R {
    h.make_local_variable(&a[0])
}
fn make_variable_buffer_local(h: &mut ElispHost, a: &[Value]) -> R {
    h.make_variable_buffer_local(&a[0])
}
fn local_variable_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.local_variable_p(&a[0])))
}
fn local_variable_if_set_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.local_variable_if_set_p(&a[0])))
}
fn kill_local_variable(h: &mut ElispHost, a: &[Value]) -> R {
    h.kill_local_variable(&a[0])
}
fn buffer_local_symbols_fn(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.buffer_local_symbols())
}
fn buffer_local_value_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // BUFFER (a[1]) selects the buffer; default to the current one.
    let idx = match a.get(1) {
        Some(v) if !is_nil(v) => h
            .resolve_buffer(v)
            .ok_or("error: Wrong type argument: bufferp")?,
        _ => h.current,
    };
    h.buffer_local_or_default(&a[0], idx)
}
/// `(default-value SYMBOL)` — SYMBOL's default (global) value, ignoring any
/// buffer-local binding. Signals `void-variable` when there is no default.
fn default_value_fn(h: &mut ElispHost, a: &[Value]) -> R {
    h.raw_global_value(&a[0])
}
/// `(set-default SYMBOL VALUE)` — set SYMBOL's default (global) value, bypassing
/// any buffer-local binding. Returns VALUE.
fn set_default_fn(h: &mut ElispHost, a: &[Value]) -> R {
    h.set_raw_global(&a[0], a[1].clone())?;
    Ok(a[1].clone())
}
// ── buffer-local keymap slot ──
fn use_local_map_fn(h: &mut ElispHost, a: &[Value]) -> R {
    h.use_local_map(a[0].clone());
    Ok(Value::Undef)
}
fn current_local_map_fn(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.current_local_map())
}
fn insert_chars(v: &Value) -> Result<Vec<char>, String> {
    match v {
        Value::Str(s) => Ok(s.chars().collect()),
        Value::Int(n) => Ok(vec![char::from_u32(*n as u32).unwrap_or('\u{fffd}')]),
        _ => Err("wrong-type-argument: char-or-string-p".to_string()),
    }
}
fn insert_fn(h: &mut ElispHost, a: &[Value]) -> R {
    for v in a {
        let start = h.cur_buf_ref().point; // 1-based insertion position
        let chars = insert_chars(v)?;
        let n = chars.len();
        h.cur_insert(chars, true);
        // A propertized string carries its text properties into the buffer.
        if let Value::Str(arc) = v {
            if let Some(plists) = h.string_props_vec(arc) {
                for (i, pl) in plists.into_iter().enumerate().take(n) {
                    if !is_nil(&pl) {
                        h.buffer_set_plist_at(start - 1 + i, pl);
                    }
                }
            }
        }
    }
    Ok(Value::Undef)
}
/// `--insert-before-markers--`: insert one string/char, relocating markers at the
/// insertion point past the new text (the `insert-before-markers` primitive).
fn insert_before_markers_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let chunks = insert_chars(&a[0])?;
    h.cur_insert_before_markers(chunks);
    Ok(Value::Undef)
}
fn buffer_string(h: &mut ElispHost, _a: &[Value]) -> R {
    // Only the accessible (narrowed) portion `[begv, zv)`, carrying its text
    // properties (like Emacs `buffer-string`).
    let (begv, zv) = {
        let b = h.cur_buf_ref();
        (b.begv, b.zv)
    };
    let text: String = h.cur_buf_ref().text[(begv - 1)..(zv - 1)].iter().collect();
    let out = Value::str(text);
    let plists: Vec<Value> = (begv - 1..zv - 1)
        .map(|i| {
            h.cur_buf_ref()
                .props
                .get(i)
                .cloned()
                .unwrap_or(Value::Undef)
        })
        .collect();
    if plists.iter().any(|p| !is_nil(p)) {
        if let Value::Str(arc) = &out {
            h.string_set_props_vec(arc, plists);
        }
    }
    Ok(out)
}
fn buffer_size(h: &mut ElispHost, _a: &[Value]) -> R {
    // The full buffer size, ignoring any narrowing (like Emacs `buffer-size`).
    Ok(Value::Int(h.cur_buf().text.len() as i64))
}
fn point_fn(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Int(h.cur_buf().point as i64))
}
fn point_min(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Int(h.cur_buf().begv as i64))
}
fn point_max(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(Value::Int(h.cur_buf().zv as i64))
}
fn goto_char(h: &mut ElispHost, a: &[Value]) -> R {
    let arg = as_int(h, &a[0])?;
    let buf = h.cur_buf();
    // Point is clamped to the accessible region, but `goto-char` returns its
    // POSITION argument unchanged (verified against the binary).
    buf.point = arg.clamp(buf.begv as i64, buf.zv as i64) as usize;
    Ok(Value::Int(arg))
}
fn erase_buffer(h: &mut ElispHost, _a: &[Value]) -> R {
    // Delete the whole buffer (ignoring narrowing) and remove the restriction.
    let len = h.cur_buf().text.len();
    h.cur_delete(1, len + 1);
    h.widen();
    h.cur_buf().point = 1;
    Ok(Value::Undef)
}
fn char_after(h: &mut ElispHost, a: &[Value]) -> R {
    let arg = match a.first() {
        Some(v) if !is_nil(v) => Some(as_int(h, v)? as usize),
        _ => None,
    };
    let buf = h.cur_buf();
    let pos = arg.unwrap_or(buf.point);
    // Only positions inside the accessible region `[begv, zv)` hold a char.
    Ok(if pos >= buf.begv && pos < buf.zv {
        Value::Int(buf.text[pos - 1] as i64)
    } else {
        Value::Undef
    })
}
/// Shared core of `buffer-substring` (WITH-props) and `-no-properties`. Returns
/// the accessible-region-clamped `[lo, hi)` character range as a fresh string;
/// when `with_props`, the source characters' text properties are copied onto the
/// returned string (registered in the string-property side table).
fn buffer_substring_core(h: &mut ElispHost, a: &[Value], with_props: bool) -> R {
    let s0 = as_int(h, &a[0])?;
    let e0 = as_int(h, &a[1])?;
    let buf = h.cur_buf_ref();
    let (lo0, hi0) = (buf.begv as i64, buf.zv as i64);
    let s = s0.clamp(lo0, hi0);
    let e = e0.clamp(lo0, hi0);
    let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
    let text: String = buf.text[(lo - 1) as usize..(hi - 1) as usize]
        .iter()
        .collect();
    let out = Value::str(text);
    if with_props {
        // Copy the covered per-char plists (offset to the substring's indices).
        let plists: Vec<Value> = ((lo - 1) as usize..(hi - 1) as usize)
            .map(|i| {
                h.cur_buf_ref()
                    .props
                    .get(i)
                    .cloned()
                    .unwrap_or(Value::Undef)
            })
            .collect();
        if plists.iter().any(|p| !is_nil(p)) {
            if let Value::Str(arc) = &out {
                h.string_set_props_vec(arc, plists);
            }
        }
    }
    Ok(out)
}
fn buffer_substring(h: &mut ElispHost, a: &[Value]) -> R {
    buffer_substring_core(h, a, true)
}
fn buffer_substring_no_properties(h: &mut ElispHost, a: &[Value]) -> R {
    buffer_substring_core(h, a, false)
}

// ── markers ──────────────────────────────────────────────────────────────
fn make_marker(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.alloc_marker(None, 0, false))
}
fn point_marker(h: &mut ElispHost, _a: &[Value]) -> R {
    let p = h.cur_buf_ref().point;
    let bi = h.current;
    Ok(h.alloc_marker(Some(bi), p, false))
}
fn markerp_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.is_marker(&a[0])))
}
fn require_marker(h: &ElispHost, v: &Value) -> Result<(), String> {
    if h.is_marker(v) {
        Ok(())
    } else {
        Err(format!("wrong-type-argument: markerp {}", h.print(v, true)))
    }
}
fn marker_position_fn(h: &mut ElispHost, a: &[Value]) -> R {
    require_marker(h, &a[0])?;
    Ok(match h.marker_position(&a[0]) {
        Some(p) => Value::Int(p as i64),
        None => Value::Undef,
    })
}
fn marker_buffer_fn(h: &mut ElispHost, a: &[Value]) -> R {
    require_marker(h, &a[0])?;
    Ok(h.marker_buffer(&a[0]).unwrap_or(Value::Undef))
}
fn marker_insertion_type_fn(h: &mut ElispHost, a: &[Value]) -> R {
    require_marker(h, &a[0])?;
    Ok(nil_or(h.marker_insertion_type(&a[0]).unwrap_or(false)))
}
fn set_marker_insertion_type_fn(h: &mut ElispHost, a: &[Value]) -> R {
    require_marker(h, &a[0])?;
    h.set_marker_insertion_type(&a[0], !is_nil(&a[1]));
    Ok(a[1].clone())
}
/// `(set-marker MARKER POSITION &optional BUFFER)` / `move-marker`.
fn set_marker_fn(h: &mut ElispHost, a: &[Value]) -> R {
    require_marker(h, &a[0])?;
    let pos_arg = a.get(1).cloned().unwrap_or(Value::Undef);
    if is_nil(&pos_arg) {
        h.set_marker_to(&a[0], None, 0)?;
        return Ok(a[0].clone());
    }
    let pos = as_int(h, &pos_arg)?;
    let bi = match a.get(2) {
        Some(v) if !is_nil(v) => h
            .resolve_buffer(v)
            .ok_or("error: Marker does not point anywhere")?,
        _ => h.current,
    };
    h.set_marker_to(&a[0], Some(bi), pos.max(1) as usize)?;
    Ok(a[0].clone())
}
/// `(copy-marker &optional POSITION TYPE)`.
fn copy_marker_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let itype = a.get(1).is_some_and(|v| !is_nil(v));
    let arg = a.first().cloned().unwrap_or(Value::Undef);
    if is_nil(&arg) {
        return Ok(h.alloc_marker(None, 0, itype));
    }
    if h.is_marker(&arg) {
        // Copy the source marker's buffer + position (detached when it is).
        if let (Some(p), Some(bufv)) = (h.marker_position(&arg), h.marker_buffer(&arg)) {
            let bi = h.resolve_buffer(&bufv).unwrap_or(h.current);
            return Ok(h.alloc_marker(Some(bi), p, itype));
        }
        return Ok(h.alloc_marker(None, 0, itype));
    }
    let pos = as_int(h, &arg)?;
    let bi = h.current;
    let size = h.cur_buf_ref().text.len();
    let p = (pos.max(1) as usize).min(size + 1);
    Ok(h.alloc_marker(Some(bi), p, itype))
}

// ── text properties ────────────────────────────────────────────────────────
/// A text-property OBJECT arg: a string (its Arc) or a buffer slot index.
/// `nil`/absent is the current buffer.
enum PropObj {
    Str(std::sync::Arc<String>),
    Buf(usize),
}
fn prop_object(h: &ElispHost, obj: Option<&Value>) -> Result<PropObj, String> {
    match obj {
        Some(Value::Str(s)) => Ok(PropObj::Str(s.clone())),
        None | Some(Value::Undef) | Some(Value::Bool(false)) => Ok(PropObj::Buf(h.current)),
        Some(v) => match h.resolve_buffer(v) {
            Some(bi) => Ok(PropObj::Buf(bi)),
            None => Err(format!("wrong-type-argument: bufferp {}", h.print(v, true))),
        },
    }
}
/// The plist at POS in OBJECT, validating range like Emacs (string idx `[0,len]`,
/// buffer pos `[begv,zv]`; the upper bound yields nil, past it errors).
fn plist_at_pos(h: &ElispHost, obj: &PropObj, pos: i64) -> Result<Value, String> {
    match obj {
        PropObj::Str(s) => {
            let len = s.chars().count() as i64;
            if pos < 0 || pos > len {
                return Err(format!("args-out-of-range: {pos} {pos}"));
            }
            if pos == len {
                return Ok(Value::Undef);
            }
            Ok(h.string_plist_at(s, pos as usize))
        }
        PropObj::Buf(bi) => {
            let (begv, zv) = h.buffer_begv_zv(*bi);
            if pos < begv as i64 || pos > zv as i64 {
                return Err(format!("args-out-of-range: {pos} {pos}"));
            }
            if pos == zv as i64 {
                return Ok(Value::Undef);
            }
            Ok(h.buffer_plist_at_idx(*bi, (pos - 1) as usize))
        }
    }
}
fn get_text_property_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let pos = as_int(h, &a[0])?;
    let obj = prop_object(h, a.get(2))?;
    let plist = plist_at_pos(h, &obj, pos)?;
    Ok(h.plist_get_eq(&plist, &a[1]))
}
fn text_properties_at_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let pos = as_int(h, &a[0])?;
    let obj = prop_object(h, a.get(1))?;
    plist_at_pos(h, &obj, pos)
}
/// Convert a text-property START/END pair to `(lo0, hi0)` character indices
/// (0-based, half-open) for OBJECT: strings index from 0, buffers from `begv`.
fn prop_range(obj: &PropObj, start: i64, end: i64, h: &ElispHost) -> (usize, usize) {
    let base = match obj {
        PropObj::Str(_) => 0,
        PropObj::Buf(_) => 1,
    };
    let s = (start - base).max(0);
    let e = (end - base).max(0);
    let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
    let _ = h;
    (lo as usize, hi as usize)
}
/// Run `f` with the current buffer temporarily set to `bi` (restored after).
fn with_buffer<T>(h: &mut ElispHost, bi: usize, f: impl FnOnce(&mut ElispHost) -> T) -> T {
    let saved = h.current;
    h.current = bi;
    let r = f(h);
    h.current = saved;
    r
}
fn put_text_property_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let start = as_int(h, &a[0])?;
    let end = as_int(h, &a[1])?;
    let (prop, val) = (a[2].clone(), a[3].clone());
    let obj = prop_object(h, a.get(4))?;
    let (lo, hi) = prop_range(&obj, start, end, h);
    match obj {
        PropObj::Str(s) => h.string_put_prop(&s, lo, hi, &prop, &val),
        PropObj::Buf(bi) => with_buffer(h, bi, |h| h.buffer_put_prop(lo, hi, &prop, &val)),
    }
    Ok(Value::Undef)
}
fn set_text_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let start = as_int(h, &a[0])?;
    let end = as_int(h, &a[1])?;
    let plist = a[2].clone();
    let obj = prop_object(h, a.get(3))?;
    let (lo, hi) = prop_range(&obj, start, end, h);
    match obj {
        PropObj::Str(s) => h.string_set_props(&s, lo, hi, &plist),
        PropObj::Buf(bi) => with_buffer(h, bi, |h| h.buffer_set_props(lo, hi, &plist)),
    }
    Ok(Value::Undef)
}
/// Apply each `(prop val)` pair of PROPS via `f` (for add/remove).
fn each_prop_pair(h: &ElispHost, props: &Value) -> Vec<(Value, Value)> {
    let mut out = Vec::new();
    let mut cur = props.clone();
    while let Some(Obj::Cons(k, d)) = h.obj(&cur) {
        let k = k.clone();
        let rest = d.clone();
        match h.obj(&rest) {
            Some(Obj::Cons(v, d2)) => {
                out.push((k, v.clone()));
                cur = d2.clone();
            }
            _ => break,
        }
    }
    out
}
fn add_text_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let start = as_int(h, &a[0])?;
    let end = as_int(h, &a[1])?;
    let obj = prop_object(h, a.get(3))?;
    let (lo, hi) = prop_range(&obj, start, end, h);
    for (prop, val) in each_prop_pair(h, &a[2]) {
        match &obj {
            PropObj::Str(s) => h.string_put_prop(s, lo, hi, &prop, &val),
            PropObj::Buf(bi) => {
                let bi = *bi;
                with_buffer(h, bi, |h| h.buffer_put_prop(lo, hi, &prop, &val));
            }
        }
    }
    Ok(Value::Undef)
}
fn remove_text_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let start = as_int(h, &a[0])?;
    let end = as_int(h, &a[1])?;
    let obj = prop_object(h, a.get(3))?;
    let (lo, hi) = prop_range(&obj, start, end, h);
    for (prop, _) in each_prop_pair(h, &a[2]) {
        match &obj {
            PropObj::Str(s) => h.string_remove_prop(s, lo, hi, &prop),
            PropObj::Buf(bi) => {
                let bi = *bi;
                with_buffer(h, bi, |h| h.buffer_remove_prop(lo, hi, &prop));
            }
        }
    }
    Ok(Value::Undef)
}
/// `(propertize STRING &rest PROPS)` — a fresh copy of STRING carrying PROPS.
fn propertize_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let base = as_string(&a[0])?;
    let len = base.chars().count();
    let out = Value::str(base);
    // Build the plist from the trailing PROP VALUE pairs (in given order).
    let mut flat: Vec<Value> = Vec::new();
    let mut i = 1;
    while i + 1 < a.len() {
        flat.push(a[i].clone());
        flat.push(a[i + 1].clone());
        i += 2;
    }
    if !flat.is_empty() {
        let plist = h.list_from(flat);
        if let Value::Str(arc) = &out {
            h.string_set_props(arc, 0, len, &plist);
        }
    }
    Ok(out)
}
fn delete_region(h: &mut ElispHost, a: &[Value]) -> R {
    let len = h.cur_buf().text.len() as i64;
    let s = as_int(h, &a[0])?.clamp(1, len + 1);
    let e = as_int(h, &a[1])?.clamp(1, len + 1);
    let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
    h.cur_delete(lo as usize, hi as usize);
    Ok(Value::Undef)
}
fn insert_file_contents(h: &mut ElispHost, a: &[Value]) -> R {
    let raw = as_string(&a[0])?;
    let content = std::fs::read_to_string(fs_expand(&raw))
        .map_err(|_| format!("file-missing: Opening input file: No such file: {raw}"))?;
    let chars: Vec<char> = content.chars().collect();
    let n = chars.len() as i64;
    h.cur_insert(chars, false); // leaves point at the beginning of the inserted text
    Ok(h.list_from(vec![Value::str(raw), Value::Int(n)]))
}

// ── buffer motion ──
/// Move point by DELTA characters, faithful to `Fforward_char` (cmds.c): on
/// overshoot, point is set to the boundary (BEGV/ZV) *and* the corresponding
/// `beginning-of-buffer`/`end-of-buffer` condition is signaled.
fn move_point_by(h: &mut ElispHost, delta: i64) -> R {
    let buf = h.cur_buf();
    let target = buf.point as i64 + delta;
    if target < buf.begv as i64 {
        buf.point = buf.begv;
        return Err("beginning-of-buffer".to_string());
    }
    if target > buf.zv as i64 {
        buf.point = buf.zv;
        return Err("end-of-buffer".to_string());
    }
    buf.point = target as usize;
    Ok(Value::Undef)
}
fn forward_char(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_int(h, v)?,
        _ => 1,
    };
    move_point_by(h, n)
}
fn backward_char(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_int(h, v)?,
        _ => 1,
    };
    move_point_by(h, -n)
}
/// 1-based position of the beginning of POINT's line, not before `begv`.
fn bol_of(t: &[char], point: usize, begv: usize) -> usize {
    let mut p = point;
    while p > begv && t[p - 2] != '\n' {
        p -= 1;
    }
    p
}
/// 1-based position of the end of POINT's line (before the newline / at `zv`).
fn eol_of(t: &[char], point: usize, zv: usize) -> usize {
    let mut p = point;
    while p < zv && t[p - 1] != '\n' {
        p += 1;
    }
    p
}
fn beginning_of_line(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    buf.point = bol_of(&buf.text, buf.point, buf.begv);
    Ok(Value::Undef)
}
fn end_of_line(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    buf.point = eol_of(&buf.text, buf.point, buf.zv);
    Ok(Value::Undef)
}
/// 1-based start of the line N-1 lines forward (N<1 = backward) from POINT's line,
/// bounded by the accessible region `[begv, zv]`.
fn bol_after_lines(t: &[char], point: usize, n: i64, begv: usize, zv: usize) -> usize {
    let mut p = bol_of(t, point, begv);
    let mut k = n;
    while k > 0 {
        let e = eol_of(t, p, zv);
        if e < zv {
            p = e + 1;
        } else {
            p = e;
            break;
        }
        k -= 1;
    }
    while k < 0 && p > begv {
        p = bol_of(t, p - 1, begv);
        k += 1;
    }
    p
}
fn line_beginning_position(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_int(h, v)?,
        _ => 1,
    };
    let buf = h.cur_buf();
    Ok(Value::Int(
        bol_after_lines(&buf.text, buf.point, n - 1, buf.begv, buf.zv) as i64,
    ))
}
fn line_end_position(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_int(h, v)?,
        _ => 1,
    };
    let buf = h.cur_buf();
    let bol = bol_after_lines(&buf.text, buf.point, n - 1, buf.begv, buf.zv);
    Ok(Value::Int(eol_of(&buf.text, bol, buf.zv) as i64))
}
fn bolp(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    Ok(nil_or(
        buf.point == buf.begv || buf.text[buf.point - 2] == '\n',
    ))
}
fn eolp(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    Ok(nil_or(
        buf.point == buf.zv || buf.text[buf.point - 1] == '\n',
    ))
}
fn bobp(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    Ok(nil_or(buf.point == buf.begv))
}
fn eobp(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    Ok(nil_or(buf.point == buf.zv))
}
fn forward_line(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_int(h, v)?,
        _ => 1,
    };
    let buf = h.cur_buf();
    let len = buf.zv - 1;
    let mut p = buf.point;
    let mut short = 0i64;
    if n >= 0 {
        let mut moved = 0;
        while moved < n {
            let mut q = p;
            while q <= len && buf.text[q - 1] != '\n' {
                q += 1;
            }
            if q > len {
                // No newline before eob: land at eob; the partial line counts as
                // one not-fully-moved line unless we were already at bol/eob.
                short = n - moved - if p <= len { 1 } else { 0 };
                p = len + 1;
                break;
            }
            p = q + 1;
            moved += 1;
        }
    } else {
        p = bol_of(&buf.text, p, buf.begv);
        let mut moved = 0;
        while moved < -n {
            if p == buf.begv {
                short = -n - moved;
                break;
            }
            p = bol_of(&buf.text, p - 1, buf.begv);
            moved += 1;
        }
    }
    buf.point = p;
    Ok(Value::Int(short))
}

// ── buffer search (sets buffer-position match data) ──
fn set_buf_match(h: &mut ElispHost, spans0: &[Option<(usize, usize)>], text: String) {
    let spans = spans0
        .iter()
        .map(|o| o.map(|(b, e)| (b + 1, e + 1)))
        .collect();
    h.match_data = Some(MatchData {
        subject: text,
        spans,
        from_buffer: true,
    });
}
fn search_forward(h: &mut ElispHost, a: &[Value]) -> R {
    let needle: Vec<char> = as_string(&a[0])?.chars().collect();
    let len = h.cur_buf().text.len();
    let bound = match a.get(1) {
        Some(v) if !is_nil(v) => (as_int(h, v)?.max(0) as usize).min(len + 1),
        _ => len + 1,
    };
    let noerror = a.get(2).is_some_and(|v| !is_nil(v));
    let start = h.cur_buf().point - 1;
    let nlen = needle.len();
    let found = {
        let hay = &h.cur_buf().text;
        let mut res = None;
        let mut i = start;
        // match must end at or before bound-1 (0-based) => i+nlen <= bound-1
        while i + nlen <= (bound - 1).max(start) || (nlen == 0 && i == start) {
            if i + nlen <= len && hay[i..i + nlen] == needle[..] {
                res = Some(i);
                break;
            }
            if nlen == 0 {
                res = Some(i);
                break;
            }
            i += 1;
        }
        res
    };
    match found {
        Some(i) => {
            let end = i + nlen;
            let text: String = h.cur_buf().text.iter().collect();
            h.cur_buf().point = end + 1;
            set_buf_match(h, &[Some((i, end))], text);
            Ok(Value::Int((end + 1) as i64))
        }
        None if noerror => Ok(Value::Undef),
        None => Err(format!("search-failed: {}", as_string(&a[0])?)),
    }
}
fn re_search_forward(h: &mut ElispHost, a: &[Value]) -> R {
    let pat = as_string(&a[0])?;
    let re = compile_cf(&pat, case_fold_search(h))?;
    let bound = match a.get(1) {
        Some(v) if !is_nil(v) => Some(as_int(h, v)?.max(0) as usize),
        _ => None,
    };
    let noerror = a.get(2).is_some_and(|v| !is_nil(v));
    let text: String = h.cur_buf().text.iter().collect();
    let start_char = h.cur_buf().point - 1;
    let m = run_match(&re, &text, start_char).filter(|spans| {
        spans[0]
            .map(|(_, e)| bound.is_none_or(|b| e < b))
            .unwrap_or(false)
    });
    match m {
        Some(spans0) => {
            let endc = spans0[0].unwrap().1;
            h.cur_buf().point = endc + 1;
            set_buf_match(h, &spans0, text);
            Ok(Value::Int((endc + 1) as i64))
        }
        None if noerror => Ok(Value::Undef),
        None => Err(format!("search-failed: {pat}")),
    }
}
fn looking_at(h: &mut ElispHost, a: &[Value]) -> R {
    let re = compile_cf(&as_string(&a[0])?, case_fold_search(h))?;
    let text: String = h.cur_buf().text.iter().collect();
    let start_char = h.cur_buf().point - 1;
    match run_match(&re, &text, start_char) {
        Some(spans0) if spans0[0].map(|(b, _)| b == start_char).unwrap_or(false) => {
            set_buf_match(h, &spans0, text);
            Ok(Value::Bool(true))
        }
        _ => Ok(Value::Undef),
    }
}
fn looking_at_p(h: &mut ElispHost, a: &[Value]) -> R {
    let saved = h.match_data.take();
    let r = looking_at(h, a);
    h.match_data = saved;
    r
}
/// Expand NEWTEXT's `\&` (whole match), `\N` (group N), and `\\` escapes using
/// GT, an accessor returning the text of group N.
fn expand_repl(newtext: &str, gt: &dyn Fn(usize) -> String) -> String {
    let chars: Vec<char> = newtext.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let c = chars[i + 1];
            if c == '&' {
                out.push_str(&gt(0));
            } else if c.is_ascii_digit() {
                out.push_str(&gt(c as usize - '0' as usize));
            } else {
                out.push(c);
            }
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}
/// `(replace-match NEWTEXT &optional FIXEDCASE LITERAL STRING SUBEXP)`. With a
/// STRING argument, returns a new string with the last `string-match` of STRING
/// replaced; otherwise edits the current buffer and leaves point after the
/// replacement. Expands `\&`/`\N`/`\\` unless LITERAL, adapts case unless
/// FIXEDCASE.
fn replace_match(h: &mut ElispHost, a: &[Value]) -> R {
    let newtext = as_string(&a[0])?;
    let fixedcase = !matches!(
        a.get(1),
        Some(Value::Undef) | Some(Value::Bool(false)) | None
    );
    let literal = !matches!(
        a.get(2),
        Some(Value::Undef) | Some(Value::Bool(false)) | None
    );
    let subexp = match a.get(4) {
        Some(v) if !is_nil(v) => as_int(h, v)?.max(0) as usize,
        _ => 0,
    };
    let spans = {
        let md = h
            .match_data
            .as_ref()
            .ok_or("args-out-of-range: no match data".to_string())?;
        md.spans.clone()
    };
    // STRING mode: spans are 0-based char indices into STRING; return a new string.
    if let Some(Value::Str(s)) = a.get(3) {
        let subject: Vec<char> = s.chars().collect();
        let (b, e) = spans
            .get(subexp)
            .copied()
            .flatten()
            .ok_or("args-out-of-range: no such subexpression".to_string())?;
        let gt = |n: usize| -> String {
            spans
                .get(n)
                .copied()
                .flatten()
                .map(|(gb, ge)| subject[gb..ge].iter().collect::<String>())
                .unwrap_or_default()
        };
        let matched = gt(subexp);
        let rep = if literal {
            newtext
        } else {
            expand_repl(&newtext, &gt)
        };
        let rep = if fixedcase {
            rep
        } else {
            adapt_replacement_case(&matched, rep)
        };
        let mut out: String = subject[..b].iter().collect();
        out.push_str(&rep);
        out.extend(&subject[e..]);
        return Ok(Value::str(out));
    }
    let text: Vec<char> = h.cur_buf().text.clone();
    let (b, e) = spans
        .get(subexp)
        .copied()
        .flatten()
        .ok_or("args-out-of-range: no such subexpression".to_string())?;
    let gt = |n: usize| -> String {
        spans
            .get(n)
            .copied()
            .flatten()
            .map(|(gb, ge)| text[(gb - 1)..(ge - 1)].iter().collect::<String>())
            .unwrap_or_default()
    };
    let matched = gt(subexp);
    let rep = if literal {
        newtext
    } else {
        expand_repl(&newtext, &gt)
    };
    let rep = if fixedcase {
        rep
    } else {
        adapt_replacement_case(&matched, rep)
    };
    let rep_chars: Vec<char> = rep.chars().collect();
    // Delete the matched span, then insert the replacement at its start (point is
    // left after the replacement, matching Emacs and adjusting markers/narrowing).
    h.cur_buf().point = b;
    h.cur_delete(b, e);
    h.cur_insert(rep_chars, true);
    Ok(Value::Undef)
}

// ── filesystem writes / mutations ──
fn write_region(h: &mut ElispHost, a: &[Value]) -> R {
    let append = a.get(3).is_some_and(|v| !is_nil(v));
    // START may be a string (write it directly) or a buffer position.
    let content: String = match &a[0] {
        Value::Str(s) => s.to_string(),
        _ => {
            let len = h.cur_buf_ref().text.len() as i64;
            let s = as_int(h, &a[0])?.clamp(1, len + 1);
            let e = match a.get(1) {
                Some(v) if !is_nil(v) => as_int(h, v)?.clamp(1, len + 1),
                _ => len + 1,
            };
            let buf = h.cur_buf();
            let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
            buf.text[(lo - 1) as usize..(hi - 1) as usize]
                .iter()
                .collect()
        }
    };
    let filename = as_string(&a[2])?;
    let path = fs_expand(&filename);
    let res = if append {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(content.as_bytes()))
    } else {
        std::fs::write(&path, content.as_bytes())
    };
    res.map_err(|_| format!("file-error: Opening output file: {filename}"))?;
    Ok(Value::Undef)
}
fn delete_file(_h: &mut ElispHost, a: &[Value]) -> R {
    let f = as_string(&a[0])?;
    std::fs::remove_file(fs_expand(&f)).map_err(|_| format!("file-error: Removing file: {f}"))?;
    Ok(Value::Undef)
}
fn make_directory(_h: &mut ElispHost, a: &[Value]) -> R {
    let f = as_string(&a[0])?;
    let parents = a.get(1).is_some_and(|v| !is_nil(v));
    let p = fs_expand(&f);
    let r = if parents {
        std::fs::create_dir_all(&p)
    } else {
        std::fs::create_dir(&p)
    };
    r.map_err(|_| format!("file-error: Creating directory: {f}"))?;
    Ok(Value::Undef)
}
fn rename_file(_h: &mut ElispHost, a: &[Value]) -> R {
    let (o, n) = (as_string(&a[0])?, as_string(&a[1])?);
    std::fs::rename(fs_expand(&o), fs_expand(&n))
        .map_err(|_| format!("file-error: Renaming: {o}"))?;
    Ok(Value::Undef)
}
fn copy_file(_h: &mut ElispHost, a: &[Value]) -> R {
    let (o, n) = (as_string(&a[0])?, as_string(&a[1])?);
    std::fs::copy(fs_expand(&o), fs_expand(&n)).map_err(|_| format!("file-error: Copying: {o}"))?;
    Ok(Value::Undef)
}

// ── subprocesses ──
fn shell_command_to_string(_h: &mut ElispHost, a: &[Value]) -> R {
    let cmd = as_string(&a[0])?;
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .output()
    {
        Ok(o) => Ok(Value::str(String::from_utf8_lossy(&o.stdout).into_owned())),
        Err(_) => Ok(Value::str(String::new())),
    }
}
fn call_process(h: &mut ElispHost, a: &[Value]) -> R {
    let program = as_string(&a[0])?;
    // a[1] INFILE and a[3] DISPLAY are ignored; a[2] DESTINATION; a[4..] ARGS.
    let args: Vec<String> = a
        .get(4..)
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| as_string(v).ok())
        .collect();
    let insert = matches!(a.get(2), Some(v) if !is_nil(v));
    let out = std::process::Command::new(&program)
        .args(&args)
        .output()
        .map_err(|_| format!("file-error: Searching for program: {program}"))?;
    if insert {
        let chars: Vec<char> = String::from_utf8_lossy(&out.stdout).chars().collect();
        h.cur_insert(chars, true);
    }
    Ok(Value::Int(out.status.code().unwrap_or(-1) as i64))
}
fn process_lines(h: &mut ElispHost, a: &[Value]) -> R {
    let program = as_string(&a[0])?;
    let args: Vec<String> = a
        .get(1..)
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| as_string(v).ok())
        .collect();
    let out = std::process::Command::new(&program)
        .args(&args)
        .output()
        .map_err(|_| format!("file-error: Searching for program: {program}"))?;
    if !out.status.success() {
        return Err(format!("error: {program} exited with non-zero status"));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let mut lines: Vec<&str> = s.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    Ok(h.list_from(lines.into_iter().map(Value::str).collect()))
}

// ── more buffer editing/motion ──
fn char_before(h: &mut ElispHost, a: &[Value]) -> R {
    let arg = match a.first() {
        Some(v) if !is_nil(v) => Some(as_int(h, v)? as usize),
        _ => None,
    };
    let buf = h.cur_buf();
    let pos = arg.unwrap_or(buf.point);
    // The char at pos-1, only when pos-1 is inside the accessible region.
    Ok(if pos > buf.begv && pos <= buf.zv {
        Value::Int(buf.text[pos - 2] as i64)
    } else {
        Value::Undef
    })
}
fn delete_char(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_int(h, &a[0])?;
    let point = h.cur_buf().point;
    let zv = h.cur_buf().zv;
    let begv = h.cur_buf().begv;
    if n >= 0 {
        let end = (point + n as usize).min(zv);
        h.cur_delete(point, end);
    } else {
        let start = (point as i64 + n).max(begv as i64) as usize;
        h.cur_delete(start, point);
    }
    Ok(Value::Undef)
}
fn insert_char(h: &mut ElispHost, a: &[Value]) -> R {
    let c = char::from_u32(as_int(h, &a[0])? as u32).unwrap_or('\u{fffd}');
    let count = match a.get(1) {
        Some(v) if !is_nil(v) => as_int(h, v)?.max(0) as usize,
        _ => 1,
    };
    h.cur_insert(vec![c; count], true);
    Ok(Value::Undef)
}
fn count_lines(h: &mut ElispHost, a: &[Value]) -> R {
    let len = h.cur_buf_ref().text.len() as i64;
    let s = as_int(h, &a[0])?.clamp(1, len + 1);
    let e = as_int(h, &a[1])?.clamp(1, len + 1);
    let buf = h.cur_buf();
    let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
    let region = &buf.text[(lo - 1) as usize..(hi - 1) as usize];
    let nl = region.iter().filter(|&&c| c == '\n').count();
    // Count the final partial line (region non-empty and not ending in newline).
    let extra = if !region.is_empty() && region[region.len() - 1] != '\n' {
        1
    } else {
        0
    };
    Ok(Value::Int((nl + extra) as i64))
}
fn line_number_at_pos(h: &mut ElispHost, a: &[Value]) -> R {
    let arg = match a.first() {
        Some(v) if !is_nil(v) => Some(as_int(h, v)? as usize),
        _ => None,
    };
    let buf = h.cur_buf();
    let pos = arg.unwrap_or(buf.point);
    let upto = (pos.saturating_sub(1)).min(buf.text.len());
    let n = buf.text[..upto].iter().filter(|&&c| c == '\n').count();
    Ok(Value::Int(n as i64 + 1))
}
fn current_column(h: &mut ElispHost, _a: &[Value]) -> R {
    let buf = h.cur_buf();
    // Expand tabs to the next multiple of tab-width (8) like Emacs.
    let bol = bol_of(&buf.text, buf.point, buf.begv);
    let mut col = 0usize;
    for i in bol..buf.point {
        if buf.text[i - 1] == '\t' {
            col = (col / 8 + 1) * 8;
        } else {
            col += 1;
        }
    }
    Ok(Value::Int(col as i64))
}
fn search_backward(h: &mut ElispHost, a: &[Value]) -> R {
    let needle: Vec<char> = as_string(&a[0])?.chars().collect();
    let bound = match a.get(1) {
        Some(v) if !is_nil(v) => (as_int(h, v)?.max(1) as usize) - 1,
        _ => 0,
    };
    let noerror = a.get(2).is_some_and(|v| !is_nil(v));
    let point = h.cur_buf().point;
    let nlen = needle.len();
    let found = {
        let hay = &h.cur_buf().text;
        let mut res = None;
        if nlen == 0 {
            res = Some(point - 1);
        } else if point > nlen {
            let mut i = point - 1 - nlen; // max start so match ends at point-1
            loop {
                if hay[i..i + nlen] == needle[..] {
                    res = Some(i);
                    break;
                }
                if i <= bound {
                    break;
                }
                i -= 1;
            }
        }
        res
    };
    match found {
        Some(i) => {
            let text: String = h.cur_buf().text.iter().collect();
            h.cur_buf().point = i + 1;
            set_buf_match(h, &[Some((i, i + nlen))], text);
            Ok(Value::Int((i + 1) as i64))
        }
        None if noerror => Ok(Value::Undef),
        None => Err(format!("search-failed: {}", as_string(&a[0])?)),
    }
}
fn re_search_backward(h: &mut ElispHost, a: &[Value]) -> R {
    let pat = as_string(&a[0])?;
    let re = compile_cf(&pat, case_fold_search(h))?;
    let noerror = a.get(2).is_some_and(|v| !is_nil(v));
    let text: String = h.cur_buf().text.iter().collect();
    let point_char = h.cur_buf().point - 1;
    // Last non-overlapping match that ends at or before point.
    let mut best: Option<Vec<Option<(usize, usize)>>> = None;
    let mut from = 0;
    while let Some(spans) = run_match(&re, &text, from) {
        let (b, e) = spans[0].unwrap();
        if e > point_char {
            break;
        }
        best = Some(spans.clone());
        from = if e > b { e } else { e + 1 };
    }
    match best {
        Some(spans0) => {
            let bc = spans0[0].unwrap().0;
            h.cur_buf().point = bc + 1;
            set_buf_match(h, &spans0, text);
            Ok(Value::Int((bc + 1) as i64))
        }
        None if noerror => Ok(Value::Undef),
        None => Err(format!("search-failed: {pat}")),
    }
}
fn parse_char_set(spec: &str) -> (bool, Vec<(char, char)>) {
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;
    let neg = chars.first() == Some(&'^');
    if neg {
        i = 1;
    }
    let mut ranges = Vec::new();
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            ranges.push((chars[i], chars[i + 2]));
            i += 3;
        } else {
            ranges.push((chars[i], chars[i]));
            i += 1;
        }
    }
    (neg, ranges)
}
fn in_char_set(c: char, ranges: &[(char, char)], neg: bool) -> bool {
    let m = ranges.iter().any(|&(a, b)| c >= a && c <= b);
    m != neg
}
fn skip_chars_forward(h: &mut ElispHost, a: &[Value]) -> R {
    let (neg, ranges) = parse_char_set(&as_string(&a[0])?);
    let buf = h.cur_buf();
    let start = buf.point;
    while buf.point <= buf.text.len() && in_char_set(buf.text[buf.point - 1], &ranges, neg) {
        buf.point += 1;
    }
    Ok(Value::Int((buf.point - start) as i64))
}
fn skip_chars_backward(h: &mut ElispHost, a: &[Value]) -> R {
    let (neg, ranges) = parse_char_set(&as_string(&a[0])?);
    let buf = h.cur_buf();
    let start = buf.point;
    while buf.point > 1 && in_char_set(buf.text[buf.point - 2], &ranges, neg) {
        buf.point -= 1;
    }
    Ok(Value::Int(buf.point as i64 - start as i64))
}
fn forward_word(h: &mut ElispHost, a: &[Value]) -> R {
    // Word = run of alphanumerics (no syntax tables).
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_int(h, v)?,
        _ => 1,
    };
    let buf = h.cur_buf();
    let len = buf.text.len();
    for _ in 0..n {
        while buf.point <= len && !buf.text[buf.point - 1].is_alphanumeric() {
            buf.point += 1;
        }
        while buf.point <= len && buf.text[buf.point - 1].is_alphanumeric() {
            buf.point += 1;
        }
    }
    Ok(Value::Bool(true))
}
fn backward_word(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_int(h, v)?,
        _ => 1,
    };
    let buf = h.cur_buf();
    for _ in 0..n {
        while buf.point > 1 && !buf.text[buf.point - 2].is_alphanumeric() {
            buf.point -= 1;
        }
        while buf.point > 1 && buf.text[buf.point - 2].is_alphanumeric() {
            buf.point -= 1;
        }
    }
    Ok(Value::Bool(true))
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
    s("car-less-than-car", 2, Some(2), car_less_than_car);
    s("list", 0, None, list_fn);
    s("append", 0, None, append_fn);
    s("reverse", 1, Some(1), reverse_fn);
    s("length", 1, Some(1), length_fn);
    s("nth", 2, Some(2), nth_fn);
    // c[ad]+r combinators (3-level completers + cl-lib 2-level aliases)
    s("caadr", 1, Some(1), caadr);
    s("cadar", 1, Some(1), cadar);
    s("cdaar", 1, Some(1), cdaar);
    s("cdadr", 1, Some(1), cdadr);
    s("cddar", 1, Some(1), cddar);
    s("cl-caar", 1, Some(1), cl_caar);
    s("cl-cadr", 1, Some(1), cl_cadr);
    s("cl-cdar", 1, Some(1), cl_cdar);
    s("cl-cddr", 1, Some(1), cl_cddr);
    // vectors
    s("vector", 0, None, vector_fn);
    s("make-vector", 2, Some(2), make_vector);
    s("aref", 2, Some(2), aref);
    s("aset", 3, Some(3), aset);
    s("fillarray", 2, Some(2), fillarray);
    s("make-char-table--new", 3, Some(3), make_char_table_new);
    s("char-table-p", 1, Some(1), char_table_p);
    s("char-table-subtype", 1, Some(1), char_table_subtype);
    s("char-table-parent", 1, Some(1), char_table_parent);
    s("set-char-table-parent", 2, Some(2), set_char_table_parent);
    s("char-table-extra-slot", 2, Some(2), char_table_extra_slot);
    s(
        "set-char-table-extra-slot",
        3,
        Some(3),
        set_char_table_extra_slot,
    );
    s("char-table-range", 2, Some(2), char_table_range);
    s("set-char-table-range", 3, Some(3), set_char_table_range);
    // symbols
    s("symbol-name", 1, Some(1), symbol_name);
    s("intern", 1, Some(2), intern_fn);
    s("make-symbol", 1, Some(1), make_symbol_fn);
    s("set", 2, Some(2), set_fn);
    s("symbol-value", 1, Some(1), symbol_value);
    s("boundp", 1, Some(1), boundp);
    s("default-boundp", 1, Some(1), default_boundp);
    s("default-toplevel-value", 1, Some(1), default_toplevel_value);
    s("bare-symbol-p", 1, Some(1), bare_symbol_p);
    s("makunbound", 1, Some(1), makunbound);
    s("sha1", 1, Some(4), sha1_fn);
    s("md5", 1, Some(5), md5_fn);
    s("secure-hash", 2, Some(5), secure_hash);
    s("base64-encode-string", 1, Some(2), base64_encode_string);
    s("base64-decode-string", 1, Some(3), base64_decode_string);
    s(
        "base64url-encode-string",
        1,
        Some(2),
        base64url_encode_string,
    );
    s(
        "base64url-decode-string",
        1,
        Some(2),
        base64url_decode_string,
    );
    s("url-hexify-string", 1, Some(2), url_hexify_string);
    s("url-unhex-string", 1, Some(2), url_unhex_string);
    s("fset", 2, Some(2), fset);
    s("fboundp", 1, Some(1), fboundp);
    s("indirect-function", 1, Some(2), indirect_function);
    // functional (funcall/apply/mapcar/mapc are handled in host::call_function)
    s("identity", 1, Some(1), identity);
    s("terpri", 0, Some(1), terpri);
    s("print", 1, Some(2), print_fn);
    s("prin1-to-string", 1, Some(1), prin1_to_string);
    // nonlocal exits (catch/unwind-protect/condition-case are compiler intrinsics)
    s("throw", 2, Some(2), throw_fn);
    s("error", 1, None, error_fn);
    s("user-error", 1, None, user_error_fn);
    s("signal", 2, Some(2), signal_fn);
    // hash tables (maphash is intercepted in host::call_function)
    s("make-hash-table", 0, None, make_hash_table);
    s("gethash", 2, Some(3), gethash);
    s("puthash", 3, Some(3), puthash);
    s("remhash", 2, Some(2), remhash);
    s("clrhash", 1, Some(1), clrhash);
    s("hash-table-count", 1, Some(1), hash_table_count);
    s("hash-table-test", 1, Some(1), hash_table_test);
    s("hash-table-p", 1, Some(1), hash_table_p);
    s("hash-table-keys", 1, Some(1), hash_table_keys);
    s("hash-table-values", 1, Some(1), hash_table_values);
    s("copy-hash-table", 1, Some(1), copy_hash_table);
    // time
    s("getenv", 1, Some(2), getenv_fn);
    s("setenv", 1, Some(3), setenv_fn);
    s("special-variable-p", 1, Some(1), special_variable_p);
    s("func-arity", 1, Some(1), func_arity);
    s("subr-arity", 1, Some(1), func_arity);
    s("subr-name", 1, Some(1), subr_name);
    s("--current-directory--", 0, Some(0), current_directory);
    s("file-exists-p", 1, Some(1), file_exists_p);
    s("file-directory-p", 1, Some(1), file_directory_p);
    s("file-regular-p", 1, Some(1), file_regular_p);
    s("file-readable-p", 1, Some(1), file_readable_p);
    s("file-writable-p", 1, Some(1), file_writable_p);
    s("file-symlink-p", 1, Some(1), file_symlink_p);
    s("--directory-files--", 1, Some(3), directory_files_raw);
    // buffer registry
    s("bufferp", 1, Some(1), bufferp);
    s("current-buffer", 0, Some(0), current_buffer_fn);
    s("set-buffer", 1, Some(1), set_buffer_fn);
    s("get-buffer", 1, Some(1), get_buffer);
    s("get-buffer-create", 1, Some(2), get_buffer_create);
    s("generate-new-buffer", 1, Some(3), generate_new_buffer);
    s(
        "generate-new-buffer-name",
        1,
        Some(2),
        generate_new_buffer_name,
    );
    s("buffer-name", 0, Some(1), buffer_name);
    s("buffer-live-p", 1, Some(1), buffer_live_p);
    s("kill-buffer", 0, Some(1), kill_buffer);
    s("rename-buffer", 1, Some(2), rename_buffer);
    s("buffer-list", 0, Some(1), buffer_list);
    // mark + narrowing
    s("set-mark", 1, Some(1), set_mark_fn);
    s("mark", 0, Some(1), mark_fn);
    s("region-beginning", 0, Some(0), region_beginning);
    s("region-end", 0, Some(0), region_end);
    s("narrow-to-region", 2, Some(2), narrow_to_region);
    s("widen", 0, Some(0), widen_fn);
    s("--se-push--", 0, Some(0), se_push);
    s("--se-pop--", 0, Some(0), se_pop);
    s("--save-restriction--", 0, Some(0), save_restriction_push);
    s("--restore-restriction--", 0, Some(0), restore_restriction);
    // buffer-local variables
    s("make-local-variable", 1, Some(1), make_local_variable);
    s(
        "make-variable-buffer-local",
        1,
        Some(1),
        make_variable_buffer_local,
    );
    s("local-variable-p", 1, Some(2), local_variable_p);
    s(
        "local-variable-if-set-p",
        1,
        Some(2),
        local_variable_if_set_p,
    );
    s("kill-local-variable", 1, Some(1), kill_local_variable);
    s(
        "--buffer-local-symbols--",
        0,
        Some(0),
        buffer_local_symbols_fn,
    );
    s("buffer-local-value", 2, Some(2), buffer_local_value_fn);
    s("default-value", 1, Some(1), default_value_fn);
    s("set-default", 2, Some(2), set_default_fn);
    s("use-local-map", 1, Some(1), use_local_map_fn);
    s("current-local-map", 0, Some(0), current_local_map_fn);
    s("insert", 0, None, insert_fn);
    s("buffer-string", 0, Some(0), buffer_string);
    s("buffer-size", 0, Some(1), buffer_size);
    s("point", 0, Some(0), point_fn);
    s("point-min", 0, Some(0), point_min);
    s("point-max", 0, Some(0), point_max);
    s("goto-char", 1, Some(1), goto_char);
    s("erase-buffer", 0, Some(0), erase_buffer);
    s("char-after", 0, Some(1), char_after);
    s("buffer-substring", 2, Some(2), buffer_substring);
    s(
        "buffer-substring-no-properties",
        2,
        Some(2),
        buffer_substring_no_properties,
    );
    s("delete-region", 2, Some(2), delete_region);
    // markers
    s("make-marker", 0, Some(0), make_marker);
    s("point-marker", 0, Some(0), point_marker);
    s("markerp", 1, Some(1), markerp_fn);
    s("marker-position", 1, Some(1), marker_position_fn);
    s("marker-buffer", 1, Some(1), marker_buffer_fn);
    s(
        "marker-insertion-type",
        1,
        Some(1),
        marker_insertion_type_fn,
    );
    s(
        "set-marker-insertion-type",
        2,
        Some(2),
        set_marker_insertion_type_fn,
    );
    s("set-marker", 2, Some(3), set_marker_fn);
    s("move-marker", 2, Some(3), set_marker_fn);
    s("copy-marker", 0, Some(2), copy_marker_fn);
    // text properties
    s("get-text-property", 2, Some(3), get_text_property_fn);
    s("text-properties-at", 1, Some(2), text_properties_at_fn);
    s("put-text-property", 4, Some(5), put_text_property_fn);
    s("set-text-properties", 3, Some(4), set_text_properties_fn);
    s("add-text-properties", 3, Some(4), add_text_properties_fn);
    s(
        "remove-text-properties",
        3,
        Some(4),
        remove_text_properties_fn,
    );
    s("propertize", 1, None, propertize_fn);
    s(
        "--insert-before-markers--",
        1,
        Some(1),
        insert_before_markers_fn,
    );
    s("insert-file-contents", 1, None, insert_file_contents);
    s("forward-char", 0, Some(1), forward_char);
    s("backward-char", 0, Some(1), backward_char);
    s("beginning-of-line", 0, Some(1), beginning_of_line);
    s("end-of-line", 0, Some(1), end_of_line);
    s(
        "line-beginning-position",
        0,
        Some(1),
        line_beginning_position,
    );
    s("line-end-position", 0, Some(1), line_end_position);
    s("pos-bol", 0, Some(1), line_beginning_position);
    s("pos-eol", 0, Some(1), line_end_position);
    s("bolp", 0, Some(0), bolp);
    s("eolp", 0, Some(0), eolp);
    s("bobp", 0, Some(0), bobp);
    s("eobp", 0, Some(0), eobp);
    s("forward-line", 0, Some(1), forward_line);
    s("search-forward", 1, Some(4), search_forward);
    s("re-search-forward", 1, Some(4), re_search_forward);
    s("looking-at", 1, Some(2), looking_at);
    s("looking-at-p", 1, Some(1), looking_at_p);
    s("replace-match", 1, Some(5), replace_match);
    // filesystem writes / mutations
    s("write-region", 3, Some(7), write_region);
    s("delete-file", 1, Some(2), delete_file);
    s("make-directory", 1, Some(2), make_directory);
    s("rename-file", 2, Some(3), rename_file);
    s("copy-file", 2, Some(6), copy_file);
    s(
        "shell-command-to-string",
        1,
        Some(1),
        shell_command_to_string,
    );
    s("call-process", 1, None, call_process);
    s("process-lines", 1, None, process_lines);
    s("char-before", 0, Some(1), char_before);
    s("delete-char", 1, Some(2), delete_char);
    s("insert-char", 1, Some(3), insert_char);
    s("count-lines", 2, Some(2), count_lines);
    s("line-number-at-pos", 0, Some(2), line_number_at_pos);
    s("current-column", 0, Some(0), current_column);
    s("search-backward", 1, Some(4), search_backward);
    s("re-search-backward", 1, Some(4), re_search_backward);
    s("skip-chars-forward", 1, Some(2), skip_chars_forward);
    s("skip-chars-backward", 1, Some(2), skip_chars_backward);
    s("forward-word", 0, Some(1), forward_word);
    s("backward-word", 0, Some(1), backward_word);
    s("random", 0, Some(1), random_fn);
    s("float-time", 0, Some(1), float_time);
    s("current-time", 0, Some(0), current_time);
    s("format-time-string", 1, Some(3), format_time_string);
    s("current-time-string", 0, Some(2), current_time_string);
    s("decode-time", 0, Some(3), decode_time);
    s("encode-time", 1, None, encode_time);
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
    s("--push-output-capture--", 0, Some(0), push_output_capture);
    s("--pop-output-capture--", 0, Some(0), pop_output_capture);
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
    s("lsh", 2, Some(2), lsh_fn);
    // parity: float math / parsing / introspection
    s("expt", 2, Some(2), expt_fn);
    s("sqrt", 1, Some(1), sqrt_fn);
    s("exp", 1, Some(1), exp_fn);
    s("log", 1, Some(2), log_fn);
    s("sin", 1, Some(1), sin_fn);
    s("cos", 1, Some(1), cos_fn);
    s("tan", 1, Some(1), tan_fn);
    s("asin", 1, Some(1), asin_fn);
    s("acos", 1, Some(1), acos_fn);
    s("atan", 1, Some(2), atan_fn);
    s("ldexp", 2, Some(2), ldexp_fn);
    s("copysign", 2, Some(2), copysign_fn);
    s("frexp", 1, Some(1), frexp_fn);
    s("isnan", 1, Some(1), isnan_fn);
    s("fround", 1, Some(1), fround_fn);
    s("ffloor", 1, Some(1), ffloor_fn);
    s("fceiling", 1, Some(1), fceiling_fn);
    s("ftruncate", 1, Some(1), ftruncate_fn);
    s("string-to-number", 1, Some(2), string_to_number);
    s("downcase", 1, Some(1), downcase_fn);
    s("upcase", 1, Some(1), upcase_fn);
    s("type-of", 1, Some(1), type_of);
    s("recordp", 1, Some(1), recordp);
    s("closurep", 1, Some(1), closurep_fn);
    s("oclosure--fix-type", 4, Some(4), oclosure_fix_type);
    s("oclosure-type", 1, Some(1), oclosure_type_fn);
    s("oclosure--get", 3, Some(3), oclosure_get_fn);
    s("oclosure--set", 3, Some(3), oclosure_set_fn);
    s("oclosure--copy", 2, None, oclosure_copy_fn);
    s("sxhash-equal", 1, Some(1), sxhash_equal_fn);
    s("sxhash", 1, Some(1), sxhash_equal_fn);
    s("sxhash-eq", 1, Some(1), sxhash_eq_fn);
    s("sxhash-eql", 1, Some(1), sxhash_eql_fn);
    s("cl-struct-p", 1, Some(1), recordp);
    s("functionp", 1, Some(1), functionp);
    s("char-or-string-p", 1, Some(1), char_or_string_p);
    s("char-equal", 2, Some(2), char_equal);
    s("vconcat", 0, None, vconcat_fn);
    s("string-to-vector", 1, Some(1), string_to_vector);
    s("abs", 1, Some(1), abs_fn);
    s("logcount", 1, Some(1), logcount_fn);
    s("symbol-function", 1, Some(1), symbol_function);
    s("intern-soft", 1, Some(1), intern_soft);
    s("subrp", 1, Some(1), subrp);
    s("macrop", 1, Some(1), macrop);
    s("special-form-p", 1, Some(1), special_form_p);
    s("char-uppercase-p", 1, Some(1), char_uppercase_p);
    s("string-distance", 2, Some(3), string_distance);
    s("logb", 1, Some(1), logb_fn);
    s("max-char", 0, Some(1), max_char);
    s("byteorder", 0, Some(0), byteorder);
    s("char-resolve-modifiers", 1, Some(1), char_resolve_modifiers);
    s("text-char-description", 1, Some(1), text_char_description);
    s(
        "unibyte-char-to-multibyte",
        1,
        Some(1),
        unibyte_char_to_multibyte,
    );
    s(
        "multibyte-char-to-unibyte",
        1,
        Some(1),
        multibyte_char_to_unibyte,
    );
    s("emacs-pid", 0, Some(0), emacs_pid);
    s("load-average", 0, Some(1), load_average);
    s("read", 1, Some(1), read_fn);
    s("read-from-string", 1, Some(3), read_from_string);
    s("compare-strings", 6, Some(7), compare_strings);
    // Emacs 28 alias for split-string with identical semantics (direct forwarder).
    s("string-split", 1, Some(4), split_string);
    s("member-ignore-case", 2, Some(2), member_ignore_case);
}

#[cfg(test)]
mod tests {
    use crate::{eval_str, print, reset_host};

    fn eval(src: &str) -> String {
        reset_host();
        let v = eval_str(src).expect("eval failed");
        print(&v, true)
    }

    fn eval_err(src: &str) -> String {
        reset_host();
        eval_str(src).unwrap_err()
    }

    #[test]
    fn cadr_family_composition() {
        // caadr = (car (car (cdr X)))
        assert_eq!(eval("(caadr '(1 (2 3) 4))"), "2");
        // cadar = (car (cdr (car X)))
        assert_eq!(eval("(cadar '((1 2 3) 4))"), "2");
        // cdaar = (cdr (car (car X)))
        assert_eq!(eval("(cdaar '(((1 2) 3) 4))"), "(2)");
        // cdadr = (cdr (car (cdr X)))
        assert_eq!(eval("(cdadr '(1 (2 3) 4))"), "(3)");
        // cddar = (cdr (cdr (car X)))
        assert_eq!(eval("(cddar '((1 2 3) 4))"), "(3)");
    }

    #[test]
    fn cadr_family_nil_edges() {
        // Intermediate nil propagates to nil (no error) on short lists.
        assert_eq!(eval("(caadr '(1))"), "nil");
        assert_eq!(eval("(cadar '(nil))"), "nil");
        assert_eq!(eval("(cddar '((1)))"), "nil");
        // A non-nil non-cons intermediate signals wrong-type-argument listp.
        assert!(eval_err("(caadr '(1 2 3))").contains("listp"));
    }

    #[test]
    fn cl_two_level_aliases() {
        assert_eq!(eval("(cl-caar '((1 2) 3))"), "1");
        assert_eq!(eval("(cl-cadr '(1 2 3))"), "2");
        assert_eq!(eval("(cl-cdar '((1 2) 3))"), "(2)");
        assert_eq!(eval("(cl-cddr '(1 2 3 4))"), "(3 4)");
        // Short/nil lists yield nil.
        assert_eq!(eval("(cl-cadr '(1))"), "nil");
        assert_eq!(eval("(cl-cddr '(1))"), "nil");
    }

    #[test]
    fn string_split_forwards_to_split_string() {
        // Default separators: whitespace, omit-nulls implicitly on.
        assert_eq!(eval("(string-split \"  a  b c \")"), "(\"a\" \"b\" \"c\")");
        // Empty string with default separators -> nil.
        assert_eq!(eval("(string-split \"\")"), "nil");
        // Explicit separator regexp, omit-nulls default off keeps empty fields.
        assert_eq!(eval("(string-split \"a,,b\" \",\")"), "(\"a\" \"\" \"b\")");
        // Empty separator splits into single characters.
        assert_eq!(eval("(string-split \"abc\" \"\")"), "(\"a\" \"b\" \"c\")");
    }

    #[test]
    fn member_ignore_case_semantics() {
        // Returns the tail beginning at the first case-insensitive string match.
        assert_eq!(
            eval("(member-ignore-case \"b\" '(\"A\" \"B\" \"C\"))"),
            "(\"B\" \"C\")"
        );
        // No match -> nil.
        assert_eq!(eval("(member-ignore-case \"z\" '(\"a\" \"b\"))"), "nil");
        // Non-string elements are skipped, never match.
        assert_eq!(eval("(member-ignore-case \"b\" '(1 2 \"B\"))"), "(\"B\")");
        // Empty list -> nil.
        assert_eq!(eval("(member-ignore-case \"a\" nil)"), "nil");
    }
}
