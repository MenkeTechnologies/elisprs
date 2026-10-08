//! Primitive subrs, written in Rust. Per the research inventory these are the
//! ~irreducible core; the large derived surface (caar.., seq-*, cl-*, alist
//! helpers) will be defined in an elisp prelude on top of these.

use crate::host::{
    bigint_to_f64, num_cmp, CharTable, ElHashTable, ElispHost, MatchData, Num, Obj, Resolved,
};
use fusevm::Value;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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
/// Approximate numeric accessor: `(fixnum, as-f64, is-float)`.
///
/// Kept for the float-valued builtins (`sqrt`, `sin`, `log`, `ffloor`, the `%e`/
/// `%f`/`%g` format directives …), which read only the `f64`. Anything whose
/// result must be *exact* — arithmetic, comparison, the bit ops — uses
/// [`as_number`] instead: a bignum cannot fit the `i64` field, and above 2^53 the
/// `f64` field is lossy.
fn as_num(h: &ElispHost, v: &Value) -> Result<(i64, f64, bool), String> {
    match v {
        Value::Int(n) => Ok((*n, *n as f64, false)),
        Value::Float(f) => Ok((*f as i64, *f, true)),
        _ => {
            // A bignum is exact only in `f64` here — every caller of this
            // accessor produces a float anyway.
            if let Some(b) = h.as_bigint(v) {
                let f = bigint_to_f64(&b);
                return Ok((f as i64, f, false));
            }
            // A marker coerces to its (integer) buffer position in arithmetic.
            match h.marker_position(v) {
                Some(p) => Ok((p as i64, p as f64, false)),
                None => Err(format!(
                    "wrong-type-argument: number-or-marker-p {}",
                    h.print(v, true)
                )),
            }
        }
    }
}
fn as_int(h: &ElispHost, v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        _ => match h.marker_position(v) {
            Some(p) => Ok(p as i64),
            None => Err(format!(
                "wrong-type-argument: integerp {}",
                h.print(v, true)
            )),
        },
    }
}
/// The string value of `v`, or Emacs's `(wrong-type-argument stringp X)` — with
/// X printed as `prin1` would, never as the raw heap handle the value carries.
fn as_string(h: &ElispHost, v: &Value) -> Result<String, String> {
    match h.str_text(v) {
        Some(s) => Ok(s.to_string()),
        None => Err(format!("wrong-type-argument: stringp {}", h.print(v, true))),
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
/// Emacs `most-positive-fixnum` (2^61-1). Integers above this are bignums in
/// Emacs, and the fixed-size allocators (`make-vector`, `make-string`,
/// `make-record`) reject a bignum length as a non-wholenum.
const MOST_POSITIVE_FIXNUM: i64 = 2305843009213693951;
/// Rendered form of `Vmemory_signal_data`: the plain `error` Emacs signals when
/// an allocation request cannot be satisfied. `make_error_object` splits this
/// into `(error "Memory exhausted--use C-x s then exit and restart Emacs")`.
const MEMORY_EXHAUSTED: &str = "error: Memory exhausted--use C-x s then exit and restart Emacs";

/// [`check_array_len`] on an elisp value: a bignum length is out of fixnum range
/// by construction, so it is the `wholenump` rejection rather than a type error.
fn check_array_len_val(h: &ElispHost, v: &Value) -> Result<usize, String> {
    if h.is_bignum(v) {
        return Err(format!(
            "wrong-type-argument: wholenump {}",
            h.print(v, true)
        ));
    }
    check_array_len(as_num(h, v)?.0)
}

/// `CHECK_FIXNAT (V)`: V must be an integer in `[0, most-positive-fixnum]`, and
/// anything else — a negative fixnum, a float, a bignum, a string, nil — is
/// `wrong-type-argument wholenump V` naming the value itself.
///
/// [`check_array_len_val`] is the same test for a caller that goes on to
/// ALLOCATE the length; this one is for a caller that only counts with it.
fn check_fixnat(h: &ElispHost, v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(n) if (0..=MOST_POSITIVE_FIXNUM).contains(n) => Ok(*n),
        _ => Err(format!(
            "wrong-type-argument: wholenump {}",
            h.print(v, true)
        )),
    }
}

/// Validate a requested array/string length the way Emacs's `CHECK_FIXNAT`
/// does: a negative value or one above `most-positive-fixnum` (a bignum) is not
/// a wholenum, so signal `wrong-type-argument wholenump N` rather than
/// attempting an allocation that would panic or abort the process.
fn check_array_len(n: i64) -> Result<usize, String> {
    if !(0..=MOST_POSITIVE_FIXNUM).contains(&n) {
        return Err(format!("wrong-type-argument: wholenump {n}"));
    }
    Ok(n as usize)
}

/// Character-code accessor: a valid character is an integer in [0, #x3FFFFF];
/// anything else signals `wrong-type-argument characterp VALUE` (a negative code
/// or one past the upper bound, as `char-to-string`/`make-string` do).
/// `CHECK_FIXNUM` under a caller-chosen predicate name: a float, a bignum, a
/// marker and a string are all rejected. `forward-char`/`backward-char` name
/// `fixnump`, where the shared `as_int` accessor would have said `integerp`.
fn as_fixnum_named(h: &mut ElispHost, v: &Value, pred: &str) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(h.signal_wrong_type(pred, v)),
    }
}

/// `CHECK_FIXNUM_COERCE_MARKER` under a caller-chosen predicate name: a fixnum
/// or a marker (which contributes its position), never a float or a bignum.
fn as_int_or_marker(h: &mut ElispHost, v: &Value, pred: &str) -> Result<i64, String> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => match h.marker_position(v) {
            Some(p) => Ok(p as i64),
            None => Err(h.signal_wrong_type(pred, v)),
        },
    }
}

fn as_char(h: &mut ElispHost, v: &Value) -> Result<u32, String> {
    match v {
        Value::Int(n) if (0..=0x3F_FFFF).contains(n) => Ok(*n as u32),
        _ => Err(h.signal_wrong_type("characterp", v)),
    }
}

/// The exact value of an elisp number: an integer of any size, or a float.
///
/// This is the accessor every builtin whose result must be *exact* uses —
/// `+`, `*`, `-`, `/`, `expt`, `abs`, the comparisons. `as_num`'s `(i64, f64,
/// bool)` triple cannot represent a bignum, and comparing large integers as
/// `f64` (which it forces) calls 2^62 and 2^62+1 equal.
///
/// A marker coerces to its buffer position, as in Emacs. Anything that is not a
/// number signals with Emacs's predicate and its `prin1` form (never the raw
/// heap handle).
fn as_number(h: &ElispHost, v: &Value) -> Result<Num, String> {
    as_number_p(h, v, true)
}

/// [`as_number`], but choosing Emacs's predicate for the calling builtin.
///
/// `markers_ok` distinguishes the two families: the arithmetic ops accept a
/// marker (as its buffer position) and signal `number-or-marker-p`, while
/// `abs`/`floor`/`ceiling`/`round`/`truncate`/`float`/`expt`/`sqrt`/
/// `number-to-string` take strictly a number and signal `numberp`.
fn as_number_p(h: &ElispHost, v: &Value, markers_ok: bool) -> Result<Num, String> {
    match v {
        Value::Int(n) => Ok(Num::Int(BigInt::from(*n))),
        Value::Float(f) => Ok(Num::Float(*f)),
        _ => {
            if let Some(b) = h.as_bigint(v) {
                return Ok(Num::Int(b));
            }
            if markers_ok {
                if let Some(p) = h.marker_position(v) {
                    return Ok(Num::Int(BigInt::from(p)));
                }
            }
            Err(format!(
                "wrong-type-argument: {} {}",
                if markers_ok {
                    "number-or-marker-p"
                } else {
                    "numberp"
                },
                h.print(v, true)
            ))
        }
    }
}

/// Fold an n-ary exact arithmetic op (`+`, `*`, `-`) over its arguments.
/// Integer arithmetic stays exact and promotes to a bignum; a single float
/// operand makes the whole result a float, as in Emacs.
fn fold_arith(
    h: &mut ElispHost,
    a: &[Value],
    init: i64,
    int_op: fn(BigInt, BigInt) -> BigInt,
    float_op: fn(f64, f64) -> f64,
) -> R {
    // Seed from the FIRST argument, not from the identity — that is what Emacs's
    // `arith_driver' does, and it is observable on signed zero: `(+ -0.0)' is
    // -0.0 because the lone argument is returned (type-checked) rather than
    // added to 0, while `(+ -0.0 0)' really does add and gives 0.0. Only an
    // empty argument list falls back to the identity.
    let mut acc = match a.first() {
        Some(v) => as_number(h, v)?,
        None => Num::Int(BigInt::from(init)),
    };
    for v in a.iter().skip(1) {
        let n = as_number(h, v)?;
        acc = match (acc, n) {
            (Num::Int(x), Num::Int(y)) => Num::Int(int_op(x, y)),
            (x, y) => Num::Float(float_op(x.to_f64(), y.to_f64())),
        };
    }
    Ok(match acc {
        Num::Int(i) => h.make_integer(i),
        Num::Float(f) => Value::Float(f),
    })
}

fn add(h: &mut ElispHost, a: &[Value]) -> R {
    fold_arith(h, a, 0, |x, y| x + y, |x, y| x + y)
}
fn mul(h: &mut ElispHost, a: &[Value]) -> R {
    fold_arith(h, a, 1, |x, y| x * y, |x, y| x * y)
}
fn sub(h: &mut ElispHost, a: &[Value]) -> R {
    if a.is_empty() {
        return Ok(Value::Int(0));
    }
    // Unary `-` negates; n-ary subtracts the rest from the first.
    if a.len() == 1 {
        return Ok(match as_number(h, &a[0])? {
            Num::Int(x) => h.make_integer(-x),
            Num::Float(f) => Value::Float(-f),
        });
    }
    let mut acc = as_number(h, &a[0])?;
    for v in &a[1..] {
        let n = as_number(h, v)?;
        acc = match (acc, n) {
            (Num::Int(x), Num::Int(y)) => Num::Int(x - y),
            (x, y) => Num::Float(x.to_f64() - y.to_f64()),
        };
    }
    Ok(match acc {
        Num::Int(i) => h.make_integer(i),
        Num::Float(f) => Value::Float(f),
    })
}
fn div(h: &mut ElispHost, a: &[Value]) -> R {
    // Integer division truncates toward zero and stays exact (a bignum quotient is
    // a bignum); the first float operand makes the rest float division.
    let first = as_number(h, &a[0])?;
    // Unary `/` is `1/x` in Emacs — `(/ 4)` is 0 (integer division), `(/ 4.0)` is
    // 0.25 — not the argument itself.
    let (mut acc, rest): (Num, &[Value]) = if a.len() == 1 {
        (Num::Int(BigInt::from(1)), &a[0..1])
    } else {
        (first, &a[1..])
    };
    // Whether a NaN was *handed to* the division rather than produced by it.
    let mut nan_operand = matches!(acc, Num::Float(f) if f.is_nan());
    for v in rest {
        let n = as_number(h, v)?;
        nan_operand |= matches!(n, Num::Float(f) if f.is_nan());
        acc = match (acc, n) {
            (Num::Int(x), Num::Int(y)) => {
                if y == BigInt::from(0) {
                    return Err("arith-error: division by zero".to_string());
                }
                Num::Int(x / y)
            }
            (x, y) => Num::Float(x.to_f64() / y.to_f64()),
        };
    }
    Ok(match acc {
        Num::Int(i) => h.make_integer(i),
        Num::Float(mut f) => {
            // A NaN the division *invents* (`(/ 0.0 0.0)`) gets a
            // hardware-dependent sign: x86-64 yields a sign-negative NaN, ARM a
            // positive one. Emacs inherits that difference from the C division;
            // canonicalizing to positive keeps elisprs's output the same on both.
            //
            // A NaN that merely *passed through* is a different matter: IEEE
            // propagates an operand NaN unchanged on every ISA, so `(/ -0.0e+NaN
            // -1)' is -0.0e+NaN in Emacs everywhere. Flattening that one too made
            // the sign of an existing NaN unobservable through `/'.
            if f.is_nan() && !nan_operand {
                f = f.abs();
            }
            Value::Float(f)
        }
    })
}
fn modulo(h: &mut ElispHost, a: &[Value]) -> R {
    let x = as_int_exact(h, &a[0])?;
    let y = as_int_exact(h, &a[1])?;
    if y == BigInt::from(0) {
        return Err("arith-error: division by zero".to_string());
    }
    let r = x % y;
    Ok(h.make_integer(r))
}
// `mod` (vs `%`): the result takes the sign of the divisor, and either operand
// may be a float — (mod 13.5 4) => 1.5, (mod -1 3) => 2.
fn mod_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let xn = as_number(h, &a[0])?;
    let yn = as_number(h, &a[1])?;
    if matches!(xn, Num::Float(_)) || matches!(yn, Num::Float(_)) {
        let (xf, yf) = (xn.to_f64(), yn.to_f64());
        // Faithful port of Emacs `Fmod` (data.c): fmod, then fix the sign so the
        // result matches the divisor. `%` on f64 is fmod (the remainder keeps the
        // dividend's sign, so `(mod -0.0 5)` stays `-0.0`, as Emacs returns). A
        // zero float divisor yields NaN — Emacs does NOT signal arith-error for
        // float mod-by-zero (only integer mod-by-zero, handled below).
        let mut r = xf % yf;
        if if yf < 0.0 { r > 0.0 } else { r < 0.0 } {
            r += yf;
        }
        // Canonicalize only a NaN this computation invented — `(mod 5.0 0)` —
        // whose sign is hardware-dependent. An operand that was already a NaN
        // propagates unchanged on every ISA, so `(mod 0 -0.0e+NaN)' keeps its
        // sign in Emacs and must here too. Mirrors the `div` handling above.
        if r.is_nan() && !(xf.is_nan() || yf.is_nan()) {
            r = r.abs();
        }
        return Ok(Value::Float(r));
    }
    let x = as_int_exact(h, &a[0])?;
    let y = as_int_exact(h, &a[1])?;
    if y == BigInt::from(0) {
        return Err("arith-error: division by zero".to_string());
    }
    let zero = BigInt::from(0);
    let mut r = &x % &y;
    if r != zero && (r < zero) != (y < zero) {
        r += &y;
    }
    Ok(h.make_integer(r))
}
/// `(max NUM…)` / `(min NUM…)` — Emacs checks each argument in order, so the
/// error names the FIRST non-number, and both are subrs (which is what their
/// `wrong-number-of-arguments` and `#<subr min>` printing depend on).
///
/// Emacs picks the winner with `arithcompare`, so the comparison is exact
/// ([`num_cmp`]) and a mixed integer/float pair is decided on real values:
/// `(min (expt 3 34) (float (expt 3 34)))` is the *float*, because it is the
/// smaller number even though both round to the same `f64`.
fn min_max(h: &mut ElispHost, a: &[Value], want_max: bool) -> R {
    let mut best = as_number(h, &a[0])?;
    let mut best_v = a[0].clone();
    for v in &a[1..] {
        let n = as_number(h, v)?;
        // A NaN operand wins, as in Emacs.
        let nan = matches!(n, Num::Float(f) if f.is_nan());
        // `None` is a NaN pair, which the `nan` flag above already handles.
        let better = match num_cmp(&n, &best) {
            Some(o) => {
                if want_max {
                    o.is_gt()
                } else {
                    o.is_lt()
                }
            }
            None => false,
        };
        if better || nan {
            best = n;
            best_v = v.clone();
        }
    }
    // Return the argument itself, so a marker stays a marker as in Emacs.
    let _ = &best;
    Ok(best_v)
}
fn max_fn(h: &mut ElispHost, a: &[Value]) -> R {
    min_max(h, a, true)
}
fn min_fn(h: &mut ElispHost, a: &[Value]) -> R {
    min_max(h, a, false)
}

fn one_plus(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(match as_number(h, &a[0])? {
        Num::Int(i) => h.make_integer(i + 1),
        Num::Float(f) => Value::Float(f + 1.0),
    })
}
fn one_minus(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(match as_number(h, &a[0])? {
        Num::Int(i) => h.make_integer(i - 1),
        Num::Float(f) => Value::Float(f - 1.0),
    })
}

/// Compare adjacent arguments with `pred`.
///
/// Every pair compares *exactly*, never through `f64` — see [`num_cmp`]. At 2^53
/// an `f64` runs out of mantissa, so a float-only comparison answered `t` to
/// `(= 2305843009213693950 2305843009213693951)` and, for a mixed integer/float
/// pair, `t` to `(= (expt 3 34) (float (expt 3 34)))` where Emacs answers nil.
fn cmp(h: &ElispHost, a: &[Value], pred: fn(std::cmp::Ordering) -> bool, nan_val: bool) -> R {
    for w in a.windows(2) {
        let (x, y) = (as_number(h, &w[0])?, as_number(h, &w[1])?);
        let ord = match num_cmp(&x, &y) {
            Some(o) => o,
            // A NaN operand: `=`/`<`/`>`/`<=`/`>=` are all false.
            None => {
                if nan_val {
                    continue;
                }
                return Ok(Value::Undef);
            }
        };
        if !pred(ord) {
            return Ok(Value::Undef);
        }
    }
    Ok(Value::Bool(true))
}
fn num_eq(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |o| o.is_eq(), false)
}
fn lt(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |o| o.is_lt(), false)
}
fn gt(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |o| o.is_gt(), false)
}
fn le(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |o| o.is_le(), false)
}
fn ge(h: &mut ElispHost, a: &[Value]) -> R {
    cmp(h, a, |o| o.is_ge(), false)
}

// ── equality ──
// `eq` is object identity. Fixnums and interned symbols/heap handles compare by
// value, but two distinct float *objects* are never `eq` (matching Emacs:
// `(eq 1.0 1.0)` => nil). `eql` adds by-value float comparison on top of `eq`.
pub(crate) fn el_eq(h: &ElispHost, a: &Value, b: &Value) -> bool {
    if is_nil(a) && is_nil(b) {
        return true;
    }
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Obj(x), Value::Obj(y)) => x == y,
        (Value::Bool(true), Value::Bool(true)) => true,
        // Strings are objects, and `eq` is object identity: two references to the
        // SAME string are `eq` even though two equal literals are not.
        // `Value::Str` is an `Arc<String>`, and every construction that Emacs
        // calls a fresh object (`copy-sequence`, `substring`, `concat`,
        // `make-string`, each literal read) allocates its own `Arc`, so pointer
        // identity IS the object identity Emacs compares. Without this
        // `(let ((s "abc")) (eq s s))` answered nil, and every identity-based
        // list op over strings — `memq`, `assq`, `delq`, `memql` — answered as
        // though the string were absent.
        //
        // The empty string keeps its own clause: Emacs shares ONE
        // `empty_unibyte_string` object (alloc.c), so every 0-length string is
        // `eq` to every other regardless of where it came from.
        (Value::Str(x), Value::Str(y)) => {
            std::sync::Arc::ptr_eq(x, y) || (x.is_empty() && y.is_empty())
        }
        _ => {
            let _ = h;
            false
        }
    }
}
fn el_eql(h: &ElispHost, a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        // Bignums compare by value, like every other number: Emacs's `eql` is
        // `eq` plus by-value comparison for the boxed numeric types.
        (Value::Obj(_), Value::Obj(_)) => match (h.obj(a), h.obj(b)) {
            (Some(Obj::Bignum(x)), Some(Obj::Bignum(y))) => x == y,
            _ => el_eq(h, a, b),
        },
        _ => el_eq(h, a, b),
    }
}
fn el_equal(h: &ElispHost, a: &Value, b: &Value) -> bool {
    if el_eql(h, a, b) {
        return true;
    }
    // Two strings are `equal` when their CURRENT text matches — read through
    // `str_text` so a string that `aset`/`store-substring` has rewritten
    // compares as what it now holds, not as what it was allocated with.
    if let (Some(x), Some(y)) = (h.str_text(a), h.str_text(b)) {
        return x == y;
    }
    match (a, b) {
        (Value::Obj(_), Value::Obj(_)) => match (h.obj(a), h.obj(b)) {
            (Some(Obj::Cons(a1, a2)), Some(Obj::Cons(b1, b2))) => {
                el_equal(h, a1, b1) && el_equal(h, a2, b2)
            }
            (Some(Obj::Vector(va)), Some(Obj::Vector(vb)))
            | (Some(Obj::Record(va)), Some(Obj::Record(vb))) => {
                va.len() == vb.len() && va.iter().zip(vb).all(|(x, y)| el_equal(h, x, y))
            }
            (Some(Obj::BoolVector(ba)), Some(Obj::BoolVector(bb))) => ba == bb,
            // Two markers are `equal` when they share a buffer and position.
            (Some(Obj::Marker(_)), Some(Obj::Marker(_))) => h.markers_equal(a, b),
            // An interpreted closure is its `#[ARGLIST BODY ENV]` structure, and
            // `equal` descends into it: `(equal (lambda () 1) (lambda () 1))` is
            // t, `(equal (lambda (x) x) (lambda (y) y))` is nil. `remove-hook`
            // depends on it — it matches the function to drop with `member`, so
            // an anonymous hook function could not be removed without this.
            (Some(Obj::Closure { .. }), Some(Obj::Closure { .. })) => {
                match (h.closure_parts(a), h.closure_parts(b)) {
                    (Some((am, ad, aa, abody, aenv)), Some((bm, bd, ba, bbody, benv))) => {
                        am == bm
                            && ad == bd
                            && el_equal(h, &aa, &ba)
                            && abody.len() == bbody.len()
                            && abody.iter().zip(&bbody).all(|(x, y)| el_equal(h, x, y))
                            && aenv.len() == benv.len()
                            && aenv
                                .iter()
                                .zip(&benv)
                                .all(|((s1, v1), (s2, v2))| s1 == s2 && el_equal(h, v1, v2))
                    }
                    _ => false,
                }
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
    Err(format!(
        "wrong-type-argument: consp {}",
        h.print(&a[0], true)
    ))
}
fn setcdr(h: &mut ElispHost, a: &[Value]) -> R {
    if let Value::Obj(id) = &a[0] {
        if let Some(Obj::Cons(_, d)) = h.arena.get_mut(*id as usize) {
            *d = a[1].clone();
            return Ok(a[1].clone());
        }
    }
    Err(format!(
        "wrong-type-argument: consp {}",
        h.print(&a[0], true)
    ))
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
        match h.str_text(v) {
            Some(s) => out.extend(s.chars().map(|c| Value::Int(c as i64)).collect::<Vec<_>>()),
            None => match h.obj(v) {
                Some(Obj::Vector(items)) => out.extend(items.clone()),
                _ => out.extend(h.seq_vec_checked(v)?),
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
    if let Some(s) = h.str_text(&a[0]) {
        let rev = s.chars().rev().collect::<String>();
        return Ok(h.new_string(rev));
    }
    // Reject an improper list (and a non-sequence) with Emacs's error before
    // falling through to the list path, which would otherwise silently ignore
    // the dotted tail.
    h.seq_vec_checked(&a[0])?;
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
fn case_fold(h: &mut ElispHost, a: &[Value], upper: bool) -> R {
    match &a[0] {
        // casefiddle.c: a negative fixnum is not a character (char-or-string-p),
        // but one above the character range is returned UNCHANGED — Emacs treats
        // the high bits as event modifiers, so (upcase 4194304) => 4194304.
        Value::Int(c) if *c < 0 => Err(h.signal_wrong_type("char-or-string-p", &a[0])),
        Value::Int(c) if *c > 0x3F_FFFF => Ok(Value::Int(*c)),
        Value::Int(c) => Ok(Value::Int(simple_case(*c, upper))),
        _ if h.is_string(&a[0]) => {
            // Case folding is character-for-character here, so the text
            // properties land on the same characters they were on.
            let src = h.str_arc(&a[0]).expect("checked stringp");
            let text = if upper {
                src.to_uppercase()
            } else {
                src.to_lowercase()
            };
            let same_len = text.chars().count() == src.chars().count();
            let (folded, out) = h.new_string_keyed(text);
            if same_len {
                h.string_carry_all(&out, &src);
            }
            Ok(folded)
        }
        // The datum has to travel as the *object*: a subr prints `#<subr +>` and
        // a closure `#[…]`, neither of which the reader can turn back into what
        // it came from, so rendering it into the message and re-reading it (which
        // is what the message-only path does) silently dropped it and left a bare
        // `(wrong-type-argument char-or-string-p)`.
        v => {
            let v = v.clone();
            Err(h.signal_wrong_type("char-or-string-p", &v))
        }
    }
}
/// `(--char-titlecase-- CHAR)` — the Unicode *title-case* mapping of CHAR, as a
/// string, because it is one-to-many for some characters (ß → "Ss", ﬁ → "Fi")
/// and differs from upper case for the digraphs (ǳ → ǲ, not Ǳ).
///
/// This is what `casefiddle.c` reaches for at a word start: `case_character_impl`
/// consults `special-titlecase` first, then the `titlecase` char-table, and only
/// then falls back to `upcase`. Not an Emacs function — `capitalize` and
/// `upcase-initials` in the prelude are its only callers.
fn char_titlecase(h: &mut ElispHost, a: &[Value]) -> R {
    match &a[0] {
        Value::Int(c) if (0..=0x3F_FFFF).contains(c) => match char::from_u32(*c as u32) {
            Some(ch) => Ok(h.new_string(titlecase_str(ch))),
            // A code point with no scalar value (a surrogate) has no case.
            None => Ok(h.new_string(String::new())),
        },
        v => Err(format!(
            "wrong-type-argument: characterp {}",
            h.print(v, true)
        )),
    }
}

/// The Unicode title-case mapping of `ch`, as a string.
///
/// `char::to_titlecase` is unstable, so this derives the same answer from the
/// stable uppercase mapping plus the two closed sets where title case is not
/// "upper-case the first, lower-case the rest":
///
/// * the four Latin digraph triples, whose title form is the *middle* code
///   point of the triple (ǳ/Ǳ title to ǲ, not to Ǳ);
/// * Greek letters with ypogegrammeni, whose full uppercase is two characters
///   (ᾀ → "ἈΙ") but whose title case is the single precomposed capital
///   (ᾀ → ᾈ), laid out one block of eight below its lowercase run.
///
/// Everywhere else the rule holds exactly: ß → "SS" → "Ss", ﬁ → "FI" → "Fi",
/// ﬃ → "FFI" → "Ffi". `titlecase_char_only` is the same mapping restricted to
/// the one-to-one cases, which is what a *character* argument gets.
fn titlecase_str(ch: char) -> String {
    if let Some(t) = titlecase_char_only(ch) {
        return t.to_string();
    }
    // Everything up to and including the first *cased* character of the
    // uppercase expansion stays as it is; the rest is lower-cased. Taking
    // "index 0" instead is wrong for ŉ (U+0149), whose uppercase is "ʼN": the
    // apostrophe carries no case, so the N is the letter that must stay
    // capital, and Emacs answers "ʼN" where "lower-case everything after the
    // first character" answers "ʼn".
    let mut out = String::new();
    let mut seen_cased = false;
    for u in ch.to_uppercase() {
        if seen_cased {
            // A non-initial capital iota in an uppercase expansion can only
            // have come from a ypogegrammeni, and its title form is the
            // COMBINING GREEK YPOGEGRAMMENI, not a lowercase iota: ᾲ title-cases
            // to "Ὰ" + U+0345, which is what Emacs answers.
            if u == '\u{0399}' {
                out.push('\u{0345}');
            } else {
                out.extend(u.to_lowercase());
            }
        } else {
            out.push(u);
            if u.to_lowercase().next() != Some(u) {
                seen_cased = true;
            }
        }
    }
    out
}

/// The one-to-one title-case mapping, when it differs from `to_uppercase` or
/// when `to_uppercase` is one-to-many but a single title character exists.
/// `None` means "no special case — derive it from the uppercase mapping".
fn titlecase_char_only(ch: char) -> Option<char> {
    let c = ch as u32;
    let t = match c {
        // Latin digraphs: DŽ/Dž/dž, LJ/Lj/lj, NJ/Nj/nj, DZ/Dz/dz.
        0x01C4..=0x01C6 => 0x01C5,
        0x01C7..=0x01C9 => 0x01C8,
        0x01CA..=0x01CC => 0x01CB,
        0x01F1..=0x01F3 => 0x01F2,
        // Greek with ypogegrammeni: three runs of sixteen, each a lowercase
        // eight followed by the capital eight that is also the title form.
        0x1F80..=0x1F87 => c + 8,
        0x1F90..=0x1F97 => c + 8,
        0x1FA0..=0x1FA7 => c + 8,
        0x1FB3 => 0x1FBC,
        0x1FC3 => 0x1FCC,
        0x1FF3 => 0x1FFC,
        // The capital halves of those runs are already the title forms, and
        // must stay put: their full *uppercase* is two characters (ᾈ → "ἈΙ"),
        // so deriving from it would decompose a character Emacs leaves alone.
        0x1F88..=0x1F8F | 0x1F98..=0x1F9F | 0x1FA8..=0x1FAF => c,
        0x1FBC | 0x1FCC | 0x1FFC => c,
        // Georgian Mkhedruli: Unicode gives these an uppercase (the Mtavruli
        // block) but no title case, so a word-initial ა stays ა even though
        // `(upcase ?ა)` is Ა. Deriving the title form from the uppercase one
        // would capitalize Georgian text that Emacs leaves alone.
        0x10D0..=0x10FA | 0x10FD..=0x10FF => c,
        _ => return None,
    };
    char::from_u32(t)
}
fn downcase_fn(h: &mut ElispHost, a: &[Value]) -> R {
    case_fold(h, a, false)
}
fn upcase_fn(h: &mut ElispHost, a: &[Value]) -> R {
    case_fold(h, a, true)
}
fn length_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // nil is the empty list in either of its two VM spellings (`Undef` for a
    // literal, `Bool(false)` for a comparison that answered false).
    if is_nil(&a[0]) {
        return Ok(Value::Int(0));
    }
    // A closure is a pseudovector of 3, 5 or 6 slots.
    if let Some(slots) = h.closure_slots(&a[0]) {
        return Ok(Value::Int(slots.len() as i64));
    }
    if let Some(s) = h.str_text(&a[0]) {
        return Ok(Value::Int(s.chars().count() as i64));
    }
    match &a[0] {
        // A symbol, a number, t … are not sequences: Emacs signals rather than
        // answering 0. (`safe-length` is the one that answers 0.)
        Value::Bool(_) | Value::Int(_) | Value::Float(_) => Err(format!(
            "wrong-type-argument: sequencep {}",
            h.print(&a[0], true)
        )),
        Value::Obj(_) => match h.obj(&a[0]) {
            Some(Obj::Vector(items)) | Some(Obj::Record(items)) => {
                Ok(Value::Int(items.len() as i64))
            }
            Some(Obj::BoolVector(bits)) => Ok(Value::Int(bits.len() as i64)),
            Some(Obj::Cons(..)) => {
                // fns.c `list_length`, which is `Flength`'s cons arm:
                //
                //   ptrdiff_t i = 0;
                //   FOR_EACH_TAIL (list) i++;
                //   CHECK_LIST_END (list, list);
                //
                // Both signals name the LOOP VARIABLE, not the original argument:
                // an improper list reports its tail (`(length '(1 2 . 3))` signals
                // with 3) and a circular one reports the tail Brent's walk was
                // standing on when the hare caught the tortoise. A Floyd walk here
                // found the cycle but could not name that tail, and passed a
                // placeholder string as the datum instead:
                //
                //   (butlast (let ((c (list 9 3 2.5))) (setcdr (last c) (cdr c)) c))
                //   emacs => (circular-list #1=(2.5 3 . #1#))
                //   elisp => (circular-list "circular list")   [before]
                let mut n: i64 = 0;
                let mut w = TailWalk::new(&a[0]);
                while let Some((_, cdr)) = w.cons(h) {
                    n += 1;
                    if w.step(cdr) {
                        let tail = w.tail.clone();
                        return Err(h.signal_circular_list(&tail));
                    }
                }
                let tail = w.tail.clone();
                check_list_end(h, &tail, &tail)?;
                Ok(Value::Int(n))
            }
            // A bool-vector/char-table/record has a length; a symbol, a subr, a
            // buffer … do not — Emacs signals rather than answering 0.
            Some(Obj::CharTable(_)) | Some(Obj::HashTable(_)) => Ok(Value::Int(0)),
            _ => Err(format!(
                "wrong-type-argument: sequencep {}",
                h.print(&a[0], true)
            )),
        },
        _ => Err(format!(
            "wrong-type-argument: sequencep {}",
            h.print(&a[0], true)
        )),
    }
}
/// fns.c `CHECK_LIST_END (x, y)`: `CHECK_TYPE (NILP (x), Qlistp, y)` — a nil tail
/// is a proper end, anything else names the WHOLE list under `listp`.
fn check_list_end(h: &mut ElispHost, tail: &Value, list: &Value) -> Result<(), String> {
    if is_nil(tail) {
        Ok(())
    } else {
        // The offender travels as the OBJECT. Rendering it into the message and
        // re-reading it — which is what this did — loses any list the reader
        // cannot reconstruct, and `print-circle` t makes that the common case:
        // the text becomes `(#1="foo10" #1# #1# . 2)`, the re-read fails, and the
        // condition came back as a bare `(wrong-type-argument listp)` with no
        // offender at all.
        Err(h.signal_wrong_type("listp", list))
    }
}

/// fns.c `Fnthcdr`, ported. Take cdr N times on LIST and return the result.
///
/// The two things a naive `while (> n 0)` loop gets wrong, both of which Emacs
/// handles here and both of which the fuzzer reached:
///
/// - **N may be a bignum.** `CHECK_INTEGER` accepts one, so
///   `(nth (floor 1.5e+300) '(a))` is `nil`, not `(wrong-type-argument integerp …)`.
///   A negative bignum returns LIST untouched; a positive one is walked with
///   `EMACS_INT_MAX` substituted, and the substitution error is undone below.
/// - **LIST may be circular.** Counting down 4611686018427387903 cdrs of a
///   three-element cycle never terminates. Emacs runs Brent's teleporting
///   tortoise, then reduces the remaining count modulo the distance the hare
///   travelled since the last teleport. That distance is always a MULTIPLE of the
///   true cycle period (both pointers sit on the same cell when they meet), so
///   reducing by it lands on the same cell the full walk would have — which is why
///   the answer does not depend on the tortoise's schedule.
fn nthcdr_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let list = a[1].clone();
    let mut tail = list.clone();
    // `CHECK_INTEGER (n)`: a bignum passes, a float does not — `(nthcdr 1.5 '(a))`
    // is `(wrong-type-argument integerp 1.5)` even though 1.5 has no fraction bits
    // to spare.
    if !h.is_integer(&a[0]) {
        return Err(format!(
            "wrong-type-argument: integerp {}",
            h.print(&a[0], true)
        ));
    }
    // "A huge but in-range EMACS_INT that can be substituted for a positive
    // bignum while counting down."
    const LARGE_NUM: i64 = i64::MAX;
    // fns.c `SMALL_LIST_LEN_MAX`: below this, skip circularity and quit checking.
    const SMALL_LIST_LEN_MAX: i64 = 127;
    let bignum: Option<BigInt> = match h.obj(&a[0]) {
        Some(Obj::Bignum(b)) => Some(b.clone()),
        _ => None,
    };
    let mut num: i64 = match (&a[0], &bignum) {
        (Value::Int(n), _) => {
            let mut num = *n;
            if num <= SMALL_LIST_LEN_MAX {
                while num > 0 {
                    match h.obj(&tail) {
                        Some(Obj::Cons(_, d)) => tail = d.clone(),
                        _ => {
                            check_list_end(h, &tail, &list)?;
                            return Ok(Value::Undef);
                        }
                    }
                    num -= 1;
                }
                return Ok(tail);
            }
            num
        }
        (_, Some(b)) => {
            if b.sign() == num_bigint::Sign::Minus {
                return Ok(tail);
            }
            LARGE_NUM
        }
        // `is_integer` already accepted it, so this arm is unreachable.
        _ => return Ok(tail),
    };

    // `FOR_EACH_TAIL_SAFE (tail)` with the body of Fnthcdr. The C two-level
    // countdown (an `unsigned short` q plus an `intptr_t` n, both refilled from a
    // doubling `max`) is one i64 counter here; it is the same total period.
    let mut tortoise = tail.clone();
    let mut tortoise_num = num;
    let mut saved_tail = tail.clone();
    let mut period: i64 = 2;
    let mut countdown: i64 = 2;
    while let Some(Obj::Cons(_, d)) = h.obj(&tail) {
        // "If the tortoise just jumped (which is rare), update TORTOISE_NUM."
        if value_eq_obj(&tail, &tortoise) {
            tortoise_num = num;
        }
        saved_tail = d.clone();
        num -= 1;
        if num == 0 {
            return Ok(saved_tail);
        }
        tail = saved_tail.clone();
        countdown -= 1;
        if countdown == 0 {
            // Teleport; no cycle test on the step that teleports.
            period <<= 1;
            countdown = period;
            tortoise = tail.clone();
        } else if value_eq_obj(&tail, &tortoise) {
            break;
        }
    }

    tail = saved_tail;
    if !matches!(h.obj(&tail), Some(Obj::Cons(..))) {
        check_list_end(h, &tail, &list)?;
        return Ok(Value::Undef);
    }

    // TAIL is part of a cycle. Reduce NUM modulo the cycle length.
    let cycle_length = tortoise_num - num;
    if let Some(b) = &bignum {
        // Undo the LARGE_NUM substitution: add (N - LARGE_NUM) mod CYCLE_LENGTH.
        let m = BigInt::from(cycle_length);
        let r = (b % &m).to_i64().unwrap_or(0);
        num += r;
        num += cycle_length - LARGE_NUM % cycle_length;
    }
    num = num.rem_euclid(cycle_length);
    while num > 0 {
        match h.obj(&tail) {
            Some(Obj::Cons(_, d)) => tail = d.clone(),
            _ => break,
        }
        num -= 1;
    }
    Ok(tail)
}

/// `BASE_EQ` for the two cases a list walk can produce: two heap handles, or two
/// identical immediates. Enough for cycle detection, where only conses can match.
fn value_eq_obj(a: &Value, b: &Value) -> bool {
    matches!((a, b), (Value::Obj(x), Value::Obj(y)) if x == y)
}

// ── fns.c list search (FOR_EACH_TAIL) ────────────────────────────────────────

/// lisp.h `struct for_each_tail_internal` plus the advance clause of
/// `FOR_EACH_TAIL_INTERNAL`, ported as a driver so a walk can be run from a subr
/// body (which holds the host borrow) and from `host::call_function` (which must
/// release it between elisp calls) with one implementation.
///
/// The C is Brent's teleporting tortoise-hare, and the schedule is not
/// incidental — it decides *which* tail the `circular-list` signal names:
///
/// ```text
///   for (struct for_each_tail_internal li = { tail, 2, 0, 2 };
///        CONSP (tail);
///        ((tail) = XCDR (tail),
///     ((--li.q != 0
///       || ((check_quit) ? maybe_quit () : (void) 0, 0 < --li.n)
///       || (li.q = li.n = li.max <<= 1, li.n >>= USHRT_WIDTH,
///           li.tortoise = (tail), false))
///      && BASE_EQ (tail, li.tortoise))
///     ? (cycle) : (void) 0))
/// ```
///
/// This replaces seven prelude `defun`s (`memq`, `memql`, `member`, `assq`,
/// `assoc`, `rassq`, `rassoc`) that walked with a bare `while (consp l)` and so
/// **did not terminate at all** on a circular list, where Emacs signals:
///
/// ```text
///   $ emacs -Q --batch -l circ.el    # (circular-list (3 1 2 3 1 . #2))
///   $ elisp circ.el                  # hangs
/// ```
pub struct TailWalk {
    /// The C loop variable: the current cons while walking, the terminating
    /// non-cons once the loop exits.
    pub tail: Value,
    tortoise: Value,
    max: i64,
    n: i64,
    q: u16,
}

impl TailWalk {
    pub fn new(list: &Value) -> Self {
        // `struct for_each_tail_internal li = { tail, 2, 0, 2 }`.
        Self {
            tail: list.clone(),
            tortoise: list.clone(),
            max: 2,
            n: 0,
            q: 2,
        }
    }

    /// The loop's `CONSP (tail)` test, returning `(car, cdr)` while it holds.
    pub fn cons(&self, h: &ElispHost) -> Option<(Value, Value)> {
        match h.obj(&self.tail) {
            Some(Obj::Cons(car, cdr)) => Some((car.clone(), cdr.clone())),
            _ => None,
        }
    }

    /// The `for` statement's third clause. `cdr` is `XCDR (tail)` read while the
    /// host was borrowed. Returns whether a cycle was just detected — the point
    /// at which `FOR_EACH_TAIL` evaluates `circular_list (tail)`.
    #[must_use]
    pub fn step(&mut self, cdr: Value) -> bool {
        self.tail = cdr;
        self.q = self.q.wrapping_sub(1);
        let advanced = if self.q != 0 {
            true
        } else {
            self.n -= 1;
            if self.n > 0 {
                true
            } else {
                // `li.q = li.n = li.max <<= 1, li.n >>= USHRT_WIDTH,
                //  li.tortoise = (tail), false` — the teleport, which yields
                // false so the `BASE_EQ` that follows is short-circuited away.
                self.max <<= 1;
                self.n = self.max;
                self.q = (self.n & 0xffff) as u16;
                self.n >>= 16;
                self.tortoise = self.tail.clone();
                false
            }
        };
        advanced && value_eq_obj(&self.tail, &self.tortoise)
    }
}

/// fns.c `eq_comparable_value`: `SYMBOLP (x) || FIXNUMP (x)`. `member`, `assoc`
/// and `rassoc` route to their `eq` siblings for such a key, which is why
/// `(member 1 L)` reports `memq`'s walk and not `equal`'s.
fn eq_comparable_value(h: &ElispHost, v: &Value) -> bool {
    match v {
        Value::Int(_) | Value::Bool(_) | Value::Undef => true,
        Value::Obj(_) => matches!(h.obj(v), Some(Obj::Symbol(_))),
        _ => false,
    }
}

/// Run a `FOR_EACH_TAIL` walk inside a single host borrow: `hit` decides, per
/// cons, whether this is the answer. Returns nil after `CHECK_LIST_END`.
fn for_each_tail(
    h: &mut ElispHost,
    list: &Value,
    mut hit: impl FnMut(&ElispHost, &Value, &Value) -> Option<Value>,
) -> R {
    let mut w = TailWalk::new(list);
    while let Some((car, cdr)) = w.cons(h) {
        if let Some(found) = hit(h, &car, &w.tail) {
            return Ok(found);
        }
        if w.step(cdr) {
            return Err(h.signal_circular_list(&w.tail));
        }
    }
    check_list_end(h, &w.tail, list)?;
    Ok(Value::Undef)
}

/// fns.c `Fmemq`.
fn memq_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let elt = a[0].clone();
    for_each_tail(h, &a[1], |h, car, tail| {
        el_eq(h, car, &elt).then(|| tail.clone())
    })
}

/// fns.c `Fmemql`. Only a float or a bignum takes the by-value path; every other
/// element type is `Fmemq`'s job, which is why `(memql "a" (list "a"))` is nil.
fn memql_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let elt = a[0].clone();
    let float = matches!(elt, Value::Float(_));
    let bignum = matches!(h.obj(&elt), Some(Obj::Bignum(_)));
    if !float && !bignum {
        return memq_fn(h, a);
    }
    for_each_tail(h, &a[1], |h, car, tail| {
        let same = if float {
            matches!(car, Value::Float(_)) && el_eql(h, &elt, car)
        } else {
            matches!(h.obj(car), Some(Obj::Bignum(_))) && el_eql(h, &elt, car)
        };
        same.then(|| tail.clone())
    })
}

/// fns.c `Fmember`.
fn member_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if eq_comparable_value(h, &a[0]) {
        return memq_fn(h, a);
    }
    let elt = a[0].clone();
    for_each_tail(h, &a[1], |h, car, tail| {
        el_equal(h, &elt, car).then(|| tail.clone())
    })
}

/// fns.c `Fassq`. A non-cons element is skipped, not an error.
fn assq_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let key = a[0].clone();
    for_each_tail(h, &a[1], |h, car, _| match h.obj(car) {
        Some(Obj::Cons(k, _)) if el_eq(h, k, &key) => Some(car.clone()),
        _ => None,
    })
}

/// fns.c `Frassq`.
fn rassq_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let key = a[0].clone();
    for_each_tail(h, &a[1], |h, car, _| match h.obj(car) {
        Some(Obj::Cons(_, v)) if el_eq(h, v, &key) => Some(car.clone()),
        _ => None,
    })
}

// `assoc` and `mapconcat` call back into elisp (TESTFN, FUNCTION), so their
// bodies live in `host::call_function` rather than in a subr that would already
// hold the host borrow. These are the pieces of the fns.c ports they reuse.

/// fns.c `eq_comparable_value`, for the `Fassoc` fast path.
pub fn eq_comparable(h: &ElispHost, v: &Value) -> bool {
    eq_comparable_value(h, v)
}

/// fns.c `Fassq`, callable from the `assoc` intercept.
pub fn assq(h: &mut ElispHost, a: &[Value]) -> R {
    assq_fn(h, a)
}

/// `Fassoc`'s default test: `EQ (XCAR (car), key) || !NILP (Fequal (…))`.
pub fn equal_or_eq(h: &ElispHost, a: &Value, b: &Value) -> bool {
    el_eq(h, a, b) || el_equal(h, a, b)
}

/// fns.c `CHECK_LIST_END`, callable from the `assoc` intercept.
pub fn check_end(h: &mut ElispHost, tail: &Value, list: &Value) -> Result<(), String> {
    check_list_end(h, tail, list)
}

/// fns.c `Fconcat`, callable from the `mapconcat` intercept.
pub fn concat(h: &mut ElispHost, a: &[Value]) -> R {
    concat_fn(h, a)
}

/// fns.c `Frassoc`.
fn rassoc_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if eq_comparable_value(h, &a[0]) {
        return rassq_fn(h, a);
    }
    let key = a[0].clone();
    for_each_tail(h, &a[1], |h, car, _| match h.obj(car) {
        Some(Obj::Cons(_, v)) if el_eq(h, v, &key) || el_equal(h, v, &key) => Some(car.clone()),
        _ => None,
    })
}

fn nth_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // fns.c: `Fnth` is literally `Fcar (Fnthcdr (n, list))`.
    let cur = nthcdr_fn(h, a)?;
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
// subr.el defines every two-, three- and four-letter composition as Lisp
// (`caddr` is `(car (cdr (cdr x)))`), so each one inherits car/cdr's edge
// semantics: car/cdr of nil yield nil (a short list answers nil), while car/cdr
// of a non-nil non-cons signals `wrong-type-argument listp` naming the value
// reached at that step. OPS is the letters between `c` and `r`, applied
// right-to-left: `cxr(h, a, "add")` is caddr. Every name is listed in
// `host::LISP_LEVEL_ARITY`, because a wrong argument count in Emacs is caught
// by exec_byte_code and reported as `(1 . 1)`, not as the function.
fn cxr(h: &mut ElispHost, a: &[Value], ops: &str) -> R {
    let mut v = a[0].clone();
    for op in ops.bytes().rev() {
        v = if op == b'a' {
            car(h, &[v])?
        } else {
            cdr(h, &[v])?
        };
    }
    Ok(v)
}
macro_rules! cxr_subrs {
    ($($f:ident $ops:literal),* $(,)?) => {
        $(fn $f(h: &mut ElispHost, a: &[Value]) -> R { cxr(h, a, $ops) })*
        /// Every c[ad]+r subr as `(NAME, FN)`, for `install`.
        const CXR_SUBRS: &[(&str, crate::host::SubrFn)] =
            &[$((concat!("c", $ops, "r"), $f)),*];
    };
}
cxr_subrs!(
    caar "aa", cadr "ad", cdar "da", cddr "dd",
    caaar "aaa", caadr "aad", cadar "ada", caddr "add",
    cdaar "daa", cdadr "dad", cddar "dda", cdddr "ddd",
    caaaar "aaaa", caaadr "aaad", caadar "aada", caaddr "aadd",
    cadaar "adaa", cadadr "adad", caddar "adda", cadddr "addd",
    cdaaar "daaa", cdaadr "daad", cdadar "dada", cdaddr "dadd",
    cddaar "ddaa", cddadr "ddad", cdddar "ddda", cddddr "dddd",
);

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
    // `nil` travels as either `Undef` or `Bool(false)`; both are the symbol nil.
    Ok(nil_or(
        matches!(a[0], Value::Bool(_) | Value::Undef)
            || matches!(h.obj(&a[0]), Some(Obj::Symbol(_))),
    ))
}
fn stringp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.is_string(&a[0])))
}
/// `natnump` is a C subr in Emacs (data.c), so `#'natnump` must be — and print
/// as — `#<subr natnump>` (a prelude lambda would print its closure source in
/// e.g. a `wrong-number-of-arguments` error).
fn natnump_fn(h: &mut ElispHost, a: &[Value]) -> R {
    use num_traits::Signed;
    let ok = match &a[0] {
        Value::Int(n) => *n >= 0,
        v => h.as_bigint(v).is_some_and(|b| !b.is_negative()),
    };
    Ok(nil_or(ok))
}
/// `nlistp` is a C subr in Emacs (data.c) — same identity requirement as natnump.
fn nlistp_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let is_list = is_nil(&a[0]) || matches!(h.obj(&a[0]), Some(Obj::Cons(..)));
    Ok(nil_or(!is_list))
}
fn numberp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.is_number(&a[0])))
}
fn integerp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.is_integer(&a[0])))
}
/// `fixnump` — an integer small enough to need no heap cell. A bignum is an
/// integer, so `integerp` accepts it and this does not.
fn fixnump(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(a[0], Value::Int(_))))
}
fn bignump(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(h.is_bignum(&a[0])))
}
fn floatp(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(a[0], Value::Float(_))))
}
fn vectorp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Vector(_)))))
}

// ── vectors ──
fn vector_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(h.alloc(Obj::Vector(a.to_vec())))
}
/// `(make-list LENGTH INIT)` — port of `Fmake_list` (alloc.c:2991-3005).
///
/// This was a prelude `defun` with the same semantics, which made every
/// observable that names the FUNCTION disagree: Emacs reports
/// `#<subr make-list>` and elisprs printed the whole closure, so
/// `(apply #'make-list nil)` differed in its error, and `subrp`, `subr-name`
/// and `type-of` all answered for an interpreted function. The values and the
/// `wholenump` rejections already matched; only the identity did not.
fn make_list(h: &mut ElispHost, a: &[Value]) -> R {
    let n = check_fixnat(h, &a[0])?;
    let mut val = Value::Undef;
    for _ in 0..n {
        val = h.cons(a[1].clone(), val);
    }
    Ok(val)
}

fn make_vector(h: &mut ElispHost, a: &[Value]) -> R {
    let n = check_array_len_val(h, &a[0])?;
    // Fallible allocation: a length that fits `most-positive-fixnum` can still
    // exceed available memory (or `isize::MAX` bytes). Emacs signals a plain
    // `error` there instead of aborting, so `try_reserve_exact` maps both the
    // capacity-overflow and out-of-memory cases to `MEMORY_EXHAUSTED`.
    let mut v: Vec<Value> = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| MEMORY_EXHAUSTED.to_string())?;
    v.resize(n, a[1].clone());
    Ok(h.alloc(Obj::Vector(v)))
}

// ── records ──
/// `(record TYPE &rest SLOTS)` — a new record whose slot 0 is TYPE and whose
/// remaining slots are SLOTS (Emacs `Frecord`).
fn record_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(h.alloc(Obj::Record(a.to_vec())))
}
/// `(make-record TYPE SLOTS INIT)` — a record with SLOTS non-type slots, each
/// INIT, and slot 0 = TYPE (Emacs `Fmake_record`). SLOTS must be a wholenum, and
/// the record is capped at `PSEUDOVECTOR_SIZE_MASK` (4095) total slots.
fn make_record(h: &mut ElispHost, a: &[Value]) -> R {
    let slots = match &a[1] {
        Value::Int(n) if *n >= 0 => *n as usize,
        _ => {
            return Err(format!(
                "wrong-type-argument: wholenump {}",
                h.print(&a[1], true)
            ))
        }
    };
    let total = slots + 1;
    if total > 4095 {
        return Err(format!(
            "Attempt to allocate a record of {total} slots; max is 4095"
        ));
    }
    let mut v: Vec<Value> = Vec::new();
    v.try_reserve_exact(total)
        .map_err(|_| MEMORY_EXHAUSTED.to_string())?;
    v.resize(total, a[2].clone());
    v[0] = a[0].clone();
    Ok(h.alloc(Obj::Record(v)))
}

// ── bool-vectors ──
/// Clone a bool-vector's bits, or signal `(wrong-type-argument bool-vector-p X)`.
fn as_bool_vector(h: &ElispHost, v: &Value) -> Result<Vec<bool>, String> {
    match h.obj(v) {
        Some(Obj::BoolVector(bits)) => Ok(bits.clone()),
        _ => Err(format!(
            "wrong-type-argument: bool-vector-p {}",
            h.print(v, true)
        )),
    }
}
/// `(make-bool-vector LENGTH INIT)` — a bool-vector of LENGTH elements, each `t`
/// if INIT is non-nil, else `nil` (Emacs `Fmake_bool_vector`).
fn make_bool_vector(h: &mut ElispHost, a: &[Value]) -> R {
    let n = check_array_len_val(h, &a[0])?;
    let bit = !is_nil(&a[1]);
    let mut v: Vec<bool> = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| MEMORY_EXHAUSTED.to_string())?;
    v.resize(n, bit);
    Ok(h.alloc(Obj::BoolVector(v)))
}
/// `(bool-vector &rest OBJECTS)` — a bool-vector whose element I is `t` when
/// OBJECT I is non-nil (Emacs `Fbool_vector`).
fn bool_vector_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let bits: Vec<bool> = a.iter().map(|x| !is_nil(x)).collect();
    Ok(h.alloc(Obj::BoolVector(bits)))
}
fn bool_vector_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::BoolVector(_)))))
}
/// `(bool-vector-count-population A)` — the number of `t` elements of A.
fn bool_vector_count_population(h: &mut ElispHost, a: &[Value]) -> R {
    let bits = as_bool_vector(h, &a[0])?;
    Ok(Value::Int(bits.iter().filter(|&&b| b).count() as i64))
}
/// `(bool-vector-subsetp A B)` — non-nil when every `t` bit of A is `t` in B. A
/// and B must have equal length, else `wrong-length-argument`.
fn bool_vector_subsetp(h: &mut ElispHost, a: &[Value]) -> R {
    let ba = as_bool_vector(h, &a[0])?;
    let bb = as_bool_vector(h, &a[1])?;
    if ba.len() != bb.len() {
        // Emacs's `Fbool_vector_subsetp` reports (len-A len-B len-B).
        return Err(format!(
            "wrong-length-argument: {} {} {}",
            ba.len(),
            bb.len(),
            bb.len()
        ));
    }
    Ok(nil_or(ba.iter().zip(&bb).all(|(&x, &y)| !x || y)))
}
/// data.c `bool_vector_binop_driver` for the four set operations: the result
/// goes into C when given (C must match A and B in length) and is returned
/// only if C changed, otherwise into a fresh bool vector.
fn bool_vector_binop(h: &mut ElispHost, a: &[Value], op: fn(bool, bool) -> bool) -> R {
    let ba = as_bool_vector(h, &a[0])?;
    let bb = as_bool_vector(h, &a[1])?;
    let dest = a.get(2).filter(|v| !is_nil(v)).cloned();
    let wrong_length = |extra: Option<usize>| match extra {
        Some(n) => format!("wrong-length-argument: {} {} {n}", ba.len(), bb.len()),
        None => format!("wrong-length-argument: {} {}", ba.len(), bb.len()),
    };
    if bb.len() != ba.len() {
        let extra = match &dest {
            Some(d) => Some(as_bool_vector(h, d)?.len()),
            None => None,
        };
        return Err(wrong_length(extra));
    }
    let out: Vec<bool> = ba.iter().zip(&bb).map(|(&x, &y)| op(x, y)).collect();
    let Some(dest) = dest else {
        return Ok(h.alloc(Obj::BoolVector(out)));
    };
    let bd = as_bool_vector(h, &dest)?;
    if bd.len() != ba.len() {
        return Err(wrong_length(Some(bd.len())));
    }
    if bd == out {
        return Ok(Value::Undef);
    }
    if let Value::Obj(id) = &dest {
        if let Some(Obj::BoolVector(bits)) = h.arena.get_mut(*id as usize) {
            *bits = out;
        }
    }
    Ok(dest)
}
fn bool_vector_exclusive_or(h: &mut ElispHost, a: &[Value]) -> R {
    bool_vector_binop(h, a, |x, y| x ^ y)
}
fn bool_vector_union(h: &mut ElispHost, a: &[Value]) -> R {
    bool_vector_binop(h, a, |x, y| x | y)
}
fn bool_vector_intersection(h: &mut ElispHost, a: &[Value]) -> R {
    bool_vector_binop(h, a, |x, y| x & y)
}
fn bool_vector_set_difference(h: &mut ElispHost, a: &[Value]) -> R {
    bool_vector_binop(h, a, |x, y| x & !y)
}
/// data.c `Fbool_vector_count_consecutive`: how many elements of A from I on
/// equal B; I may be one past the end.
fn bool_vector_count_consecutive(h: &mut ElispHost, a: &[Value]) -> R {
    let ba = as_bool_vector(h, &a[0])?;
    let i = match &a[2] {
        Value::Int(n) if *n >= 0 => *n as usize,
        v => return Err(h.signal_wrong_type("wholenump", v)),
    };
    if i > ba.len() {
        let (v, n) = (a[0].clone(), a[2].clone());
        let data = h.list_from(vec![v, n]);
        let sym = h.intern("args-out-of-range");
        let obj = h.cons(sym, data);
        let msg = format!("args-out-of-range: {}", h.print(&obj, true));
        h.set_pending_error(&msg, obj);
        return Err(msg);
    }
    let b = !is_nil(&a[1]);
    Ok(Value::Int(
        ba[i..].iter().take_while(|&&x| x == b).count() as i64
    ))
}
/// `(bool-vector-not A &optional B)` — store the complement of A into B (or a new
/// bool-vector) and return it. B, if given, must have A's length.
fn bool_vector_not(h: &mut ElispHost, a: &[Value]) -> R {
    let ba = as_bool_vector(h, &a[0])?;
    let out: Vec<bool> = ba.iter().map(|&x| !x).collect();
    if a.len() > 1 && !is_nil(&a[1]) {
        let bb = as_bool_vector(h, &a[1])?;
        if bb.len() != ba.len() {
            return Err(format!("wrong-length-argument: {} {}", ba.len(), bb.len()));
        }
        if let Value::Obj(id) = &a[1] {
            if let Some(Obj::BoolVector(bits)) = h.arena.get_mut(*id as usize) {
                bits.clear();
                bits.extend_from_slice(&out);
            }
        }
        return Ok(a[1].clone());
    }
    Ok(h.alloc(Obj::BoolVector(out)))
}
/// `(elt SEQUENCE N)` — port of `Felt` (fns.c).
///
/// ```c
///   if (CONSP (sequence) || NILP (sequence))
///     return Fnth (n, sequence);
///   else
///     {
///       CHECK_ARRAY (sequence, Qsequencep);
///       return Faref (sequence, n);
///     }
/// ```
///
/// It was an elisp `defun` in the prelude, which behaved the same but could not
/// name itself the way a C primitive does. Emacs reports the SUBR when a
/// function object is called with the wrong count, so
/// `(funcall #'elt)` is `(wrong-number-of-arguments #<subr elt> 0)`; the prelude
/// version answered with its own printed closure source instead — a
/// `#[(seq n) (…)]` blob in place of `#<subr elt>`.
///
/// The type checks are `Fnth`'s and `Faref`'s, exactly as the C delegates them:
/// the array path's index check is `Faref`'s `CHECK_FIXNUM`
/// (`(elt [1 2 3] 1.5)` is `(wrong-type-argument fixnump 1.5)`), and the list
/// path's is `Fnthcdr`'s, which accepts an integer only.
fn elt_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // `CONSP (sequence) || NILP (sequence)` — a list, including the empty one.
    if is_nil(&a[0]) || matches!(h.obj(&a[0]), Some(Obj::Cons(..))) {
        // Fnth takes (N, LIST); elt takes (SEQUENCE, N).
        return nth_fn(h, &[a[1].clone(), a[0].clone()]);
    }
    // CHECK_ARRAY (sequence, Qsequencep): the TEST is `ARRAYP` but the
    // PREDICATE reported is `sequencep`, because a non-array reaching here is
    // not a sequence at all. `ARRAYP` is vector | string | bool-vector |
    // char-table — a RECORD is not one, even though `Faref` accepts it:
    //
    //   (elt (record 'a 1 2) 1)          => (wrong-type-argument sequencep #s(a 1 2))
    //   (elt (make-char-table 'test 7) ?a) => 7
    //   (arrayp (record 'a 1))           => nil
    if !matches!(
        h.obj(&a[0]),
        Some(Obj::Vector(_))
            | Some(Obj::Str(_))
            | Some(Obj::BoolVector(_))
            | Some(Obj::CharTable(_))
    ) {
        return Err(format!(
            "wrong-type-argument: sequencep {}",
            h.print(&a[0], true)
        ));
    }
    aref(h, a)
}

fn aref(h: &mut ElispHost, a: &[Value]) -> R {
    // A char-table indexes by character (0..=MAX_CHAR) with parent/default
    // fallback, unlike a plain vector's positional index.
    if matches!(h.obj(&a[0]), Some(Obj::CharTable(_))) {
        let c = as_char(h, &a[1])?;
        return Ok(h.char_table_ref(&a[0], c));
    }
    let idx = match &a[1] {
        Value::Int(n) => *n,
        v => return Err(format!("wrong-type-argument: fixnump {}", h.print(v, true))),
    };
    // Faref dispatches on the ARRAY's type before any bounds check, so a
    // non-array names itself even with a negative index: (aref 97 -7) =>
    // (wrong-type-argument arrayp 97), while (aref "ab" -1) is
    // args-out-of-range.
    let oor = |h: &ElispHost| format!("args-out-of-range: {} {idx}", h.print(&a[0], true));
    let get = |len: usize| -> Option<usize> { usize::try_from(idx).ok().filter(|i| *i < len) };
    // An interpreted closure is a pseudovector: `aref` reads its slots.
    if let Some(slots) = h.closure_slots(&a[0]) {
        return get(slots.len())
            .map(|i| slots[i].clone())
            .ok_or_else(|| oor(h));
    }
    match h.obj(&a[0]) {
        Some(Obj::Vector(items)) | Some(Obj::Record(items)) => get(items.len())
            .map(|i| items[i].clone())
            .ok_or_else(|| oor(h)),
        Some(Obj::BoolVector(bits)) => get(bits.len())
            .map(|i| nil_or(bits[i]))
            .ok_or_else(|| oor(h)),
        _ => match h.str_text(&a[0]) {
            Some(s) => get(s.chars().count())
                .and_then(|i| s.chars().nth(i))
                .map(|c| Value::Int(c as i64))
                .ok_or_else(|| oor(h)),
            None => Err(format!(
                "wrong-type-argument: arrayp {}",
                h.print(&a[0], true)
            )),
        },
    }
}
/// `(--note-compiler-macro SYMBOL)` — record that SYMBOL now carries a
/// `compiler-macro` property, so `macroexpand-all` knows to look it up. Called
/// from the prelude's `function-put`; not an Emacs function.
fn note_compiler_macro(h: &mut ElispHost, a: &[Value]) -> R {
    if let Value::Obj(id) = &a[0] {
        h.compiler_macros.insert(*id);
    }
    Ok(a[0].clone())
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
    // A bool-vector stores `t`/`nil`: any non-nil VALUE is stored as `t`.
    let bit = !is_nil(&a[2]);
    if let Value::Obj(id) = &a[0] {
        match h.arena.get_mut(*id as usize) {
            // A record is aset-able exactly like a vector (including slot 0).
            Some(Obj::Vector(items)) | Some(Obj::Record(items)) => {
                if i < items.len() {
                    items[i] = a[2].clone();
                    return Ok(a[2].clone());
                }
                return Err(format!("args-out-of-range: {} {idx}", h.print(&a[0], true)));
            }
            Some(Obj::BoolVector(bits)) => {
                if i < bits.len() {
                    bits[i] = bit;
                    return Ok(a[2].clone());
                }
                return Err(format!("args-out-of-range: {} {idx}", h.print(&a[0], true)));
            }
            _ => {}
        }
    }
    // A STRING is mutable, and the write lands on the string OBJECT, so every
    // reference to it — an alias, a list element, a function's literal — sees
    // the new character. Emacs 30 removed pure space, so even a literal is
    // writable: `(progn (defun f () "ab") (aset (f) 0 ?z) (f))` is `"zb"`.
    //
    // `Faset` (data.c) checks the INDEX before the character: `(aset "ab" 5 'x)`
    // is `(args-out-of-range "ab" 5)`, not `(wrong-type-argument characterp x)`.
    if h.is_string(&a[0]) {
        let mut chars: Vec<char> = h
            .str_text(&a[0])
            .expect("checked stringp")
            .chars()
            .collect();
        if i >= chars.len() {
            return Err(format!("args-out-of-range: {} {idx}", h.print(&a[0], true)));
        }
        let c = as_char(h, &a[2])?;
        chars[i] = char::from_u32(c).unwrap_or('\u{fffd}');
        let text: String = chars.into_iter().collect();
        h.set_string_text(&a[0], text);
        return Ok(a[2].clone());
    }
    // Emacs names the offending object: `(aset 5 0 1)` is
    // `(wrong-type-argument arrayp 5)`, never a bare `(wrong-type-argument arrayp)`.
    Err(format!(
        "wrong-type-argument: arrayp {}",
        h.print(&a[0], true)
    ))
}
/// `(fillarray ARRAY ITEM)` — set every element of ARRAY to ITEM, in place.
/// ARRAY may be a vector or a string; for a string ITEM must be a character
/// (`(fillarray (copy-sequence "ab") ?z)` leaves `"zz"`).
fn fillarray(h: &mut ElispHost, a: &[Value]) -> R {
    if h.is_string(&a[0]) {
        let n = h.str_text(&a[0]).expect("checked stringp").chars().count();
        let c = as_char(h, &a[1])?;
        let ch = char::from_u32(c).unwrap_or('\u{fffd}');
        let text: String = std::iter::repeat_n(ch, n).collect();
        h.set_string_text(&a[0], text);
        return Ok(a[0].clone());
    }
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

/// `(store-substring STRING IDX OBJ)` — port of `Fstore_substring` (editfns.c).
/// OBJ is a character or a string; its characters overwrite STRING starting at
/// IDX, and STRING itself is returned.
///
/// The bounds check is *per character*, not up front, so a too-long OBJ writes
/// what fits and only then signals — and the error names the PARTIALLY WRITTEN
/// string, because the datum is the string object:
///
/// ```text
/// (let ((s (copy-sequence "abc"))) (store-substring s 1 "XYZW"))
///   => (args-out-of-range "aXY" 3)
/// ```
fn store_substring(h: &mut ElispHost, a: &[Value]) -> R {
    if !h.is_string(&a[0]) {
        return Err(h.signal_wrong_type("stringp", &a[0]));
    }
    let mut chars: Vec<char> = h
        .str_text(&a[0])
        .expect("checked stringp")
        .chars()
        .collect();
    let idx = as_int(h, &a[1])?;
    let incoming: Vec<char> = match h.str_text(&a[2]) {
        Some(s) => s.chars().collect(),
        None => vec![char::from_u32(as_char(h, &a[2])?).unwrap_or('\u{fffd}')],
    };
    for (k, c) in incoming.into_iter().enumerate() {
        let pos = idx + k as i64;
        if pos < 0 || pos as usize >= chars.len() {
            // Flush what was written before naming the range, so the datum
            // shows the partial result exactly as Emacs's does.
            let text: String = chars.into_iter().collect();
            h.set_string_text(&a[0], text);
            return Err(format!("args-out-of-range: {} {pos}", h.print(&a[0], true)));
        }
        chars[pos as usize] = c;
    }
    let text: String = chars.into_iter().collect();
    h.set_string_text(&a[0], text);
    Ok(a[0].clone())
}

/// `(clear-string STRING)` — port of `Fclear_string` (fns.c): overwrite every
/// character of STRING with NUL, in place, and answer nil. The length is
/// unchanged (`(length (clear-string (copy-sequence "abc")))` reads 3 on the
/// string), which is what makes it useful for wiping a password buffer.
fn clear_string(h: &mut ElispHost, a: &[Value]) -> R {
    if !h.is_string(&a[0]) {
        return Err(h.signal_wrong_type("stringp", &a[0]));
    }
    let n = h.str_text(&a[0]).expect("checked stringp").chars().count();
    let text: String = std::iter::repeat_n('\0', n).collect();
    h.set_string_text(&a[0], text);
    Ok(Value::Undef)
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
        Value::Bool(true) => {
            Err("Invalid RANGE argument to \u{2018}char-table-range\u{2019}".to_string())
        }
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
                Err("Invalid RANGE argument to \u{2018}char-table-range\u{2019}".to_string())
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
                _ => {
                    return Err(
                        "Invalid RANGE argument to \u{2018}set-char-table-range\u{2019}"
                            .to_string(),
                    )
                }
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
        Some(s) => Ok(h.new_string(s)),
        None => Err(format!(
            "wrong-type-argument: symbolp {}",
            h.print(&a[0], true)
        )),
    }
}
/// Classify the optional obarray argument shared by `intern`/`intern-soft`/
/// `unintern`: `None` (absent/nil → the global obarray), `Some(Some(id))` for a
/// private obarray, or an `obarrayp` type error. A non-nil, non-obarray value
/// signals `(wrong-type-argument obarrayp VALUE)`, matching Emacs's `CHECK_OBARRAY`.
fn obarray_arg(h: &ElispHost, v: Option<&Value>) -> Result<Option<u32>, String> {
    match v {
        None | Some(Value::Undef) | Some(Value::Bool(false)) => Ok(None),
        Some(ob) => match h.obj(ob) {
            Some(Obj::Obarray(d)) if d.global => Ok(None),
            Some(Obj::Obarray(_)) => match ob {
                Value::Obj(id) => Ok(Some(*id)),
                _ => Ok(None),
            },
            _ => Err(format!(
                "wrong-type-argument: obarrayp {}",
                h.print(ob, true)
            )),
        },
    }
}
fn intern_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let name = match h.str_text(&a[0]) {
        Some(s) => s.to_string(),
        None => {
            return Err(format!(
                "wrong-type-argument: stringp {}",
                h.print(&a[0], true)
            ))
        }
    };
    match obarray_arg(h, a.get(1))? {
        // `nil` and `t` are already in the global obarray in Emacs, so interning
        // their names hands back those very objects. elisprs represents them as
        // `Value::Undef`/`Value::Bool(true)` rather than heap symbols, and
        // creating a heap symbol *named* "nil" produced something that prints as
        // `nil`, is `symbolp`, and is not `eq` to nil — so `(and (intern "nil")
        // 1)` answered 1 where Emacs answers nil.
        None if name == "nil" => Ok(Value::Undef),
        None if name == "t" => Ok(Value::Bool(true)),
        None => Ok(h.intern(&name)),
        Some(id) => Ok(h.obarray_intern(id, &name)),
    }
}
/// `(obarray-make &optional SIZE)` — a fresh, empty private obarray. SIZE (a
/// vestigial capacity hint since Emacs 29's obarrays auto-grow) must be a
/// wholenum when supplied, matching Emacs's `CHECK_FIXNAT`.
fn obarray_make_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if let Some(v) = a.first() {
        match v {
            Value::Int(n) if *n >= 0 => {}
            Value::Undef | Value::Bool(false) => {}
            _ => {
                return Err(format!(
                    "wrong-type-argument: wholenump {}",
                    h.print(v, true)
                ))
            }
        }
    }
    Ok(h.alloc(Obj::Obarray(crate::host::ObarrayData {
        symbols: std::collections::HashMap::new(),
        global: false,
    })))
}
/// `(obarrayp OBJECT)` — t iff OBJECT is an obarray. (Emacs 30 no longer accepts
/// a plain vector as an obarray, so neither do we.)
fn obarrayp_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Obarray(_)))))
}
/// `(unintern NAME &optional OBARRAY)` — remove NAME (a symbol or string) from
/// OBARRAY, returning t if a symbol was removed, nil otherwise.
fn unintern_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let name = match h.str_text(&a[0]) {
        Some(s) => s.to_string(),
        None => h
            .sym_name(&a[0])
            .ok_or_else(|| format!("wrong-type-argument: stringp {}", h.print(&a[0], true)))?,
    };
    let removed = match obarray_arg(h, a.get(1))? {
        None => h.obarray_unintern_global(&name),
        Some(id) => h.obarray_unintern(id, &name),
    };
    Ok(nil_or(removed))
}
fn make_symbol_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match h.str_text(&a[0]).map(str::to_string) {
        Some(s) => Ok(h.make_symbol(&s)),
        None => Err(format!(
            "wrong-type-argument: stringp {}",
            h.print(&a[0], true)
        )),
    }
}
fn set_fn(h: &mut ElispHost, a: &[Value]) -> R {
    h.set_dynamic_value(&a[0], a[1].clone())?;
    Ok(a[1].clone())
}
/// `(keywordp OBJECT)` — OBJECT is a symbol interned in the standard obarray
/// under a `:`-prefixed name.
///
/// Spelling alone is not the test: `(make-symbol ":u")` and
/// `(intern ":u" (obarray-make))` both read back as `:u` and are both non-keywords,
/// because `intern_driver` applies the keyword treatment only for the standard
/// obarray. Only the host knows which object the obarray holds, so this cannot
/// live in the prelude.
fn keywordp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Bool(h.is_keyword(&a[0])))
}
/// `(symbol-value SYMBOL)` — SYMBOL's dynamic value.
///
/// A keyword is its own value, seeded into its value cell by `intern`. The
/// compiler already loads a keyword as a self-evaluating constant, so the only
/// way to reach this path with one is an indirect `(symbol-value SYM)`.
fn symbol_value(h: &mut ElispHost, a: &[Value]) -> R {
    if h.is_keyword(&a[0]) {
        return Ok(a[0].clone());
    }
    h.get_dynamic_value(&a[0])
}
/// `(makunbound SYMBOL)` — clear SYMBOL's value cell, returning SYMBOL.
fn makunbound(h: &mut ElispHost, a: &[Value]) -> R {
    h.unset_value(&a[0])?;
    Ok(a[0].clone())
}
/// `(fset SYMBOL DEFINITION)` — set SYMBOL's function cell, returning DEFINITION.
fn fset(h: &mut ElispHost, a: &[Value]) -> R {
    h.set_function_value(&a[0], a[1].clone())?;
    // A symbol pointed at a subr can be pointed at a *narrower* one before its
    // next call, so calls to it need the pre-argument arity guard from here on.
    h.note_subr_alias(&a[0], &a[1]);
    Ok(a[1].clone())
}
/// `(fmakunbound SYMBOL)` — data.c `Ffmakunbound`: empty SYMBOL's function cell
/// and return SYMBOL. `nil` and `t` are refused with `setting-constant`; a
/// keyword is not (only its value cell is constant).
fn fmakunbound(h: &mut ElispHost, a: &[Value]) -> R {
    if is_nil(&a[0]) || matches!(a[0], Value::Bool(true)) {
        let name = if is_nil(&a[0]) { "nil" } else { "t" };
        return Err(format!("setting-constant: {name}"));
    }
    h.set_function_value(&a[0], Value::Undef)?;
    Ok(a[0].clone())
}
/// `(fboundp SYMBOL)` — non-nil if SYMBOL has a function definition.
fn fboundp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(
        h.resolve_function(&a[0]).is_ok() || h.introspect_function_cell(&a[0]).is_some(),
    ))
}
/// `(indirect-function OBJECT)` — follow symbol→function-cell aliases to the final
/// function object (a subr/closure), or nil if undefined.
fn indirect_function(h: &mut ElispHost, a: &[Value]) -> R {
    let mut cur = a[0].clone();
    for _ in 0..64 {
        // `t` and `nil` ARE symbols in Emacs, with no function cell — so
        // `(indirect-function t)` is nil, not `t`. elisprs represents them as
        // `Value::Bool`/`Value::Undef` rather than heap symbols, so they miss the
        // symbol arm below and used to be returned unchanged, as if a non-symbol.
        if is_nil(&cur) || matches!(cur, Value::Bool(true)) {
            return Ok(Value::Undef);
        }
        match h.obj(&cur) {
            Some(Obj::Symbol(_)) => match h.introspect_function_cell(&cur) {
                Some(def) => cur = def,
                None => return Ok(Value::Undef),
            },
            _ => return Ok(cur),
        }
    }
    Ok(cur)
}
/// `(defvaralias NEW-ALIAS BASE-VARIABLE &optional DOCSTRING)` — make NEW-ALIAS a
/// variable alias for BASE-VARIABLE: value operations on either affect both. The
/// optional DOCSTRING is accepted and ignored (no interactive doc store). Returns
/// BASE-VARIABLE.
fn defvaralias(h: &mut ElispHost, a: &[Value]) -> R {
    h.defvaralias(&a[0], &a[1])
}
/// `(indirect-variable OBJECT)` — follow the `defvaralias` chain from OBJECT to the
/// base variable symbol, or return OBJECT unchanged when it is not a symbol.
fn indirect_variable(h: &mut ElispHost, a: &[Value]) -> R {
    match &a[0] {
        Value::Obj(id) => {
            let base = h.indirect_var(*id);
            Ok(Value::Obj(base))
        }
        _ => Ok(a[0].clone()),
    }
}
/// `(boundp SYMBOL)` — non-nil if SYMBOL currently has a value.
fn boundp(h: &mut ElispHost, a: &[Value]) -> R {
    // nil and t are always bound; otherwise the value cell must resolve.
    let bound =
        is_nil(&a[0]) || matches!(a[0], Value::Bool(true)) || h.get_dynamic_value(&a[0]).is_ok();
    Ok(nil_or(bound))
}

// ── functional ──
// `funcall`/`apply`/`mapcar`/`mapc` are intercepted in `host::call_function`
// (they re-enter elisp, so they can't run inside a host borrow) — they are not
// plain subrs here.
fn identity(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(a[0].clone())
}
/// Where a print primitive's output goes: print.c `PRINTPREPARE`'s reading of
/// PRINTCHARFUN. nil means the value of `standard-output`; t is stdout in
/// batch; a buffer inserts at its point; a marker inserts at the marker and
/// moves it past the text; anything else is called once per character.
pub(crate) enum PrintDest {
    Stdout,
    Buffer(usize),
    Marker(Value),
    Function(Value),
}

/// Resolve PRINTCHARFUN (absent = nil) the way `PRINTPREPARE` does.
pub(crate) fn print_dest(h: &ElispHost, pcf: Option<&Value>) -> Result<PrintDest, String> {
    let mut v = pcf.cloned().unwrap_or(Value::Undef);
    if is_nil(&v) {
        v = h
            .find_symbol("standard-output")
            .and_then(|s| h.get_value(&s).ok())
            .unwrap_or(Value::Bool(true));
    }
    if is_nil(&v) {
        // `standard-output' itself nil: print.c then prints to the echo area,
        // which is stdout in batch.
        return Ok(PrintDest::Stdout);
    }
    if matches!(v, Value::Bool(true)) {
        return Ok(PrintDest::Stdout);
    }
    match h.obj(&v) {
        Some(Obj::Buffer(idx)) => {
            if h.buffers.get(*idx).is_none_or(|b| b.name.is_none()) {
                return Err("error: Selecting deleted buffer".to_string());
            }
            Ok(PrintDest::Buffer(*idx))
        }
        Some(Obj::Marker(m)) => {
            if m.borrow().buffer.is_none() {
                return Err("error: Marker does not point anywhere".to_string());
            }
            Ok(PrintDest::Marker(v.clone()))
        }
        _ => Ok(PrintDest::Function(v)),
    }
}

/// Send S to a non-function destination (`PRINTFINISH` for a buffer or a
/// marker: the text goes in at the insertion position, the marker ends after
/// it, the target buffer's point moves with the insertion, and the current
/// buffer is restored).
pub(crate) fn print_to(h: &mut ElispHost, dest: &PrintDest, s: &str) -> Result<(), String> {
    match dest {
        PrintDest::Stdout => h.emit(s),
        PrintDest::Buffer(idx) => {
            let old = h.current;
            h.current = *idx;
            h.cur_insert(s.chars().collect(), true);
            h.current = old;
        }
        PrintDest::Marker(mv) => {
            let Some(Obj::Marker(m)) = h.obj(mv) else {
                unreachable!("print_dest only yields markers here")
            };
            let m = m.clone();
            let (bi, pos) = {
                let md = m.borrow();
                (md.buffer.expect("checked in print_dest"), md.pos)
            };
            let old = h.current;
            h.current = bi;
            let (begv, zv, old_point) = {
                let b = h.cur_buf_ref();
                (b.begv, b.zv, b.point)
            };
            if pos < begv || pos > zv {
                h.current = old;
                return Err(
                    h.signal_error_arg("Marker is outside the accessible part of the buffer", mv)
                );
            }
            h.cur_buf().point = pos;
            let n = s.chars().count();
            h.cur_insert(s.chars().collect(), true);
            m.borrow_mut().pos = pos + n;
            h.cur_buf().point = if old_point >= pos {
                old_point + n
            } else {
                old_point
            };
            h.current = old;
        }
        PrintDest::Function(_) => {
            unreachable!("function destinations are driven by host::call_function")
        }
    }
    Ok(())
}

/// The text a print primitive produces, for the function-destination path in
/// `host::call_function`, which calls PRINTCHARFUN once per character outside
/// any host borrow. Returns None for an arity the subr itself must reject.
pub(crate) fn print_text(
    h: &mut ElispHost,
    name: &str,
    a: &[Value],
) -> Result<Option<String>, String> {
    Ok(Some(match name {
        "princ" if !a.is_empty() => h.print_checked(&a[0], false)?,
        "prin1" if !a.is_empty() => h.print_checked(&a[0], true)?,
        "print" if !a.is_empty() => format!("\n{}\n", h.print_checked(&a[0], true)?),
        "terpri" => "\n".to_string(),
        "write-char" if !a.is_empty() => {
            let c = write_char_code(h, &a[0])?;
            char::from_u32(c).unwrap_or('\u{fffd}').to_string()
        }
        _ => return Ok(None),
    }))
}

/// `(backquote-process S &optional LEVEL)` — backquote.el's expander; returns
/// `(TAG . CODE)`. See `crate::backquote`.
fn backquote_process_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let level = match a.get(1) {
        Some(Value::Int(n)) => *n,
        Some(v) if !is_nil(v) => return Err(h.signal_wrong_type("number-or-marker-p", v)),
        _ => 0,
    };
    let (tag, code) = crate::backquote::process(h, &a[0], level)?;
    Ok(h.cons(Value::Int(tag as i64), code))
}

/// print.c `Fwrite_char`: `CHECK_FIXNUM (character)`.
fn write_char_code(h: &mut ElispHost, v: &Value) -> Result<u32, String> {
    match v {
        Value::Int(n) => Ok(*n as u32),
        _ => Err(h.signal_wrong_type("fixnump", v)),
    }
}
/// `bolp` at POS (1-based) in TEXT whose accessible start is BEGV.
fn at_bol(text: &[char], begv: usize, pos: usize) -> bool {
    pos <= begv || text[pos - 2] == '\n'
}
/// `(terpri &optional PRINTCHARFUN ENSURE)`. With ENSURE, print.c writes the
/// newline only when the output is not already at the start of a line: for
/// stdout that is the last byte written there, for a buffer or marker it is
/// `bolp` at the insertion position, and a function destination is an error.
fn terpri(h: &mut ElispHost, a: &[Value]) -> R {
    let dest = print_dest(h, a.first())?;
    let ensure = a.get(1).is_some_and(|v| !is_nil(v));
    let needed = if !ensure {
        true
    } else {
        match &dest {
            PrintDest::Stdout => h.stdout_last != '\n',
            PrintDest::Buffer(idx) => {
                let b = &h.buffers[*idx];
                !at_bol(&b.text, b.begv, b.point)
            }
            PrintDest::Marker(mv) => match h.obj(mv) {
                Some(Obj::Marker(m)) => {
                    let md = m.borrow();
                    let b = &h.buffers[md.buffer.expect("checked in print_dest")];
                    !at_bol(&b.text, b.begv, md.pos)
                }
                _ => true,
            },
            PrintDest::Function(f) => {
                let f = f.clone();
                return Err(h.signal_error_arg("Unsupported function argument", &f));
            }
        }
    };
    if needed {
        print_to(h, &dest, "\n")?;
    }
    Ok(nil_or(needed))
}
/// `(print OBJECT &optional PRINTCHARFUN)` — `prin1` surrounded by newlines.
/// print.c writes a newline BEFORE the object as well as after ("Output a
/// newline, then OBJECT, then a newline"), which is what separates successive
/// `print` calls; emitting only the trailing one made
/// `(with-output-to-string (print 'a))` answer "a\n" instead of "\na\n".
fn print_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let dest = print_dest(h, a.get(1))?;
    let s = h.print_checked(&a[0], true)?;
    print_to(h, &dest, &format!("\n{s}\n"))?;
    Ok(a[0].clone())
}
/// `(write-char CHAR &optional PRINTCHARFUN)` — output one character; returns
/// CHAR. print.c checks CHAR with `CHECK_FIXNUM`, so a non-integer is
/// `(wrong-type-argument fixnump X)`.
fn write_char_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let c = write_char_code(h, &a[0])?;
    let dest = print_dest(h, a.get(1))?;
    print_to(
        h,
        &dest,
        &char::from_u32(c).unwrap_or('\u{fffd}').to_string(),
    )?;
    Ok(a[0].clone())
}
fn prin1_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    // `(prin1-to-string OBJECT &optional NOESCAPE)` — a non-nil NOESCAPE prints
    // the way `princ` does (no quotes, no escapes).
    let readable = a.get(1).is_none_or(is_nil);
    Ok(h.new_string(h.print_checked(&a[0], readable)?))
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
        let obj = h.cons(sym, data);
        let msg = format!("no-catch: {display}");
        h.set_pending_error(&msg, obj);
        Err(msg)
    }
}
/// Apply the default `text-quoting-style` (`curve`) to a format *template*:
/// a grave accent becomes a left single quotation mark and an apostrophe a right
/// one. This is the message half of `styled_format` (`src/editfns.c`), which
/// `Ferror`, `Fuser_error` and `Fmessage` all route through — `Ferror` is
/// literally `Fsignal (Qerror, list1 (Fformat_message (nargs, args)))`.
///
/// Only the template is translated, never a substituted argument: the scan
/// happens while walking the format string, and `%s` output is not re-scanned.
/// `\=` is NOT an escape here — that is `substitute-command-keys`, a different
/// function. Measured on GNU Emacs 30.2: `(error "a %s" "x `y'")` keeps the
/// argument's quotes, and `(error "a \\=`b c")` still curves the backtick.
fn curve_quotes(h: &ElispHost, fmt: &str) -> String {
    // doc.c `Ftext_quoting_style`: `grave` leaves both alone, `straight` turns a
    // grave accent into an apostrophe, nil or anything else is `curve`.
    let style = h
        .find_symbol("text-quoting-style")
        .and_then(|s| match h.obj(&s) {
            Some(Obj::Symbol(d)) => d.value.clone(),
            _ => None,
        })
        .and_then(|v| h.sym_name(&v));
    fmt.chars()
        .map(|c| match (c, style.as_deref()) {
            (_, Some("grave")) => c,
            ('`', Some("straight")) => '\'',
            (_, Some("straight")) => c,
            ('`', _) => '\u{2018}',
            ('\'', _) => '\u{2019}',
            _ => c,
        })
        .collect()
}

/// `el_format` with the format template curve-quoted (`format-message`).
fn el_format_message(h: &mut ElispHost, a: &[Value]) -> Result<String, String> {
    match a
        .first()
        .and_then(|v| h.str_text(v))
        .map(|t| curve_quotes(h, t))
    {
        Some(curved) => {
            let mut args = a.to_vec();
            args[0] = h.new_string(curved);
            el_format(h, &args)
        }
        // A non-string template is el_format's error to report, unchanged.
        None => el_format(h, a),
    }
}

fn error_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // Error object: (error "MESSAGE"). Keep it for condition-case.
    let msg = el_format_message(h, a)?;
    let esym = h.intern("error");
    let mstr = h.new_string(msg.clone());
    let data = h.list_from(vec![mstr]);
    let obj = h.cons(esym, data);
    let full = format!("error: {msg}");
    h.set_pending_error(&full, obj);
    Err(full)
}
fn user_error_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // Like `error`, but signals the `user-error` condition.
    let msg = el_format_message(h, a)?;
    let esym = h.intern("user-error");
    let mstr = h.new_string(msg.clone());
    let data = h.list_from(vec![mstr]);
    let obj = h.cons(esym, data);
    let full = format!("user-error: {msg}");
    h.set_pending_error(&full, obj);
    Err(full)
}
/// `(get SYM PROP)` read from Rust. Symbol plists live in the prelude's
/// `symbol-plist--table` (an `eq` table keyed by the symbol), so this is the
/// same lookup the prelude's `get` does, without a round trip through elisp.
fn symbol_get(h: &mut ElispHost, sym: &Value, prop: &str) -> Value {
    let table_sym = h.intern("symbol-plist--table");
    let Ok(table) = h.get_dynamic_value(&table_sym) else {
        return Value::Undef;
    };
    let plist = gethash(h, &[sym.clone(), table]).unwrap_or(Value::Undef);
    let prop = h.intern(prop);
    h.plist_get_eq(&plist, &prop)
}
/// `(signal ERROR-SYMBOL DATA)` — eval.c `Fsignal` / `signal_or_quit`, 31.1.
///
/// A nil ERROR-SYMBOL takes the symbol from `(car DATA)` (and the rest as
/// DATA); a nil or non-cons DATA makes it `error`. The symbol must be a symbol
/// (`wrong-type-argument symbolp`) and must have `error-conditions`, or the
/// signal becomes `(error "Invalid error symbol" SYM)`; a non-list conditions
/// property is `wrong-type-argument listp`.
fn signal_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (mut symv, mut data) = (a[0].clone(), a[1].clone());
    if is_nil(&symv) {
        match h.obj(&data) {
            Some(Obj::Cons(car, cdr)) => {
                let (car, cdr) = (car.clone(), cdr.clone());
                symv = car;
                data = cdr;
            }
            _ => symv = h.intern("error"),
        }
    }
    let is_symbol =
        matches!(symv, Value::Bool(true)) || matches!(h.obj(&symv), Some(Obj::Symbol(_)));
    if !is_symbol {
        return Err(format!(
            "wrong-type-argument: symbolp {}",
            h.print(&symv, true)
        ));
    }
    let conditions = symbol_get(h, &symv, "error-conditions");
    if is_nil(&conditions) {
        let msg = h.new_string("Invalid error symbol");
        let err = h.intern("error");
        let obj = h.list_from(vec![err, msg, symv.clone()]);
        let text = format!("error: Invalid error symbol: {}", h.print(&symv, true));
        h.set_pending_error(&text, obj);
        return Err(text);
    }
    if !matches!(h.obj(&conditions), Some(Obj::Cons(..))) {
        return Err(format!(
            "wrong-type-argument: listp {}",
            h.print(&conditions, true)
        ));
    }
    let sym = h.sym_name(&symv).unwrap_or_else(|| "error".to_string());
    let display = h.print(&data, true);
    let obj = h.cons(symv, data);
    let msg = format!("{sym}: {display}");
    h.set_pending_error(&msg, obj);
    Err(msg)
}

// ── strings / format / IO ──
fn concat_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // Fconcat: each argument is a string, nil, or a list/vector of CHARACTERS.
    // Errors are per-argument, left to right: a non-char element signals
    // `characterp' naming it ((concat '(a)) => characterp a), a dotted list
    // names its tail ((concat '(1 . 2)) => listp 2), and a non-sequence names
    // itself ((concat t) => sequencep t).
    let mut out = String::new();
    // Where each argument's characters land in the result, so a string
    // argument's text properties follow them (`Fconcat` copies intervals the
    // same way). A `None` piece is that many property-less characters — the
    // char-list and vector arguments, which carry none.
    let mut pieces: Vec<(Option<std::sync::Arc<String>>, usize, usize)> = Vec::new();
    let mut carried = 0usize;
    // A char code Emacs accepts (characterp: 0..=#x3FFFFF); codes valid in
    // Emacs but unrepresentable as a Rust char (surrogates, > #x10FFFF)
    // degrade to U+FFFD rather than mis-signalling characterp.
    let push_char = |h: &ElispHost, out: &mut String, it: &Value| -> Result<(), String> {
        match it {
            Value::Int(c) if (0..=0x3F_FFFF).contains(c) => {
                out.push(char::from_u32(*c as u32).unwrap_or('\u{FFFD}'));
                Ok(())
            }
            _ => Err(format!(
                "wrong-type-argument: characterp {}",
                h.print(it, true)
            )),
        }
    };
    for v in a {
        if is_nil(v) {
            continue;
        }
        match h.str_arc(v) {
            Some(s) => {
                let n = s.chars().count();
                out.push_str(&s);
                pieces.push((Some(s), 0, n));
                carried += n;
            }
            None => match h.obj(v) {
                Some(Obj::Vector(items)) => {
                    for it in items.clone() {
                        push_char(h, &mut out, &it)?;
                    }
                }
                Some(Obj::Cons(..)) => {
                    // concat_to_string validates each list arg's STRUCTURE
                    // (list_length => listp on a dotted tail) before checking
                    // any element: (concat '(a . 2)) => listp 2, not
                    // characterp a. seq_vec_checked is exactly that walk.
                    for it in h.seq_vec_checked(v)? {
                        push_char(h, &mut out, &it)?;
                    }
                }
                _ => {
                    return Err(format!(
                        "wrong-type-argument: sequencep {}",
                        h.print(v, true)
                    ))
                }
            },
        }
        // Whatever this argument contributed that was not a string is that many
        // property-less characters, and the offsets after it depend on them.
        let so_far = out.chars().count();
        if so_far > carried {
            pieces.push((None, 0, so_far - carried));
            carried = so_far;
        }
    }
    let (out, key) = h.new_string_keyed(out);
    h.string_carry_props(&key, &pieces);
    Ok(out)
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
fn format_radix(n: &BigInt, radix: u32, upper: bool, alt: bool, prec: Option<usize>) -> String {
    use num_traits::Signed;
    let negative = n.is_negative();
    let mag = n.abs();
    let sign = if negative { "-" } else { "" };
    // Emacs prints a negative %x/%o as sign + magnitude, not the two's complement.
    let body = match (radix, upper) {
        (16, true) => mag.to_str_radix(16).to_uppercase(),
        (16, false) => mag.to_str_radix(16),
        _ => mag.to_str_radix(8),
    };
    let body = pad_digits(&body, prec);
    let prefix = if alt && mag != BigInt::from(0) {
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

/// Rust's `core::fmt` stores a dynamic precision in a `u16`, so `format!` with
/// a precision >= 65536 panics ("Formatting argument out of range"). Emacs (via
/// C `printf`) accepts an arbitrary precision, padding past the value's exact
/// decimal expansion with zeros. `FMT_PREC_CAP` is the largest precision Rust's
/// formatter accepts; beyond it we render at the cap and append the remaining
/// zeros ourselves, reproducing Emacs's output without the panic.
const FMT_PREC_CAP: usize = 65535;

/// A finite `f64`'s exact decimal expansion has at most ~1074 fractional (767
/// significant) digits; past that every additional digit is `0`. `%g` trims
/// trailing zeros, so any precision at or above this cap yields the identical
/// trimmed string — clamping here keeps `format_g` under `FMT_PREC_CAP` without
/// changing its output.
const FMT_G_CAP: usize = 1100;

/// `%f` body: `v` with exactly `prec` fractional digits, avoiding Rust's
/// `u16`-precision panic for huge `prec` by rendering at `FMT_PREC_CAP` and
/// zero-padding the tail (which C `printf`/Emacs also emit as zeros).
fn format_fixed(v: f64, prec: usize) -> String {
    if prec <= FMT_PREC_CAP {
        format!("{:.*}", prec, v)
    } else {
        let mut s = format!("{:.*}", FMT_PREC_CAP, v);
        s.push_str(&"0".repeat(prec - FMT_PREC_CAP));
        s
    }
}

/// C-style `%e`: a `prec`-digit mantissa, then `e`, a sign, and a ≥2-digit
/// exponent (`1000.0` => `1.000000e+03`). Rust's `{:e}` omits the padding/sign.
/// C-printf `%g`: pick `%e` or `%f` by the decimal exponent, with PREC
/// significant digits; trailing zeros are trimmed unless `alt` (the `#` flag).
fn format_g(v: f64, prec: usize, alt: bool) -> String {
    let p = prec.clamp(1, FMT_G_CAP);
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
/// Insert the decimal point printf's `#` flag guarantees: before the exponent of
/// an `e` form, else at the end, when S has none.
fn with_decimal_point(s: String) -> String {
    if s.contains('.') {
        return s;
    }
    match s.find('e') {
        Some(ep) => format!("{}.{}", &s[..ep], &s[ep..]),
        None => s + ".",
    }
}
fn format_e(v: f64, prec: usize) -> String {
    // Beyond FMT_PREC_CAP, render the mantissa at the cap and zero-pad the extra
    // fractional digits before the exponent (Rust's u16 precision would panic).
    let s = if prec <= FMT_PREC_CAP {
        format!("{:.*e}", prec, v)
    } else {
        let capped = format!("{:.*e}", FMT_PREC_CAP, v);
        match capped.find('e') {
            Some(epos) => {
                let mut m = capped[..epos].to_string();
                m.push_str(&"0".repeat(prec - FMT_PREC_CAP));
                m.push_str(&capped[epos..]);
                m
            }
            None => capped,
        }
    };
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

/// A character's width in display columns — Emacs's `char-width` (`indent.c`),
/// which reads `char-width-table` and the `ctl-arrow`/`tab-width` display model.
///
/// Measured against `emacs -Q --batch` (GNU Emacs 30.2):
/// `(list (char-width ?\t) (char-width ?\n) (char-width 7) (char-width 127)
/// (char-width 200) (char-width ?中))` => `(8 0 2 2 1 2)`. A TAB is a flat
/// `tab-width` here rather than a distance to the next tab stop, which
/// `(string-width "a\t")` => `9` confirms.
pub fn char_display_width(c: u32) -> usize {
    match c {
        0x0A => 0,                    // newline occupies no columns
        0x09 => 8,                    // tab-width
        0..=0x1F | 0x7F => 2,         // control chars display as `^X`
        0x0300..=0x036F => 0,         // combining diacriticals
        0x200B..=0x200F => 0,         // zero-width / directional marks
        0x1100..=0x115F               // Hangul Jamo
        | 0x2E80..=0x303E             // CJK radicals … symbols
        | 0x3041..=0x33FF             // Hiragana … CJK compatibility
        | 0x3400..=0x4DBF             // CJK ext A
        | 0x4E00..=0x9FFF             // CJK unified
        | 0xA000..=0xA4CF             // Yi
        | 0xAC00..=0xD7A3             // Hangul syllables
        | 0xF900..=0xFAFF             // CJK compatibility ideographs
        | 0xFF00..=0xFF60             // fullwidth forms
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F           // misc symbols & pictographs, emoticons
        | 0x1F900..=0x1F9FF           // supplemental symbols & pictographs
        | 0x1FA70..=0x1FAFF           // symbols & pictographs ext A
        | 0x20000..=0x3FFFD => 2,     // CJK ext B+
        _ => 1,
    }
}

/// A string's width in display columns (Emacs's `string-width`).
pub fn string_display_width(s: &str) -> usize {
    s.chars().map(|c| char_display_width(c as u32)).sum()
}

/// The longest prefix of `s` whose display width is at most `cols`.
///
/// `format`'s `%.Ns` precision is a column budget, not a character count, and a
/// character that would overflow the budget is dropped whole:
/// `(format "%.3s" "\tXY")` is `""` and `(format "%.3s" "中中")` is `"中"` in
/// Emacs 30.2.
fn truncate_to_columns(s: &str, cols: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in s.chars() {
        let w = char_display_width(c as u32);
        if used + w > cols {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}

/// Pad `body` to `spec.width` honoring the `-` (left) and `0` (zero-fill) flags.
/// Zero-fill only applies to right-justified numerics and goes after any sign.
///
/// The field width is a count of display *columns*, as every conversion shows:
/// `(format "%6s|" "中")` is `"    中|"`, `(format "%4c|" ?中)` is `"  中|"` and
/// `(format "%6S|" "中")` is `"  \"中\"|"` — each padded to six columns, not six
/// characters.
fn pad(body: String, spec: &FmtSpec) -> String {
    let body_cols = string_display_width(&body);
    if body_cols >= spec.width {
        return body;
    }
    let fill = spec.width - body_cols;
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

/// One run of a formatted result and where its text properties come from:
/// `Some(source)` at a char offset, or `None` for characters that carry none.
type FmtPiece = (Option<std::sync::Arc<String>>, usize, usize);

fn el_format(h: &ElispHost, a: &[Value]) -> Result<String, String> {
    el_format_pieces(h, a).map(|(s, _)| s)
}

/// `format`, also reporting where each run of the result came from so the
/// caller can carry text properties onto it. Emacs propagates two things: the
/// properties of the format string's own literal text, and those of a `%s`
/// argument, each onto the characters they produced. Padding carries none.
fn el_format_pieces(h: &ElispHost, a: &[Value]) -> Result<(String, Vec<FmtPiece>), String> {
    let fmt = match h.str_text(&a[0]) {
        Some(s) => s.to_string(),
        None => {
            return Err(format!(
                "wrong-type-argument: stringp {}",
                h.print(&a[0], true)
            ))
        }
    };
    // Runs of the result, in order. Everything but a `%s` argument's own
    // characters carries no properties: the format string's literal text and
    // the padding are `None` pieces. (Emacs also propagates the *format
    // string's* properties onto its literal text; that half is not modelled —
    // see BUGS.md.)
    let mut pieces: Vec<FmtPiece> = Vec::new();
    // How much of `out` is already accounted for by a piece.
    let mut placed = 0usize;
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
        // A `%` with no conversion character after it is an error, not a literal
        // `%`: `(format "abc%")` signals in Emacs and answered "abc%" here.
        let Some(conv) = chars.next() else {
            return Err("Format string ends in middle of format specifier".to_string());
        };
        // editfns.c `styled_format`: a `%` conversion copies a `%` whatever the
        // flags, width and precision before it, and consumes no argument — but a
        // field number still repositions the argument counter.
        if conv == '%' {
            if let Some(f) = field {
                ai = f;
            }
            out.push('%');
            continue;
        }
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
        // The integer directives (%d/%o/%x/%X) print the *exact* value: a bignum
        // must not be truncated to an i64. A float argument truncates toward
        // zero, as in Emacs.
        let bigf = |idx: usize| -> Result<BigInt, String> {
            let arg = a.get(idx).ok_or_else(|| NOT_ENOUGH.to_string())?;
            match arg {
                Value::Float(f) => <BigInt as num_traits::FromPrimitive>::from_f64(f.trunc())
                    .ok_or_else(|| BAD_TYPE.to_string()),
                _ => as_int_exact(h, arg).map_err(|_| BAD_TYPE.to_string()),
            }
        };
        // A `%s` of a string argument reproduces that string's characters, so
        // its properties belong on them. Truncation by a precision drops the
        // tail but keeps the head's, which the offsets below already express.
        let carries: Option<std::sync::Arc<String>> = match conv {
            's' => a.get(idx).and_then(|v| h.str_arc(v)),
            _ => None,
        };
        let body = match conv {
            's' => {
                let arg = a.get(idx).ok_or_else(|| NOT_ENOUGH.to_string())?;
                let s = h.print_checked(arg, false)?;
                match spec.prec {
                    Some(p) => truncate_to_columns(&s, p),
                    None => s,
                }
            }
            'S' => {
                let arg = a.get(idx).ok_or_else(|| NOT_ENOUGH.to_string())?;
                let s = h.print_checked(arg, true)?;
                // `%S`'s precision truncates the *printed* representation, quote
                // marks included: `(format "%.3S" "中中中")` is `"\"中"` in
                // Emacs 30.2 — one column for the opening quote, two for 中.
                match spec.prec {
                    Some(p) => truncate_to_columns(&s, p),
                    None => s,
                }
            }
            // The `+`/space sign flags apply to the signed conversions (d/e/f/g).
            // `%i` is an accepted alias for `%d` (as in C printf).
            'd' | 'i' => match a.get(idx) {
                // A non-finite float renders as the word "nan"/"inf"/"-inf",
                // exactly like %e/%f/%g (editfns.c styled_format): precision and
                // the `0` flag are ignored (space-padded to width), and the
                // `+`/space sign flags apply to infinities but never to NaN.
                Some(Value::Float(f)) if !f.is_finite() => {
                    spec.zero = false;
                    if f.is_nan() {
                        "nan".to_string()
                    } else if *f < 0.0 {
                        "-inf".to_string()
                    } else {
                        apply_sign("inf".to_string(), &spec)
                    }
                }
                _ => {
                    use num_traits::Signed;
                    let n = bigf(idx)?;
                    let mag = pad_digits(&n.abs().to_string(), spec.prec);
                    apply_sign(
                        if n.is_negative() {
                            format!("-{mag}")
                        } else {
                            mag
                        },
                        &spec,
                    )
                }
            },
            // The unsigned radix conversions have no word rendering for a
            // non-finite float: Emacs signals `(overflow-error)` (a NaN/Inf has
            // no integer value to print).
            'o' | 'x' | 'X' => {
                if let Some(Value::Float(f)) = a.get(idx) {
                    if !f.is_finite() {
                        return Err("overflow-error".to_string());
                    }
                }
                let upper = conv == 'X';
                let radix = if conv == 'o' { 8 } else { 16 };
                format_radix(&bigf(idx)?, radix, upper, spec.alt, spec.prec)
            }
            'c' => {
                // %c takes a character: any float — even an integral one — is a
                // type mismatch in Emacs (`CHARACTERP` is integers only).
                let (i, _, is_float) = numf(idx)?;
                if is_float {
                    return Err(BAD_TYPE.to_string());
                }
                // Emacs applies `CHECK_CHARACTER` to the argument (editfns.c
                // `styled_format`), so an integer outside `0 … #x3FFFFF` signals
                // `(wrong-type-argument characterp N)` — it is not silently
                // dropped, and the error is `wrong-type-argument`, not the
                // "Format specifier doesn't match argument type" a float gets.
                if !(0..=0x3F_FFFF).contains(&i) {
                    return Err(format!("wrong-type-argument: characterp {i}"));
                }
                char::from_u32(i as u32)
                    .map(String::from)
                    .unwrap_or_default()
            }
            'e' | 'f' | 'g' => {
                let v = numf(idx)?.1;
                if v.is_finite() {
                    let raw = match conv {
                        'e' => format_e(v, spec.prec.unwrap_or(6)),
                        'f' => format_fixed(v, spec.prec.unwrap_or(6)),
                        _ => format_g(v, spec.prec.unwrap_or(6), spec.alt),
                    };
                    // printf's `#`: the result always has a decimal point, even
                    // with no digits after it (`%#.0f` of 1.0 is "1.").
                    let raw = if spec.alt {
                        with_decimal_point(raw)
                    } else {
                        raw
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
        // A field number repositions the counter: `(format "%2$s %s" 1 2 3)` is "2 3".
        ai = idx + 1;
        // Literal format-string text since the last directive carries nothing.
        let before = out.chars().count();
        if before > placed {
            pieces.push((None, 0, before - placed));
        }
        let body_len = body.chars().count();
        let padded = pad(body, &spec);
        out.push_str(&padded);
        // `pad` adds spaces on one side. Emacs puts padding that follows the
        // argument *inside* its interval — `(format "%-10s|" (propertize "ab"
        // 'p 1))` is propertized over all ten columns — while padding that
        // precedes it stays outside, so only the trailing kind carries.
        let pad_len = padded.chars().count() - body_len;
        if !spec.left && pad_len > 0 {
            pieces.push((None, 0, pad_len));
        }
        pieces.push((carries.clone(), 0, body_len));
        if spec.left && pad_len > 0 {
            match (&carries, body_len) {
                // The trailing padding continues the last character's plist.
                (Some(src), n) if n > 0 => {
                    for _ in 0..pad_len {
                        pieces.push((Some(std::sync::Arc::clone(src)), n - 1, 1));
                    }
                }
                _ => pieces.push((None, 0, pad_len)),
            }
        }
        placed = out.chars().count();
    }
    let total = out.chars().count();
    if total > placed {
        pieces.push((None, 0, total - placed));
    }
    Ok((out, pieces))
}
fn format_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (s, pieces) = el_format_pieces(h, a)?;
    let (out, key) = h.new_string_keyed(s);
    h.string_carry_props(&key, &pieces);
    Ok(out)
}
/// `(message FORMAT-STRING &rest ARGS)`. Port of `Fmessage` (`src/xdisp.c`) plus
/// the batch tail it reaches, `message_to_stderr`:
///
/// ```c
///   if (NILP (args[0]) || (STRINGP (args[0]) && SBYTES (args[0]) == 0))
///     { message1 (0); return args[0]; }
///   else { val = Fformat_message (nargs, args); message3 (val); return val; }
///
///   /* message_to_stderr (m) */
///   if (noninteractive_need_newline)
///     { noninteractive_need_newline = false; errputc ('\n'); }
///   if (STRINGP (m)) errwrite (SDATA (s), SBYTES (s));
///   if (STRINGP (m) || !cursor_in_echo_area) errputc ('\n');
/// ```
///
/// Three things that a bare `eprintln!("{}", format(...))` gets wrong:
///
/// - nil (and "") clear the echo area and answer the argument unchanged, so
///   `(message nil)` is nil, not `(wrong-type-argument stringp nil)`.
/// - the template is curve-quoted (`format-message`, not `format`).
/// - a pending `noninteractive_need_newline` — set by any batch write to stdout —
///   is flushed to stderr first, so a `princ` and a following `message` do not
///   share a line. `cursor_in_echo_area` is always false in batch, so the
///   trailing newline is unconditional and nil therefore emits *two*.
fn message_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let blank = match a.first() {
        Some(v) if is_nil(v) => true,
        Some(v) => h.str_text(v).is_some_and(str::is_empty),
        None => false,
    };
    let text = if blank {
        None
    } else {
        Some(el_format_message(h, a)?)
    };
    if h.need_newline {
        h.need_newline = false;
        eprintln!();
    }
    match text {
        Some(s) => {
            eprintln!("{s}");
            Ok(h.new_string(s))
        }
        None => {
            eprintln!();
            Ok(a.first().cloned().unwrap_or(Value::Undef))
        }
    }
}
fn princ_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let dest = print_dest(h, a.get(1))?;
    let s = h.print_checked(&a[0], false)?;
    print_to(h, &dest, &s)?;
    Ok(a[0].clone())
}
fn prin1_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let dest = print_dest(h, a.get(1))?;
    let s = h.print_checked(&a[0], true)?;
    print_to(h, &dest, &s)?;
    Ok(a[0].clone())
}
fn number_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    as_number_p(h, &a[0], false)?;
    Ok(h.new_string(h.print(&a[0], false)))
}

// ── hash tables ──
fn hash_eq(h: &ElispHost, test: u8, a: &Value, b: &Value) -> bool {
    match test {
        // `equal` test: deep structural equality.
        2 => el_equal(h, a, b),
        // `eql` test (the make-hash-table default): like `eq` but equal floats and
        // equal bignums compare the same, so a float or bignum key put with
        // `puthash` is found again by `gethash`.
        1 => el_eql(h, a, b),
        // `eq` test: identity.
        _ => el_eq(h, a, b),
    }
}

/// How deep [`hash_key`] descends into an `equal`-test key.
///
/// The cap is what makes a circular key terminate. It only weakens the hash:
/// two keys that agree down to this depth land in one bucket and are then told
/// apart by `hash_eq`, so the answer stays exact.
const HASH_DEPTH_LIMIT: u32 = 8;

/// A hash of KEY consistent with [`hash_eq`] for TEST: keys the test calls equal
/// always hash alike. The converse is not required — a collision costs one extra
/// `hash_eq` call, never a wrong answer.
fn hash_key(h: &ElispHost, test: u8, v: &Value) -> u64 {
    use std::hash::Hasher;
    let mut st = std::collections::hash_map::DefaultHasher::new();
    hash_into(h, test, v, 0, &mut st);
    st.finish()
}

fn hash_into(h: &ElispHost, test: u8, v: &Value, depth: u32, st: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    if is_nil(v) {
        st.write_u8(0);
        return;
    }
    if depth > HASH_DEPTH_LIMIT {
        st.write_u8(0xFF);
        return;
    }
    match v {
        Value::Int(n) => {
            st.write_u8(1);
            st.write_i64(*n);
        }
        // A float is `eql`/`equal` by bit pattern, so hash the bits. Under `eq`
        // no two floats are equal at all, so any hash is consistent.
        Value::Float(f) => {
            st.write_u8(2);
            st.write_u64(f.to_bits());
        }
        Value::Bool(b) => {
            st.write_u8(3);
            st.write_u8(u8::from(*b));
        }
        // A string hashes by CONTENT under every test: `equal` requires it, and
        // under `eq`/`eql` the only pair that must agree is a string with
        // itself, which content also gives.
        Value::Str(txt) => {
            st.write_u8(4);
            txt.as_str().hash(st);
        }
        // A string CELL hashes as its text, not as its handle: `equal` compares
        // the text, so two distinct cells holding "ab" must share a bucket.
        Value::Obj(_) if h.is_string(v) => {
            st.write_u8(4);
            h.str_text(v).unwrap_or_default().hash(st);
        }
        Value::Obj(id) => match h.arena.get(*id as usize) {
            // A bignum is `eql` by VALUE, so its handle cannot be the hash.
            Some(Obj::Bignum(b)) => {
                st.write_u8(5);
                b.to_string().hash(st);
            }
            // Only the `equal` test looks inside a container; under `eq`/`eql`
            // two containers are equal only when they are the same object.
            Some(Obj::Cons(a, d)) if test == 2 => {
                st.write_u8(6);
                hash_into(h, test, a, depth + 1, st);
                hash_into(h, test, d, depth + 1, st);
            }
            Some(Obj::Vector(items)) | Some(Obj::Record(items)) if test == 2 => {
                st.write_u8(7);
                st.write_usize(items.len());
                for it in items.iter() {
                    hash_into(h, test, it, depth + 1, st);
                }
            }
            Some(Obj::BoolVector(bits)) if test == 2 => {
                st.write_u8(8);
                bits.hash(st);
            }
            // Two markers are `equal` when they share a buffer and a position,
            // which is not readable from here — put every marker in one bucket
            // and let `hash_eq` decide.
            Some(Obj::Marker(_)) if test == 2 => st.write_u8(9),
            // Two closures are `equal` when their arglist, body and captures
            // are, so the handle cannot be the hash under the `equal` test.
            Some(Obj::Closure { .. }) if test == 2 => {
                st.write_u8(12);
                if let Some((is_macro, dynamic, arglist, body, captures)) = h.closure_parts(v) {
                    st.write_u8(u8::from(is_macro));
                    st.write_u8(u8::from(dynamic));
                    hash_into(h, test, &arglist, depth + 1, st);
                    st.write_usize(body.len());
                    for form in &body {
                        hash_into(h, test, form, depth + 1, st);
                    }
                    st.write_usize(captures.len());
                    for (sym, val) in &captures {
                        st.write_u32(*sym);
                        hash_into(h, test, val, depth + 1, st);
                    }
                }
            }
            _ => {
                st.write_u8(10);
                st.write_u32(*id);
            }
        },
        _ => st.write_u8(11),
    }
}

pub(crate) fn ht_ref<'a>(h: &'a ElispHost, v: &Value) -> Result<&'a ElHashTable, String> {
    match h.obj(v) {
        Some(Obj::HashTable(t)) => Ok(t),
        _ => Err(format!(
            "wrong-type-argument: hash-table-p {}",
            h.print(v, true)
        )),
    }
}

fn ht_mut<'a>(h: &'a mut ElispHost, v: &Value) -> Option<&'a mut ElHashTable> {
    match v {
        Value::Obj(id) => match h.arena.get_mut(*id as usize) {
            Some(Obj::HashTable(t)) => Some(t),
            _ => None,
        },
        _ => None,
    }
}

/// Fill a table's hash index if it arrived from a serialized image without one.
/// See [`ElHashTable::needs_index`].
fn ht_ensure_index(h: &mut ElispHost, v: &Value) -> Result<(), String> {
    let t = ht_ref(h, v)?;
    if !t.needs_index() {
        return Ok(());
    }
    let test = t.test;
    let keys: Vec<(u32, Value)> = t
        .slots
        .iter()
        .enumerate()
        .filter_map(|(i, slot)| slot.as_ref().map(|(k, _)| (i as u32, k.clone())))
        .collect();
    let hashes: Vec<(u32, u64)> = keys
        .iter()
        .map(|(i, k)| (*i, hash_key(h, test, k)))
        .collect();
    if let Some(t) = ht_mut(h, v) {
        t.reindex(&hashes);
    }
    Ok(())
}

/// The slot holding KEY, and KEY's hash — the shared front half of `gethash`,
/// `puthash` and `remhash`.
fn ht_find(h: &mut ElispHost, table: &Value, key: &Value) -> Result<(u64, Option<u32>), String> {
    ht_ensure_index(h, table)?;
    let t = ht_ref(h, table)?;
    let test = t.test;
    let hk = hash_key(h, test, key);
    let t = ht_ref(h, table)?;
    let slot = t
        .candidates(hk)
        .iter()
        .copied()
        .find(|&i| t.key_at(i).is_some_and(|k| hash_eq(h, test, key, k)));
    Ok((hk, slot))
}

/// `(make-hash-table &rest KEYWORD-ARGS)`.
///
/// `user` is the `(NAME TESTFN HASHFN)` of a `define-hash-table-test` test, when
/// the `:test` argument named one. Resolving that lives in
/// [`crate::host::call_function`] rather than here, because the declaration is
/// kept on the symbol's `hash-table-test` property — an elisp plist, so reading
/// it CALLS elisp, which cannot happen inside this host borrow.
/// `get_key_arg` (fns.c:4633-4647, emacs-30.2): the first index whose
/// PREDECESSOR is KEY and whose pair is still unconsumed, marking both as used.
///
/// The scan starts at 1 and returns the index of the VALUE, so a keyword in the
/// last slot has no pair and is never found — which is exactly why
/// `(make-hash-table :size)` reports a leftover argument instead of quietly
/// ignoring it. C returns 0 for "absent"; `Option` says it without the sentinel.
fn get_key_arg(h: &ElispHost, key: &str, a: &[Value], used: &mut [bool]) -> Option<usize> {
    for i in 1..a.len() {
        if !used[i - 1] && h.sym_name(&a[i - 1]).as_deref() == Some(key) {
            used[i - 1] = true;
            used[i] = true;
            return Some(i);
        }
    }
    None
}

pub(crate) fn make_hash_table_with(
    h: &mut ElispHost,
    a: &[Value],
    user: Option<(Value, Value, Value)>,
) -> R {
    // Port of `Fmake_hash_table' (fns.c:5749-5815, emacs-30.2). The keyword list
    // is NOT scanned pairwise. `get_key_arg' searches the whole vector for each
    // keyword the function knows, marking the pair it consumed, and only then
    // does a second pass reject whatever is left over. Both halves of that are
    // observable, and the pairwise loop this replaces had neither:
    //
    //   (make-hash-table 1)             (error "Invalid argument list" 1)
    //   (make-hash-table :size)         (error "Invalid argument list" :size)
    //   (make-hash-table :test 'eq :size)
    //                                   (error "Invalid argument list" :size)
    //   (make-hash-table 'foo 1)        (error "Invalid argument list" foo)
    //
    // where every one of them used to build a table and return it. `:size' and
    // `:weakness' validate their values too, and the obsolete `:rehash-size' /
    // `:rehash-threshold' are skipped WITH their value rather than rejected.
    let mut used = vec![false; a.len()];
    let test = match get_key_arg(h, ":test", a, &mut used) {
        None => 1u8, // eql default
        Some(i) => match h.sym_name(&a[i]).as_deref() {
            Some("eq") => 0,
            Some("equal") => 2,
            // A name the caller resolved to a user test; anything else
            // is `eql` (including the `eql` spelling itself).
            Some(other) if other != "eql" && user.is_some() => 3,
            _ => 1,
        },
    };
    // Consumed but unused: elisprs has no pure space, and a `:purecopy' left
    // unmarked would be rejected as a stray argument.
    let _ = get_key_arg(h, ":purecopy", a, &mut used);
    // `:size' is the initial allocation, reported back by `hash-table-size'
    // until the table outgrows it. `FIXNATP' rejects a negative or non-integer
    // size outright; nil selects the default, which is elisprs's 0.
    let size = match get_key_arg(h, ":size", a, &mut used) {
        None => 0usize,
        Some(i) => match &a[i] {
            v if crate::host::el_nil(v) => 0,
            Value::Int(n) if *n >= 0 => *n as usize,
            v => {
                let v = v.clone();
                return Err(h.signal_error_arg("Invalid hash table size", &v));
            }
        },
    };
    let weakness = match get_key_arg(h, ":weakness", a, &mut used) {
        None => Value::Undef,
        Some(i) => {
            let w = a[i].clone();
            hash_table_weakness_arg(h, &w)?
        }
    };
    // "Now, all args should have been used up, or there's a problem."
    let mut i = 0;
    while i < a.len() {
        if !used[i] {
            match h.sym_name(&a[i]).as_deref() {
                // Obsolete since Emacs 29 and ignored, along with the value that
                // follows it (the C bumps `i' a second time inside the loop).
                Some(":rehash-threshold") | Some(":rehash-size") => i += 1,
                _ => {
                    let v = a[i].clone();
                    return Err(h.signal_error_arg("Invalid argument list", &v));
                }
            }
        }
        i += 1;
    }
    let mut t = ElHashTable::new(test, size, weakness);
    if test == 3 {
        t.user_test = user;
    }
    Ok(h.alloc(Obj::HashTable(t)))
}
/// Validate a `:weakness` value the way `Fmake_hash_table` does: nil, `key`,
/// `value`, `key-or-value` and `key-and-value` are kept, `t` is the historical
/// spelling of `key-and-value` and is stored as that, anything else is
/// `(error "Invalid hash table weakness" W)`. The `#s(hash-table weakness W)`
/// reader goes through the same check.
pub(crate) fn hash_table_weakness_arg(h: &mut ElispHost, w: &Value) -> R {
    if matches!(w, Value::Bool(true)) {
        return Ok(h.intern("key-and-value"));
    }
    let named = h.sym_name(w);
    let ok = crate::host::el_nil(w)
        || matches!(
            named.as_deref(),
            Some("key") | Some("value") | Some("key-or-value") | Some("key-and-value")
        );
    if !ok {
        return Err(h.signal_error_arg("Invalid hash table weakness", w));
    }
    Ok(w.clone())
}

/// The `(NAME TESTFN HASHFN)` of TABLE's user-defined test, if it has one.
pub(crate) fn ht_user_test(table: &Value) -> Option<(Value, Value, Value)> {
    crate::host::with_host(|h| ht_ref(h, table).ok().and_then(|t| t.user_test.clone()))
}

/// [`ht_find`] for a table whose test is elisp.
///
/// This is the whole reason `gethash`/`puthash`/`remhash` are dispatched from
/// [`crate::host::call_function`]: HASHFN and TESTFN are elisp functions, and
/// calling one needs the host borrow released. So the walk takes the host in
/// short bursts — read the candidate slots out, drop the borrow, run the test —
/// rather than holding a `&mut ElispHost` across the probe.
///
/// Emacs (`hashfn_user_defined`, fns.c) hashes the value HASHFN *returns* with
/// the ordinary `equal` hash, so two keys HASHFN maps to `equal` values land in
/// one bucket and TESTFN then decides.
pub(crate) fn ht_find_user(
    table: &Value,
    key: &Value,
    testfn: &Value,
    hashfn: &Value,
) -> Result<(u64, Option<u32>), String> {
    use crate::host::{call_function, with_host};
    let hv = call_function(hashfn, std::slice::from_ref(key))?;
    let hk = with_host(|h| hash_key(h, 2, &hv));
    let cands: Vec<(u32, Value)> = with_host(|h| {
        let t = ht_ref(h, table)?;
        Ok::<_, String>(
            t.candidates(hk)
                .iter()
                .filter_map(|&i| t.key_at(i).map(|k| (i, k.clone())))
                .collect(),
        )
    })?;
    for (i, k) in cands {
        if crate::host::el_truthy(&call_function(testfn, &[key.clone(), k])?) {
            return Ok((hk, Some(i)));
        }
    }
    Ok((hk, None))
}

/// `gethash` on a user-test table (see [`ht_find_user`]).
pub(crate) fn gethash_user(a: &[Value], t: &(Value, Value, Value)) -> R {
    let (_, slot) = ht_find_user(&a[1], &a[0], &t.1, &t.2)?;
    crate::host::with_host(|h| match slot {
        Some(i) => Ok(ht_ref(h, &a[1])?
            .value_at(i)
            .cloned()
            .unwrap_or(Value::Undef)),
        None => Ok(a.get(2).cloned().unwrap_or(Value::Undef)),
    })
}

/// `puthash` on a user-test table. A key the test already matches keeps the
/// ORIGINAL key object and only its value is replaced, exactly as for the
/// built-in tests.
pub(crate) fn puthash_user(a: &[Value], t: &(Value, Value, Value)) -> R {
    let (hk, slot) = ht_find_user(&a[2], &a[0], &t.1, &t.2)?;
    crate::host::with_host(|h| {
        if let Some(tbl) = ht_mut(h, &a[2]) {
            match slot {
                Some(i) => tbl.set_value_at(i, a[1].clone()),
                None => tbl.insert(hk, a[0].clone(), a[1].clone()),
            }
        }
    });
    Ok(a[1].clone())
}

/// `remhash` on a user-test table.
pub(crate) fn remhash_user(a: &[Value], t: &(Value, Value, Value)) -> R {
    let (hk, slot) = ht_find_user(&a[1], &a[0], &t.1, &t.2)?;
    crate::host::with_host(|h| {
        if let (Some(i), Some(tbl)) = (slot, ht_mut(h, &a[1])) {
            tbl.remove(hk, i);
        }
    });
    Ok(Value::Undef)
}

pub(crate) fn gethash(h: &mut ElispHost, a: &[Value]) -> R {
    let (_, slot) = ht_find(h, &a[1], &a[0])?;
    match slot {
        Some(i) => Ok(ht_ref(h, &a[1])?
            .value_at(i)
            .cloned()
            .unwrap_or(Value::Undef)),
        None => Ok(a.get(2).cloned().unwrap_or(Value::Undef)),
    }
}
pub(crate) fn puthash(h: &mut ElispHost, a: &[Value]) -> R {
    let (hk, slot) = ht_find(h, &a[2], &a[0])?;
    if let Some(t) = ht_mut(h, &a[2]) {
        match slot {
            Some(i) => t.set_value_at(i, a[1].clone()),
            None => t.insert(hk, a[0].clone(), a[1].clone()),
        }
    }
    Ok(a[1].clone())
}
pub(crate) fn remhash(h: &mut ElispHost, a: &[Value]) -> R {
    let (hk, slot) = ht_find(h, &a[1], &a[0])?;
    if let (Some(i), Some(t)) = (slot, ht_mut(h, &a[1])) {
        t.remove(hk, i);
    }
    Ok(Value::Undef) // remhash always returns nil
}
fn clrhash(h: &mut ElispHost, a: &[Value]) -> R {
    if let Some(t) = ht_mut(h, &a[0]) {
        t.clear();
    }
    Ok(a[0].clone())
}
fn hash_table_count(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(ht_ref(h, &a[0])?.count() as i64))
}
fn hash_table_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::HashTable(_)))))
}
/// `(hash-table-test TABLE)` — the symbol naming TABLE's comparison test.
fn hash_table_test(h: &mut ElispHost, a: &[Value]) -> R {
    let t = ht_ref(h, &a[0])?;
    // A `define-hash-table-test` table reports the NAME it was made with, not
    // one of the three built-in test symbols.
    if let Some((name, _, _)) = &t.user_test {
        return Ok(name.clone());
    }
    let name = match t.test {
        0 => "eq",
        1 => "eql",
        _ => "equal",
    };
    Ok(h.intern(name))
}
/// `(hash-table-size TABLE)` — the current ALLOCATION size, not the entry count.
fn hash_table_size(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(ht_ref(h, &a[0])?.size as i64))
}
/// `(hash-table-weakness TABLE)` — the `:weakness` argument as given.
fn hash_table_weakness(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(ht_ref(h, &a[0])?.weakness.clone())
}
/// subr-x's `hash-table-keys`/`hash-table-values` are a `maphash` that PUSHES
/// onto a list and never reverses it, so both come back in REVERSE slot order.
fn hash_table_keys(h: &mut ElispHost, a: &[Value]) -> R {
    let mut keys: Vec<Value> = ht_ref(h, &a[0])?.pairs().map(|(k, _)| k.clone()).collect();
    keys.reverse();
    Ok(h.list_from(keys))
}
fn hash_table_values(h: &mut ElispHost, a: &[Value]) -> R {
    let mut vals: Vec<Value> = ht_ref(h, &a[0])?.pairs().map(|(_, v)| v.clone()).collect();
    vals.reverse();
    Ok(h.list_from(vals))
}
fn copy_hash_table(h: &mut ElispHost, a: &[Value]) -> R {
    // `Fcopy_hash_table` copies the table structure as it stands: the test
    // (a `define-hash-table-test` one included), weakness, size, slot order,
    // free list and hashes. Rehashing here would need the user test's HASHFN,
    // which is elisp, and would compact the free list Emacs keeps.
    ht_ensure_index(h, &a[0])?;
    let copy = ht_ref(h, &a[0])?.clone();
    Ok(h.alloc(Obj::HashTable(copy)))
}

// ── strings ──
/// `(substring-no-properties STRING &optional FROM TO)` — a property-free copy.
/// A C subr in Emacs (editfns.c) that demands `stringp` (never a vector, unlike
/// `substring`); index handling is then identical to `substring`.
fn substring_no_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if !h.is_string(&a[0]) {
        return Err(format!(
            "wrong-type-argument: stringp {}",
            h.print(&a[0], true)
        ));
    }
    // `substring` carries the slice's text properties; this subr is exactly
    // that slice without them.
    let out = substring(h, a)?;
    if let Some(s) = h.str_arc(&out) {
        h.string_clear_props(&s);
    }
    Ok(out)
}
fn substring(h: &mut ElispHost, a: &[Value]) -> R {
    // Emacs `substring` works on both strings and vectors (arrays), and checks
    // the array BEFORE the indices: (substring -1 1.5) is `arrayp -1`.
    enum Seq {
        Str(Vec<char>),
        Vec(Vec<Value>),
    }
    let seq = match h.str_text(&a[0]) {
        Some(s) => Seq::Str(s.chars().collect()),
        None => match h.obj(&a[0]) {
            Some(Obj::Vector(items)) => Seq::Vec(items.clone()),
            _ => {
                return Err(format!(
                    "wrong-type-argument: arrayp {}",
                    h.print(&a[0], true)
                ));
            }
        },
    };
    // FROM/TO must be fixnums: Emacs signals `integerp` rather than truncating
    // a float — and a bignum index draws the same signal.
    for idx in a.iter().skip(1) {
        if !is_nil(idx) && !matches!(idx, Value::Int(_)) {
            return Err(format!(
                "wrong-type-argument: integerp {}",
                h.print(idx, true)
            ));
        }
    }
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
        Seq::Str(c) => {
            let text = c[start as usize..end as usize].iter().collect::<String>();
            let src = h.str_arc(&a[0]);
            let (out, dst) = h.new_string_keyed(text);
            // The slice's characters keep the properties they had in the
            // original, re-based to the new string's start.
            if let Some(src) = src {
                h.string_carry_props(&dst, &[(Some(src), start as usize, (end - start) as usize)]);
            }
            Ok(out)
        }
        Seq::Vec(v) => Ok(h.alloc(Obj::Vector(v[start as usize..end as usize].to_vec()))),
    }
}
/// subr.el `split-string-default-separators` — space, formfeed, tab, newline,
/// carriage return and vertical tab, and nothing else. Notably NOT "Unicode
/// whitespace": `(split-string "a\u{a0}b")` is `("a\u{a0}b")` in Emacs, one
/// element, because a no-break space is not in this class.
pub(crate) const SPLIT_STRING_DEFAULT_SEPARATORS: &str = "[ \u{c}\t\n\r\u{b}]+";

/// subr.el `split-string`, ported including its `push-one` closure.
///
/// The previous implementation split with `Regex::split` and dropped TRIM on the
/// floor, which is three separate divergences: TRIM was never applied, never
/// type-checked (`(split-string "abc" "b" nil 97)` must be
/// `(wrong-type-argument stringp 97)`), and the default-separator path used
/// Rust's Unicode-aware `split_whitespace` instead of the ASCII-only regexp
/// above. Reproducing subr.el's index walk also reproduces its one sharp edge: a
/// leading TRIM whose match runs past the end of the segment leaves
/// `this-start > this-end`, and `substring` then signals
/// `(args-out-of-range "aXb" 2 1)` rather than silently yielding "".
fn split_string(h: &mut ElispHost, a: &[Value]) -> R {
    // `(keep-nulls (not (if separators omit-nulls t)))` — with the default
    // separators OMIT-NULLS is implicitly on; with an explicit SEPARATORS it is
    // off unless the 3rd argument says otherwise.
    let has_seps = a.len() > 1 && !is_nil(&a[1]);
    let keep_nulls = has_seps && a.get(2).is_none_or(is_nil);
    // `(rexp (or separators split-string-default-separators))`. `string-match`
    // type-checks the regexp before the string, so a bad SEPARATORS is reported
    // before a bad STRING: (split-string [1 2] 97) is `stringp 97`.
    let rexp = if has_seps {
        as_string(h, &a[1])?
    } else {
        SPLIT_STRING_DEFAULT_SEPARATORS.to_string()
    };
    let string = as_string(h, &a[0])?;
    let cf = case_fold_search(h);
    let re = compile_cf(h, &rexp, cf)?;
    // TRIM is only ever touched inside `push-one`, i.e. after STRING and
    // SEPARATORS have both been accepted.
    let trim = match a.get(3) {
        Some(v) if !is_nil(v) => Some(as_string(h, v)?),
        _ => None,
    };
    let trim_re = match &trim {
        Some(t) => Some((
            compile_cf(h, t, cf)?,
            // `(concat trim "\\'")` — anchored at the end of the SUBSTRING.
            compile_cf(h, &format!("{t}\\'"), cf)?,
        )),
        None => None,
    };

    let chars: Vec<char> = string.chars().collect();
    let len = chars.len();
    let mut out: Vec<String> = Vec::new();
    // `substring`'s args-out-of-range names the string itself; render it now, as
    // `push-one` below borrows `out` and cannot also hold the host.
    let string_readable = h.print(&Value::str(string.clone()), true);

    // `push-one`: trim both ends of [this_start, this_end) and keep what is left.
    let mut push_one = |this_start: usize, this_end: usize| -> Result<(), String> {
        let mut this_start = this_start;
        if let Some((head_re, tail_re)) = &trim_re {
            // "Discard the trim from start of this substring." The match is taken
            // against the WHOLE string from this-start, and only counts when it
            // begins exactly there — so a context-sensitive TRIM like "\\<a\\>"
            // sees the characters before the segment, as in Emacs.
            if let Some(sp) = run_match(head_re, &string, this_start) {
                if let Some((b, e)) = sp[0] {
                    if b == this_start {
                        this_start = e;
                    }
                }
            }
            if keep_nulls || this_start < this_end {
                if this_start > this_end {
                    return Err(format!(
                        "args-out-of-range: {} {this_start} {this_end}",
                        string_readable
                    ));
                }
                let mut this: String = chars[this_start..this_end].iter().collect();
                // "Discard the trim from end of this substring."
                if let Some(sp) = run_match(tail_re, &this, 0) {
                    if let Some((b, _)) = sp[0] {
                        let n = this.chars().count();
                        if b < n {
                            this = this.chars().take(b).collect();
                        }
                    }
                }
                // "Trimming could make it empty; check again."
                if keep_nulls || !this.is_empty() {
                    out.push(this);
                }
            }
            return Ok(());
        }
        if keep_nulls || this_start < this_end {
            out.push(chars[this_start..this_end].iter().collect());
        }
        Ok(())
    };

    let mut start = 0usize;
    let mut notfirst = false;
    let mut match_begin = 0usize;
    loop {
        // `(if (and notfirst (= start (match-beginning 0)) (< start (length string)))
        //      (1+ start) start)` — step past a zero-width separator match.
        let from = if notfirst && start == match_begin && start < len {
            start + 1
        } else {
            start
        };
        let Some(spans) = run_match(&re, &string, from) else {
            break;
        };
        let Some((mb, me)) = spans[0] else { break };
        // `string-match` has already run (and set the match data) before the
        // `(< start (length string))` conjunct is tested, so MATCH_BEGIN updates
        // even on the iteration that ends the loop.
        match_begin = mb;
        if start >= len {
            break;
        }
        notfirst = true;
        let (this_start, this_end) = (start, mb);
        start = me;
        push_one(this_start, this_end)?;
    }
    // "Handle the substring at the end of STRING."
    push_one(start, len)?;
    Ok(h.list_from(out.into_iter().map(Value::str).collect()))
}
/// The `(length V)` the Lisp definitions of `string-prefix-p`/`string-suffix-p`
/// take before any string check — with `length`'s own `sequencep`/`listp`
/// signals for a non-sequence or an improper list.
fn emacs_length(h: &mut ElispHost, v: &Value) -> Result<i64, String> {
    match length_fn(h, std::slice::from_ref(v))? {
        Value::Int(n) => Ok(n),
        _ => Ok(0), // length_fn only ever returns an Int on success
    }
}
/// `compare-strings`' per-char case fold: simple one-char lowercasing.
fn cs_fold(ignore_case: bool) -> impl Fn(char) -> char {
    move |c| {
        if ignore_case {
            c.to_lowercase().next().unwrap_or(c)
        } else {
            c
        }
    }
}
fn string_prefix_p(h: &mut ElispHost, a: &[Value]) -> R {
    // subr.el: (if (> (length prefix) (length string)) nil
    //            (eq t (compare-strings prefix 0 prefix-length string 0 …)))
    // Both lengths are taken before any string check — so a non-sequence
    // signals `sequencep` (PREFIX first), a too-long PREFIX answers nil even
    // for a non-string STRING, and only then does `compare-strings` signal
    // `stringp` (again PREFIX first).
    let plen = emacs_length(h, &a[0])?;
    let slen = emacs_length(h, &a[1])?;
    if plen > slen {
        return Ok(Value::Undef);
    }
    let pre = as_string(h, &a[0])?;
    let s = as_string(h, &a[1])?;
    let fold = cs_fold(a.get(2).is_some_and(|v| !is_nil(v)));
    Ok(nil_or(
        pre.chars()
            .map(&fold)
            .eq(s.chars().take(plen as usize).map(&fold)),
    ))
}
fn string_suffix_p(h: &mut ElispHost, a: &[Value]) -> R {
    // subr.el: (let ((start-pos (- (length string) (length suffix))))
    //          (and (>= start-pos 0) (eq t (compare-strings suffix nil nil …))))
    // STRING's length is taken first (so (string-suffix-p 97 0) signals
    // `sequencep 0`, not about 97), a SUFFIX longer than STRING answers nil
    // before any string check, and `compare-strings` signals `stringp` for
    // SUFFIX first.
    let slen = emacs_length(h, &a[1])?;
    let suflen = emacs_length(h, &a[0])?;
    if slen < suflen {
        return Ok(Value::Undef);
    }
    let suf = as_string(h, &a[0])?;
    let s = as_string(h, &a[1])?;
    let fold = cs_fold(a.get(2).is_some_and(|v| !is_nil(v)));
    Ok(nil_or(
        suf.chars()
            .map(&fold)
            .eq(s.chars().skip((slen - suflen) as usize).map(&fold)),
    ))
}
fn string_empty_p(h: &mut ElispHost, a: &[Value]) -> R {
    // simple.el: (string= STRING "") — so a symbol compares by name
    // ((string-empty-p nil) => nil, no error) and anything else draws
    // `string=`'s `stringp` signal.
    match &a[0] {
        v if h.is_string(v) => Ok(nil_or(h.str_text(v).is_some_and(str::is_empty))),
        v if is_nil(v) => Ok(Value::Undef), // symbol name "nil" is non-empty
        v => match h.sym_name(v) {
            Some(name) => Ok(nil_or(name.is_empty())),
            None => Err(format!("wrong-type-argument: stringp {}", h.print(v, true))),
        },
    }
}
fn string_join(h: &mut ElispHost, a: &[Value]) -> R {
    // subr-x: (mapconcat #'identity strings separator) — mapconcat takes
    // `(length STRINGS)` up front, so an improper list signals `listp TAIL`
    // ((string-join (cons '- 9) 1.5) => (wrong-type-argument listp 9)) and a
    // non-sequence signals `sequencep`, before any element is looked at.
    emacs_length(h, &a[0])?;
    let items = h
        .seq_vec(&a[0])
        .ok_or_else(|| format!("wrong-type-argument: sequencep {}", h.print(&a[0], true)))?;
    let sep = a.get(1).cloned().unwrap_or(Value::Undef);
    // Interleave the separator and concatenate: `concat`'s element rules (and its
    // `sequencep` error) are exactly Emacs's here.
    let mut parts: Vec<Value> = Vec::with_capacity(items.len() * 2);
    for (i, it) in items.into_iter().enumerate() {
        if i > 0 && !is_nil(&sep) {
            parts.push(sep.clone());
        }
        parts.push(it);
    }
    concat_fn(h, &parts)
}
fn char_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_char(h, &a[0])?;
    Ok(h.new_string(char::from_u32(n).map(|c| c.to_string()).unwrap_or_default()))
}
fn string_to_char(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(
        as_string(h, &a[0])?
            .chars()
            .next()
            .map(|c| c as i64)
            .unwrap_or(0),
    ))
}
fn make_string(h: &mut ElispHost, a: &[Value]) -> R {
    let n = check_array_len_val(h, &a[0])?;
    let c = char::from_u32(as_char(h, &a[1])?).unwrap_or(' ');
    // Fallible allocation, as in `make_vector`: `n * len_utf8` may overflow
    // `usize` or exceed available memory. Emacs signals a plain `error` rather
    // than aborting the process, so reserve up front and map failure to it.
    let bytes = n
        .checked_mul(c.len_utf8())
        .ok_or_else(|| MEMORY_EXHAUSTED.to_string())?;
    let mut s = String::new();
    s.try_reserve_exact(bytes)
        .map_err(|_| MEMORY_EXHAUSTED.to_string())?;
    for _ in 0..n {
        s.push(c);
    }
    Ok(h.new_string(s))
}
/// `(string &rest CHARACTERS)` — a string of CHARACTERS.
///
/// Emacs `Fstring` runs `CHECK_CHARACTER` over every argument, so a non-character
/// signals `(wrong-type-argument characterp ARG)` — not `integerp`, which is what
/// reading the argument as a plain integer reports.
fn string_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut s = String::new();
    for v in a {
        let c = as_char(h, v)?;
        if let Some(c) = char::from_u32(c) {
            s.push(c);
        }
    }
    Ok(h.new_string(s))
}
fn string_to_list(h: &mut ElispHost, a: &[Value]) -> R {
    // Emacs defines this as `(append STRING nil)`, so it accepts any sequence and
    // signals `sequencep` — not `stringp` — on anything else.
    match h
        .str_text(&a[0])
        .map(|st| st.chars().map(|c| Value::Int(c as i64)).collect())
    {
        Some(items) => Ok(h.list_from(items)),
        None => match h.seq_vec(&a[0]) {
            Some(items) => Ok(h.list_from(items)),
            None => Err(format!(
                "wrong-type-argument: sequencep {}",
                h.print(&a[0], true)
            )),
        },
    }
}
fn string_search(h: &mut ElispHost, a: &[Value]) -> R {
    let needle = as_string(h, &a[0])?;
    let hay = as_string(h, &a[1])?;
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

/// `\sC` resolved against the live syntax table.
///
/// It borrows the host rather than reaching for the thread-local: every
/// `compile_cf` caller is already inside a subr holding `&mut ElispHost`, so a
/// `with_host` here re-enters the same `RefCell` and aborts with
/// "RefCell already borrowed" on the first `(string-match "\\s_" "-")`.
struct HostSyntax<'a> {
    host: &'a ElispHost,
    /// Set when the translation actually asked for a syntax class — i.e. the
    /// pattern used `\sC`, `\SC`, `\w`, `\W` or `\cC`. Such a translation is
    /// only valid for the syntax table in force at the time, so it is the one
    /// kind of pattern [`compile_cf`] must NOT cache.
    used_syntax: std::cell::Cell<bool>,
}
impl crate::regexp::SyntaxLookup for HostSyntax<'_> {
    fn ranges(&self, class: char) -> Vec<(u32, u32)> {
        self.used_syntax.set(true);
        self.host.syntax_class_ranges(class)
    }
}

/// Compile an elisp regexp to a `fancy_regex::Regex` (optionally case-insensitively,
/// for `case-fold-search`), surfacing translation and compilation failures as
/// elisp-style `invalid-regexp` errors.
/// A compiled elisp regexp: the `fancy_regex` program plus, when the pattern
/// used `\(?N:…\)`, the Emacs group number of each of its capture groups.
///
/// The map is `None` for the overwhelmingly common case of a pattern built only
/// from plain `\(`, where the two numberings already agree and `run_match` can
/// hand the spans straight through.
pub(crate) struct CompiledRe {
    re: fancy_regex::Regex,
    /// Element `i` is the Emacs group number of compiled group `i + 1`.
    emacs_groups: Option<Vec<u32>>,
}

impl std::ops::Deref for CompiledRe {
    type Target = fancy_regex::Regex;
    fn deref(&self) -> &fancy_regex::Regex {
        &self.re
    }
}

/// One `case-fold-search` setting's worth of compiled regexps. The value keeps
/// the FAILURE as well as the success, so an invalid pattern keeps signalling.
type ReCacheMap = HashMap<String, Result<Rc<CompiledRe>, String>>;

thread_local! {
    /// Compiled regexps, keyed by the elisp pattern — one map per
    /// `case-fold-search` setting, so the lookup can borrow the pattern instead
    /// of building a tuple key.
    ///
    /// Every `string-match`/`re-search-forward`/`looking-at` used to re-run the
    /// elisp→`fancy_regex` translation AND `fancy_regex::Regex::new` from
    /// scratch, so a match inside a loop paid a full regexp compile per
    /// iteration: 40 000 `(string-match "\\([a-z]+\\)-\\([0-9]+\\)" "abc-123")`
    /// took 56 s. Emacs keeps a compiled-pattern cache of its own for exactly
    /// this reason (`compile_pattern`/`searchbufs`, search.c).
    ///
    /// The FAILURE is cached with the success: an invalid pattern has to keep
    /// signalling `invalid-regexp` on every call, not silently start matching.
    static RE_CACHE: RefCell<[ReCacheMap; 2]> = RefCell::new([HashMap::new(), HashMap::new()]);
}

/// Entries kept before the cache is dropped wholesale. Bounded because elisp
/// builds patterns at runtime (`regexp-quote` of user input, `rx` expansions),
/// so an unbounded cache would grow with the program's input, not its code.
const RE_CACHE_MAX: usize = 1024;

pub(crate) fn compile_cf(
    h: &ElispHost,
    pat: &str,
    case_insensitive: bool,
) -> Result<Rc<CompiledRe>, String> {
    let slot = usize::from(case_insensitive);
    if let Some(hit) = RE_CACHE.with(|c| c.borrow()[slot].get(pat).cloned()) {
        return hit;
    }
    let syntax = HostSyntax {
        host: h,
        used_syntax: std::cell::Cell::new(false),
    };
    let compiled = compile_uncached(pat, case_insensitive, &syntax);
    // A pattern whose translation read the syntax table is valid only for the
    // table it read; caching it would answer with a stale character set after
    // `modify-syntax-entry` or a buffer switch.
    if !syntax.used_syntax.get() {
        RE_CACHE.with(|c| {
            let mut maps = c.borrow_mut();
            if maps[slot].len() >= RE_CACHE_MAX {
                maps[slot].clear();
            }
            maps[slot].insert(pat.to_string(), compiled.clone());
        });
    }
    compiled
}

fn compile_uncached(
    pat: &str,
    case_insensitive: bool,
    syntax: &HostSyntax<'_>,
) -> Result<Rc<CompiledRe>, String> {
    // `translate` reports Emacs's own diagnostics ("Unmatched [ or [^", …); they
    // ride under the `invalid-regexp` error symbol, as in Emacs.
    let (translated, groups) =
        crate::regexp::translate_groups(pat, syntax).map_err(|e| format!("invalid-regexp: {e}"))?;
    // Elisp `^`/`$` always match line boundaries, so compile in multiline mode;
    // `\``/`\'` (translated to \A/\z) keep matching the absolute start/end.
    let flags = if case_insensitive { "(?mi)" } else { "(?m)" };
    let full = format!("{flags}{translated}");
    let re = fancy_regex::Regex::new(&full).map_err(|e| format!("invalid-regexp: {e}"))?;
    // Identity numbering needs no remap, and skipping it keeps every ordinary
    // pattern on exactly the path it was on before explicit numbering existed.
    let identity = groups.iter().enumerate().all(|(i, &g)| g as usize == i + 1);
    Ok(Rc::new(CompiledRe {
        re,
        emacs_groups: (!identity).then_some(groups),
    }))
}

/// Drop every cached regexp. Called from `reset_host` so a fresh host never
/// inherits patterns compiled against the previous one's syntax table.
pub fn clear_regexp_cache() {
    RE_CACHE.with(|c| {
        let mut maps = c.borrow_mut();
        maps[0].clear();
        maps[1].clear();
    });
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
fn run_match(re: &CompiledRe, subject: &str, start: usize) -> Option<Vec<Option<(usize, usize)>>> {
    let start_byte = byte_of_char(subject, start);
    let caps = re.captures_from_pos(subject, start_byte).ok().flatten()?;
    let spans: Vec<Option<(usize, usize)>> = (0..caps.len())
        .map(|i| {
            caps.get(i).map(|m| {
                (
                    char_of_byte(subject, m.start()),
                    char_of_byte(subject, m.end()),
                )
            })
        })
        .collect();
    let Some(map) = &re.emacs_groups else {
        return Some(spans);
    };
    // Scatter the positional spans onto their Emacs group numbers. Groups the
    // pattern never names stay nil, which is what `(match-data)` reports for the
    // holes an explicit number leaves behind.
    let width = map.iter().copied().max().unwrap_or(0) as usize + 1;
    let mut out = vec![None; width];
    out[0] = spans.first().copied().flatten();
    for (i, &g) in map.iter().enumerate() {
        let Some(span) = spans.get(i + 1).copied().flatten() else {
            // Two groups may share one Emacs number (`\(?1:a\)\|\(?1:b\)`); in
            // Emacs they share one register, so the branch that did not match
            // must not erase the one that did.
            continue;
        };
        out[g as usize] = Some(span);
    }
    Some(out)
}

/// `(string-match REGEXP STRING &optional START)` — search STRING for REGEXP,
/// set the match data, and return the char index where the match begins (nil if
/// no match).
fn string_match(h: &mut ElispHost, a: &[Value]) -> R {
    let pat = as_string(h, &a[0])?;
    let subject = as_string(h, &a[1])?;
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
    let re = compile_cf(h, &pat, case_fold_search(h))?;
    match run_match(&re, &subject, start) {
        Some(spans) => {
            let begin = spans[0].map(|(b, _)| b as i64).unwrap_or(0);
            h.match_data = Some(MatchData {
                subject,
                spans,
                from_buffer: false,
                buffer: None,
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
    let explicit = a.get(1).and_then(|v| h.str_text(v)).map(str::to_string);
    if md.from_buffer && explicit.is_none() {
        return Ok(match span {
            Some((b, e)) => {
                let t = &h.cur_buf().text;
                let (lo, hi) = ((b - 1).min(t.len()), (e - 1).min(t.len()));
                let text = t[lo..hi].iter().collect::<String>();
                h.new_string(text)
            }
            None => Value::Undef,
        });
    }
    let Some(md) = h.match_data.as_ref() else {
        return Ok(Value::Undef);
    };
    let subject = match explicit {
        Some(s) => s,
        None => md.subject.clone(),
    };
    match span {
        Some((b, e)) => {
            let bb = byte_of_char(&subject, b);
            let eb = byte_of_char(&subject, e);
            Ok(h.new_string(subject.get(bb..eb).unwrap_or("").to_string()))
        }
        None => Ok(Value::Undef),
    }
}

/// Port of `Fmatch_data` (search.c): `(match-data &optional INTEGERS REUSE RESEAT)`.
///
/// A match made in a buffer is reported as MARKERS into that buffer (markers
/// pointing nowhere once it is killed), unless INTEGERS is non-nil, in which
/// case the positions are integers and the buffer itself is appended. Only up
/// to the last group that matched is reported. A cons REUSE receives the
/// values in place — surplus cells are set to nil, a shortfall is consed on —
/// and is the value; RESEAT first detaches the markers REUSE holds.
fn match_data_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let integers = a.first().is_some_and(|v| !is_nil(v));
    let reuse = a.get(1).cloned().unwrap_or(Value::Undef);
    if a.get(2).is_some_and(|v| !is_nil(v)) {
        let mut tail = reuse.clone();
        while let Some(Obj::Cons(car, cdr)) = h.obj(&tail).cloned() {
            if h.is_marker(&car) {
                h.set_marker_to(&car, None, 0)?;
                setcar(h, &[tail.clone(), Value::Undef])?;
            }
            tail = cdr;
        }
    }
    let Some(md) = h.match_data.clone() else {
        return Ok(Value::Undef);
    };
    let live = md.buffer.filter(|&bi| h.buffers[bi].name.is_some());
    let mut data = Vec::with_capacity(md.spans.len() * 2 + 1);
    let mut len = 0;
    for span in &md.spans {
        match *span {
            Some((b, e)) => {
                if md.buffer.is_none() || integers {
                    data.push(Value::Int(b as i64));
                    data.push(Value::Int(e as i64));
                } else {
                    data.push(h.alloc_marker(live, if live.is_some() { b } else { 0 }, false));
                    data.push(h.alloc_marker(live, if live.is_some() { e } else { 0 }, false));
                }
                len = data.len();
            }
            None => {
                data.push(Value::Undef);
                data.push(Value::Undef);
            }
        }
    }
    data.truncate(len);
    if let (Some(bi), true) = (md.buffer, integers) {
        data.push(h.buffer_object(bi));
    }
    if !matches!(h.obj(&reuse), Some(Obj::Cons(..))) {
        return Ok(h.list_from(data));
    }
    let mut items = data.into_iter();
    let mut tail = reuse.clone();
    let mut prev = tail.clone();
    while let Some(Obj::Cons(_, cdr)) = h.obj(&tail).cloned() {
        setcar(h, &[tail.clone(), items.next().unwrap_or(Value::Undef)])?;
        prev = tail;
        tail = cdr;
    }
    let rest: Vec<Value> = items.collect();
    if !rest.is_empty() {
        let rest = h.list_from(rest);
        setcdr(h, &[prev, rest])?;
    }
    Ok(reuse)
}

/// Port of `Fset_match_data` (search.c): `(set-match-data LIST &optional RESEAT)`.
///
/// LIST is what `match-data` produces: integer or marker pairs, nil pairs for
/// groups that did not match, optionally ending in a buffer. A marker or a
/// trailing buffer makes the data a buffer match in that buffer; a marker
/// pointing nowhere reads as 0. RESEAT detaches each marker once read.
fn set_match_data(h: &mut ElispHost, a: &[Value]) -> R {
    let list = a[0].clone();
    if !is_nil(&list) && !matches!(h.obj(&list), Some(Obj::Cons(..))) {
        return Err(h.signal_wrong_type("listp", &list));
    }
    let reseat = a.get(1).is_some_and(|v| !is_nil(v));
    let flat = h.list_vec(&list).unwrap_or_default();
    let pairs = flat.len() / 2;
    let mut buffer: Option<usize> = None;
    let mut spans: Vec<Option<(usize, usize)>> = Vec::with_capacity(pairs);
    let pos = |h: &mut ElispHost, v: &Value, buffer: &mut Option<usize>| -> Result<i64, String> {
        if h.is_marker(v) {
            let p = match h.marker_buffer(v) {
                Some(bv) => {
                    if let Some(Obj::Buffer(bi)) = h.obj(&bv) {
                        *buffer = Some(*bi);
                    }
                    h.marker_position(v).unwrap_or(0) as i64
                }
                None => 0,
            };
            if reseat {
                h.set_marker_to(v, None, 0)?;
            }
            return Ok(p);
        }
        as_int_or_marker(h, v, "integer-or-marker-p")
    };
    let mut i = 0;
    while i < flat.len() {
        if let Some(Obj::Buffer(bi)) = h.obj(&flat[i]) {
            buffer = Some(*bi);
            break;
        }
        if spans.len() >= pairs {
            break;
        }
        if is_nil(&flat[i]) {
            // A nil start skips its (unread) end as well.
            spans.push(None);
            i += 2;
            continue;
        }
        let from = pos(h, &flat[i], &mut buffer)?;
        let Some(end_v) = flat.get(i + 1).cloned() else {
            break;
        };
        let to = pos(h, &end_v, &mut buffer)?;
        spans.push(Some((from.max(0) as usize, to.max(0) as usize)));
        i += 2;
    }
    while spans.last().is_some_and(|s| s.is_none()) {
        spans.pop();
    }
    if spans.is_empty() {
        h.match_data = None;
        return Ok(Value::Undef);
    }
    let subject = h
        .match_data
        .as_ref()
        .map(|m| m.subject.clone())
        .unwrap_or_default();
    h.match_data = Some(MatchData {
        subject,
        spans,
        from_buffer: buffer.is_some(),
        buffer,
    });
    Ok(Value::Undef)
}

/// `(regexp-quote STRING)` — STRING with every regexp-special character escaped
/// so it matches literally under elisp regexp rules.
fn regexp_quote(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(h, &a[0])?;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        // The set Emacs's own `regexp-quote` escapes (search.c Fregexp_quote:
        // `*.\?+[^$`). Notably `]` is NOT escaped.
        if matches!(c, '.' | '*' | '+' | '?' | '[' | '^' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    Ok(h.new_string(out))
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

// `replace-regexp-in-string` is a Lisp function in Emacs (subr.el) and here
// (prelude.rs): its argument checks, function-valued REP, and zero-width-match
// stepping all fall out of the Lisp definition, which a Rust reimplementation
// kept getting subtly wrong (it type-checked REP eagerly and appended a
// replacement for the empty match at end-of-string).

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
fn quotient(h: &mut ElispHost, a: &[Value], rm: Rm) -> R {
    // `floor`/`ceiling`/`round`/`truncate` signal `numberp`, not
    // `number-or-marker-p`.
    // Integer operands stay exact — `(floor (expt 2 70) 3)` is a bignum, and
    // `(truncate 1e30)` is the exact integer value of that float, not a clamped
    // i64.
    let xn = as_number_p(h, &a[0], false)?;
    match a.get(1) {
        Some(d) if !is_nil(d) => {
            let dn = as_number_p(h, d, false)?;
            if let (Num::Int(x), Num::Int(y)) = (&xn, &dn) {
                if *y == BigInt::from(0) {
                    return Err("arith-error: division by zero".to_string());
                }
                let q = big_div(x, y, rm);
                return Ok(h.make_integer(q));
            }
            // A float on either side. Emacs does NOT round the `f64` quotient —
            // it divides *exactly*, because every finite float is an exact
            // dyadic rational. `(floor 1e30 3)' is 333333333333333339961541612885
            // (the exact integer 1e30 denotes, divided by 3), not
            // 333333333333333316505293553664 (`floor' of the `f64' quotient),
            // and not a saturated `i64::MAX'.
            let df = dn.to_f64();
            if df == 0.0 {
                return Err("arith-error: division by zero".to_string());
            }
            if matches!(xn, Num::Float(f) if !f.is_finite()) {
                // A non-finite NUMERATOR has no integer value at all.
                // (floor 1.0e+INF 2) and (floor 0.0e+NaN 2) => overflow-error.
                return Err("overflow-error".to_string());
            }
            if df.is_nan() {
                // (floor 2 0.0e+NaN) => overflow-error, but an *infinite*
                // divisor is not an error: Emacs rounds the exactly-zero
                // quotient, so (ceiling 2 1.0e+INF) is 0, not 1.
                return Err("overflow-error".to_string());
            }
            if df.is_infinite() {
                return Ok(Value::Int(0));
            }
            let (xnum, xden) = exact_ratio(&xn);
            let (dnum, dden) = exact_ratio(&dn);
            Ok(h.make_integer(big_div(&(xnum * dden), &(xden * dnum), rm)))
        }
        _ => match xn {
            Num::Int(i) => Ok(h.make_integer(i)),
            Num::Float(f) => {
                if !f.is_finite() {
                    // (truncate 1.0e+INF), (round (/ 0.0 0.0)) => overflow-error.
                    return Err("overflow-error".to_string());
                }
                Ok(float_to_integer(h, apply_rm(f, rm)))
            }
        },
    }
}
/// Divide two exact integers with the given rounding mode (Emacs's `floor`,
/// `ceiling`, `round`, `truncate` with a DIVISOR).
fn big_div(x: &BigInt, y: &BigInt, rm: Rm) -> BigInt {
    use num_integer::Integer;
    let zero = BigInt::from(0);
    let (q, r) = x.div_rem(y);
    match rm {
        Rm::Trunc => q,
        Rm::Floor if r != zero && (r < zero) != (*y < zero) => q - 1,
        Rm::Ceil if r != zero && (r < zero) == (*y < zero) => q + 1,
        Rm::Round => {
            // Round half to even, as Emacs does: compare |2r| with |y|.
            use num_traits::Signed;
            let twice: BigInt = (&r * BigInt::from(2)).abs();
            let mag = y.abs();
            let away = match twice.cmp(&mag) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Equal => q.is_odd(),
                std::cmp::Ordering::Less => false,
            };
            let toward = if (r < zero) != (*y < zero) { -1 } else { 1 };
            if away {
                q + toward
            } else {
                q
            }
        }
        _ => q,
    }
}

/// A finite number as the exact fraction `num / den`, `den` always positive.
///
/// Every finite `f64` is a dyadic rational — mantissa times a power of two — so
/// this loses nothing. It is what lets `floor`/`ceiling`/`round`/`truncate`
/// divide a float by an integer exactly, the way Emacs does, instead of
/// rounding an already-lossy `f64` quotient.
fn exact_ratio(n: &Num) -> (BigInt, BigInt) {
    let one = BigInt::from(1);
    let f = match n {
        Num::Int(i) => return (i.clone(), one),
        Num::Float(f) => *f,
    };
    let bits = f.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    // A subnormal has no implicit leading 1 and a fixed exponent; a normal
    // number carries the hidden bit. 1075 = 1023 bias + 52 mantissa bits.
    let (mantissa, exp) = if biased == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), biased - 1075)
    };
    let mut num = BigInt::from(mantissa);
    if bits >> 63 == 1 {
        num = -num;
    }
    if exp >= 0 {
        (num << exp as usize, one)
    } else {
        (num, one << (-exp) as usize)
    }
}

/// An already-rounded `f64` as an exact elisp integer. A float beyond `i64` is a
/// bignum in Emacs: `(truncate 1e30)` is 1000000000000000019884624838656.
fn float_to_integer(h: &mut ElispHost, f: f64) -> Value {
    use num_traits::ToPrimitive;
    match f.to_i64() {
        Some(i) => h.make_integer(BigInt::from(i)),
        None => match <BigInt as num_traits::FromPrimitive>::from_f64(f) {
            Some(b) => h.make_integer(b),
            None => Value::Float(f),
        },
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
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64()))
}
/// The exact integer value of `v` for a bit/shift op, bignum included.
/// Emacs's bitwise ops reject floats (`integer-or-marker-p`) but accept any
/// integer, of any size: `(logand (expt 2 70) (expt 2 70))` is 2^70.
fn as_int_exact(h: &ElispHost, v: &Value) -> Result<BigInt, String> {
    as_int_exact_p(h, v, true)
}

/// [`as_int_exact`] with Emacs's predicate for the calling builtin: the bit-logic
/// ops (`logand`/`logior`/`logxor`/`%`) take a marker and signal
/// `integer-or-marker-p`, while the shifts and `lognot`/`logcount` take strictly
/// an integer and signal `integerp`.
/// The exact integer of a bit-logic argument (`logand`/`logior`/`logxor`), with
/// the predicate Emacs names for that *position*.
///
/// Emacs's `bit_op` checks the FIRST argument with a direct `CHECK_INTEGER`, so a
/// bad one there is always `integer-or-marker-p`. Each *later* argument is checked
/// for number-ness first, so a non-number there is `number-or-marker-p` while a
/// float is still `integer-or-marker-p`:
///
/// ```text
/// (logand "x" 2)  => (wrong-type-argument integer-or-marker-p "x")   ; first
/// (logand 2 "x")  => (wrong-type-argument number-or-marker-p  "x")   ; later
/// (logand 2 1.0)  => (wrong-type-argument integer-or-marker-p 1.0)   ; a number
/// ```
fn as_int_bitop(h: &ElispHost, v: &Value, first: bool) -> Result<BigInt, String> {
    if !first && !h.is_number(v) && h.marker_position(v).is_none() {
        return Err(format!(
            "wrong-type-argument: number-or-marker-p {}",
            h.print(v, true)
        ));
    }
    as_int_exact(h, v)
}

fn as_int_exact_p(h: &ElispHost, v: &Value, markers_ok: bool) -> Result<BigInt, String> {
    match v {
        Value::Int(n) => Ok(BigInt::from(*n)),
        _ => {
            if let Some(b) = h.as_bigint(v) {
                return Ok(b);
            }
            if markers_ok {
                if let Some(p) = h.marker_position(v) {
                    return Ok(BigInt::from(p));
                }
            }
            Err(format!(
                "wrong-type-argument: {} {}",
                if markers_ok {
                    "integer-or-marker-p"
                } else {
                    "integerp"
                },
                h.print(v, true)
            ))
        }
    }
}

fn logand_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut r = BigInt::from(-1);
    for (i, v) in a.iter().enumerate() {
        r &= as_int_bitop(h, v, i == 0)?;
    }
    Ok(h.make_integer(r))
}
fn logior_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut r = BigInt::from(0);
    for (i, v) in a.iter().enumerate() {
        r |= as_int_bitop(h, v, i == 0)?;
    }
    Ok(h.make_integer(r))
}
fn logxor_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let mut r = BigInt::from(0);
    for (i, v) in a.iter().enumerate() {
        r ^= as_int_bitop(h, v, i == 0)?;
    }
    Ok(h.make_integer(r))
}
fn lognot_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_int_exact_p(h, &a[0], false)?;
    Ok(h.make_integer(!n))
}
/// `(ash VALUE COUNT)` — arithmetic shift. A left shift is exact: Emacs's
/// integers are unbounded, so `(ash 1 70)` is 2^70, not a wrapped fixnum.
/// The `integer-width` bound in bits: how wide a bignum Emacs will build before
/// signalling `overflow-error`. Defaults to 65536, as `emacs -Q` reports.
fn integer_width(h: &ElispHost) -> u64 {
    h.find_symbol("integer-width")
        .and_then(|s| h.get_value(&s).ok())
        .and_then(|v| match v {
            Value::Int(n) if n > 0 => Some(n as u64),
            _ => None,
        })
        .unwrap_or(65536)
}

fn ash_fn(h: &mut ElispHost, a: &[Value]) -> R {
    use num_traits::{Signed, ToPrimitive};
    let n = as_int_exact_p(h, &a[0], false)?;
    let cb = as_int_exact_p(h, &a[1], false)?;
    let c = match cb.to_i64() {
        Some(c) => c,
        // A count that does not even fit an i64: shifting left by it would exhaust
        // memory; shifting right by it collapses to the sign.
        None if cb.is_negative() => {
            return Ok(h.make_integer(if n.is_negative() {
                BigInt::from(-1)
            } else {
                BigInt::from(0)
            }))
        }
        None => return Err("overflow-error".to_string()),
    };
    let r = if c >= 0 {
        // Emacs bounds an integer at `integer-width` bits (65536 by default) and
        // signals `overflow-error` for a result wider than that — rather than
        // building the 15-megabyte number `(ash 3 123456788)` asks for. Zero
        // shifts to zero at any count, so it is not bounded.
        let width = integer_width(h);
        if n.sign() != num_bigint::Sign::NoSign && n.bits().saturating_add(c as u64) > width {
            return Err("overflow-error".to_string());
        }
        n << (c as u64)
    } else {
        // `BigInt`'s `>>` is already an arithmetic shift (it floors toward
        // negative infinity for negatives), which is what Emacs's `ash` does.
        let sh = c.unsigned_abs();
        if sh > 1 << 30 {
            if n.sign() == num_bigint::Sign::Minus {
                BigInt::from(-1)
            } else {
                BigInt::from(0)
            }
        } else {
            n >> sh
        }
    };
    Ok(h.make_integer(r))
}

/// `(lsh VALUE COUNT)` — *logical* shift, unlike `ash`'s arithmetic shift.
/// Left shift matches `ash`. For a right shift (negative COUNT) Emacs treats the
/// fixnum as an *unsigned* value of the fixnum bit width, so vacated high bits
/// fill with zeros rather than the sign bit: `(lsh -1 -1)` => 2305843009213693951
/// (`(2^62-1) >> 1`), not -1. Fixnums are 62-bit here (`most-positive-fixnum`
/// = 2^61-1), so mask to 62 bits before the unsigned shift.
fn lsh_fn(h: &mut ElispHost, a: &[Value]) -> R {
    const FIXNUM_MASK: u64 = (1u64 << 62) - 1;
    // VALUE goes through `CHECK_NUMBER` before its integer check, so a
    // non-number reports `number-or-marker-p` and a float reports `integerp`;
    // COUNT goes straight through `CHECK_INTEGER` and always reports `integerp`.
    // Measured on GNU Emacs 30.2:
    //
    //   (lsh ""  6)  => (wrong-type-argument number-or-marker-p "")
    //   (lsh 1.5 2)  => (wrong-type-argument integerp 1.5)
    //   (lsh 1  "")  => (wrong-type-argument integerp "")
    //
    // The check has to come first: the exact-shift path below reports `integerp`
    // on its own and would answer for VALUE with the wrong predicate.
    if as_number_p(h, &a[0], true).is_err() {
        let v = a[0].clone();
        return Err(h.signal_wrong_type("number-or-marker-p", &v));
    }
    // A left shift is exact, like `ash` — `(lsh 1 70)` is 2^70.
    if let Ok(c) = as_integer(h, &a[1]) {
        if c >= 0 {
            let n = as_int_exact_p(h, &a[0], false)?;
            if c > 1 << 30 {
                return Err("overflow-error".to_string());
            }
            let r = n << (c as u64);
            return Ok(h.make_integer(r));
        }
    }
    // `Flsh` runs `CHECK_NUMBER` first (data.c), so a non-number reports
    // `number-or-marker-p` — `ash`, which runs `CHECK_INTEGER`, reports
    // `integerp`. The two differ and elisp code reads the predicate.
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
    let base = as_number_p(h, &a[0], false)?;
    let exp = as_number_p(h, &a[1], false)?;
    if let (Num::Int(b), Num::Int(e)) = (&base, &exp) {
        use num_traits::{Signed, ToPrimitive};
        if !e.is_negative() {
            // Exact: `(expt 2 70)` is 2^70, not a wrapped i64. A huge exponent
            // would exhaust memory; Emacs signals rather than trying.
            let e_u32 = e.to_u32().ok_or_else(|| "overflow-error".to_string())?;
            if e_u32 > 1 << 24 {
                return Err("overflow-error".to_string());
            }
            let r = num_traits::Pow::pow(b.clone(), e_u32);
            return Ok(h.make_integer(r));
        }
    }
    Ok(Value::Float(base.to_f64().powf(exp.to_f64())))
}
fn sqrt_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64().sqrt()))
}
fn exp_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64().exp()))
}
fn log_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let x = as_number_p(h, &a[0], false)?.to_f64();
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
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64().sin()))
}
fn cos_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64().cos()))
}
fn tan_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64().tan()))
}
fn asin_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64().asin()))
}
fn acos_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_number_p(h, &a[0], false)?.to_f64().acos()))
}
fn atan_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let y = as_number_p(h, &a[0], false)?.to_f64();
    Ok(Value::Float(match a.get(1) {
        // X takes the same `CHECK_NUMBER` as Y (floatfns.c `Fatan`), so it reports
        // `numberp` — `as_num` reports `number-or-marker-p`, which is the
        // predicate the arithmetic ops use, not this one.
        Some(x) => {
            let xv = as_number_p(h, x, false)
                .map_err(|_| format!("wrong-type-argument: numberp {}", h.print(x, true)))?;
            y.atan2(xv.to_f64())
        }
        None => y.atan(),
    }))
}
/// `ldexp(x, n)` = `x * 2^n`, computed like C's `scalbn` (musl) so subnormal
/// results are preserved instead of flushing to 0. A naive `x * 2f64.powi(n)`
/// overflows `2^n` to infinity for very negative `n` and returns 0.0, whereas
/// Emacs (via C `ldexp`) yields the smallest subnormal (e.g. `(ldexp 1.0 -1074)`
/// => 5e-324). The staged scaling keeps every intermediate in range.
pub(crate) fn scalbn(x: f64, mut n: i64) -> f64 {
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
    let m = as_number_p(h, &a[0], false)?.to_f64();
    let e = as_num(h, &a[1])?.0;
    Ok(Value::Float(scalbn(m, e)))
}
/// `(copysign X Y)` — X with Y's sign.
///
/// Both arguments must be *floats*: Emacs `Fcopysign` uses `CHECK_TYPE (FLOATP …)`
/// rather than the number check the rest of the float library uses, so an integer
/// signals `(wrong-type-argument floatp N)` instead of being coerced.
fn copysign_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let f = |h: &mut ElispHost, v: &Value| -> Result<f64, String> {
        match v {
            Value::Float(x) => Ok(*x),
            _ => Err(h.signal_wrong_type("floatp", v)),
        }
    };
    let x = f(h, &a[0])?;
    let y = f(h, &a[1])?;
    Ok(Value::Float(x.copysign(y)))
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
    let (m, e) = frexp_parts(as_number_p(h, &a[0], false)?.to_f64());
    Ok(h.cons(Value::Float(m), Value::Int(e)))
}
fn isnan_fn(h: &mut ElispHost, a: &[Value]) -> R {
    match a[0] {
        Value::Float(f) => Ok(nil_or(f.is_nan())),
        _ => Err(format!(
            "wrong-type-argument: floatp {}",
            h.print(&a[0], true)
        )),
    }
}
/// The argument of the `f*` rounding subrs (`fround`/`ffloor`/`fceiling`/
/// `ftruncate`): exactly a float — Emacs's `CHECK_FLOAT` — so an integer is as
/// much a `(wrong-type-argument floatp X)` as a non-number is.
fn as_float_strict(h: &ElispHost, v: &Value) -> Result<f64, String> {
    match v {
        Value::Float(f) => Ok(*f),
        _ => Err(format!("wrong-type-argument: floatp {}", h.print(v, true))),
    }
}
fn fround_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_float_strict(h, &a[0])?.round_ties_even()))
}
fn ffloor_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_float_strict(h, &a[0])?.floor()))
}
fn fceiling_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_float_strict(h, &a[0])?.ceil()))
}
fn ftruncate_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Float(as_float_strict(h, &a[0])?.trunc()))
}

/// `(string-to-number STRING &optional BASE)` — parse a leading number. With
/// BASE (2–16) parse an integer in that radix; otherwise parse an int, or a
/// float when a `.` or exponent is present. Non-numeric input yields 0.
fn string_to_number(h: &mut ElispHost, a: &[Value]) -> R {
    let raw = as_string(h, &a[0])?;
    // Emacs skips exactly SPC and TAB, never the rest of the ASCII whitespace
    // set and never Unicode spaces: `(string-to-number "\n12")` is 0, as are the
    // `\r` `\f` `\v` U+00A0 and U+3000 forms. `trim_start` skipped all of them.
    let s = raw.trim_start_matches([' ', '\t']);
    if let Some(bv) = a.get(1) {
        // Base 10 (and nil) use the float-capable default parser below; only a
        // non-decimal base forces integer-only parsing.
        if !is_nil(bv) {
            // BASE is CHECK_FIXNUM: a float, a bignum, a marker and `t` are all
            // `fixnump`, and that check precedes the 2..=16 range check.
            let base_i = match bv {
                Value::Int(n) => *n,
                v => {
                    return Err(format!("wrong-type-argument: fixnump {}", h.print(v, true)));
                }
            };
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
                let (mut n, mut seen) = (BigInt::from(0), false);
                for c in chars {
                    match c.to_digit(base) {
                        Some(d) => {
                            n = n * base as i64 + d as i64;
                            seen = true;
                        }
                        None => break,
                    }
                }
                return Ok(if seen {
                    h.make_integer(n * sign)
                } else {
                    Value::Int(0)
                });
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
    // Non-finite float syntax, straight out of lread.c `string_to_number': after
    // `e' an explicit `+' may be followed by the literal `INF' or `NaN'. Both are
    // case-sensitive and both require the `+' — `1.0e-INF', `1.0eINF' and
    // `1.0e+inf' are not numbers. Without this, `(string-to-number "1.0e+INF")'
    // silently answered the finite 1.0, so any float round-tripped through
    // `number-to-string' lost its infinity.
    let mut nonfinite: Option<f64> = None;
    if has_digit && i < n && (b[i] == 'e' || b[i] == 'E') {
        let mut j = i + 1;
        let plus = j < n && b[j] == '+';
        if j < n && (b[j] == '+' || b[j] == '-') {
            j += 1;
        }
        if j < n && b[j].is_ascii_digit() {
            is_float = true;
            i = j;
            while i < n && b[i].is_ascii_digit() {
                i += 1;
            }
        } else if plus && b[j..].starts_with(&['I', 'N', 'F']) {
            is_float = true;
            nonfinite = Some(f64::INFINITY);
            i = j + 3;
        } else if plus && b[j..].starts_with(&['N', 'a', 'N']) {
            is_float = true;
            // The payload is the token's leading integer, as in the reader.
            let mant: String = b[start..i].iter().collect();
            nonfinite = Some(crate::reader::nan_with_payload(
                crate::reader::leading_integer(&mant),
                false,
            ));
            i = j + 3;
        }
    }
    if !has_digit {
        return Ok(Value::Int(0));
    }
    if is_float {
        // lread.c negates the value itself so `-0.0e+NaN' and `-1.0e+INF' keep
        // their sign — the sign is not part of what INF/NaN parsed.
        if let Some(v) = nonfinite {
            return Ok(Value::Float(if b[start] == '-' { -v } else { v }));
        }
        let tok: String = b[start..i].iter().collect();
        Ok(Value::Float(tok.parse().unwrap_or(0.0)))
    } else {
        // Integer parse must exclude a trailing dot ("1." => 1), so stop at the
        // dot position when one was consumed without becoming a float.
        let end = dot_pos.unwrap_or(i);
        let tok: String = b[start..end].iter().collect();
        // Exact at any size: `(string-to-number "1180591620717411303424")` is that
        // integer, not the 0 an `i64` parse failure used to yield.
        match tok.parse::<BigInt>() {
            Ok(n) => Ok(h.make_integer(n)),
            Err(_) => Ok(Value::Int(0)),
        }
    }
}

// ── sxhash ──
// fns.c's `sxhash_obj` for the objects whose hash Emacs derives from their
// contents (fixnums, floats, bignums, strings, conses, vectors, bool-vectors),
// on a 64-bit build (`EMACS_INT_WIDTH` 64, `FIXNUM_BITS` 62). Symbols and the
// other objects Emacs hashes by address keep an elisprs-local hash: equal
// objects still hash equally, but the numbers are not Emacs's.
const SXHASH_MAX_DEPTH: u32 = 3;
const SXHASH_MAX_LEN: usize = 7;
const INTMASK: u64 = (1 << 62) - 1;
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
/// lisp.h `sxhash_combine`.
fn sxhash_combine(x: u64, y: u64) -> u64 {
    (x << 4).wrapping_add(x >> 60).wrapping_add(y)
}
/// fns.c `hash_char_array` over a string's internal (UTF-8) bytes.
fn hash_char_array(p: &[u8]) -> u64 {
    let len = p.len();
    let mut hash = len as u64;
    let word = |i: usize| u64::from_ne_bytes(p[i..i + 8].try_into().unwrap());
    if len >= 8 {
        let step = 8.max(len >> 3);
        let mut i = 0;
        while i + 8 <= len {
            hash = sxhash_combine(hash, word(i));
            i += step;
        }
        hash = sxhash_combine(hash, word(len - 8));
    } else {
        let mut tail = 0u64;
        let mut i = 0;
        if len - i >= 4 {
            tail = (tail << 32) + u32::from_ne_bytes(p[i..i + 4].try_into().unwrap()) as u64;
            i += 4;
        }
        if len - i >= 2 {
            tail = (tail << 16) + u16::from_ne_bytes(p[i..i + 2].try_into().unwrap()) as u64;
            i += 2;
        }
        if i < len {
            tail = (tail << 8) + p[i] as u64;
        }
        hash = sxhash_combine(hash, tail);
    }
    hash
}
/// fns.c `sxhash_obj`.
fn sxhash_obj(h: &ElispHost, v: &Value, depth: u32) -> u64 {
    if depth > SXHASH_MAX_DEPTH {
        return 0;
    }
    // `sxhash_bignum`: the sign, then each 64-bit limb of the magnitude.
    if let Some(b) = h.as_bigint(v).filter(|_| h.is_bignum(v)) {
        let mut hash = u64::from(b.sign() == num_bigint::Sign::Minus);
        for limb in b.magnitude().iter_u64_digits() {
            hash = sxhash_combine(hash, limb);
        }
        return hash;
    }
    match v {
        Value::Int(n) => (*n as u64) & INTMASK,
        // `sxhash_float`: one word per double on a 64-bit build.
        Value::Float(f) => sxhash_combine(0, f.to_bits()),
        Value::Str(s) => hash_char_array(s.as_bytes()),
        // `XHASH (Qnil)` is 0: nil is the first entry of `lispsym`.
        Value::Bool(false) | Value::Undef => 0,
        Value::Bool(true) => 1,
        Value::Obj(_) => match h.obj(v) {
            Some(Obj::Str(s)) => hash_char_array(s.as_bytes()),
            // `XHASH` of a symbol is its tagged address, which on a 64-bit
            // host stays below 2^47; the name-derived stand-in keeps that range.
            Some(Obj::Symbol(s)) => hash_mix(0x5111, hash_bytes(&s.name)) & ((1 << 47) - 1),
            Some(Obj::Cons(..)) => {
                // `sxhash_list`.
                let mut hash = 0u64;
                let mut list = v.clone();
                if depth < SXHASH_MAX_DEPTH {
                    let mut i = 0;
                    while let Some(Obj::Cons(car, cdr)) = h.obj(&list) {
                        if i >= SXHASH_MAX_LEN {
                            break;
                        }
                        let (car, cdr) = (car.clone(), cdr.clone());
                        hash = sxhash_combine(hash, sxhash_obj(h, &car, depth + 1));
                        list = cdr;
                        i += 1;
                    }
                }
                if !matches!(list, Value::Undef | Value::Bool(false)) {
                    hash = sxhash_combine(hash, sxhash_obj(h, &list, depth + 1));
                }
                hash
            }
            Some(Obj::Vector(items)) => {
                // `sxhash_vector`.
                let mut hash = items.len() as u64;
                for it in items.iter().take(SXHASH_MAX_LEN) {
                    hash = sxhash_combine(hash, sxhash_obj(h, it, depth + 1));
                }
                hash
            }
            Some(Obj::BoolVector(bits)) => {
                // `sxhash_bool_vector`: the size, then the first words of bits.
                let mut hash = bits.len() as u64;
                for chunk in bits.chunks(64).take(SXHASH_MAX_LEN) {
                    let word =
                        chunk
                            .iter()
                            .enumerate()
                            .fold(0u64, |w, (i, &b)| if b { w | (1 << i) } else { w });
                    hash = sxhash_combine(hash, word);
                }
                hash
            }
            _ => 0x6,
        },
        _ => 0x8,
    }
}
/// fns.c `sxhash_eq` (`XHASH (k) ^ XTYPE (k)`) for a fixnum, whose tag is
/// `Lisp_Int0` (2) or `Lisp_Int1` (6) by its low bit; other objects hash by
/// identity here (an address in Emacs).
fn sxhash_eq(v: &Value) -> u64 {
    match v {
        Value::Int(n) => ((*n as u64) & INTMASK) ^ if n & 1 == 0 { 2 } else { 6 },
        Value::Float(f) => f.to_bits(),
        Value::Bool(false) | Value::Undef => 0,
        Value::Bool(true) => 1,
        Value::Str(s) => hash_bytes(s),
        Value::Obj(id) => hash_mix(0xab, *id as u64),
        _ => 0x8,
    }
}
/// fns.c `reduce_emacs_uint_to_fixnum`: `SXHASH_REDUCE` folds the top bits in
/// and masks to `INTMASK`; `make_ufixnum` then reads back as a signed fixnum.
fn sxhash_fixnum(x: u64) -> Value {
    let r = (x ^ (x >> 2)) & INTMASK;
    Value::Int(((r << 2) as i64) >> 2)
}
fn sxhash_equal_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(sxhash_fixnum(sxhash_obj(h, &a[0], 0)))
}
fn sxhash_eq_fn(_h: &mut ElispHost, a: &[Value]) -> R {
    Ok(sxhash_fixnum(sxhash_eq(&a[0])))
}
fn sxhash_eql_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // fns.c `sxhash_eql`: floats and bignums by value, the rest as `sxhash_eq`.
    let x = match &a[0] {
        Value::Float(_) => sxhash_obj(h, &a[0], 0),
        other if h.is_bignum(other) => sxhash_obj(h, other, 0),
        other => sxhash_eq(other),
    };
    Ok(sxhash_fixnum(x))
}

/// `(type-of OBJECT)` — the symbol naming OBJECT's primitive type.
fn type_of(h: &mut ElispHost, a: &[Value]) -> R {
    // A record (including every cl-defstruct instance) reports its slot 0 verbatim
    // — the type symbol for a normal record — matching Emacs's `Ftype_of`.
    if let Some(Obj::Record(items)) = h.obj(&a[0]) {
        let slot0 = items.first().cloned();
        return match slot0 {
            Some(t) => Ok(t),
            None => Ok(h.intern("record")),
        };
    }
    let name = match &a[0] {
        Value::Int(_) => "integer",
        Value::Float(_) => "float",
        Value::Str(_) => "string",
        _ if h.is_string(&a[0]) => "string",
        Value::Bool(_) | Value::Undef => "symbol",
        Value::Obj(_) => match h.obj(&a[0]) {
            Some(Obj::Cons(..)) => "cons",
            Some(Obj::Overlay(_)) => "overlay",
            // Emacs answers `integer` for a bignum too — fixnum and bignum are one
            // type, split only by `fixnump`/`bignump`.
            Some(Obj::Bignum(_)) => "integer",
            Some(Obj::Symbol(_)) => "symbol",
            Some(Obj::Vector(_)) => "vector",
            // Unreachable: a record is handled by the early return above; this arm
            // only keeps the match exhaustive over `Obj`.
            Some(Obj::Record(_)) => "record",
            Some(Obj::BoolVector(_)) => "bool-vector",
            // Unreachable: the `Value::Str` arm above matches first, because
            // `type_of` views its argument through `str_view`. Here for exhaustiveness.
            Some(Obj::Str(_)) => "string",
            Some(Obj::Subr { .. }) => "subr",
            // Emacs 30 renamed the interpreted-closure type: `(type-of (lambda ()))`
            // is `interpreted-function` (it was `function` only up to Emacs 29).
            // A macro is not a function object at all in Emacs — the function cell
            // holds the cons `(macro . FUNCTION)` — so `type-of` answers `cons`,
            // which is also what this printer already emits for one.
            Some(Obj::Closure { is_macro, .. }) => {
                if *is_macro {
                    "cons"
                } else {
                    "interpreted-function"
                }
            }
            Some(Obj::HashTable(_)) => "hash-table",
            Some(Obj::CharTable(_)) => "char-table",
            Some(Obj::Buffer(_)) => "buffer",
            Some(Obj::Marker(_)) => "marker",
            Some(Obj::Obarray(_)) => "obarray",
            None => "symbol",
        },
        _ => "symbol",
    };
    Ok(h.intern(name))
}
/// `(recordp OBJECT)` — non-nil for any record (a `record`/`make-record` result,
/// a `#s(…)` literal, or a cl-defstruct instance). Distinct from `cl-struct-p`,
/// which the prelude overrides to accept only cl-defstruct records.
fn recordp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(matches!(h.obj(&a[0]), Some(Obj::Record(_)))))
}

// ── OClosure C primitives (oclosure.el) ──
// These implement the host-specific seam `oclosure.el` builds on. The rest of
// oclosure.el is ported faithfully into the prelude. Because elisprs closures
// are compiled (not aref-indexable interpreted-functions), the type + slot
// layout is attached via a side table and slot values live in the closure's
// captured env — see `ElispHost::oclosure_*`.

/// `(closurep OBJECT)` — t if OBJECT is a closure.
fn closurep_fn(h: &mut ElispHost, a: &[Value]) -> R {
    // A macro is the cons `(macro . FN)` in Emacs, so `CLOSUREP` is false for it.
    let macro_cell = matches!(h.obj(&a[0]), Some(Obj::Closure { is_macro: true, .. }));
    Ok(nil_or(h.is_closure(&a[0]) && !macro_cell))
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
        return Err("cl-assertion-failed: (closurep oclosure)".to_string());
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
        return Err("cl-assertion-failed: (closurep oclosure)".to_string());
    }
    let idx = as_int(h, &a[1])? as usize;
    h.oclosure_get(&a[0], idx)
        .ok_or_else(|| "oclosure--get: slot index out of range".to_string())
}

/// `(oclosure--set V OCLOSURE INDEX)` — set slot INDEX to V; returns V.
fn oclosure_set_fn(h: &mut ElispHost, a: &[Value]) -> R {
    if !h.is_closure(&a[1]) {
        return Err("cl-assertion-failed: (closurep oclosure)".to_string());
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
        return Err("cl-assertion-failed: (closurep oclosure)".to_string());
    }
    let args: Vec<Value> = a[2..].to_vec();
    h.oclosure_copy(&a[0], &args)
        .ok_or_else(|| "oclosure--copy: not an OClosure".to_string())
}

/// `(functionp OBJECT)` — non-nil if OBJECT can be called as a function (a subr,
/// a non-macro closure, or a symbol whose function cell resolves to one).
fn functionp(h: &mut ElispHost, a: &[Value]) -> R {
    // eval.c `FUNCTIONP`: a SUBR object is a function only when its `max_args`
    // is not `UNEVALLED`, so `(functionp (symbol-function 'if))` is nil even
    // though the subr object itself resolves.
    if let Some(Obj::Subr { name, .. }) = h.obj(&a[0]) {
        if SPECIAL_FORMS.iter().any(|(sf, _)| *sf == name.as_str()) {
            return Ok(Value::Bool(false));
        }
    }
    // A list is a function when its car is `lambda`, and a symbol whose
    // indirect definition is an `autoload` is one unless TYPE (the fifth
    // element) is non-nil — a macro or keymap autoload.
    if h.indirect_lambda_list(&a[0]).is_some() {
        return Ok(Value::Bool(true));
    }
    let mut cur = a[0].clone();
    for _ in 0..64 {
        match h.obj(&cur) {
            Some(Obj::Symbol(s)) => match &s.function {
                Some(def) => cur = def.clone(),
                None => break,
            },
            _ => break,
        }
    }
    if let Some(items) = h.list_vec(&cur) {
        if items.first().and_then(|c| h.sym_name(c)).as_deref() == Some("autoload")
            && matches!(h.obj(&a[0]), Some(Obj::Symbol(_)))
        {
            return Ok(nil_or(!items.get(4).is_some_and(crate::host::el_truthy)));
        }
    }
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
        v => _h.is_string(v),
    };
    Ok(nil_or(ok))
}
/// `(char-width CHAR)` — a C primitive in Emacs (`Fchar_width`, `indent.c`), so
/// it is one here too rather than a prelude `defun`; `format`'s column-measured
/// width and precision need the same table from Rust, and two copies of it
/// would be two answers.
fn char_width_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let c = as_char(h, &a[0])?;
    Ok(Value::Int(char_display_width(c) as i64))
}
fn char_equal(h: &mut ElispHost, a: &[Value]) -> R {
    // `CHECK_CHARACTER`, not `CHECK_FIXNUM`: a float, a negative code, a code
    // past MAX_CHAR and a marker are all `(wrong-type-argument characterp X)`.
    let (c1, c2) = (as_char(h, &a[0])? as i64, as_char(h, &a[1])? as i64);
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
    Ok(h.introspect_function_cell(&a[0]).unwrap_or(Value::Undef))
}
/// The body of a subr whose calls `host::call_function` intercepts by name. It
/// exists so the function cell — and therefore `fboundp`, `functionp`,
/// `func-arity`, `subrp`, `indirect-function` and `symbol-function` — reports
/// what Emacs reports. Reaching it means a call site bypassed the intercept,
/// which is a routing bug, not a user error.
pub(crate) fn intercepted_subr(_h: &mut ElispHost, _a: &[Value]) -> R {
    Err(
        "internal: an intercepted higher-order primitive was called through its \
         subr body; host::call_function should have matched it by name"
            .to_string(),
    )
}
/// `(--set-intrinsic-macro-cell SYM CELL)` — record the `(macro . FUNCTION)` an
/// intrinsically-lowered macro reports through `symbol-function` / `fboundp` /
/// `indirect-function`. Called once from the prelude; not an Emacs function.
fn set_intrinsic_macro_cell(h: &mut ElispHost, a: &[Value]) -> R {
    h.set_intrinsic_macro_cell(&a[0], a[1].clone());
    Ok(a[0].clone())
}
/// `(intern-soft NAME)` — the interned symbol named NAME, or nil if none exists.
fn intern_soft(h: &mut ElispHost, a: &[Value]) -> R {
    let name = match &a[0] {
        v if h.is_string(v) => h.str_text(v).unwrap_or_default().to_string(),
        // `t` and `nil` ARE interned symbols in Emacs; elisprs represents them as
        // `Value::Bool`/`Value::Undef` rather than heap symbols, so `sym_name`
        // misses them and `(intern-soft t)` answered nil where Emacs answers `t`.
        Value::Bool(true) => "t".to_string(),
        v if is_nil(v) => "nil".to_string(),
        // A symbol argument is looked up by its own name; anything else names
        // itself in the error data, as Emacs's `CHECK_STRING` does.
        v => h
            .sym_name(v)
            .ok_or_else(|| format!("wrong-type-argument: stringp {}", h.print(v, true)))?,
    };
    match obarray_arg(h, a.get(1))? {
        None => match name.as_str() {
            // Not heap symbols here, but interned symbols in Emacs, so they must
            // answer themselves rather than "no such symbol".
            "t" => Ok(Value::Bool(true)),
            "nil" => Ok(Value::Undef),
            _ => Ok(h.find_symbol(&name).unwrap_or(Value::Undef)),
        },
        Some(id) => Ok(h.obarray_intern_soft(id, &name)),
    }
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
    // Emacs `Fspecial_form_p` dereferences a symbol to its function and then asks
    // the *subr* whether its `max_args` is `UNEVALLED`, so the subr object answers
    // `t` just as the symbol naming it does: `(special-form-p (symbol-function
    // 'if))` is `t`, not nil.
    let name = match h.obj(&a[0]) {
        Some(Obj::Subr { name, .. }) => Some(name.clone()),
        _ => h.sym_name(&a[0]),
    };
    let ok = name.is_some_and(|n| SPECIAL_FORMS.iter().any(|(sf, _)| *sf == n.as_str()));
    Ok(nil_or(ok))
}
fn char_uppercase_p(h: &mut ElispHost, a: &[Value]) -> R {
    let c = char::from_u32(as_int(h, &a[0])? as u32);
    Ok(nil_or(c.is_some_and(|c| c.is_uppercase())))
}
/// `(string-distance S1 S2 &optional BYTECOMPARE)` — Levenshtein edit distance.
/// With BYTECOMPARE non-nil, Emacs measures over UTF-8 bytes, not characters
/// (editfns.c Fstring_distance).
fn string_distance(h: &mut ElispHost, a: &[Value]) -> R {
    let a0 = as_string(h, &a[0])?;
    let a1 = as_string(h, &a[1])?;
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
            _ => match h
                .str_text(v)
                .map(|s| s.chars().map(|c| Value::Int(c as i64)).collect::<Vec<_>>())
            {
                Some(chars) => out.extend(chars),
                // Fvconcat's list walk names a dotted TAIL with listp
                // ((vconcat '(t . 9)) => listp 9) and a non-sequence with
                // sequencep — exactly seq_vec_checked's contract.
                _ => out.extend(h.seq_vec_checked(v)?),
            },
        }
    }
    Ok(h.alloc(Obj::Vector(out)))
}
/// `(abs NUMBER)` — absolute value, keeping the int/float type (and turning
/// `-0.0` into `0.0`, which a `(< x 0)` test would miss).
fn abs_fn(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(match as_number_p(h, &a[0], false)? {
        Num::Int(n) => {
            use num_traits::Signed;
            h.make_integer(n.abs())
        }
        Num::Float(f) => Value::Float(f.abs()),
    })
}
/// `(logcount N)` — count of set bits for N≥0, or of clear bits for N<0 (i.e.
/// bits differing from the sign bit), matching Emacs.
fn logcount_fn(h: &mut ElispHost, a: &[Value]) -> R {
    use num_traits::Signed;
    let n = as_int_exact_p(h, &a[0], false)?;
    // Emacs counts set bits for N>=0 and clear bits (of the two's complement)
    // for N<0 — i.e. the bits that differ from the sign bit either way.
    let counted = if n.is_negative() { !n } else { n };
    let bits: u64 = counted
        .to_bytes_le()
        .1
        .iter()
        .map(|b| b.count_ones() as u64)
        .sum();
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
/// Generic SHA-2 (32-bit words: SHA-224/SHA-256).  `iv` is the initial hash
/// state; the digest is truncated to `out_len` bytes (28 for SHA-224).
fn sha2_32(msg: &[u8], iv: [u32; 8], out_len: usize) -> Vec<u8> {
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
    let mut h = iv;
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
    let full: Vec<u8> = h.iter().flat_map(|x| x.to_be_bytes()).collect();
    full[..out_len].to_vec()
}
fn sha256_bytes(msg: &[u8]) -> Vec<u8> {
    sha2_32(
        msg,
        [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ],
        32,
    )
}
fn sha224_bytes(msg: &[u8]) -> Vec<u8> {
    sha2_32(
        msg,
        [
            0xc1059ed8, 0x367cd507, 0x3070dd17, 0xf70e5939, 0xffc00b31, 0x68581511, 0x64f98fa7,
            0xbefa4fa4,
        ],
        28,
    )
}
/// Generic SHA-2 (64-bit words: SHA-384/SHA-512).  `iv` is the initial hash
/// state; the digest is truncated to `out_len` bytes (48 for SHA-384).
fn sha2_64(msg: &[u8], iv: [u64; 8], out_len: usize) -> Vec<u8> {
    const K: [u64; 80] = [
        0x428a2f98d728ae22,
        0x7137449123ef65cd,
        0xb5c0fbcfec4d3b2f,
        0xe9b5dba58189dbbc,
        0x3956c25bf348b538,
        0x59f111f1b605d019,
        0x923f82a4af194f9b,
        0xab1c5ed5da6d8118,
        0xd807aa98a3030242,
        0x12835b0145706fbe,
        0x243185be4ee4b28c,
        0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f,
        0x80deb1fe3b1696b1,
        0x9bdc06a725c71235,
        0xc19bf174cf692694,
        0xe49b69c19ef14ad2,
        0xefbe4786384f25e3,
        0x0fc19dc68b8cd5b5,
        0x240ca1cc77ac9c65,
        0x2de92c6f592b0275,
        0x4a7484aa6ea6e483,
        0x5cb0a9dcbd41fbd4,
        0x76f988da831153b5,
        0x983e5152ee66dfab,
        0xa831c66d2db43210,
        0xb00327c898fb213f,
        0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2,
        0xd5a79147930aa725,
        0x06ca6351e003826f,
        0x142929670a0e6e70,
        0x27b70a8546d22ffc,
        0x2e1b21385c26c926,
        0x4d2c6dfc5ac42aed,
        0x53380d139d95b3df,
        0x650a73548baf63de,
        0x766a0abb3c77b2a8,
        0x81c2c92e47edaee6,
        0x92722c851482353b,
        0xa2bfe8a14cf10364,
        0xa81a664bbc423001,
        0xc24b8b70d0f89791,
        0xc76c51a30654be30,
        0xd192e819d6ef5218,
        0xd69906245565a910,
        0xf40e35855771202a,
        0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8,
        0x1e376c085141ab53,
        0x2748774cdf8eeb99,
        0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63,
        0x4ed8aa4ae3418acb,
        0x5b9cca4f7763e373,
        0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc,
        0x78a5636f43172f60,
        0x84c87814a1f0ab72,
        0x8cc702081a6439ec,
        0x90befffa23631e28,
        0xa4506cebde82bde9,
        0xbef9a3f7b2c67915,
        0xc67178f2e372532b,
        0xca273eceea26619c,
        0xd186b8c721c0c207,
        0xeada7dd6cde0eb1e,
        0xf57d4f7fee6ed178,
        0x06f067aa72176fba,
        0x0a637dc5a2c898a6,
        0x113f9804bef90dae,
        0x1b710b35131c471b,
        0x28db77f523047d84,
        0x32caab7b40c72493,
        0x3c9ebe0a15c9bebc,
        0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6,
        0x597f299cfc657e2a,
        0x5fcb6fab3ad6faec,
        0x6c44198c4a475817,
    ];
    let mut h = iv;
    let bitlen = (msg.len() as u128).wrapping_mul(8);
    let mut data = msg.to_vec();
    data.push(0x80);
    while data.len() % 128 != 112 {
        data.push(0);
    }
    data.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in data.chunks(128) {
        let mut w = [0u64; 80];
        for (i, wi) in w.iter_mut().enumerate().take(16) {
            *wi = u64::from_be_bytes([
                chunk[i * 8],
                chunk[i * 8 + 1],
                chunk[i * 8 + 2],
                chunk[i * 8 + 3],
                chunk[i * 8 + 4],
                chunk[i * 8 + 5],
                chunk[i * 8 + 6],
                chunk[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..80 {
            let s1 = v[4].rotate_right(14) ^ v[4].rotate_right(18) ^ v[4].rotate_right(41);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(28) ^ v[0].rotate_right(34) ^ v[0].rotate_right(39);
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
    let full: Vec<u8> = h.iter().flat_map(|x| x.to_be_bytes()).collect();
    full[..out_len].to_vec()
}
fn sha512_bytes(msg: &[u8]) -> Vec<u8> {
    sha2_64(
        msg,
        [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ],
        64,
    )
}
fn sha384_bytes(msg: &[u8]) -> Vec<u8> {
    sha2_64(
        msg,
        [
            0xcbbb9d5dc1059ed8,
            0x629a292a367cd507,
            0x9159015a3070dd17,
            0x152fecd8f70e5939,
            0x67332667ffc00b31,
            0x8eb44a8768581511,
            0xdb0c2e0d64f98fa7,
            0x47b5481dbefa4fa4,
        ],
        48,
    )
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
fn hash_input(h: &mut ElispHost, a: &[Value], obj_idx: usize) -> Result<Vec<u8>, String> {
    // Emacs's `secure_hash` accepts a string or a buffer and rejects anything
    // else with `xsignal2 (Qerror, "Invalid object argument", OBJECT)` — a plain
    // `error` carrying the object beside the message, not the
    // `wrong-type-argument` a string accessor reports.
    let s = match as_string(h, &a[obj_idx]) {
        Ok(s) => s,
        Err(_) => {
            let v = a[obj_idx].clone();
            return Err(h.signal_error_with("Invalid object argument", &v));
        }
    };
    // START and END index the *encoded bytes*, not the characters: Emacs hashes
    // the string's byte representation, so `(md5 "αβγ" 0 3)` covers three of its
    // six UTF-8 bytes and is NOT the whole three-character string.
    let bytes = s.as_bytes();
    let len = bytes.len() as i64;
    let idx = |h: &mut ElispHost, v: &Value| -> Result<i64, String> {
        // A float index is `(wrong-type-argument integerp F)` — `as_int` would
        // truncate it and hash some other range instead.
        match v {
            Value::Int(n) => Ok(*n),
            _ if as_integer(h, v).is_ok() => as_integer(h, v),
            _ => Err(h.signal_wrong_type("integerp", v)),
        }
    };
    let raw_start = match a.get(obj_idx + 1) {
        Some(v) if !is_nil(v) => Some(idx(h, &v.clone())?),
        _ => None,
    };
    let raw_end = match a.get(obj_idx + 2) {
        Some(v) if !is_nil(v) => Some(idx(h, &v.clone())?),
        _ => None,
    };
    // A negative bound counts from the end (`validate_subarray` adds the length
    // once), so `(md5 "abc" -1)` hashes "c".
    let adj = |n: i64| if n < 0 { n + len } else { n };
    let start = raw_start.map_or(0, adj);
    let end = raw_end.map_or(len, adj);
    // Emacs `validate_subarray`: START and END must bracket a real subrange, and
    // one that does not is `(args-out-of-range OBJECT START END)` with the two
    // bounds *as written* (nil included). Clamping them instead quietly hashed
    // the empty string — `(secure-hash 'md5 "abc" 5 nil)` answered the digest of
    // "" rather than signalling.
    if start < 0 || end > len || start > end {
        let obj = h.print(&a[obj_idx], true);
        let sv = raw_start.map_or("nil".to_string(), |n| n.to_string());
        let ev = raw_end.map_or("nil".to_string(), |n| n.to_string());
        return Err(format!("args-out-of-range: {obj} {sv} {ev}"));
    }
    Ok(bytes[start as usize..end as usize].to_vec())
}
fn sha1_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let digest = to_hex(&sha1_bytes(&hash_input(h, a, 0)?));
    Ok(h.new_string(digest))
}
fn md5_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let digest = to_hex(&md5_bytes(&hash_input(h, a, 0)?));
    Ok(h.new_string(digest))
}
/// `(secure-hash ALGORITHM OBJECT &optional START END BINARY)`.
fn secure_hash(h: &mut ElispHost, a: &[Value]) -> R {
    // ALGORITHM is a symbol (md5/sha1/sha256/…); accept a string too.
    let algo = h
        .sym_name(&a[0])
        .or_else(|| as_string(h, &a[0]).ok())
        .unwrap_or_default();
    let bytes = hash_input(h, a, 1)?;
    let digest = match algo.as_str() {
        "md5" => md5_bytes(&bytes),
        "sha1" => sha1_bytes(&bytes),
        "sha224" => sha224_bytes(&bytes),
        "sha256" => sha256_bytes(&bytes),
        "sha384" => sha384_bytes(&bytes),
        "sha512" => sha512_bytes(&bytes),
        // Emacs `secure_hash` (fns.c): `error ("Invalid algorithm arg: %s", …)`,
        // with the argument rendered by `prin1` (a symbol prints bare).
        other => return Err(format!("error: Invalid algorithm arg: {other}")),
    };
    // BINARY (4th optional, index 4): return the raw bytes as a string.
    if a.get(4).is_some_and(|v| !is_nil(v)) {
        Ok(h.new_string(digest.iter().map(|&b| b as char).collect::<String>()))
    } else {
        Ok(h.new_string(to_hex(&digest)))
    }
}
/// `(secure-hash-algorithms)` — the list of algorithms `secure-hash' accepts.
/// Faithful to C `Fsecure_hash_algorithms` (fns.c): a fixed 6-symbol list.
fn secure_hash_algorithms(h: &mut ElispHost, _a: &[Value]) -> R {
    let syms = ["md5", "sha1", "sha224", "sha256", "sha384", "sha512"]
        .iter()
        .map(|n| h.intern(n))
        .collect();
    Ok(h.list_from(syms))
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
/// Emacs's own diagnostic (fns.c `base64_decode_string`), capitalised as it is
/// there — elisp code catches this error and prints the string.
const INVALID_B64: &str = "error: Invalid base64 data";

/// Decode base64 (`url` selects the `-_` alphabet and the unpadded reading Emacs
/// uses for its BASE64URL argument).
///
/// The padded form is *strict*, as `base64_decode_1` is: whitespace is ignored
/// anywhere, but what is left must be whole 4-character quadruples over the
/// `+/` alphabet, `=` may only trail inside a quadruple, and a quadruple
/// carrying fewer than two data characters is rejected. Reading the input as a
/// loose bit stream instead accepted `"YWJ"` (which Emacs rejects) and `"-_-_"`
/// (whose alphabet belongs to the other mode).
fn b64_decode(input: &str, url: bool) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' if !url => Some(62),
            b'/' if !url => Some(63),
            b'-' if url => Some(62),
            b'_' if url => Some(63),
            _ => None,
        }
    };
    let chars: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(chars.len() / 4 * 3);
    // BASE64URL padding is optional, but everything else about a quadruple still
    // holds: a short final group may carry no `=` at all (`"YWJ"` decodes,
    // `"YQ="` does not), and no group may hold fewer than two data characters
    // (`"A==="` and `"===="` are errors even here).
    if !url && !chars.len().is_multiple_of(4) {
        return Err(INVALID_B64.into());
    }
    for quad in chars.chunks(4) {
        if quad.len() < 4 {
            if quad.len() < 2 || quad.contains(&b'=') {
                return Err(INVALID_B64.into());
            }
            let mut v = [0u32; 3];
            for (i, &c) in quad.iter().enumerate() {
                v[i] = val(c).ok_or(INVALID_B64)?;
            }
            out.push(((v[0] << 2) | (v[1] >> 4)) as u8);
            if quad.len() == 3 {
                out.push((((v[1] & 0x0F) << 4) | (v[2] >> 2)) as u8);
            }
            continue;
        }
        let mut v = [0u32; 4];
        let mut n = 0usize;
        let mut padded = false;
        for (i, &c) in quad.iter().enumerate() {
            if c == b'=' {
                padded = true;
                continue;
            }
            // `"AB=C"`: once a quadruple has started padding, everything after it
            // must be padding too.
            if padded {
                return Err(INVALID_B64.into());
            }
            v[i] = val(c).ok_or(INVALID_B64)?;
            n += 1;
        }
        // `"===="` carries no data and `"A==="` carries too few bits to make even
        // one byte; both are errors, not empty output.
        if n < 2 {
            return Err(INVALID_B64.into());
        }
        out.push(((v[0] << 2) | (v[1] >> 4)) as u8);
        if n >= 3 {
            out.push((((v[1] & 0x0F) << 4) | (v[2] >> 2)) as u8);
        }
        if n == 4 {
            out.push((((v[2] & 0x03) << 6) | v[3]) as u8);
        }
    }
    Ok(out)
}

/// The bytes Emacs's base64 encoders see for STRING.
///
/// Emacs encodes a *unibyte* string one byte per character and refuses a
/// multibyte one outright (`error ("Multibyte character in data for base64
/// encoding")`), so the input's characters are the bytes — not their UTF-8
/// expansion, which is what `str::as_bytes` yields and which made
/// `(base64-encode-string "\303\251")` answer `"w4PCqQ=="` where Emacs answers
/// `"w6k="`.
fn b64_input_bytes(s: &str) -> Result<Vec<u8>, String> {
    s.chars()
        .map(|c| {
            u8::try_from(c as u32)
                .map_err(|_| "error: Multibyte character in data for base64 encoding".to_string())
        })
        .collect()
}
/// Render decoded bytes as a string with each byte a char 0–255 (unibyte-ish).
fn bytes_to_str(h: &mut ElispHost, bytes: &[u8]) -> Value {
    let text = bytes.iter().map(|&b| b as char).collect::<String>();
    h.new_string(text)
}
fn base64_encode_string(h: &mut ElispHost, a: &[Value]) -> R {
    let raw = b64_encode(&b64_input_bytes(&as_string(h, &a[0])?)?, B64_STD, true);
    let no_break = a.get(1).is_some_and(|v| !is_nil(v));
    Ok(h.new_string(if no_break { raw } else { b64_wrap(&raw) }))
}
/// `(base64-decode-string STRING &optional BASE64URL IGNORE-INVALID)`.
fn base64_decode_string(h: &mut ElispHost, a: &[Value]) -> R {
    let url = a.get(1).is_some_and(|v| !is_nil(v));
    let bytes = b64_decode(&as_string(h, &a[0])?, url)?;
    Ok(bytes_to_str(h, &bytes))
}
fn base64url_encode_string(h: &mut ElispHost, a: &[Value]) -> R {
    let no_pad = a.get(1).is_some_and(|v| !is_nil(v));
    Ok(h.new_string(b64_encode(
        &b64_input_bytes(&as_string(h, &a[0])?)?,
        B64_URL,
        !no_pad,
    )))
}
fn base64url_decode_string(h: &mut ElispHost, a: &[Value]) -> R {
    let bytes = b64_decode(&as_string(h, &a[0])?, true)?;
    Ok(bytes_to_str(h, &bytes))
}
/// `(url-hexify-string STRING)` — percent-encode all but `[A-Za-z0-9-._~]`.
fn url_hexify_string(h: &mut ElispHost, a: &[Value]) -> R {
    // Emacs's `url-hexify-string` is a `mapconcat` over its argument, so it takes
    // any *sequence* — `[1 2]` hexifies to "%01%02" and nil to "" — and a
    // non-sequence reports `sequencep`, not `stringp`.
    let src = match h.str_text(&a[0]).map(str::to_string) {
        Some(s) => s,
        None => {
            let v = &a[0];
            let items = h
                .seq_vec(v)
                .ok_or_else(|| h.signal_wrong_type("sequencep", v))?;
            let mut acc = String::new();
            for it in &items {
                match char::from_u32(as_char(h, it)?) {
                    Some(c) => acc.push(c),
                    None => return Err(h.signal_wrong_type("characterp", it)),
                }
            }
            acc
        }
    };
    let mut out = String::new();
    for b in src.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(h.new_string(out))
}
/// `(url-unhex-string STRING)` — decode `%XX` escapes.
fn url_unhex_string(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(h, &a[0])?;
    // Character-wise, not byte-wise: only `%XX` names a byte; every other
    // character is copied through as itself. Walking the UTF-8 bytes expanded
    // each non-ASCII character into its encoding, so `(url-unhex-string "αβγ")`
    // — which contains no escape at all — came back six characters long.
    let chars: Vec<char> = s.chars().collect();
    let hex = |c: char| c.is_ascii_hexdigit();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 2 < chars.len() && hex(chars[i + 1]) && hex(chars[i + 2]) {
            let pair: String = chars[i + 1..i + 3].iter().collect();
            if let Ok(v) = u8::from_str_radix(&pair, 16) {
                // An escape names one *byte*, and it stays one byte: Emacs hands
                // the raw bytes back and leaves decoding to the caller, so
                // `(url-unhex-string "%CE%B1")` is two characters (206 177), not
                // the one character those bytes happen to spell in UTF-8.
                // Re-assembling them made it answer `(945)`, with length 1.
                out.push(v as char);
                i += 3;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(h.new_string(out))
}
/// `(string-to-vector STRING)` — a vector of STRING's character codes.
fn string_to_vector(h: &mut ElispHost, a: &[Value]) -> R {
    // `(string-to-vector SEQ)` is `(vconcat SEQ)`: any sequence, so `nil` is the
    // empty vector rather than a `stringp` error.
    let items = h
        .seq_vec(&a[0])
        .ok_or_else(|| format!("wrong-type-argument: sequencep {}", h.print(&a[0], true)))?;
    Ok(h.alloc(Obj::Vector(items)))
}
/// `(logb X)` — the binary exponent of |X|: floor(log2(|X|)).
///
/// Faithful to Emacs 30 `Flogb` (floatfns.c): a finite nonzero argument yields
/// the integer `frexp` exponent minus one; every other case (zero, ±infinity,
/// NaN) falls through to C `logb`, which returns a *float* — `-inf` for zero,
/// `+inf` for either infinity, and NaN for NaN.
fn logb_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_number_p(h, &a[0], false)?;
    // An integer answer is exact in Emacs (`Flogb` takes the integer path for an
    // integer argument), and converting through `f64` first rounds: 2^61-1 rounds
    // UP to 2^61, so `(logb 2305843009213693951)` answered 61 where Emacs answers
    // 60. The exponent is the bit length of |N| minus one.
    if let crate::host::Num::Int(i) = &n {
        let bits = i.bits();
        if bits > 0 {
            return Ok(Value::Int(bits as i64 - 1));
        }
    }
    let f = n.to_f64();
    if f.is_finite() && f != 0.0 {
        return Ok(Value::Int(f.abs().log2().floor() as i64));
    }
    let val = if f.is_nan() {
        // A NaN passes through unchanged, sign and payload included — Emacs
        // prints `-0.0e+NaN` for `(logb -0.0e+NaN)`, and a freshly built
        // `f64::NAN` is a different NaN.
        f
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
        Ok(h.new_string(s))
    } else {
        Ok(h.new_string(
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

/// Read a `decode-char` CODE-POINT argument: an integer, an integral float, or
/// a cons `(HIGH . LOW)` giving `HIGH * 0x10000 + LOW` (the obsolescent form).
/// This mirrors the `CONSP`/`FIXNUM`/`FLOATP` dispatch in `Fdecode_char`
/// (src/charset.c).
fn decode_char_code_point(h: &ElispHost, v: &Value) -> Result<i64, String> {
    if let Some(Obj::Cons(hi, lo)) = h.obj(v) {
        let hi = as_int(h, &hi.clone())?;
        let lo = as_int(h, &lo.clone())?;
        return Ok((hi << 16) | lo);
    }
    as_int(h, v)
}

/// `(decode-char CHARSET CODE-POINT)` — port of `Fdecode_char` (src/charset.c).
/// Decode CODE-POINT in CHARSET to a character, or nil if the code point is not
/// valid in CHARSET. CODE-POINT must lie in `0..=0xFFFFFFFF`; anything else
/// signals the `error` "Not an in-range integer, integral float, or cons of
/// integers".
///
/// Only charsets whose mapping is pure arithmetic — no external mule map tables
/// — are supported faithfully: `ascii`, `eight-bit`, `iso-8859-1`, and the
/// Unicode charsets `ucs`/`unicode` (full range) and `unicode-bmp` (BMP only).
/// Every other symbol takes the `CHECK_CHARSET_GET_CHARSET` failure path and
/// signals `wrong-type-argument charsetp SYM`, exactly as Emacs does for an
/// unknown charset (the mule-table national charsets like `japanese-jisx0208`
/// are not registered here, so they land on that path rather than being
/// approximated).
fn decode_char(h: &mut ElispHost, a: &[Value]) -> R {
    let name = h
        .sym_name(&a[0])
        .filter(|n| {
            matches!(
                n.as_str(),
                "ascii" | "eight-bit" | "iso-8859-1" | "ucs" | "unicode" | "unicode-bmp"
            )
        })
        .ok_or_else(|| format!("wrong-type-argument: charsetp {}", h.print(&a[0], true)))?;
    let code = decode_char_code_point(h, &a[1])?;
    if !(0..=0xFFFF_FFFF).contains(&code) {
        return Err(
            "error: Not an in-range integer, integral float, or cons of integers".to_string(),
        );
    }
    let ch = match name.as_str() {
        "ascii" => (0..=0x7F).contains(&code).then_some(code),
        "eight-bit" => (0x80..=0xFF).contains(&code).then_some(0x3F_FF00 + code),
        "iso-8859-1" => (0..=0xFF).contains(&code).then_some(code),
        "unicode-bmp" => (0..=0xFFFF).contains(&code).then_some(code),
        // "ucs" | "unicode"
        _ => (0..=0x10_FFFF).contains(&code).then_some(code),
    };
    Ok(ch.map(Value::Int).unwrap_or(Value::Undef))
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
        matches!(a[0], Value::Bool(true))
            || is_nil(&a[0])
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
        Some(Obj::Subr { name, .. }) => Ok(h.new_string(name.clone())),
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
/// `(read &optional STREAM)` (lread.c `Fread`). A string is read from its
/// start. A buffer is read from ITS point (current or not) up to its `ZV`, and
/// its point is left just past the object; a marker is read from its position
/// in its buffer, and the marker is advanced instead.
fn read_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let stream = a.first().cloned().unwrap_or(Value::Undef);
    let (bi, marker) = match h.obj(&stream) {
        Some(Obj::Buffer(idx)) => (*idx, None),
        Some(Obj::Marker(m)) => match m.borrow().buffer {
            Some(idx) => (idx, Some(m.clone())),
            None => return Err("error: Marker does not point anywhere".to_string()),
        },
        _ => {
            let s = as_string(h, &stream)?;
            let forms = crate::reader::read_all(h, &s)?;
            return forms
                .into_iter()
                .next()
                .ok_or_else(|| "end-of-file".to_string());
        }
    };
    let (start, text) = {
        let b = &h.buffers[bi];
        let start = marker.as_ref().map_or(b.point, |m| m.borrow().pos);
        let start = start.clamp(b.begv, b.zv);
        (
            start,
            b.text[start - 1..b.zv - 1].iter().collect::<String>(),
        )
    };
    // Running out of text consumes all of it: point (or the marker) is at
    // `ZV` when `end-of-file` is signalled.
    let r = crate::reader::read_one(h, &text, 0);
    let end = match &r {
        Ok((_, end)) => start + end,
        Err(e) if e == "end-of-file" => start + text.chars().count(),
        Err(_) => start,
    };
    match marker {
        Some(m) => m.borrow_mut().pos = end,
        None => h.buffers[bi].point = end,
    }
    r.map(|(form, _)| form)
}
/// `(read-from-string STRING &optional START END)` — read the first object from
/// STRING (from char index START), returning `(OBJECT . END-INDEX)`.
fn read_from_string(h: &mut ElispHost, a: &[Value]) -> R {
    let s = as_string(h, &a[0])?;
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
/// fns.c `validate_subarray`, as `Fcompare_strings` calls it: resolve FROM/TO
/// against a sequence of `size` elements.
///
/// A nil bound takes its default (0 / `size`); a negative one counts from the
/// end; **anything else is `(wrong-type-argument integerp BOUND)`** — which is
/// the whole point of porting it, since elisprs previously defaulted a
/// non-integer bound silently and answered a comparison where Emacs signals.
/// A resolved range outside `0 <= from <= to <= size` is
/// `args_out_of_range_3`: `(args-out-of-range ARRAY FROM TO)`.
///
/// `Fcompare_strings` first clamps a too-large *positive* END down to `size`
/// "for backward compatibility", so only a negative or start-crossing bound can
/// reach the range error through it. The clamped value is what the error data
/// reports, exactly as in C.
fn validate_subarray(
    h: &ElispHost,
    array: &Value,
    from: &Value,
    to: Option<&Value>,
    size: usize,
) -> Result<(usize, usize), String> {
    let to = to.cloned().unwrap_or(Value::Undef);
    // The END clamp happens in Fcompare_strings, before validate_subarray.
    let to = match to {
        Value::Int(n) if n > size as i64 => Value::Int(size as i64),
        other => other,
    };
    let resolve = |v: &Value, default: i64| -> Result<i64, String> {
        match v {
            Value::Int(n) => Ok(if *n < 0 { n + size as i64 } else { *n }),
            _ if is_nil(v) => Ok(default),
            _ => Err(format!(
                "wrong-type-argument: integerp {}",
                h.print(v, true)
            )),
        }
    };
    let f = resolve(from, 0)?;
    let t = resolve(&to, size as i64)?;
    if !(0 <= f && f <= t && t <= size as i64) {
        return Err(format!(
            "args-out-of-range: {} {} {}",
            h.print(array, true),
            h.print(from, true),
            h.print(&to, true)
        ));
    }
    Ok((f as usize, t as usize))
}

/// `(compare-strings S1 START1 END1 S2 START2 END2 &optional IGNORE-CASE)` —
/// `t` if the substrings are equal, else a signed 1-based index of the first
/// mismatch (negative when S1 sorts before S2), per Emacs.
fn compare_strings(h: &mut ElispHost, a: &[Value]) -> R {
    let s1: Vec<char> = as_string(h, &a[0])?.chars().collect();
    let s2: Vec<char> = as_string(h, &a[3])?.chars().collect();
    let (start1, end1) = validate_subarray(h, &a[0], &a[1], a.get(2), s1.len())?;
    let (start2, end2) = validate_subarray(h, &a[3], &a[4], a.get(5), s2.len())?;
    let ignore_case = a.get(6).is_some_and(|v| !is_nil(v));
    let sub1 = &s1[start1..end1];
    let sub2 = &s2[start2..end2];
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
            // Emacs walks LIST with `CHECK_LIST_END`, so running off a non-nil
            // tail is `(wrong-type-argument listp TAIL)` — reaching the end only
            // answers nil when the end is actually nil. `(member-ignore-case "a"
            // 1.5)` answered nil here where Emacs signals, and so did every
            // improper list whose match was not found before the tail.
            _ if is_nil(&cur) => return Ok(Value::Undef),
            _ => return Err(h.signal_wrong_type("listp", &cur)),
        };
        if h.is_string(&car) {
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
pub(crate) fn rng_next() -> u64 {
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
/// A random fixnum over the whole fixnum range, as `get_random_fixnum`'s
/// unbounded case: 62 random bits, sign-extended.
fn random_fixnum() -> i64 {
    ((rng_next() << 2) as i64) >> 2
}
/// Port of `Frandom` (fns.c). LIMIT t reseeds from the clock and a string
/// reseeds from its contents; either way, as for any other non-integer LIMIT,
/// the answer is a random fixnum, negative ones included. An integer LIMIT —
/// a bignum too — must be positive (`(args-out-of-range LIMIT)`) and bounds
/// the answer to [0, LIMIT).
fn random_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let limit = a.first().cloned().unwrap_or(Value::Undef);
    match &limit {
        Value::Bool(true) => RNG_STATE.with(|s| s.set(rng_seed())),
        Value::Int(n) => {
            if *n <= 0 {
                return Err(args_out_of_range_values(h, vec![limit.clone()]));
            }
            return Ok(Value::Int((rng_next() % *n as u64) as i64));
        }
        _ => {
            if let Some(Obj::Bignum(b)) = h.obj(&limit) {
                let b = b.clone();
                if b.sign() != num_bigint::Sign::Plus {
                    return Err(args_out_of_range_values(h, vec![limit.clone()]));
                }
                // 64 bits past LIMIT's width keep the modulo bias negligible.
                let words = b.bits() / 64 + 2;
                let mut r = BigInt::from(0u8);
                for _ in 0..words {
                    r = (r << 64) + BigInt::from(rng_next());
                }
                return Ok(h.make_integer(r % b));
            }
            if h.is_string(&limit) {
                let s = as_string(h, &limit)?;
                // `seed_random`: the same string always restarts the same sequence.
                let seed = s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, c| {
                    (acc ^ c as u64).wrapping_mul(0x0100_0000_01b3)
                });
                RNG_STATE.with(|st| st.set(seed | 1));
            }
        }
    }
    Ok(Value::Int(random_fixnum()))
}

/// Convert an elisp TIME value to epoch seconds (float). Accepts nil (= now), an
/// integer/float of seconds, a `(TICKS . HZ)` pair, or a `(HIGH LOW [USEC ...])`
/// legacy list.
fn time_arg_secs(h: &ElispHost, v: Option<&Value>) -> Result<f64, String> {
    crate::timefns::float_seconds(h, v)
}

/// Decompose epoch seconds into a `struct tm` for the given ZONE (nil = local,
/// non-nil non-number = UTC, integer = fixed offset seconds east of UTC).
/// Validate Emacs's ZONE argument (`tzlookup`, editfns.c).
///
/// The accepted spellings are exactly: `nil` and the symbol `wall` (local time),
/// `t` (UTC), an integer offset in seconds, a TZ string, and a two-element
/// `(OFFSET ABBR)` list. Everything else — including any *other* symbol, a
/// float, a vector, and a one-element list — is
/// `(error "Invalid time zone specification" ZONE)`. Measured on GNU Emacs 30.2:
///
/// ```text
/// (format-time-string "%Y" 0 'wall)      => "1969"
/// (format-time-string "%Y" 0 'utc)       => error, `utc' is not a spelling
/// (format-time-string "%Y" 0 '(3600 "X")) => "1970"
/// (format-time-string "%Y" 0 '(3600))    => error
/// ```
///
/// A float ZONE used to be accepted silently here and read as UTC.
fn check_time_zone(h: &mut ElispHost, zone: Option<&Value>) -> Result<bool, String> {
    let Some(z) = zone else { return Ok(false) };
    let mut wall = false;
    let ok = match z {
        Value::Undef | Value::Bool(_) | Value::Int(_) | Value::Str(_) => true,
        v if h.is_string(v) => true,
        v => match h.obj(v) {
            Some(Obj::Symbol(s)) => {
                wall = s.name == "wall";
                wall
            }
            // `(OFFSET ABBR)`: two elements, an integer and a string.
            Some(Obj::Cons(_, _)) => {
                let items = h.seq_vec(v);
                items.is_some_and(|it| {
                    it.len() == 2 && matches!(it[0], Value::Int(_)) && h.is_string(&it[1])
                })
            }
            _ => false,
        },
    };
    if ok {
        return Ok(wall);
    }
    let z = z.clone();
    Err(h.signal_error_with("Invalid time zone specification", &z))
}

fn time_decompose(secs: f64, zone: Option<&Value>, local: bool) -> libc::tm {
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
        // `wall` is the *local* zone, the same as nil — the spelling for
        // "whatever the wall clock says", not another name for UTC. It is a heap
        // symbol, so `check_time_zone` (which holds the host) reports it rather
        // than this function reaching for the thread-local and re-entering the
        // borrow it is already inside.
        _ if local => {
            let t = secs.floor() as libc::time_t;
            unsafe { libc::localtime_r(&t, &mut tm) };
        }
        _ => {
            let t = secs.floor() as libc::time_t;
            unsafe { libc::gmtime_r(&t, &mut tm) };
            // `gmtime_r` names the zone whatever the platform calls it: glibc
            // says "GMT", the BSD/macOS libc says "UTC". Emacs reaches UTC by
            // setting `TZ=UTC0`, so `%Z` there is "UTC" on every platform
            // (measured: `emacs --batch --eval '(format-time-string "%Z" 0 t)'`).
            // Stamp the name Emacs prints rather than inheriting libc's, which
            // made `%#Z` render "gmt" on Linux and "utc" on macOS.
            tm.tm_zone = c"UTC".as_ptr() as *mut libc::c_char;
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
        // Optional flags (-_0^#) then optional field width. `-_0` control
        // padding and are read by `numpad`; `^`/`#` case-fold the directive's
        // own output and are applied after it is produced.
        let mut flag: Option<char> = None;
        let mut case_flag: Option<char> = None;
        while i < chars.len() && matches!(chars[i], '-' | '_' | '0' | '^' | '#') {
            if matches!(chars[i], '-' | '_' | '0') {
                flag = Some(chars[i]);
            } else {
                case_flag = Some(chars[i]);
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
        // nstrftime: `:`, `::` and `:::` are valid only just before `z`;
        // anything else is a bad format, copied through literally.
        let mut colons = 0;
        while i + colons < chars.len() && chars[i + colons] == ':' {
            colons += 1;
        }
        if colons > 0 {
            if colons > 3 || chars.get(i + colons) != Some(&'z') {
                let start = chars[..i].iter().rposition(|&c| c == '%').unwrap_or(0);
                out.extend(&chars[start..i + colons]);
                i += colons;
                continue;
            }
            i += colons;
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
        // Everything this directive emits, so the case flags and the field
        // width can be applied to it afterwards.
        let seg_start = out.len();
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
            'z' => out.push_str(&tz_offset(tm.tm_gmtoff, colons, flag, user_w)),
            'F' => out.push_str(&fmt_time_string("%Y-%m-%d", tm, secs)),
            'T' => out.push_str(&fmt_time_string("%H:%M:%S", tm, secs)),
            'R' => out.push_str(&fmt_time_string("%H:%M", tm, secs)),
            'D' => out.push_str(&fmt_time_string("%m/%d/%y", tm, secs)),
            'c' => out.push_str(&fmt_time_string("%a %b %e %H:%M:%S %Y", tm, secs)),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            '%' => out.push('%'),
            // Century, and the ISO 8601 week-based year and week number. These
            // are not the calendar year and week: `%G`/`%V` follow the week
            // that contains the year's first Thursday, so 2024-01-01 (a Monday)
            // is week 01 of 2024 while 2023-01-01 (a Sunday) is week 52 of 2022.
            'C' => out.push_str(&numpad(year.div_euclid(100), 2, '0')),
            'G' => out.push_str(&numpad(iso_week_year(tm).0, 1, '0')),
            'g' => out.push_str(&numpad(iso_week_year(tm).0.rem_euclid(100), 2, '0')),
            'V' => out.push_str(&numpad(iso_week_year(tm).1, 2, '0')),
            // Week of the year counting from the first Sunday (`%U`) or the
            // first Monday (`%W`); days before it are week 00.
            'U' => out.push_str(&numpad(
                (tm.tm_yday as i64 + 7 - tm.tm_wday as i64) / 7,
                2,
                '0',
            )),
            'W' => out.push_str(&numpad(
                (tm.tm_yday as i64 + 7 - (tm.tm_wday as i64 + 6) % 7) / 7,
                2,
                '0',
            )),
            other => {
                out.push('%');
                out.push(other);
            }
        }
        // `^` upcases; `#` changes case — upcase unless the text is already
        // caseless-or-upper, in which case downcase. `(format-time-string "%#a")`
        // is "THU" and `"%#p"` is "am".
        if let Some(c) = case_flag {
            let seg = out.split_off(seg_start);
            let upcase = c == '^' || seg.chars().any(|ch| ch.is_lowercase());
            out.push_str(&if upcase {
                seg.to_uppercase()
            } else {
                seg.to_lowercase()
            });
        }
        // A field width on a STRING directive right-aligns it; the numeric ones
        // already consumed `user_w` through `numpad`.
        if let Some(w) = user_w {
            if matches!(d, 'p' | 'P' | 'a' | 'A' | 'b' | 'h' | 'B' | 'Z') {
                let seg = out.split_off(seg_start);
                let n = seg.chars().count();
                if n < w {
                    out.push_str(&" ".repeat(w - n));
                }
                out.push_str(&seg);
            }
        }
    }
    out
}

/// nstrftime's `%z` family (`do_z_conversion` / `do_tz_offset`): OFF seconds
/// east of UTC as `+hhmm` (no colons), `+hh:mm` (`%:z`), `+hh:mm:ss` (`%::z`),
/// or for `%:::z` the shortest of `+hh`, `+hh:mm`, `+hh:mm:ss` that is exact.
///
/// The sign always prints; the digits are zero-padded to the conversion's own
/// width (5, 6, 9 or 3 counting the sign) or a user WIDTH. The `-` flag drops
/// the padding (`%-:z` is `+1:00`), and `_` pads with spaces BEFORE the sign.
fn tz_offset(off: i64, colons: usize, flag: Option<char>, width: Option<usize>) -> String {
    let a = off.unsigned_abs();
    let (hh, mm, ss) = (a / 3600, (a % 3600) / 60, a % 60);
    let (digits, colon_mask, value) = match colons {
        0 => (5, 0, hh * 100 + mm),
        3 if ss == 0 && mm == 0 => (3, 0, hh),
        1 => (6, 0o4, hh * 100 + mm),
        3 if ss == 0 => (6, 0o4, hh * 100 + mm),
        _ => (9, 0o24, hh * 10000 + mm * 100 + ss),
    };
    // `do_number_body`: digits from the right, a colon wherever the mask says,
    // until both the value and the mask run out.
    let (mut v, mut mask, mut num) = (value, colon_mask, Vec::new());
    loop {
        if mask & 1 == 1 {
            num.push(':');
        }
        mask >>= 1;
        num.push(char::from(b'0' + (v % 10) as u8));
        v /= 10;
        if v == 0 && mask == 0 {
            break;
        }
    }
    num.reverse();
    let sign = if off < 0 { '-' } else { '+' };
    // `do_number_sign_and_padding`.
    let width = width.unwrap_or(digits);
    let pad = width.saturating_sub(1 + num.len());
    let mut out = String::new();
    match flag {
        Some('-') => out.push(sign),
        Some('_') => {
            out.push_str(&" ".repeat(pad));
            out.push(sign);
        }
        _ => {
            out.push(sign);
            out.push_str(&"0".repeat(pad));
        }
    }
    out.extend(num);
    out
}
/// The ISO 8601 week-based year and week number for `tm`.
///
/// A week runs Monday to Sunday and belongs to the year containing its
/// Thursday, so the first days of January can fall in the previous year's week
/// 52 or 53, and the last days of December in the next year's week 01.
fn iso_week_year(tm: &libc::tm) -> (i64, i64) {
    let year = tm.tm_year as i64 + 1900;
    // Monday = 1 … Sunday = 7.
    let wday = if tm.tm_wday == 0 {
        7
    } else {
        tm.tm_wday as i64
    };
    let yday = tm.tm_yday as i64 + 1; // 1-based day of year
    let week = (yday - wday + 10) / 7;
    if week < 1 {
        (year - 1, iso_weeks_in_year(year - 1))
    } else if week > iso_weeks_in_year(year) {
        (year + 1, 1)
    } else {
        (year, week)
    }
}

/// 52 or 53 — a year has 53 ISO weeks when it starts on a Thursday, or is a
/// leap year starting on a Wednesday.
fn iso_weeks_in_year(year: i64) -> i64 {
    let leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    // Day of week of 1 January, Monday = 1 … Sunday = 7 (Zeller-style).
    let jan1 = |y: i64| {
        let d = (y + (y - 1).div_euclid(4) - (y - 1).div_euclid(100) + (y - 1).div_euclid(400))
            .rem_euclid(7);
        if d == 0 {
            7
        } else {
            d
        }
    };
    if jan1(year) == 4 || (leap(year) && jan1(year) == 3) {
        53
    } else {
        52
    }
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

// `emacs-build-time' (lisp/version.el) is `(current-time)' evaluated while the
// binary is being DUMPED, so in a real Emacs it is a constant of the build:
//
//   $ for i in 1 2 3; do emacs -Q --batch --eval '(prin1 emacs-build-time)'; done
//   (27285 4897 897505 0)(27285 4897 897505 0)(27285 4897 897505 0)
//
// elisprs has no dump step, and evaluating `(current-time)' when the prelude
// runs made the value change on every invocation. The fixed point that
// corresponds to "when this build was made" here is the executable's mtime,
// which is what this returns, in `current-time' list form.
//
// It is also what makes the cache's shared post-prelude base reproducible: the
// base is one image for every entry in the shard, so a value that differs per
// process cannot live in it. A `(current-time)' there additionally meant a warm
// run replayed an older timestamp than a cold run computed — a cold/warm
// disagreement on the side Emacs does not have.
fn build_time(h: &mut ElispHost, _a: &[Value]) -> R {
    use std::os::unix::fs::MetadataExt;
    let (isec, nsec) = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| (m.mtime(), m.mtime_nsec()))
        .unwrap_or((0, 0));
    Ok(h.list_from(vec![
        Value::Int(isec >> 16),
        Value::Int(isec & 0xffff),
        Value::Int(nsec / 1000),
        Value::Int(0),
    ]))
}

fn format_time_string(h: &mut ElispHost, a: &[Value]) -> R {
    let fmt = as_string(h, &a[0])?;
    let secs = time_arg_secs(h, a.get(1))?;
    let local = check_time_zone(h, a.get(2))?;
    let tm = time_decompose(secs, a.get(2), local);
    Ok(h.new_string(fmt_time_string(&fmt, &tm, secs)))
}

fn current_time_string(h: &mut ElispHost, a: &[Value]) -> R {
    let secs = time_arg_secs(h, a.first())?;
    let local = check_time_zone(h, a.get(1))?;
    let tm = time_decompose(secs, a.get(1), local);
    Ok(h.new_string(fmt_time_string("%a %b %e %H:%M:%S %Y", &tm, secs)))
}

// `tm_gmtoff` is `c_long`; `i64::from` is needed on 32-bit but a no-op here.
#[allow(clippy::useless_conversion)]
fn decode_time(h: &mut ElispHost, a: &[Value]) -> R {
    // timefns.c `Fdecode_time`: FORM t keeps TIME's resolution in the seconds
    // element, `(TICKS . HZ)`; any other FORM decodes whole seconds only.
    let exact = if matches!(a.get(2), Some(Value::Bool(true))) {
        Some(crate::timefns::decode(h, a.first(), false)?)
    } else {
        None
    };
    let secs = match &exact {
        Some(t) => crate::timefns::floor_seconds(t)?,
        None => crate::timefns::seconds_argument(h, a.first())?,
    };
    let local = check_time_zone(h, a.get(1))?;
    let tm = time_decompose(secs as f64, a.get(1), local);
    let dst = match tm.tm_isdst {
        0 => Value::Undef,
        n if n > 0 => Value::Bool(true),
        _ => Value::Int(-1),
    };
    let sec = match exact {
        Some(t) if t.hz != BigInt::from(1) => {
            use num_integer::Integer;
            let ticks = t.ticks.mod_floor(&t.hz) + &t.hz * BigInt::from(tm.tm_sec);
            let ticks = h.make_integer(ticks);
            let hz = h.make_integer(t.hz);
            h.cons(ticks, hz)
        }
        _ => Value::Int(tm.tm_sec as i64),
    };
    Ok(h.list_from(vec![
        sec,
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

/// timefns.c `Fencode_time`. The seconds element is any time value, so a
/// sub-second one comes back as `(TICKS . HZ)` at its own resolution; whole
/// seconds come back as `(HI LO)`.
fn encode_time(h: &mut ElispHost, a: &[Value]) -> R {
    let mut isdst: libc::c_int = -1;
    let (fields, zone): (Vec<Value>, Value) = if a.len() == 1 {
        let mut fields = Vec::with_capacity(6);
        let mut tail = a[0].clone();
        for _ in 0..6 {
            let Some(Obj::Cons(car, cdr)) = h.obj(&tail) else {
                return Err(h.signal_wrong_type("consp", &tail));
            };
            fields.push(car.clone());
            tail = cdr.clone();
        }
        let mut zone = Value::Undef;
        if !is_nil(&tail) {
            let Some(Obj::Cons(_, rest)) = h.obj(&tail) else {
                return Err(h.signal_wrong_type("consp", &tail));
            };
            let rest = rest.clone();
            let Some(Obj::Cons(dstflag, rest2)) = h.obj(&rest) else {
                return Err(h.signal_wrong_type("consp", &rest));
            };
            let (dstflag, rest2) = (dstflag.clone(), rest2.clone());
            let Some(Obj::Cons(z, _)) = h.obj(&rest2) else {
                return Err(h.signal_wrong_type("consp", &rest2));
            };
            zone = z.clone();
            let is_symbol = is_nil(&dstflag)
                || matches!(dstflag, Value::Bool(true))
                || h.sym_name(&dstflag).is_some();
            let zone_fixnum_or_cons =
                matches!(zone, Value::Int(_)) || matches!(h.obj(&zone), Some(Obj::Cons(..)));
            if is_symbol && !zone_fixnum_or_cons {
                isdst = libc::c_int::from(!is_nil(&dstflag));
            }
        }
        (fields, zone)
    } else if a.len() < 6 {
        let f = h.intern("encode-time");
        let n = h.list_from(vec![f, Value::Int(a.len() as i64)]);
        let sym = h.intern("wrong-number-of-arguments");
        let obj = h.cons(sym, n);
        let msg = format!("wrong-number-of-arguments: encode-time {}", a.len());
        h.set_pending_error(&msg, obj);
        return Err(msg);
    } else {
        let zone = if a.len() > 6 {
            a[a.len() - 1].clone()
        } else {
            Value::Undef
        };
        (a[..6].to_vec(), zone)
    };
    let t = crate::timefns::decode(h, Some(&fields[0]), false)?;
    let (sec, subsec) = {
        use num_integer::Integer;
        t.ticks.div_mod_floor(&t.hz)
    };
    // `check_tm_member`: each field a fixnum whose offset value fits an int.
    let member = |h: &mut ElispHost, v: &Value, offset: i64| -> Result<libc::c_int, String> {
        let Value::Int(n) = v else {
            return Err(h.signal_wrong_type("fixnump", v));
        };
        libc::c_int::try_from(n - offset).map_err(|_| crate::timefns::overflow())
    };
    let sec_v = h.make_integer(sec);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_sec = member(h, &sec_v, 0)?;
    tm.tm_min = member(h, &fields[1], 0)?;
    tm.tm_hour = member(h, &fields[2], 0)?;
    tm.tm_mday = member(h, &fields[3], 0)?;
    tm.tm_mon = member(h, &fields[4], 1)?;
    tm.tm_year = member(h, &fields[5], 1900)?;
    tm.tm_isdst = isdst;
    let local = check_time_zone(h, Some(&zone))?;
    let value: i64 = match &zone {
        Value::Int(off) => unsafe { libc::timegm(&mut tm) as i64 - *off },
        _ if local || is_nil(&zone) => unsafe { libc::mktime(&mut tm) as i64 },
        _ => unsafe { libc::timegm(&mut tm) as i64 },
    };
    if t.hz == BigInt::from(1) {
        return Ok(h.list_from(vec![Value::Int(value >> 16), Value::Int(value & 0xffff)]));
    }
    let ticks = BigInt::from(value) * &t.hz + subsec;
    let ticks = h.make_integer(ticks);
    let hz = h.make_integer(t.hz);
    Ok(h.cons(ticks, hz))
}

// ── environment / working directory ──
fn getenv_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let name = as_string(h, &a[0])?;
    Ok(std::env::var(&name).map(Value::str).unwrap_or(Value::Undef))
}
fn setenv_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let name = as_string(h, &a[0])?;
    match a.get(1) {
        Some(v) if !is_nil(v) => {
            let val = as_string(h, v)?;
            std::env::set_var(&name, &val);
            Ok(h.new_string(val))
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
    // eval.c `lambda_arity` for a `(lambda ARGLIST . BODY)` list.
    if let Some(fun) = h.indirect_lambda_list(&a[0]) {
        let arglist = match h.obj(&fun) {
            Some(Obj::Cons(_, rest)) => match h.obj(rest) {
                Some(Obj::Cons(args, _)) => args.clone(),
                _ => return Err(h.signal_invalid_function(&fun)),
            },
            _ => return Err(h.signal_invalid_function(&fun)),
        };
        let (mut min, mut max, mut optional) = (0i64, 0i64, false);
        let mut cur = arglist;
        while let Some(Obj::Cons(next, rest)) = h.obj(&cur) {
            let (next, rest) = (next.clone(), rest.clone());
            match h.sym_name(&next).as_deref() {
                None => return Err(h.signal_invalid_function(&fun)),
                Some("&rest") => {
                    let many = h.intern("many");
                    return Ok(h.cons(Value::Int(min), many));
                }
                Some("&optional") => optional = true,
                Some(_) => {
                    if !optional {
                        min += 1;
                    }
                    max += 1;
                }
            }
            cur = rest;
        }
        if !matches!(cur, Value::Undef | Value::Bool(false)) {
            return Err(h.signal_invalid_function(&fun));
        }
        return Ok(h.cons(Value::Int(min), Value::Int(max)));
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
fn current_directory(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.new_string(
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "/".to_string()),
    ))
}

/// Emacs C `Vsystem_type` (emacs.c), computed at build/configure time. We derive
/// the same symbol from the running platform so a `.el` file gets the value it
/// would see under a native Emacs on this host. Mapping matches Emacs's
/// `s/*.h` `SYSTEM_TYPE` conventions: macOS -> `darwin`, Linux -> `gnu/linux`,
/// the BSDs -> `berkeley-unix`, Windows -> `windows-nt`.
fn system_type(h: &mut ElispHost, _a: &[Value]) -> R {
    let sym = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "gnu/linux",
        "windows" => "windows-nt",
        "freebsd" | "openbsd" | "netbsd" | "dragonfly" => "berkeley-unix",
        "android" => "android",
        _ => "gnu/linux",
    };
    Ok(h.intern(sym))
}

// `system-name' (Fsystem_name, editfns.c): return the host name of the machine
// as a string. Emacs caches Vsystem_name from gethostname(2) at init; we query
// it directly. gethostname failure is effectively unreachable; the "unknown"
// fallback mirrors Emacs's own default when the host name is unavailable.
fn system_name(h: &mut ElispHost, _a: &[Value]) -> R {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return Ok(h.new_string("unknown"));
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    if end == 0 {
        return Ok(h.new_string("unknown"));
    }
    Ok(h.new_string(String::from_utf8_lossy(&buf[..end]).into_owned()))
}

/// Raw temp-dir string behind Emacs C `Vtemporary_file_directory`
/// (callproc.c `init_callproc`). The caller wraps this in
/// `file-name-as-directory`, exactly as the C does (`build_string` then
/// `Ffile_name_as_directory`). Resolution order matches Emacs:
///   1. `$TMPDIR` if present in the environment — even when empty, so an empty
///      `TMPDIR` yields `""` which `file-name-as-directory` turns into `"./"`.
///   2. otherwise on macOS the per-user Darwin temp dir from
///      `confstr(_CS_DARWIN_USER_TEMP_DIR)` (e.g. `/var/folders/.../T/`).
///   3. otherwise `"/tmp/"`.
fn temp_directory(h: &mut ElispHost, _a: &[Value]) -> R {
    // `std::env::var` returns Ok("") when TMPDIR is set but empty, and
    // Err(NotPresent) only when it is absent — matching Emacs's `egetenv`
    // "non-NULL means present" test.
    if let Ok(v) = std::env::var("TMPDIR") {
        return Ok(h.new_string(v));
    }
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0u8; 1024];
        let n = unsafe {
            libc::confstr(
                libc::_CS_DARWIN_USER_TEMP_DIR,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
            )
        };
        // confstr returns the byte length including the trailing NUL; 0 means
        // the name is unknown, > buf.len() means it was truncated.
        if n > 1 && n <= buf.len() {
            if let Ok(s) = std::str::from_utf8(&buf[..n - 1]) {
                return Ok(h.new_string(s.to_string()));
            }
        }
    }
    Ok(h.new_string("/tmp/".to_string()))
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
fn file_exists_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_expand(&as_string(h, &a[0])?).exists()))
}
fn file_directory_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_expand(&as_string(h, &a[0])?).is_dir()))
}
fn file_regular_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_expand(&as_string(h, &a[0])?).is_file()))
}
fn file_readable_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_access(
        &fs_expand(&as_string(h, &a[0])?),
        libc::R_OK,
    )))
}
fn file_writable_p(h: &mut ElispHost, a: &[Value]) -> R {
    let p = fs_expand(&as_string(h, &a[0])?);
    // For a non-existent file, writability is the parent directory's.
    let target = if p.exists() {
        p.clone()
    } else {
        p.parent().map(|x| x.to_path_buf()).unwrap_or(p)
    };
    Ok(nil_or(fs_access(&target, libc::W_OK)))
}
fn file_symlink_p(h: &mut ElispHost, a: &[Value]) -> R {
    match std::fs::read_link(fs_expand(&as_string(h, &a[0])?)) {
        Ok(t) => Ok(h.new_string(t.to_string_lossy().into_owned())),
        Err(_) => Ok(Value::Undef),
    }
}
fn file_executable_p(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(fs_access(
        &fs_expand(&as_string(h, &a[0])?),
        libc::X_OK,
    )))
}
/// Internal: absolute path of the running `elisp` binary, backing
/// `invocation-name'/`invocation-directory'/`exec-directory'. Falls back to the
/// bare name `"elisp"` if the OS cannot report the executable path.
fn invocation_file(h: &mut ElispHost, _a: &[Value]) -> R {
    Ok(h.new_string(
        std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "elisp".to_string()),
    ))
}
fn directory_files_raw(h: &mut ElispHost, a: &[Value]) -> R {
    let raw = as_string(h, &a[0])?;
    let match_re = match a.get(1) {
        Some(v) if !is_nil(v) => Some(compile_cf(h, &as_string(h, v)?, false)?),
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
    // `Fget_buffer` returns a buffer *object* unchanged — live or killed — and
    // only the by-name branch can answer nil. See `ElispHost::get_buffer`.
    match h.get_buffer(&a[0])? {
        Some(idx) => Ok(h.buffers[idx].self_obj.clone()),
        None => Ok(Value::Undef),
    }
}
fn get_buffer_create(h: &mut ElispHost, a: &[Value]) -> R {
    let name = as_string(h, &a[0])?;
    check_buffer_name(&name)?;
    Ok(h.get_buffer_create(&name))
}
/// buffer.c refuses the empty name outright rather than making an unnameable
/// buffer, in `get-buffer-create` and in `generate-new-buffer-name` alike.
fn check_buffer_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("error: Empty string for buffer name is not allowed".to_string());
    }
    Ok(())
}
fn generate_new_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let base = as_string(h, &a[0])?;
    check_buffer_name(&base)?;
    let name = h.generate_new_buffer_name(&base, None);
    // generate-new-buffer always makes a fresh buffer (the unique name is free).
    Ok(h.get_buffer_create(&name))
}
fn generate_new_buffer_name(h: &mut ElispHost, a: &[Value]) -> R {
    let base = as_string(h, &a[0])?;
    // IGNORE (a name that may be reused even if taken) is only honored when it is
    // a string; `Fstring_equal` is guarded by `!NILP (ignore)` for NAME itself and
    // simply never matches a nil IGNORE inside the `<N>` loop.
    let ignore = match a.get(1) {
        Some(v) if !is_nil(v) => Some(as_string(h, v)?),
        _ => None,
    };
    Ok(h.new_string(h.generate_new_buffer_name(&base, ignore.as_deref())))
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
        Some(n) => h.new_string(n.clone()),
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
    let newname = as_string(h, &a[0])?;
    let unique = a.get(1).is_some_and(|v| !is_nil(v));
    h.rename_buffer(&newname, unique)
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
/// `(narrow-to-region START END)` — port of `Fnarrow_to_region` (editfns.c).
///
/// An inverted pair is accepted and swapped, but a pair outside the BUFFER —
/// not outside the current restriction, which the call is about to replace — is
/// `(args-out-of-range START END)`, naming the arguments in the order they were
/// given rather than the swapped order. Clamping instead silently narrowed to
/// something the caller did not ask for.
fn narrow_to_region(h: &mut ElispHost, a: &[Value]) -> R {
    let (lo, hi) = narrow_bounds(h, &a[0], &a[1])?;
    h.narrow(lo, hi);
    Ok(Value::Undef)
}
/// `Fnarrow_to_region`'s argument check: positions in either order, inside the
/// WHOLE buffer (`BEG`..`Z`, not the current restriction), or
/// `(args-out-of-range START END)` naming the arguments as given.
fn narrow_bounds(h: &mut ElispHost, b: &Value, e: &Value) -> Result<(usize, usize), String> {
    let (s0, e0) = (fix_position(h, b)?, fix_position(h, e)?);
    let (lo, hi) = if e0 < s0 { (e0, s0) } else { (s0, e0) };
    let z = h.cur_buf_ref().text.len() as i64 + 1;
    if lo < 1 || hi > z {
        return Err(args_out_of_range_values(h, vec![b.clone(), e.clone()]));
    }
    Ok((lo as usize, hi as usize))
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
            .ok_or_else(|| format!("wrong-type-argument: bufferp {}", h.print(v, true)))?,
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
fn insert_chars(h: &ElispHost, v: &Value) -> Result<Vec<char>, String> {
    if let Some(s) = h.str_text(v) {
        return Ok(s.chars().collect());
    }
    match v {
        Value::Int(n) => Ok(vec![char::from_u32(*n as u32).unwrap_or('\u{fffd}')]),
        _ => Err(format!(
            "wrong-type-argument: char-or-string-p {}",
            h.print(v, true)
        )),
    }
}
fn insert_fn(h: &mut ElispHost, a: &[Value]) -> R {
    for v in a {
        let start = h.cur_buf_ref().point; // 1-based insertion position
        let chars = insert_chars(h, v)?;
        if !chars.is_empty() {
            h.barf_if_read_only()?;
        }
        let n = chars.len();
        h.cur_insert(chars, true);
        // A propertized string carries its text properties into the buffer.
        if let Some(arc) = h.str_arc(v) {
            if let Some(plists) = h.string_props_vec(&arc) {
                let plists = h.copy_plist_runs(plists);
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
    let chunks = insert_chars(h, &a[0])?;
    if !chunks.is_empty() {
        h.barf_if_read_only()?;
    }
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
    let (out, arc) = h.new_string_keyed(text);
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
        let plists = h.copy_plist_runs(plists);
        h.string_set_props_vec(&arc, plists);
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
    let arg = as_int_or_marker(h, &a[0], "integer-or-marker-p")?;
    let buf = h.cur_buf();
    // Point is clamped to the accessible region, but `goto-char` returns its
    // POSITION argument unchanged — the marker object itself when it was given
    // one, not the position that marker held.
    buf.point = arg.clamp(buf.begv as i64, buf.zv as i64) as usize;
    Ok(a[0].clone())
}
/// `(position-bytes POSITION)` (editfns.c): the byte position of POSITION in
/// the whole buffer — the restriction does not apply — or nil outside it.
/// Bytes are counted in the internal UTF-8 encoding.
fn position_bytes(h: &mut ElispHost, a: &[Value]) -> R {
    let pos = fix_position(h, &a[0])?;
    let text = &h.cur_buf_ref().text;
    if !(1 <= pos && pos <= text.len() as i64 + 1) {
        return Ok(Value::Undef);
    }
    let bytes: usize = text[..pos as usize - 1].iter().map(|c| c.len_utf8()).sum();
    Ok(Value::Int(bytes as i64 + 1))
}
/// `(byte-to-position BYTEPOS)` (editfns.c): the character position whose
/// bytes include BYTEPOS, or nil outside the whole buffer.
fn byte_to_position(h: &mut ElispHost, a: &[Value]) -> R {
    let want = as_fixnum_named(h, &a[0], "fixnump")?;
    if want < 1 {
        return Ok(Value::Undef);
    }
    let mut byte = 1i64;
    for (i, c) in h.cur_buf_ref().text.iter().enumerate() {
        let next = byte + c.len_utf8() as i64;
        if want < next {
            return Ok(Value::Int(i as i64 + 1));
        }
        byte = next;
    }
    Ok(if want == byte {
        Value::Int(h.cur_buf_ref().text.len() as i64 + 1)
    } else {
        Value::Undef
    })
}
/// `decode_buffer` (buffer.c): nil is the current buffer; anything else must
/// be a buffer object, live or killed — a buffer NAME is not accepted.
fn decode_buffer(h: &mut ElispHost, v: Option<&Value>) -> Result<usize, String> {
    match v {
        None => Ok(h.current),
        Some(v) if is_nil(v) => Ok(h.current),
        Some(v) => match h.obj(v) {
            Some(Obj::Buffer(idx)) => Ok(*idx),
            _ => Err(h.signal_wrong_type("bufferp", v)),
        },
    }
}
/// `Fbuffer_modified_p`: nil when `SAVE_MODIFF >= MODIFF`, else `autosaved`
/// when the buffer was auto-saved since its last change, else t.
fn buffer_modified_p(h: &mut ElispHost, a: &[Value]) -> R {
    let bi = decode_buffer(h, a.first())?;
    let m = h.buffers[bi].mods;
    Ok(if m.save_modiff >= m.modiff {
        Value::Undef
    } else if m.autosave_modiff == m.modiff {
        h.intern("autosaved")
    } else {
        Value::Bool(true)
    })
}
/// `Frestore_buffer_modified_p`: FLAG nil marks the current buffer unmodified;
/// non-nil makes it modified (advancing `MODIFF` when it was not), and
/// `autosaved` also records it as auto-saved. Answers FLAG.
fn restore_buffer_modified_p(h: &mut ElispHost, a: &[Value]) -> R {
    let flag = a[0].clone();
    let autosaved = h
        .find_symbol("autosaved")
        .is_some_and(|s| h.values_eq(&s, &flag));
    let m = &mut h.cur_buf().mods;
    if is_nil(&flag) {
        m.save_modiff = m.modiff;
    } else {
        if m.save_modiff >= m.modiff {
            m.save_modiff = m.modiff;
            m.incr(1);
        }
        if autosaved {
            m.autosave_modiff = m.modiff;
        }
    }
    Ok(flag)
}
/// `set-buffer-modified-p`: `restore-buffer-modified-p` plus the file-lock and
/// mode-line bookkeeping batch has no use for; it answers nil.
fn set_buffer_modified_p(h: &mut ElispHost, a: &[Value]) -> R {
    restore_buffer_modified_p(h, a)?;
    Ok(Value::Undef)
}
/// `(buffer-modified-tick &optional BUFFER)`: BUFFER's `MODIFF`.
fn buffer_modified_tick(h: &mut ElispHost, a: &[Value]) -> R {
    let bi = decode_buffer(h, a.first())?;
    Ok(Value::Int(h.buffers[bi].mods.modiff))
}
/// `(buffer-chars-modified-tick &optional BUFFER)`: BUFFER's `CHARS_MODIFF`,
/// which text-property changes leave alone.
fn buffer_chars_modified_tick(h: &mut ElispHost, a: &[Value]) -> R {
    let bi = decode_buffer(h, a.first())?;
    Ok(Value::Int(h.buffers[bi].mods.chars_modiff))
}
fn erase_buffer(h: &mut ElispHost, _a: &[Value]) -> R {
    // Delete the whole buffer (ignoring narrowing) and remove the restriction.
    let len = h.cur_buf().text.len();
    if len > 0 {
        h.barf_if_read_only()?;
    }
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
/// `args_out_of_range_3 (Fcurrent_buffer (), START, END)` — the buffer *object*
/// leads the DATA, so the object is attached rather than rendered and re-read
/// (`#<buffer zb>` has no read syntax).
fn args_out_of_range_in_buffer(h: &mut ElispHost, start: i64, end: i64) -> String {
    let buf = h.current_buffer();
    let sym = h.intern("args-out-of-range");
    let data = h.list_from(vec![buf.clone(), Value::Int(start), Value::Int(end)]);
    let obj = h.cons(sym, data);
    let msg = format!("args-out-of-range: {} {start} {end}", h.print(&buf, true));
    h.set_pending_error(&msg, obj);
    msg
}

fn buffer_substring_core(h: &mut ElispHost, a: &[Value], with_props: bool) -> R {
    let s0 = as_int_or_marker(h, &a[0], "integer-or-marker-p")?;
    let e0 = as_int_or_marker(h, &a[1], "integer-or-marker-p")?;
    let buf = h.cur_buf_ref();
    let (lo0, hi0) = (buf.begv as i64, buf.zv as i64);
    // `validate_region`: swap first, then reject anything outside the accessible
    // portion. Clamping instead silently answered the whole buffer for
    // `(buffer-substring 1 999)`, where Emacs signals.
    let (lo, hi) = if s0 <= e0 { (s0, e0) } else { (e0, s0) };
    if lo < lo0 || hi > hi0 {
        return Err(args_out_of_range_in_buffer(h, s0, e0));
    }
    let text: String = buf.text[(lo - 1) as usize..(hi - 1) as usize]
        .iter()
        .collect();
    let (out, arc) = h.new_string_keyed(text);
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
            let plists = h.copy_plist_runs(plists);
            h.string_set_props_vec(&arc, plists);
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
/// Port of `Finsert_buffer_substring` (editfns.c): insert BUFFER's text from
/// START to END (default its accessible region), with its text properties,
/// before point in the current buffer.
fn insert_buffer_substring(h: &mut ElispHost, a: &[Value]) -> R {
    let Some(bi) = h.get_buffer(&a[0])? else {
        return Err(h.nsberror(&a[0]));
    };
    if h.buffers[bi].name.is_none() {
        return Err("error: Selecting deleted buffer".to_string());
    }
    let (begv, zv) = (h.buffers[bi].begv as i64, h.buffers[bi].zv as i64);
    let b = match a.get(1) {
        Some(v) if !is_nil(v) => fix_position(h, v)?,
        _ => begv,
    };
    let e = match a.get(2) {
        Some(v) if !is_nil(v) => fix_position(h, v)?,
        _ => zv,
    };
    let (lo, hi) = if b > e { (e, b) } else { (b, e) };
    if !(begv <= lo && hi <= zv) {
        let data = vec![
            a.get(1).cloned().unwrap_or(Value::Undef),
            a.get(2).cloned().unwrap_or(Value::Undef),
        ];
        return Err(args_out_of_range_values(h, data));
    }
    let cur = h.current;
    h.current = bi;
    let text = buffer_substring_core(h, &[Value::Int(lo), Value::Int(hi)], true);
    h.current = cur;
    insert_fn(h, &[text?])?;
    Ok(Value::Undef)
}
/// `(point-min-marker)` / `(point-max-marker)`: a marker at the edge of the
/// accessible region.
fn point_min_marker(h: &mut ElispHost, _a: &[Value]) -> R {
    let (bi, p) = (h.current, h.cur_buf_ref().begv);
    Ok(h.alloc_marker(Some(bi), p, false))
}
fn point_max_marker(h: &mut ElispHost, _a: &[Value]) -> R {
    let (bi, p) = (h.current, h.cur_buf_ref().zv);
    Ok(h.alloc_marker(Some(bi), p, false))
}
/// Port of `Fdelete_and_extract_region` (editfns.c): delete START..END and
/// answer the deleted text, text properties included.
fn delete_and_extract_region(h: &mut ElispHost, a: &[Value]) -> R {
    let (lo, hi) = validate_region(h, &a[0], &a[1])?;
    if lo == hi {
        return Ok(h.new_string(String::new()));
    }
    let text = buffer_substring_core(h, &[Value::Int(lo as i64), Value::Int(hi as i64)], true)?;
    h.barf_if_read_only()?;
    h.cur_delete(lo, hi);
    Ok(text)
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
        Some(v) if h.is_string(v) => Ok(PropObj::Str(h.str_arc(v).expect("checked stringp"))),
        None | Some(Value::Undef) | Some(Value::Bool(false)) => Ok(PropObj::Buf(h.current)),
        Some(v) => match h.resolve_buffer(v) {
            Some(bi) => Ok(PropObj::Buf(bi)),
            // Emacs's text-property fns validate OBJECT with `buffer-or-string-p`
            // (textprop.c validate_interval_range), not plain `bufferp`.
            None => Err(format!(
                "wrong-type-argument: buffer-or-string-p {}",
                h.print(v, true)
            )),
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
    let changed = props_would_change(h, &obj, lo, hi, &[(prop.clone(), val.clone())], true);
    match obj {
        PropObj::Str(s) => h.string_put_prop(&s, lo, hi, &prop, &val),
        PropObj::Buf(bi) => {
            with_buffer(h, bi, |h| h.buffer_put_prop(lo, hi, &prop, &val));
            if changed {
                h.note_prop_change(bi);
            }
        }
    }
    Ok(Value::Undef)
}
fn set_text_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let start = as_int(h, &a[0])?;
    let end = as_int(h, &a[1])?;
    let plist = a[2].clone();
    let obj = prop_object(h, a.get(3))?;
    let (lo, hi) = prop_range(&obj, start, end, h);
    // textprop.c `set_text_properties`: an empty range, or an object with no
    // intervals at all and nil PROPERTIES, returns nil without touching
    // anything; otherwise t. A buffer keeps its interval tree after its
    // properties are removed, until all of its text is deleted.
    let has_props = |h: &ElispHost| match &obj {
        PropObj::Str(s) => h
            .string_props_vec(s)
            .is_some_and(|v| v.iter().any(|p| !is_nil(p))),
        PropObj::Buf(bi) => h.buffers[*bi].has_intervals,
    };
    if lo == hi {
        return Ok(Value::Undef);
    }
    if is_nil(&plist) && !has_props(h) {
        return Ok(Value::Undef);
    }
    match obj {
        PropObj::Str(s) => h.string_set_props(&s, lo, hi, &plist),
        PropObj::Buf(bi) => {
            with_buffer(h, bi, |h| h.buffer_set_props(lo, hi, &plist));
            if lo < hi {
                h.note_prop_change(bi);
            }
        }
    }
    Ok(Value::Bool(true))
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
/// The value PROP has in PLIST (`eq` keys), or `None` when PROP is absent —
/// a present nil value is `Some`.
fn plist_lookup(h: &ElispHost, plist: &Value, prop: &Value) -> Option<Value> {
    let mut cur = plist.clone();
    while let Some(Obj::Cons(k, d)) = h.obj(&cur) {
        let (k, d) = (k.clone(), d.clone());
        let Some(Obj::Cons(v, rest)) = h.obj(&d) else {
            return None;
        };
        if h.values_eq(&k, prop) {
            return Some(v.clone());
        }
        cur = rest.clone();
    }
    None
}
/// Whether any character in `[lo, hi)` of OBJ would change: textprop.c
/// `add_properties` (PROP absent or not `eq` to its new value) when ADD,
/// `remove_properties` (PROP present at all) otherwise. Their callers
/// return t exactly when this holds.
fn props_would_change(
    h: &ElispHost,
    obj: &PropObj,
    lo: usize,
    hi: usize,
    pairs: &[(Value, Value)],
    add: bool,
) -> bool {
    (lo..hi).any(|i| {
        let plist = match obj {
            PropObj::Str(s) => h.string_plist_at(s, i),
            PropObj::Buf(bi) => h.buffer_plist_at_idx(*bi, i),
        };
        pairs
            .iter()
            .any(|(prop, val)| match plist_lookup(h, &plist, prop) {
                Some(old) => !add || !h.values_eq(&old, val),
                None => add,
            })
    })
}
fn add_text_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let start = as_int(h, &a[0])?;
    let end = as_int(h, &a[1])?;
    let obj = prop_object(h, a.get(3))?;
    let (lo, hi) = prop_range(&obj, start, end, h);
    let pairs = each_prop_pair(h, &a[2]);
    let changed = props_would_change(h, &obj, lo, hi, &pairs, true);
    for (prop, val) in pairs {
        match &obj {
            PropObj::Str(s) => h.string_put_prop(s, lo, hi, &prop, &val),
            PropObj::Buf(bi) => {
                let bi = *bi;
                with_buffer(h, bi, |h| h.buffer_put_prop(lo, hi, &prop, &val));
            }
        }
    }
    if let (true, PropObj::Buf(bi)) = (changed, &obj) {
        h.note_prop_change(*bi);
    }
    Ok(Value::Bool(changed))
}
fn remove_text_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let start = as_int(h, &a[0])?;
    let end = as_int(h, &a[1])?;
    let obj = prop_object(h, a.get(3))?;
    let (lo, hi) = prop_range(&obj, start, end, h);
    let pairs = each_prop_pair(h, &a[2]);
    let changed = props_would_change(h, &obj, lo, hi, &pairs, false);
    for (prop, _) in pairs {
        match &obj {
            PropObj::Str(s) => h.string_remove_prop(s, lo, hi, &prop),
            PropObj::Buf(bi) => {
                let bi = *bi;
                with_buffer(h, bi, |h| h.buffer_remove_prop(lo, hi, &prop));
            }
        }
    }
    if let (true, PropObj::Buf(bi)) = (changed, &obj) {
        h.note_prop_change(*bi);
    }
    Ok(Value::Bool(changed))
}
/// `(elisprs--replace-chars-in-place START END STRING)`: overwrite the text
/// from START to END with STRING's characters when it has exactly as many,
/// leaving text properties, markers and point where they are — what casefiddle.c
/// and `subst-char-in-region` do to the buffer through `modify_text`, which
/// counts one change of END - START characters. Answers nil, changing nothing,
/// when the lengths differ, so the Lisp caller can fall back to delete and
/// insert. An elisprs-internal primitive: Emacs does this in C.
fn replace_chars_in_place_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (lo, hi) = validate_region(h, &a[0], &a[1])?;
    let new: Vec<char> = as_string(h, &a[2])?.chars().collect();
    if new.len() != hi - lo {
        return Ok(Value::Undef);
    }
    if lo < hi {
        h.barf_if_read_only()?;
        h.note_text_change(hi - lo);
        h.cur_buf().text[lo - 1..hi - 1].copy_from_slice(&new);
    }
    Ok(Value::Bool(true))
}
/// `(elisprs--carry-text-properties SRC DST)` — DST with SRC's per-character
/// text properties, when the two are the same length.
///
/// Emacs exposes no such function: its case and trimming primitives are written
/// in C and copy intervals internally. The ones written in elisp here — mainly
/// `capitalize`, which rebuilds its result character by character — need a way
/// to say the same thing, so this is an elisprs-internal primitive rather than
/// an Emacs one. Returns DST unchanged when the lengths differ or SRC has no
/// properties.
fn carry_text_properties_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let (Some(src), Some(dst)) = (h.str_arc(&a[0]), h.str_arc(&a[1])) else {
        return Ok(a[1].clone());
    };
    if src.chars().count() == dst.chars().count() {
        h.string_carry_all(&dst, &src);
    }
    Ok(a[1].clone())
}

/// `(propertize STRING &rest PROPS)` — a fresh copy of STRING carrying PROPS.
fn propertize_fn(h: &mut ElispHost, a: &[Value]) -> R {
    let base = as_string(h, &a[0])?;
    let len = base.chars().count();
    let (out, arc) = h.new_string_keyed(base);
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
        h.string_set_props(&arc, 0, len, &plist);
    }
    Ok(out)
}
/// Port of `fix_position` (editfns.c): an integer or marker as a buffer
/// position. A bignum is accepted and pinned to the fixnum range in its
/// direction, so it reaches the caller's range check and is reported there as
/// out of range rather than as the wrong type.
fn fix_position(h: &mut ElispHost, v: &Value) -> Result<i64, String> {
    if let Some(Obj::Bignum(b)) = h.obj(v) {
        return Ok(if b.sign() == num_bigint::Sign::Minus {
            -MOST_POSITIVE_FIXNUM - 1
        } else {
            MOST_POSITIVE_FIXNUM
        });
    }
    as_int_or_marker(h, v, "integer-or-marker-p")
}
/// Signal `(args-out-of-range ...DATA)` with DATA's objects as given.
fn args_out_of_range_values(h: &mut ElispHost, data: Vec<Value>) -> String {
    let sym = h.intern("args-out-of-range");
    let rendered: Vec<String> = data.iter().map(|v| h.print(v, true)).collect();
    let data = h.list_from(data);
    let obj = h.cons(sym, data);
    let msg = format!("args-out-of-range: {}", rendered.join(" "));
    h.set_pending_error(&msg, obj);
    msg
}
/// Port of `validate_region` (buffer.c): START and END in either order, both
/// inside the accessible portion, or `(args-out-of-range BUFFER START END)`.
fn validate_region(h: &mut ElispHost, b: &Value, e: &Value) -> Result<(usize, usize), String> {
    let (s0, e0) = (fix_position(h, b)?, fix_position(h, e)?);
    let (lo, hi) = if e0 < s0 { (e0, s0) } else { (s0, e0) };
    let buf = h.cur_buf_ref();
    if !(buf.begv as i64 <= lo && hi <= buf.zv as i64) {
        let cur = h.current_buffer();
        return Err(args_out_of_range_values(h, vec![cur, b.clone(), e.clone()]));
    }
    Ok((lo as usize, hi as usize))
}
fn delete_region(h: &mut ElispHost, a: &[Value]) -> R {
    let (lo, hi) = validate_region(h, &a[0], &a[1])?;
    if lo < hi {
        h.barf_if_read_only()?;
    }
    h.cur_delete(lo, hi);
    Ok(Value::Undef)
}
fn insert_file_contents(h: &mut ElispHost, a: &[Value]) -> R {
    let raw = as_string(h, &a[0])?;
    let content = std::fs::read_to_string(fs_expand(&raw))
        .map_err(|_| format!("file-missing: Opening input file: No such file: {raw}"))?;
    let chars: Vec<char> = content.chars().collect();
    let n = chars.len() as i64;
    h.cur_insert(chars, false); // leaves point at the beginning of the inserted text
    let raw = h.new_string(raw);
    Ok(h.list_from(vec![raw, Value::Int(n)]))
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
        Some(v) if !is_nil(v) => as_fixnum_named(h, v, "fixnump")?,
        _ => 1,
    };
    move_point_by(h, n)
}
fn backward_char(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_fixnum_named(h, v, "fixnump")?,
        _ => 1,
    };
    move_point_by(h, -n)
}
/// Port of `find_newline` (search.c) over the accessible region: scan from
/// START towards the limit for COUNT newlines (backwards when COUNT < 0).
///
/// Answers the position just past the COUNT-th newline found — in either
/// direction, `find_newline` lands AFTER the newline — or the limit (`zv`
/// forwards, `begv` backwards) when there are fewer. The second value is the
/// number found, negative when scanning backwards, like `*counted`.
fn find_newline(t: &[char], start: usize, count: i64, begv: usize, zv: usize) -> (usize, i64) {
    let mut p = start;
    let mut found = 0i64;
    if count > 0 {
        while p < zv {
            p += 1;
            if t[p - 2] == '\n' {
                found += 1;
                if found == count {
                    return (p, found);
                }
            }
        }
        (zv, found)
    } else {
        while p > begv {
            if t[p - 2] == '\n' {
                found += 1;
                if found == -count {
                    return (p, -found);
                }
            }
            p -= 1;
        }
        (begv, -found)
    }
}
/// Port of `scan_newline_from_point`: COUNT <= 0 scans back for `1 - COUNT`
/// newlines, so 0 is the start of point's own line.
fn scan_newline_from_point(buf: &crate::host::EditBuffer, count: i64) -> (usize, i64) {
    let cnt = if count <= 0 { count - 1 } else { count };
    find_newline(&buf.text, buf.point, cnt, buf.begv, buf.zv)
}
/// An optional line-count argument: nil is DEFAULT, a fixnum is taken as is, and
/// a bignum stands for "more lines than any buffer has" in its direction, as
/// `bol`/`eol` clip it to `BUF_BYTES_MAX`. Anything else is not an integer.
fn line_count_arg(h: &ElispHost, a: &[Value], default: i64) -> Result<i64, String> {
    const HUGE: i64 = i64::MAX / 4;
    match a.first() {
        None => Ok(default),
        Some(v) if is_nil(v) => Ok(default),
        Some(v) => match h.obj(v) {
            Some(Obj::Bignum(b)) => Ok(if b.sign() == num_bigint::Sign::Minus {
                -HUGE
            } else {
                HUGE
            }),
            _ => as_integer(h, v),
        },
    }
}
/// Port of `bol` (editfns.c): the start of the line N - 1 lines from point's.
fn bol_n(h: &mut ElispHost, a: &[Value]) -> Result<usize, String> {
    let n = line_count_arg(h, a, 1)?;
    Ok(scan_newline_from_point(h.cur_buf(), n.saturating_sub(1)).0)
}
/// Port of `eol` (editfns.c) via `find_before_next_newline`: the end of the
/// line N - 1 lines from point's. A scan that found every newline it wanted
/// stops ON the last one; a short scan stops at the limit.
fn eol_n(h: &mut ElispHost, a: &[Value]) -> Result<usize, String> {
    let n = line_count_arg(h, a, 1)?;
    let cnt = n - (n <= 0) as i64;
    let buf = h.cur_buf();
    let (pos, counted) = find_newline(&buf.text, buf.point, cnt, buf.begv, buf.zv);
    Ok(if counted == cnt { pos - 1 } else { pos })
}
fn beginning_of_line(h: &mut ElispHost, a: &[Value]) -> R {
    let p = bol_n(h, a)?;
    h.cur_buf().point = p;
    Ok(Value::Undef)
}
fn end_of_line(h: &mut ElispHost, a: &[Value]) -> R {
    let p = eol_n(h, a)?;
    h.cur_buf().point = p;
    Ok(Value::Undef)
}
fn line_beginning_position(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(bol_n(h, a)? as i64))
}
fn line_end_position(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(Value::Int(eol_n(h, a)? as i64))
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
    // Port of `Fforward_line` (cmds.c).
    let count = line_count_arg(h, a, 1)?;
    let buf = h.cur_buf();
    let opoint = buf.point;
    let (pos, counted) = scan_newline_from_point(buf, count);
    buf.point = pos;
    let mut shortage = count - (count <= 0) as i64 - counted;
    if shortage != 0 {
        // A non-empty last line counts as a line moved across.
        shortage -= if count <= 0 {
            -1
        } else {
            (buf.begv < buf.zv && pos != opoint && buf.text[pos - 2] != '\n') as i64
        };
    }
    // A bignum N was scanned as a clipped COUNT; report against N itself.
    match a.first().and_then(|v| h.obj(v)).and_then(|o| match o {
        Obj::Bignum(b) => Some(b.clone()),
        _ => None,
    }) {
        Some(n) => Ok(h.make_integer(n + (shortage - count))),
        None => Ok(Value::Int(shortage)),
    }
}

// ── buffer search (sets buffer-position match data) ──
/// Record SPANS (0-based buffer offsets) as a match in the current buffer, which
/// becomes `last_thing_searched`.
fn set_buf_match(h: &mut ElispHost, spans0: &[Option<(usize, usize)>]) {
    let spans = spans0
        .iter()
        .map(|o| o.map(|(b, e)| (b + 1, e + 1)))
        .collect();
    h.match_data = Some(MatchData {
        subject: String::new(),
        spans,
        from_buffer: true,
        buffer: Some(h.current),
    });
}
/// The four search commands, driven COUNT times — a port of `search_command`
/// (search.c).
///
/// COUNT is not a decoration: `(search-forward "a" nil t 2)` finds the SECOND
/// occurrence, COUNT 0 searches not at all and answers point, and a NEGATIVE
/// COUNT searches the other way — `search-forward` with -1 is a backward
/// search.
///
/// BOUND must lie on the side of point the search goes (`Invalid search
/// bound`), and is then clipped to the accessible region; the search never
/// looks outside that region, narrowed or not.
///
/// Failure is per `search_command`: with NOERROR nil it signals; with NOERROR
/// `t` it answers nil and leaves point WHERE IT WAS; with any other non-nil
/// NOERROR it answers nil and moves point to the limit.
fn search_with_count(
    h: &mut ElispHost,
    a: &[Value],
    forward: bool,
    fwd: SearchStep,
    bwd: SearchStep,
) -> R {
    let noerror = a.get(2).cloned().unwrap_or(Value::Undef);
    let count = match a.get(3) {
        Some(v) if !is_nil(v) => as_fixnum_named(h, v, "fixnump")?,
        _ => 1,
    };
    let n = if forward { count } else { -count };
    let pat = as_string(h, &a[0])?;
    let (point, begv, zv) = {
        let b = h.cur_buf_ref();
        (b.point, b.begv, b.zv)
    };
    let lim = match a.get(1) {
        Some(v) if !is_nil(v) => {
            let lim = fix_position(h, v)?;
            if if n > 0 {
                lim < point as i64
            } else {
                lim > point as i64
            } {
                return Err("error: Invalid search bound (wrong side of point)".to_string());
            }
            lim.clamp(begv as i64, zv as i64) as usize
        }
        _ if n > 0 => zv,
        _ => begv,
    };
    if n == 0 {
        return Ok(Value::Int(point as i64));
    }
    let step = if count > 0 { fwd } else { bwd };
    let mut last = None;
    for _ in 0..count.unsigned_abs() {
        match step(h, &pat, lim)? {
            Some(p) => last = Some(p),
            None => {
                h.cur_buf().point = point;
                if is_nil(&noerror) {
                    return Err(format!("search-failed: {pat}"));
                }
                if !matches!(noerror, Value::Bool(true)) {
                    h.cur_buf().point = lim;
                }
                return Ok(Value::Undef);
            }
        }
    }
    Ok(Value::Int(last.unwrap_or(point as i64)))
}
/// One search from point to LIM: answers the new point, having moved there
/// and recorded the match, or `None` without touching either.
type SearchStep = fn(&mut ElispHost, &str, usize) -> Result<Option<i64>, String>;
fn search_forward(h: &mut ElispHost, a: &[Value]) -> R {
    search_with_count(h, a, true, search_forward_once, search_backward_once)
}
fn search_backward(h: &mut ElispHost, a: &[Value]) -> R {
    search_with_count(h, a, false, search_backward_once, search_forward_once)
}
fn re_search_forward(h: &mut ElispHost, a: &[Value]) -> R {
    search_with_count(h, a, true, re_search_forward_once, re_search_backward_once)
}
fn re_search_backward(h: &mut ElispHost, a: &[Value]) -> R {
    search_with_count(h, a, false, re_search_backward_once, re_search_forward_once)
}
/// A literal search is a regexp search for the quoted string: Emacs folds case
/// in both through the same `case-fold-search` translation.
fn literal_regexp(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '.' | '*' | '+' | '?' | '[' | '^' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
fn search_forward_once(h: &mut ElispHost, s: &str, lim: usize) -> Result<Option<i64>, String> {
    re_search_forward_once(h, &literal_regexp(s), lim)
}
fn search_backward_once(h: &mut ElispHost, s: &str, lim: usize) -> Result<Option<i64>, String> {
    re_search_backward_once(h, &literal_regexp(s), lim)
}
/// The current buffer's accessible portion as a string, with the 0-based
/// buffer offset of its first character. Every buffer regexp operation runs
/// on this, never on the whole text: Emacs hands `re_search_2` exactly the
/// region `BEGV`..`ZV`, so `\`` and `^` match at `BEGV`, `\'` and `$` at
/// `ZV`, and nothing outside the restriction can match.
fn accessible_text(h: &ElispHost) -> (String, usize) {
    let b = h.cur_buf_ref();
    (b.text[b.begv - 1..b.zv - 1].iter().collect(), b.begv - 1)
}
/// A match of RE starting exactly at char S of TEXT whose end is at most
/// STOP — `re_match_2_internal` with STOP as `stop`.
///
/// fancy-regex has no stop position, so this is two attempts. Matched against
/// the whole text, a match that already ends by STOP is the one Emacs finds:
/// a stop only prunes paths, so it cannot change which path succeeds first
/// when the first success never reaches it. Only when that match runs past
/// STOP is the text cut there — and then an end-of-text assertion (`\'`,
/// `$`, `\b`) right at STOP sees an end Emacs would not.
fn match_within(
    re: &CompiledRe,
    text: &str,
    s: usize,
    stop: usize,
    anchored: bool,
) -> Option<Spans> {
    let ok = |spans: &Spans| match spans[0] {
        Some((b, e)) => (!anchored || b == s) && e <= stop,
        None => false,
    };
    let m = run_match(re, text, s)?;
    if ok(&m) {
        return Some(m);
    }
    let first = m[0]?.0;
    if anchored && first != s || first > stop {
        return None;
    }
    let cut = &text[..byte_of_char(text, stop)];
    run_match(re, cut, s).filter(ok)
}
type Spans = Vec<Option<(usize, usize)>>;
/// Record SPANS (offsets into the accessible text starting at buffer offset
/// OFF) as the current buffer's match data, and answer them rebased.
fn record_buf_match(h: &mut ElispHost, spans: Spans, off: usize) -> Spans {
    let spans: Spans = spans
        .into_iter()
        .map(|o| o.map(|(b, e)| (b + off, e + off)))
        .collect();
    set_buf_match(h, &spans);
    spans
}
fn re_search_forward_once(h: &mut ElispHost, pat: &str, lim: usize) -> Result<Option<i64>, String> {
    let re = compile_cf(h, pat, case_fold_search(h))?;
    let (text, off) = accessible_text(h);
    let p = h.cur_buf_ref().point - 1 - off;
    let Some(m) = match_within(&re, &text, p, lim - 1 - off, false) else {
        return Ok(None);
    };
    let spans = record_buf_match(h, m, off);
    let end = spans[0].map_or(p + off, |(_, e)| e) + 1;
    h.cur_buf().point = end;
    Ok(Some(end as i64))
}
/// `(re-search-backward REGEXP &optional BOUND NOERROR COUNT)`.
///
/// A backward search is not "the last forward match before point". Emacs tries
/// START positions from point downwards and takes the FIRST that matches, and
/// it stops the match at the position the search started from — so for `a+` in
/// `"aaa"` with point at 4 the answer is a one-character match at 3, not the
/// three-character match at 1 that a forward scan finds first:
///
/// ```text
/// (with-temp-buffer (insert "aaa") (goto-char 4)
///   (list (re-search-backward "a+" nil t) (match-beginning 0) (match-end 0)))
///   => (3 3 4)
/// (with-temp-buffer (insert "aaa") (goto-char 3) …)   => (2 2 3)
/// ```
///
/// The second line is the end bound: at start 2 an unbounded `a+` would reach
/// 4, past where the search began. Collecting non-overlapping forward matches
/// instead answered 1 for the first and nil for the second, and ignored BOUND
/// entirely.
fn re_search_backward_once(
    h: &mut ElispHost,
    pat: &str,
    lim: usize,
) -> Result<Option<i64>, String> {
    let re = compile_cf(h, pat, case_fold_search(h))?;
    let (text, off) = accessible_text(h);
    let p = h.cur_buf_ref().point - 1 - off;
    let lower = lim - 1 - off;
    let mut s = p + 1;
    while s > lower {
        s -= 1;
        if let Some(m) = match_within(&re, &text, s, p, true) {
            let spans = record_buf_match(h, m, off);
            let begin = spans[0].map_or(s + off, |(b, _)| b) + 1;
            h.cur_buf().point = begin;
            return Ok(Some(begin as i64));
        }
    }
    Ok(None)
}
fn looking_at(h: &mut ElispHost, a: &[Value]) -> R {
    let re = compile_cf(h, &as_string(h, &a[0])?, case_fold_search(h))?;
    let (text, off) = accessible_text(h);
    let p = h.cur_buf_ref().point - 1 - off;
    let stop = text.chars().count();
    match match_within(&re, &text, p, stop, true) {
        Some(m) => {
            record_buf_match(h, m, off);
            Ok(Value::Bool(true))
        }
        None => Ok(Value::Undef),
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
///
/// A backslash before anything else — including the end of the string — is an
/// error, not a character to drop: `search.c`'s `Freplace_match` signals
/// `Invalid use of ‘\’ in replacement text` there, so `\q` and a trailing `\`
/// both fail rather than quietly producing `q` and nothing.
fn expand_repl(
    newtext: &str,
    gt: &dyn Fn(usize) -> String,
    in_string: bool,
) -> Result<String, String> {
    const BAD: &str = "error: Invalid use of ‘\\’ in replacement text";
    let chars: Vec<char> = newtext.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            let Some(&c) = chars.get(i + 1) else {
                return Err(BAD.to_string());
            };
            match c {
                '&' => out.push_str(&gt(0)),
                '0'..='9' => out.push_str(&gt(c as usize - '0' as usize)),
                '\\' => out.push(c),
                // `\?` (meaningful only to `query-replace-regexp`) is kept as
                // is in a string, and is an error in a buffer: search.c's
                // string loop lets `?` through, its buffer loop does not.
                '?' if in_string => out.push_str("\\?"),
                _ => return Err(BAD.to_string()),
            }
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    Ok(out)
}
/// `(replace-match NEWTEXT &optional FIXEDCASE LITERAL STRING SUBEXP)`. With a
/// STRING argument, returns a new string with the last `string-match` of STRING
/// replaced; otherwise edits the current buffer and leaves point after the
/// replacement. Expands `\&`/`\N`/`\\` unless LITERAL, adapts case unless
/// FIXEDCASE.
/// search.c's `(error "replace-match subexpression does not exist" SUBEXP)`.
/// Two DATA elements, so the render-and-re-read path every other error helper
/// uses cannot express it — the object is attached directly.
fn no_such_subexp(h: &mut ElispHost, subexp: usize) -> String {
    let sym = h.intern("error");
    let text = h.new_string("replace-match subexpression does not exist");
    let data = h.list_from(vec![text, Value::Int(subexp as i64)]);
    let obj = h.cons(sym, data);
    let msg = format!("error: replace-match subexpression does not exist: {subexp}");
    h.set_pending_error(&msg, obj);
    msg
}

/// search.c `Fmatch_data__translate`: add N to every recorded position,
/// flooring each at 0. `replace-regexp-in-string` uses it to aim the match
/// data at the matched substring instead of matching REGEXP a second time.
fn match_data_translate(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_fixnum_named(h, &a[0], "fixnump")?;
    if let Some(md) = h.match_data.as_mut() {
        for (b, e) in md.spans.iter_mut().flatten() {
            *b = (*b as i64 + n).max(0) as usize;
            *e = (*e as i64 + n).max(0) as usize;
        }
    }
    Ok(Value::Undef)
}
fn replace_match(h: &mut ElispHost, a: &[Value]) -> R {
    let newtext = as_string(h, &a[0])?;
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
    if let Some(s) = a.get(3).and_then(|v| h.str_text(v)).map(str::to_string) {
        let subject: Vec<char> = s.chars().collect();
        let Some((b, e)) = spans.get(subexp).copied().flatten() else {
            return Err(no_such_subexp(h, subexp));
        };
        // `Freplace_match` bounds-checks the span against STRING before touching
        // it: match data is global and outlives the string it was set from, so
        // the span routinely points past a *different* string. Indexing without
        // this check panicked the interpreter thread.
        if e > subject.len() || b > e {
            return Err(format!("args-out-of-range: {b} {e}"));
        }
        let gt = |n: usize| -> String {
            spans
                .get(n)
                .copied()
                .flatten()
                .filter(|(gb, ge)| *ge <= subject.len() && gb <= ge)
                .map(|(gb, ge)| subject[gb..ge].iter().collect::<String>())
                .unwrap_or_default()
        };
        let matched = gt(subexp);
        let rep = if literal {
            newtext
        } else {
            expand_repl(&newtext, &gt, true)?
        };
        let rep = if fixedcase {
            rep
        } else {
            adapt_replacement_case(&matched, rep)
        };
        let mut out: String = subject[..b].iter().collect();
        out.push_str(&rep);
        out.extend(&subject[e..]);
        return Ok(h.new_string(out));
    }
    let text: Vec<char> = h.cur_buf().text.clone();
    let (begv, zv) = (h.cur_buf().begv, h.cur_buf().zv);
    let Some((b, e)) = spans.get(subexp).copied().flatten() else {
        return Err(no_such_subexp(h, subexp));
    };
    // `! (BEGV <= sub_start && sub_end <= ZV)` => args_out_of_range, exactly as
    // search.c does. The global match data survives the buffer it was set in, so
    // without the check a stale span indexed past the buffer text and panicked.
    if b < begv || e > zv || b > e {
        return Err(format!("args-out-of-range: {b} {e}"));
    }
    let gt = |n: usize| -> String {
        spans
            .get(n)
            .copied()
            .flatten()
            .filter(|(gb, ge)| *gb >= begv && *ge <= zv && gb <= ge)
            .map(|(gb, ge)| text[(gb - 1)..(ge - 1)].iter().collect::<String>())
            .unwrap_or_default()
    };
    let matched = gt(subexp);
    let rep = if literal {
        newtext
    } else {
        expand_repl(&newtext, &gt, false)?
    };
    let rep = if fixedcase {
        rep
    } else {
        adapt_replacement_case(&matched, rep)
    };
    let rep_chars: Vec<char> = rep.chars().collect();
    h.barf_if_read_only()?;
    // Delete the matched span, then insert the replacement at its start (point is
    // left after the replacement, matching Emacs and adjusting markers/narrowing).
    h.cur_buf().point = b;
    h.cur_delete(b, e);
    h.cur_insert(rep_chars, true);
    Ok(Value::Undef)
}

// ── overlays ──
//
// An overlay is a buffer range carrying a property list. It differs from a text
// property in that it is an OBJECT: it survives the text under it changing, both
// of its ends move with edits like markers, and deleting it detaches it rather
// than clearing anything. `host::OverlayData` holds the range and the two
// advance flags; the owning buffer holds the same `Rc`, which is what lets
// `adjust_for_insert`/`adjust_for_delete` move every overlay in one pass.

/// The shared data behind an overlay object, or None if `v` is not one.
fn ov_rc(h: &ElispHost, v: &Value) -> Option<Rc<RefCell<crate::host::OverlayData>>> {
    match h.obj(v) {
        Some(Obj::Overlay(o)) => Some(o.clone()),
        _ => None,
    }
}
fn ov_of(h: &mut ElispHost, v: &Value) -> Result<Rc<RefCell<crate::host::OverlayData>>, String> {
    ov_rc(h, v).ok_or_else(|| h.signal_wrong_type("overlayp", v))
}
fn overlayp(h: &mut ElispHost, a: &[Value]) -> R {
    Ok(nil_or(ov_rc(h, &a[0]).is_some()))
}
/// `(make-overlay BEG END &optional BUFFER FRONT-ADVANCE REAR-ADVANCE)`.
///
/// An inverted pair is accepted and swapped, and both ends are clamped into the
/// buffer, so `(make-overlay 4 2)` is the same overlay as `(make-overlay 2 4)`.
fn make_overlay(h: &mut ElispHost, a: &[Value]) -> R {
    let bi = match a.get(2) {
        Some(v) if !is_nil(v) => h
            .resolve_buffer(v)
            .ok_or_else(|| h.signal_wrong_type("buffer-or-string-p", v))?,
        _ => h.current,
    };
    let z = h.buffers[bi].text.len() as i64 + 1;
    let clamp = |n: i64| n.clamp(1, z) as usize;
    let b0 = clamp(as_int(h, &a[0])?);
    let e0 = clamp(as_int(h, &a[1])?);
    let (start, end) = if b0 <= e0 { (b0, e0) } else { (e0, b0) };
    let od = Rc::new(RefCell::new(crate::host::OverlayData {
        buffer: Some(bi),
        start,
        end,
        front_advance: a.get(3).is_some_and(|v| !is_nil(v)),
        rear_advance: a.get(4).is_some_and(|v| !is_nil(v)),
        props: Vec::new(),
    }));
    h.buffers[bi].overlays.push(od.clone());
    Ok(h.alloc(Obj::Overlay(od)))
}
fn overlay_start(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let od = od.borrow();
    Ok(match od.buffer {
        Some(_) => Value::Int(od.start as i64),
        None => Value::Undef,
    })
}
fn overlay_end(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let od = od.borrow();
    Ok(match od.buffer {
        Some(_) => Value::Int(od.end as i64),
        None => Value::Undef,
    })
}
fn overlay_buffer(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let bi = od.borrow().buffer;
    Ok(match bi {
        Some(i) => h.buffer_object(i),
        None => Value::Undef,
    })
}
fn overlay_get(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let od = od.borrow();
    Ok(od
        .props
        .iter()
        .find(|(k, _)| el_eq(h, k, &a[1]))
        .map(|(_, v)| v.clone())
        .unwrap_or(Value::Undef))
}
/// `(overlay-put OV PROP VALUE)`. A property set twice keeps its original
/// position in the list, which is why `overlay-properties` is newest-first only
/// for properties that are new.
fn overlay_put(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let existing = od
        .borrow()
        .props
        .iter()
        .position(|(k, _)| el_eq(h, k, &a[1]));
    let mut od = od.borrow_mut();
    match existing {
        Some(i) => od.props[i].1 = a[2].clone(),
        None => od.props.insert(0, (a[1].clone(), a[2].clone())),
    }
    Ok(a[2].clone())
}
fn overlay_properties(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let flat: Vec<Value> = od
        .borrow()
        .props
        .iter()
        .flat_map(|(k, v)| [k.clone(), v.clone()])
        .collect();
    Ok(h.list_from(flat))
}
/// `(delete-overlay OV)` — detach it. The object stays an overlay; its ends and
/// buffer read as nil, and `move-overlay` can put it back.
fn delete_overlay(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let was = od.borrow().buffer;
    if let Some(bi) = was {
        h.buffers[bi].overlays.retain(|o| !Rc::ptr_eq(o, &od));
    }
    od.borrow_mut().buffer = None;
    Ok(Value::Undef)
}
/// `(move-overlay OV BEG END &optional BUFFER)` — also re-attaches a deleted
/// overlay, to BUFFER or to the current buffer.
fn move_overlay(h: &mut ElispHost, a: &[Value]) -> R {
    let od = ov_of(h, &a[0])?;
    let old = od.borrow().buffer;
    let bi = match a.get(3) {
        Some(v) if !is_nil(v) => h
            .resolve_buffer(v)
            .ok_or_else(|| h.signal_wrong_type("buffer-or-string-p", v))?,
        _ => old.unwrap_or(h.current),
    };
    let z = h.buffers[bi].text.len() as i64 + 1;
    let clamp = |n: i64| n.clamp(1, z) as usize;
    let b0 = clamp(as_int(h, &a[1])?);
    let e0 = clamp(as_int(h, &a[2])?);
    if old != Some(bi) {
        if let Some(o) = old {
            h.buffers[o].overlays.retain(|x| !Rc::ptr_eq(x, &od));
        }
        h.buffers[bi].overlays.push(od.clone());
    }
    {
        let mut m = od.borrow_mut();
        m.buffer = Some(bi);
        m.start = b0.min(e0);
        m.end = b0.max(e0);
    }
    Ok(a[0].clone())
}
/// Overlays of the current buffer whose range satisfies `keep`, in creation
/// order — which is the order `overlays-in` reports.
fn overlays_where(
    h: &mut ElispHost,
    keep: impl Fn(usize, usize) -> bool,
) -> Vec<Rc<RefCell<crate::host::OverlayData>>> {
    h.buffers[h.current]
        .overlays
        .iter()
        .filter(|o| {
            let od = o.borrow();
            keep(od.start, od.end)
        })
        .cloned()
        .collect()
}
fn overlays_to_list(h: &mut ElispHost, ovs: Vec<Rc<RefCell<crate::host::OverlayData>>>) -> R {
    let items: Vec<Value> = ovs.into_iter().map(|o| h.alloc(Obj::Overlay(o))).collect();
    Ok(h.list_from(items))
}
/// `(overlays-at POS)` — those that COVER POS, i.e. `start <= POS < end`. An
/// overlay ending at POS does not cover it, so `(overlays-at 3)` is nil for an
/// overlay from 1 to 3.
fn overlays_at(h: &mut ElispHost, a: &[Value]) -> R {
    let pos = as_int(h, &a[0])? as usize;
    let ovs = overlays_where(h, |s, e| s <= pos && pos < e);
    overlays_to_list(h, ovs)
}
/// `(overlays-in BEG END)` — those overlapping the range. An EMPTY overlay is
/// included when it sits inside it, including when BEG equals END.
fn overlays_in(h: &mut ElispHost, a: &[Value]) -> R {
    let beg = as_int(h, &a[0])? as usize;
    let end = as_int(h, &a[1])? as usize;
    let ovs = overlays_where(h, |s, e| {
        if s == e {
            beg <= s && s <= end
        } else {
            s < end && e > beg
        }
    });
    overlays_to_list(h, ovs)
}
/// `(next-overlay-change POS)` — the next position after POS where an overlay
/// begins or ends, or `point-max` when there is none.
fn next_overlay_change(h: &mut ElispHost, a: &[Value]) -> R {
    let pos = as_int(h, &a[0])? as usize;
    let zv = h.cur_buf_ref().zv;
    let mut best = zv;
    for o in h.buffers[h.current].overlays.iter() {
        let od = o.borrow();
        for p in [od.start, od.end] {
            if p > pos && p < best {
                best = p;
            }
        }
    }
    Ok(Value::Int(best as i64))
}
/// `(previous-overlay-change POS)` — the previous such position, or `point-min`.
fn previous_overlay_change(h: &mut ElispHost, a: &[Value]) -> R {
    let pos = as_int(h, &a[0])? as usize;
    let begv = h.cur_buf_ref().begv;
    let mut best = begv;
    for o in h.buffers[h.current].overlays.iter() {
        let od = o.borrow();
        for p in [od.start, od.end] {
            if p < pos && p > best {
                best = p;
            }
        }
    }
    Ok(Value::Int(best as i64))
}

// ── filesystem writes / mutations ──
fn write_region(h: &mut ElispHost, a: &[Value]) -> R {
    let append = a.get(3).is_some_and(|v| !is_nil(v));
    // START may be a string (write it directly) or a buffer position.
    let content: String = match h.str_text(&a[0]).map(str::to_string) {
        Some(s) => s,
        None => {
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
    let filename = as_string(h, &a[2])?;
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
fn delete_file(h: &mut ElispHost, a: &[Value]) -> R {
    let f = as_string(h, &a[0])?;
    std::fs::remove_file(fs_expand(&f)).map_err(|_| format!("file-error: Removing file: {f}"))?;
    Ok(Value::Undef)
}
fn make_directory(h: &mut ElispHost, a: &[Value]) -> R {
    let f = as_string(h, &a[0])?;
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
fn rename_file(h: &mut ElispHost, a: &[Value]) -> R {
    let (o, n) = (as_string(h, &a[0])?, as_string(h, &a[1])?);
    std::fs::rename(fs_expand(&o), fs_expand(&n))
        .map_err(|_| format!("file-error: Renaming: {o}"))?;
    Ok(Value::Undef)
}
fn copy_file(h: &mut ElispHost, a: &[Value]) -> R {
    let (o, n) = (as_string(h, &a[0])?, as_string(h, &a[1])?);
    std::fs::copy(fs_expand(&o), fs_expand(&n)).map_err(|_| format!("file-error: Copying: {o}"))?;
    Ok(Value::Undef)
}

// ── subprocesses ──
fn shell_command_to_string(h: &mut ElispHost, a: &[Value]) -> R {
    let cmd = as_string(h, &a[0])?;
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .output()
    {
        Ok(o) => Ok(h.new_string(String::from_utf8_lossy(&o.stdout).into_owned())),
        Err(_) => Ok(h.new_string(String::new())),
    }
}
fn call_process(h: &mut ElispHost, a: &[Value]) -> R {
    let program = as_string(h, &a[0])?;
    // a[1] INFILE and a[3] DISPLAY are ignored; a[2] DESTINATION; a[4..] ARGS.
    let args: Vec<String> = a
        .get(4..)
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| as_string(h, v).ok())
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
    let program = as_string(h, &a[0])?;
    let args: Vec<String> = a
        .get(1..)
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| as_string(h, v).ok())
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
/// cmds.c `Fdelete_char`: N characters past either end of the accessible
/// portion signal `beginning-of-buffer`/`end-of-buffer` instead of deleting
/// what is there. (KILLFLAG, which routes through `kill-forward-chars`, is
/// not honoured: a subr cannot call back into Lisp here.)
fn delete_char(h: &mut ElispHost, a: &[Value]) -> R {
    let n = as_fixnum_named(h, &a[0], "fixnump")?;
    let (point, begv, zv) = {
        let b = h.cur_buf_ref();
        (b.point as i64, b.begv as i64, b.zv as i64)
    };
    let pos = point + n;
    let (lo, hi) = if n < 0 {
        if pos < begv {
            return Err("beginning-of-buffer".to_string());
        }
        (pos, point)
    } else {
        if pos > zv {
            return Err("end-of-buffer".to_string());
        }
        (point, pos)
    };
    if lo < hi {
        h.barf_if_read_only()?;
    }
    h.cur_delete(lo as usize, hi as usize);
    Ok(Value::Undef)
}
fn insert_char(h: &mut ElispHost, a: &[Value]) -> R {
    let c = char::from_u32(as_int(h, &a[0])? as u32).unwrap_or('\u{fffd}');
    let count = match a.get(1) {
        Some(v) if !is_nil(v) => as_int(h, v)?.max(0) as usize,
        _ => 1,
    };
    if count > 0 {
        h.barf_if_read_only()?;
    }
    h.cur_insert(vec![c; count], true);
    Ok(Value::Undef)
}
fn count_lines(h: &mut ElispHost, a: &[Value]) -> R {
    // simple.el narrows to START..END first, so the bounds are narrow-to-region's.
    let (lo, hi) = narrow_bounds(h, &a[0], &a[1])?;
    let buf = h.cur_buf();
    let region = &buf.text[lo - 1..hi - 1];
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
/// `SANE_TAB_WIDTH` (buffer.h): `tab-width` when it is an integer in
/// 1..=1000, else 8.
fn sane_tab_width(h: &ElispHost) -> i64 {
    match h
        .find_symbol("tab-width")
        .and_then(|s| h.get_value(&s).ok())
    {
        Some(Value::Int(n)) if (1..=1000).contains(&n) => n,
        _ => 8,
    }
}
/// The columns character C advances from column COL: a tab to the next tab
/// stop, anything else its `char-width`.
fn advance_column(c: char, col: i64, tab_width: i64) -> i64 {
    if c == '\t' {
        (col / tab_width + 1) * tab_width
    } else {
        col + char_display_width(c as u32) as i64
    }
}
/// Port of `scan_for_column` (indent.c) without display properties or
/// invisibility: walk from the start of point's line towards END, stopping at a
/// newline or once column GOAL is reached. Answers `(pos, col, prev_pos,
/// prev_col)`, the `prev_` pair naming the last character stepped over.
fn scan_for_column(h: &ElispHost, end: usize, goal: i64) -> (usize, i64, usize, i64) {
    let tab_width = sane_tab_width(h);
    let buf = h.cur_buf_ref();
    let mut pos = find_newline(&buf.text, buf.point, -1, buf.begv, buf.zv).0;
    let (mut col, mut prev_pos, mut prev_col) = (0i64, pos, 0i64);
    while pos < end {
        if col >= goal {
            break;
        }
        let c = buf.text[pos - 1];
        if c == '\n' {
            break;
        }
        prev_pos = pos;
        prev_col = col;
        col = advance_column(c, col, tab_width);
        pos += 1;
    }
    (pos, col, prev_pos, prev_col)
}
/// `(current-column)`: the display column of point.
fn current_column(h: &mut ElispHost, _a: &[Value]) -> R {
    let point = h.cur_buf_ref().point;
    Ok(Value::Int(scan_for_column(h, point, i64::MAX).1))
}
/// Port of `Fcurrent_indentation` / `position_indentation` (indent.c): the
/// column of the first character on point's line that is not a space or tab.
fn current_indentation(h: &mut ElispHost, _a: &[Value]) -> R {
    let tab_width = sane_tab_width(h);
    let buf = h.cur_buf_ref();
    let mut pos = find_newline(&buf.text, buf.point, -1, buf.begv, buf.zv).0;
    let mut col = 0i64;
    while pos < buf.zv && matches!(buf.text[pos - 1], ' ' | '\t') {
        col = advance_column(buf.text[pos - 1], col, tab_width);
        pos += 1;
    }
    Ok(Value::Int(col))
}
/// Port of `Findent_to` (indent.c): pad from point's column to COLUMN (and at
/// least MINIMUM columns), with tabs where `indent-tabs-mode` allows, and
/// answer the column reached.
fn indent_to(h: &mut ElispHost, a: &[Value]) -> R {
    let column = as_fixnum_named(h, &a[0], "fixnump")?;
    let minimum = match a.get(1) {
        Some(v) if !is_nil(v) => as_fixnum_named(h, v, "fixnump")?,
        _ => 0,
    };
    let tab_width = sane_tab_width(h);
    let point = h.cur_buf_ref().point;
    let mut fromcol = scan_for_column(h, point, i64::MAX).1;
    let mincol = (fromcol + minimum).max(column);
    if fromcol == mincol {
        return Ok(Value::Int(mincol));
    }
    h.barf_if_read_only()?;
    let tabs_mode = h
        .find_symbol("indent-tabs-mode")
        .and_then(|s| h.get_value(&s).ok())
        .is_some_and(|v| !is_nil(&v));
    if tabs_mode {
        let n = mincol / tab_width - fromcol / tab_width;
        if n != 0 {
            h.cur_insert(vec!['\t'; n.max(0) as usize], true);
            fromcol = (mincol / tab_width) * tab_width;
        }
    }
    h.cur_insert(vec![' '; (mincol - fromcol).max(0) as usize], true);
    Ok(Value::Int(mincol))
}
/// Port of `Fmove_to_column` (indent.c): move to COLUMN on the current line,
/// or as close as the line allows. FORCE non-nil splits a tab that straddles
/// COLUMN into spaces; FORCE `t` also pads a line too short to reach it.
fn move_to_column(h: &mut ElispHost, a: &[Value]) -> R {
    let goal = match &a[0] {
        Value::Int(n) if *n >= 0 => *n,
        other => return Err(h.signal_wrong_type("wholenump", other)),
    };
    let force = a.get(1).cloned().unwrap_or(Value::Undef);
    let zv = h.cur_buf_ref().zv;
    let (pos, mut col, prev_pos, prev_col) = scan_for_column(h, zv, goal);
    h.cur_buf().point = pos;
    if !is_nil(&force) && col > goal {
        let c = h.cur_buf_ref().text[prev_pos - 1];
        if c == '\t' && prev_col < goal && prev_pos < pos {
            h.cur_buf().point = prev_pos;
            h.cur_insert(vec![' '; (goal - prev_col) as usize], true);
            let at = h.cur_buf_ref().point;
            h.cur_delete(at, at + 1);
            indent_to(h, &[Value::Int(col)])?;
            h.cur_buf().point = at;
            col = goal;
        }
    }
    if col < goal && matches!(force, Value::Bool(true)) {
        col = goal;
        indent_to(h, &[Value::Int(col)])?;
    }
    Ok(Value::Int(col))
}
/// One member of a `skip-chars-forward` set: an inclusive character range, or
/// an ISO C class (`[:alpha:]`), tested through the regexp engine's class
/// translation so the two agree character for character.
enum SkipItem {
    Range(char, char),
    Class(Rc<CompiledRe>),
}
/// A parsed `skip-chars-forward`/`-backward` set (syntax.c `skip_chars`).
struct SkipSet {
    negate: bool,
    items: Vec<SkipItem>,
}
impl SkipSet {
    /// syntax.c `skip_chars`'s parse: a leading `^` negates; `[:NAME:]` is a
    /// character class (an unknown NAME is an error, a malformed one is plain
    /// characters); `\` quotes the next character; `A-B` is a range only when a
    /// character follows the `-`, and an inverted range is empty.
    fn parse(h: &ElispHost, spec: &str) -> Result<SkipSet, String> {
        let chars: Vec<char> = spec.chars().collect();
        let negate = chars.first() == Some(&'^');
        let mut i = usize::from(negate);
        let mut items = Vec::new();
        while i < chars.len() {
            let mut c = chars[i];
            i += 1;
            if c == '[' && chars.get(i) == Some(&':') {
                let beg = i + 1;
                let mut end = beg;
                while end + 1 < chars.len()
                    && !(chars[end] as u32 >= 0o200
                        || chars[end] as u32 <= 0o40
                        || (chars[end] == ':' && chars[end + 1] == ']'))
                {
                    end += 1;
                }
                if end > beg && chars.get(end) == Some(&':') && chars.get(end + 1) == Some(&']') {
                    let name: String = chars[beg..end].iter().collect();
                    if !crate::regexp::is_class_name(&name) {
                        return Err("error: Invalid ISO C character class".to_string());
                    }
                    items.push(SkipItem::Class(compile_cf(
                        h,
                        &format!("[[:{name}:]]"),
                        false,
                    )?));
                    i = end + 2;
                    continue;
                }
            }
            if c == '\\' {
                match chars.get(i) {
                    Some(&q) => {
                        c = q;
                        i += 1;
                    }
                    None => break,
                }
            }
            if i + 1 < chars.len() && chars[i] == '-' {
                let mut c2 = chars[i + 1];
                i += 2;
                if c2 == '\\' && i < chars.len() {
                    c2 = chars[i];
                    i += 1;
                }
                if c <= c2 {
                    items.push(SkipItem::Range(c, c2));
                }
            } else {
                items.push(SkipItem::Range(c, c));
            }
        }
        Ok(SkipSet { negate, items })
    }
    fn contains(&self, c: char) -> bool {
        let hit = self.items.iter().any(|item| match item {
            SkipItem::Range(a, b) => (*a..=*b).contains(&c),
            SkipItem::Class(re) => re.is_match(c.encode_utf8(&mut [0; 4])).unwrap_or(false),
        });
        hit != self.negate
    }
}
/// syntax.c `skip_chars`: LIM nil is ZV (forward) / BEGV (backward), and any
/// LIM is clamped to the accessible portion — so a narrowing bounds the scan
/// and point already past LIM does not move. Returns the distance moved.
fn skip_chars(h: &mut ElispHost, a: &[Value], forward: bool) -> R {
    let set = SkipSet::parse(h, &as_string(h, &a[0])?)?;
    let lim = match a.get(1) {
        Some(v) if !is_nil(v) => Some(as_int_or_marker(h, v, "integer-or-marker-p")?),
        _ => None,
    };
    let buf = h.cur_buf();
    let (begv, zv) = (buf.begv as i64, buf.zv as i64);
    let lim = lim
        .unwrap_or(if forward { zv } else { begv })
        .clamp(begv, zv) as usize;
    let start = buf.point;
    if forward {
        while buf.point < lim && set.contains(buf.text[buf.point - 1]) {
            buf.point += 1;
        }
    } else {
        while buf.point > lim && set.contains(buf.text[buf.point - 2]) {
            buf.point -= 1;
        }
    }
    Ok(Value::Int(buf.point as i64 - start as i64))
}
fn skip_chars_forward(h: &mut ElispHost, a: &[Value]) -> R {
    skip_chars(h, a, true)
}
fn skip_chars_backward(h: &mut ElispHost, a: &[Value]) -> R {
    skip_chars(h, a, false)
}
/// Port of `scan_words` (syntax.c): the position COUNT words from FROM, or
/// `None` when the accessible region ends first.
///
/// A word is a run of word-syntax characters in the buffer's syntax table
/// (plus escape / char-quote characters under `words-include-escapes`).
/// Not ported: `word_boundary_p`'s script and category split, so a run that
/// changes script mid-word (`abcαβγ`) is one word here and two in Emacs; and
/// `find-word-boundary-function-table`.
fn scan_words(h: &ElispHost, from: usize, mut count: i64) -> Option<usize> {
    let escapes = h
        .find_symbol("words-include-escapes")
        .and_then(|s| h.get_value(&s).ok())
        .is_some_and(|v| !is_nil(&v));
    let buf = h.cur_buf_ref();
    let is_word = |c: char| {
        let code = h.syntax_class_of(c);
        code == 'w' || (escapes && (code == '\\' || code == '/'))
    };
    let mut from = from;
    while count > 0 {
        loop {
            if from == buf.zv {
                return None;
            }
            from += 1;
            if is_word(buf.text[from - 2]) {
                break;
            }
        }
        while from < buf.zv && is_word(buf.text[from - 1]) {
            from += 1;
        }
        count -= 1;
    }
    while count < 0 {
        loop {
            if from == buf.begv {
                return None;
            }
            from -= 1;
            if is_word(buf.text[from - 1]) {
                break;
            }
        }
        while from > buf.begv && is_word(buf.text[from - 2]) {
            from -= 1;
        }
        count += 1;
    }
    Some(from)
}
/// Port of `Fforward_word` (syntax.c): move over ARG words; on running out of
/// buffer, stop at the limit and answer nil.
fn forward_word(h: &mut ElispHost, a: &[Value]) -> R {
    let n = match a.first() {
        Some(v) if !is_nil(v) => as_fixnum_named(h, v, "fixnump")?,
        _ => 1,
    };
    let found = scan_words(h, h.cur_buf_ref().point, n);
    let buf = h.cur_buf();
    buf.point = found.unwrap_or(if n > 0 { buf.zv } else { buf.begv });
    Ok(nil_or(found.is_some()))
}
/// `backward-word` (simple.el): `(forward-word (- (or arg 1)))`.
fn backward_word(h: &mut ElispHost, a: &[Value]) -> R {
    let arg = match a.first() {
        Some(v) if !is_nil(v) => v.clone(),
        _ => Value::Int(1),
    };
    let neg = match arg {
        Value::Int(n) => Value::Int(-n),
        Value::Float(f) => Value::Float(-f),
        other => {
            return Err(format!(
                "wrong-type-argument: number-or-marker-p {}",
                h.print(&other, true)
            ))
        }
    };
    forward_word(h, &[neg])
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
    // fns.c list search. These were prelude `defun`s, which made `subrp` nil where
    // Emacs says t, named the closure in a wrong-arity signal, and — because a
    // `while (consp l)` loop has no cycle check — did not terminate on a circular
    // list at all.
    s("memq", 2, Some(2), memq_fn);
    s("memql", 2, Some(2), memql_fn);
    s("member", 2, Some(2), member_fn);
    s("assq", 2, Some(2), assq_fn);
    s("rassq", 2, Some(2), rassq_fn);
    s("rassoc", 2, Some(2), rassoc_fn);
    s("null", 1, Some(1), null_fn);
    s("consp", 1, Some(1), consp);
    s("listp", 1, Some(1), listp);
    s("atom", 1, Some(1), atom);
    s("symbolp", 1, Some(1), symbolp);
    s("stringp", 1, Some(1), stringp);
    s("natnump", 1, Some(1), natnump_fn);
    s("nlistp", 1, Some(1), nlistp_fn);
    s("numberp", 1, Some(1), numberp);
    s("integerp", 1, Some(1), integerp);
    s("max", 1, None, max_fn);
    s("min", 1, None, min_fn);
    s("fixnump", 1, Some(1), fixnump);
    s("bignump", 1, Some(1), bignump);
    s("floatp", 1, Some(1), floatp);
    s("vectorp", 1, Some(1), vectorp);
    // NOTE: `zerop` is deliberately NOT a subr: in Emacs it is a byte-compiled
    // defsubst from subr.el ((subrp (symbol-function 'zerop)) => nil) whose
    // arity error is (wrong-number-of-arguments (1 . 1) N), so it is defined in
    // the prelude. Registering it here would also lose the prelude's shadowing
    // definition on a bytecode-cache HIT, since builtin-range symbols' function
    // cells are not part of the exported heap image.
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
    s("nthcdr", 2, Some(2), nthcdr_fn);
    // c[ad]+r compositions, two to four letters, plus the cl-lib prefixed
    // two-letter names (the prelude aliases the three- and four-letter ones).
    for (name, f) in CXR_SUBRS {
        s(name, 1, Some(1), *f);
    }
    s("cl-caar", 1, Some(1), caar);
    s("cl-cadr", 1, Some(1), cadr);
    s("cl-cdar", 1, Some(1), cdar);
    s("cl-cddr", 1, Some(1), cddr);
    // vectors
    s("vector", 0, None, vector_fn);
    s("make-vector", 2, Some(2), make_vector);
    s("make-list", 2, Some(2), make_list);
    s("record", 1, None, record_fn);
    s("make-record", 3, Some(3), make_record);
    s("make-bool-vector", 2, Some(2), make_bool_vector);
    s("bool-vector", 0, None, bool_vector_fn);
    s("bool-vector-p", 1, Some(1), bool_vector_p);
    s(
        "bool-vector-count-population",
        1,
        Some(1),
        bool_vector_count_population,
    );
    s("bool-vector-subsetp", 2, Some(2), bool_vector_subsetp);
    s("bool-vector-not", 1, Some(2), bool_vector_not);
    s(
        "bool-vector-exclusive-or",
        2,
        Some(3),
        bool_vector_exclusive_or,
    );
    s("bool-vector-union", 2, Some(3), bool_vector_union);
    s(
        "bool-vector-intersection",
        2,
        Some(3),
        bool_vector_intersection,
    );
    s(
        "bool-vector-set-difference",
        2,
        Some(3),
        bool_vector_set_difference,
    );
    s(
        "bool-vector-count-consecutive",
        3,
        Some(3),
        bool_vector_count_consecutive,
    );
    s("elt", 2, Some(2), elt_fn);
    s("aref", 2, Some(2), aref);
    s("aset", 3, Some(3), aset);
    // overlays
    s("make-overlay", 2, Some(5), make_overlay);
    s("overlayp", 1, Some(1), overlayp);
    s("overlay-start", 1, Some(1), overlay_start);
    s("overlay-end", 1, Some(1), overlay_end);
    s("overlay-buffer", 1, Some(1), overlay_buffer);
    s("overlay-get", 2, Some(2), overlay_get);
    s("overlay-put", 3, Some(3), overlay_put);
    s("overlay-properties", 1, Some(1), overlay_properties);
    s("delete-overlay", 1, Some(1), delete_overlay);
    s("move-overlay", 3, Some(4), move_overlay);
    s("overlays-at", 1, Some(2), overlays_at);
    s("overlays-in", 2, Some(2), overlays_in);
    s("next-overlay-change", 1, Some(1), next_overlay_change);
    s(
        "previous-overlay-change",
        1,
        Some(1),
        previous_overlay_change,
    );
    s("--note-compiler-macro", 1, Some(1), note_compiler_macro);
    s("store-substring", 3, Some(3), store_substring);
    s("clear-string", 1, Some(1), clear_string);
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
    s("obarray-make", 0, Some(1), obarray_make_fn);
    s("obarrayp", 1, Some(1), obarrayp_fn);
    s("unintern", 1, Some(2), unintern_fn);
    s("make-symbol", 1, Some(1), make_symbol_fn);
    s("set", 2, Some(2), set_fn);
    s("symbol-value", 1, Some(1), symbol_value);
    s("keywordp", 1, Some(1), keywordp);
    s("boundp", 1, Some(1), boundp);
    s("default-boundp", 1, Some(1), default_boundp);
    s("default-toplevel-value", 1, Some(1), default_toplevel_value);
    s("bare-symbol-p", 1, Some(1), bare_symbol_p);
    s("makunbound", 1, Some(1), makunbound);
    s("defvaralias", 2, Some(3), defvaralias);
    s("indirect-variable", 1, Some(1), indirect_variable);
    s("sha1", 1, Some(4), sha1_fn);
    s("md5", 1, Some(5), md5_fn);
    s("secure-hash", 2, Some(5), secure_hash);
    s("secure-hash-algorithms", 0, Some(0), secure_hash_algorithms);
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
    s("fmakunbound", 1, Some(1), fmakunbound);
    // json.c
    s("json-parse-string", 1, None, crate::json::json_parse_string);
    s("json-parse-buffer", 0, None, crate::json::json_parse_buffer);
    s("json-serialize", 1, None, crate::json::json_serialize);
    s("json-insert", 1, None, crate::json::json_insert);
    s(
        "json-available-p",
        0,
        Some(0),
        crate::json::json_available_p,
    );
    s("indirect-function", 1, Some(2), indirect_function);
    // The higher-order primitives run in `host::call_function`, which intercepts
    // them by name *before* any function-cell lookup so they never execute
    // inside a host borrow. That left them with no function cell at all, so
    // `(fboundp 'mapcar)`, `(functionp 'eval)`, `(func-arity 'funcall)`,
    // `(indirect-function 'apply)` and `(symbol-function 'load)` all answered as
    // though the name were undefined, where Emacs reports `#<subr NAME>`.
    // Registering the cell here restores every one of those answers; the bodies
    // below are unreachable, because `call_function` matches the name first —
    // for a bare symbol and, since this change, for the subr object too, so
    // `(funcall (symbol-function 'mapcar) …)` routes to the intercept as well.
    // `intercepted_subr` is what runs if that ever stops being true, and it says
    // so rather than answering wrongly.
    //
    // NAMED boundary: `macroexpand-1` and `macroexpand-all` are byte-compiled
    // Lisp in Emacs (macroexp.el), not subrs, so `(subrp (symbol-function
    // 'macroexpand-1))` is nil there and t here. The other four observables
    // agree; they are native here, and there is no Lisp definition to point at.
    s("funcall", 1, None, intercepted_subr);
    s("apply", 1, None, intercepted_subr);
    s("mapcar", 2, Some(2), intercepted_subr);
    s("mapc", 2, Some(2), intercepted_subr);
    // `assoc` calls TESTFN and `mapconcat` calls FUNCTION, so both re-enter elisp
    // and both are driven from `host::call_function`. Registering the cell keeps
    // `subrp` / `subr-name` / `func-arity` / `symbol-function` answering as Emacs
    // does for a C subr in fns.c.
    s("assoc", 2, Some(3), intercepted_subr);
    s("mapconcat", 2, Some(3), intercepted_subr);
    s("maphash", 2, Some(2), intercepted_subr);
    s("mapatoms", 1, Some(2), intercepted_subr);
    s("load", 1, Some(5), intercepted_subr);
    s("eval", 1, Some(2), intercepted_subr);
    s("macroexpand", 1, Some(2), intercepted_subr);
    s("macroexpand-1", 1, Some(2), intercepted_subr);
    s("macroexpand-all", 1, Some(2), intercepted_subr);
    s("sort", 1, None, intercepted_subr);
    s("identity", 1, Some(1), identity);
    s("terpri", 0, Some(2), terpri);
    s("write-char", 1, Some(2), write_char_fn);
    s("backquote-process", 1, Some(2), backquote_process_fn);
    s("print", 1, Some(2), print_fn);
    s("prin1-to-string", 1, Some(3), prin1_to_string);
    // nonlocal exits (catch/unwind-protect/condition-case are compiler intrinsics)
    s("throw", 2, Some(2), throw_fn);
    s("error", 1, None, error_fn);
    s("user-error", 1, None, user_error_fn);
    s("signal", 2, Some(2), signal_fn);
    // hash tables (maphash is intercepted in host::call_function)
    // Intercepted: resolving a `define-hash-table-test` name reads an elisp
    // plist, which cannot happen inside a host borrow.
    s("make-hash-table", 0, None, intercepted_subr);
    // Intercepted: a `define-hash-table-test` table's test and hash functions
    // are elisp, so a lookup on one has to run outside the host borrow.
    s("gethash", 2, Some(3), intercepted_subr);
    s("puthash", 3, Some(3), intercepted_subr);
    s("remhash", 2, Some(2), intercepted_subr);
    s("clrhash", 1, Some(1), clrhash);
    s("hash-table-count", 1, Some(1), hash_table_count);
    s("hash-table-test", 1, Some(1), hash_table_test);
    s("hash-table-size", 1, Some(1), hash_table_size);
    s("hash-table-weakness", 1, Some(1), hash_table_weakness);
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
    s("--system-type--", 0, Some(0), system_type);
    s("--build-time--", 0, Some(0), build_time);
    s("system-name", 0, Some(0), system_name);
    s("--temp-directory--", 0, Some(0), temp_directory);
    s("file-exists-p", 1, Some(1), file_exists_p);
    s("file-directory-p", 1, Some(1), file_directory_p);
    s("file-regular-p", 1, Some(1), file_regular_p);
    s("file-readable-p", 1, Some(1), file_readable_p);
    s("file-writable-p", 1, Some(1), file_writable_p);
    s("file-symlink-p", 1, Some(1), file_symlink_p);
    s("file-executable-p", 1, Some(1), file_executable_p);
    s("--invocation-file--", 0, Some(0), invocation_file);
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
    s(
        "insert-buffer-substring",
        1,
        Some(3),
        insert_buffer_substring,
    );
    s("point-min-marker", 0, Some(0), point_min_marker);
    s("point-max-marker", 0, Some(0), point_max_marker);
    s(
        "delete-and-extract-region",
        2,
        Some(2),
        delete_and_extract_region,
    );
    s("buffer-string", 0, Some(0), buffer_string);
    s("buffer-size", 0, Some(1), buffer_size);
    s("point", 0, Some(0), point_fn);
    s("point-min", 0, Some(0), point_min);
    s("point-max", 0, Some(0), point_max);
    s("goto-char", 1, Some(1), goto_char);
    s("erase-buffer", 0, Some(0), erase_buffer);
    s("buffer-modified-p", 0, Some(1), buffer_modified_p);
    s("position-bytes", 1, Some(1), position_bytes);
    s("byte-to-position", 1, Some(1), byte_to_position);
    s(
        "restore-buffer-modified-p",
        1,
        Some(1),
        restore_buffer_modified_p,
    );
    s("set-buffer-modified-p", 1, Some(1), set_buffer_modified_p);
    s("buffer-modified-tick", 0, Some(1), buffer_modified_tick);
    s(
        "buffer-chars-modified-tick",
        0,
        Some(1),
        buffer_chars_modified_tick,
    );
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
        "elisprs--replace-chars-in-place",
        3,
        Some(3),
        replace_chars_in_place_fn,
    );
    s(
        "elisprs--carry-text-properties",
        2,
        Some(2),
        carry_text_properties_fn,
    );
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
    s("count-lines", 2, Some(3), count_lines);
    s("line-number-at-pos", 0, Some(2), line_number_at_pos);
    s("current-column", 0, Some(0), current_column);
    s("current-indentation", 0, Some(0), current_indentation);
    s("indent-to", 1, Some(2), indent_to);
    s("move-to-column", 1, Some(2), move_to_column);
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
    s("time--decode", 1, Some(1), crate::timefns::time_decode_fn);
    // strings
    s("substring", 1, Some(3), substring);
    s(
        "substring-no-properties",
        1,
        Some(3),
        substring_no_properties_fn,
    );
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
    s("match-data--translate", 1, Some(1), match_data_translate);
    s("regexp-quote", 1, Some(1), regexp_quote);
    // `replace-regexp-in-string` is Lisp in Emacs (subr.el) and Lisp here — see
    // the prelude, which mirrors that definition.
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
    s("--char-titlecase--", 1, Some(1), char_titlecase);
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
    s("char-width", 1, Some(1), char_width_fn);
    s("vconcat", 0, None, vconcat_fn);
    s("string-to-vector", 1, Some(1), string_to_vector);
    s("abs", 1, Some(1), abs_fn);
    s("logcount", 1, Some(1), logcount_fn);
    s("symbol-function", 1, Some(1), symbol_function);
    s(
        "--set-intrinsic-macro-cell",
        2,
        Some(2),
        set_intrinsic_macro_cell,
    );
    s("intern-soft", 1, Some(2), intern_soft);
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
    s("decode-char", 2, Some(2), decode_char);
    s("emacs-pid", 0, Some(0), emacs_pid);
    s("load-average", 0, Some(1), load_average);
    s("read", 0, Some(1), read_fn);
    s("read-from-string", 1, Some(3), read_from_string);
    s("compare-strings", 6, Some(7), compare_strings);
    // Emacs 28 alias for split-string with identical semantics (direct forwarder).
    s("string-split", 1, Some(4), split_string);
    s("member-ignore-case", 2, Some(2), member_ignore_case);
    // `not` is `(defalias 'not #'null)` in Emacs (subr.el), not a subr of its
    // own: `(symbol-function 'not)` is the symbol `null`, and an arity error
    // through `#'not` names `#<subr null>`. Alias the symbol rather than
    // registering a second subr so both render exactly that way.
    let null_sym = h.intern("null");
    h.set_function("not", null_sym);
    install_special_form_cells(h);
    // AOP pattern-intercept layer (elisprs extension, ported from zshrs). Registers
    // the `intercept*` subrs and marks its context variables special.
    crate::intercepts::install(h);
}

/// Whether NAME is one of Emacs's special forms.
pub(crate) fn is_special_form(name: &str) -> bool {
    SPECIAL_FORMS.iter().any(|(sf, _)| *sf == name)
}

/// The body of the subr that stands for a special form.
///
/// Unreachable: evaluating `if`/`let`/`progn` goes through the compiler, and
/// calling one goes through `host::call_function`, which refuses it exactly as
/// Emacs's `funcall` refuses a subr whose `max_args` is `UNEVALLED`. Getting here
/// means a call site bypassed that check.
fn special_form_body(_h: &mut ElispHost, _a: &[Value]) -> R {
    Err(
        "internal: a special form's subr body was called; host::call_function \
         should have signalled invalid-function"
            .to_string(),
    )
}

/// Give every special form the `#<subr NAME>` function cell Emacs gives it.
///
/// In Emacs a special form is an ordinary subr whose `max_args` is `UNEVALLED`,
/// so it answers `fboundp`, `symbol-function`, `indirect-function`, `subrp` and
/// `subr-name` exactly like `car` does — `(fboundp 'if)` is `t` and
/// `(symbol-function 'if)` prints `#<subr if>`. elisprs lowers these forms in the
/// compiler and so had no function cell at all for them, which made all five
/// answer as though `if` were undefined.
///
/// The cell goes in the introspection side table rather than the symbol's real
/// function cell: `resolve_function` must keep failing for it, because
/// `(functionp 'if)` is nil in Emacs and `(funcall 'if …)` is an error, not a
/// call.
fn install_special_form_cells(h: &mut ElispHost) {
    for (name, min) in SPECIAL_FORMS {
        let subr = h.alloc(Obj::Subr {
            name: (*name).to_string(),
            min: *min as usize,
            max: None,
            f: special_form_body,
        });
        let sym = h.intern(name);
        h.set_intrinsic_macro_cell(&sym, subr);
    }
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
        // An empty separator regexp matches at every position, INCLUDING before
        // the first character and after the last, so Emacs 30.2 answers
        // ("" "a" "b" "c" "") — verified against `emacs -Q --batch`. This
        // assertion previously encoded elisprs's own (leading/trailing-less)
        // output rather than Emacs's.
        assert_eq!(
            eval("(string-split \"abc\" \"\")"),
            "(\"\" \"a\" \"b\" \"c\" \"\")"
        );
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
