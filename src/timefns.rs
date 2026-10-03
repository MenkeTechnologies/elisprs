//! Lisp timestamps, ported from timefns.c (GNU Emacs 31.1).
//!
//! Every time value Emacs accepts decodes to an exact `(TICKS . HZ)` rational:
//! an integer is `(N . 1)`, a float is its exact binary value over a power of
//! two, `(HI LO US PS)` is counted at the resolution its length implies, and
//! nil is the current time in nanoseconds. Converting to a double then rounds
//! once, correctly, the way `frac_to_double` does — so `float-time` of a
//! picosecond list or of a bignum tick count matches Emacs to the last bit.
//!
//! The arithmetic built on this (`time-add`, `time-subtract`, `time-less-p`,
//! `time-convert`) lives in the prelude and reaches it through `time--decode`.

use crate::host::{el_truthy, ElispHost, Obj};
use fusevm::Value;
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{Signed, ToPrimitive, Zero};

const TRILLION: i64 = 1_000_000_000_000;
const MILLION: i64 = 1_000_000;
/// `DBL_MANT_DIG`.
const MANT_DIG: i64 = 53;
/// `flt_radix_power_size - 1`: the largest scale a double ever needs.
const MAX_SCALE: i64 = 1074;

/// An exact timestamp: `ticks / hz` seconds, `hz` positive.
pub(crate) struct TicksHz {
    pub ticks: BigInt,
    pub hz: BigInt,
}

/// `time_spec_invalid`.
pub(crate) fn spec_invalid() -> String {
    "error: Invalid time specification".to_string()
}

/// `time_overflow`.
pub(crate) fn overflow() -> String {
    "error: Specified time is not representable".to_string()
}

/// The current time as `(TICKS . 10^9)` — `current_time_in_cform`.
fn now() -> TicksHz {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i128)
        .unwrap_or(0);
    TicksHz {
        ticks: BigInt::from(ns),
        hz: BigInt::from(1_000_000_000),
    }
}

/// `decode_lisp_time`. With SECS_ONLY (`CFORM_SECS_ONLY`) a list's
/// sub-second elements are neither read nor validated.
pub(crate) fn decode(h: &ElispHost, v: Option<&Value>, secs_only: bool) -> Result<TicksHz, String> {
    let v = match v {
        Some(v) if el_truthy(v) => v,
        _ => return Ok(now()),
    };
    if let Value::Float(f) = v {
        return decode_float(*f);
    }
    if let Some(n) = h.as_bigint(v) {
        return Ok(TicksHz {
            ticks: n,
            hz: BigInt::from(1),
        });
    }
    let Some(Obj::Cons(high, low)) = h.obj(v) else {
        return Err(spec_invalid());
    };
    let (high, low) = (high.clone(), low.clone());
    let Some(Obj::Cons(lo, tail)) = h.obj(&low) else {
        // (TICKS . HZ)
        let ticks = h.as_bigint(&high).ok_or_else(spec_invalid)?;
        let hz = h
            .as_bigint(&low)
            .filter(|z| z.is_positive())
            .ok_or_else(spec_invalid)?;
        return Ok(TicksHz { ticks, hz });
    };
    let (lo, tail) = (lo.clone(), tail.clone());
    let (mut us, mut ps, mut hz) = (Value::Int(0), Value::Int(0), 1);
    if !secs_only {
        match h.obj(&tail) {
            Some(Obj::Cons(u, rest)) => {
                us = u.clone();
                match h.obj(rest) {
                    Some(Obj::Cons(p, _)) => {
                        ps = p.clone();
                        hz = TRILLION;
                    }
                    _ => hz = MILLION,
                }
            }
            _ if el_truthy(&tail) => {
                us = tail.clone();
                hz = MILLION;
            }
            _ => {}
        }
    }
    decode_components(h, &high, &lo, &us, &ps, hz)
}

/// `decode_time_components`: carry out-of-range microseconds and picoseconds
/// into the seconds, then count at HZ (1, 10^6 or 10^12).
fn decode_components(
    h: &ElispHost,
    high: &Value,
    low: &Value,
    us: &Value,
    ps: &Value,
    hz: i64,
) -> Result<TicksHz, String> {
    let (Value::Int(us), Value::Int(ps)) = (us, ps) else {
        return Err(spec_invalid());
    };
    let (us, ps) = (i128::from(*us), i128::from(*ps));
    let us = us + ps.div_euclid(MILLION as i128);
    let s_from_us_ps = us.div_euclid(MILLION as i128);
    let ps = ps.rem_euclid(MILLION as i128);
    let us = us.rem_euclid(MILLION as i128);
    let high = h.as_bigint(high).ok_or_else(spec_invalid)?;
    let low = h.as_bigint(low).ok_or_else(spec_invalid)?;
    let s = high * 65536 + low + BigInt::from(s_from_us_ps);
    let ticks = match hz {
        TRILLION => s * TRILLION + BigInt::from(us * MILLION as i128 + ps),
        MILLION => s * MILLION + BigInt::from(us),
        _ => s,
    };
    Ok(TicksHz {
        ticks,
        hz: BigInt::from(hz),
    })
}

/// `decode_float_time`: F exactly, over `2^scale` where scale is
/// `double_integer_scale` (`DBL_MANT_DIG - 1 - ilogb (F)`) floored at 0.
fn decode_float(f: f64) -> Result<TicksHz, String> {
    if f.is_nan() {
        return Err(spec_invalid());
    }
    if f.is_infinite() {
        return Err(overflow());
    }
    if f == 0.0 {
        return Ok(TicksHz {
            ticks: BigInt::zero(),
            hz: BigInt::from(1),
        });
    }
    // F = mant * 2^exp exactly.
    let bits = f.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, exp) = if exp_bits == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp_bits - 1075)
    };
    let ilogb = exp + 63 - i64::from(mant.leading_zeros());
    let scale = (MANT_DIG - 1 - ilogb).max(0);
    // mant * 2^(exp + scale) is an integer: exp + scale >= 0 by construction.
    let mut ticks = BigInt::from(mant) << ((exp + scale) as usize);
    if f < 0.0 {
        ticks = -ticks;
    }
    Ok(TicksHz {
        ticks,
        hz: BigInt::from(1) << scale as usize,
    })
}

/// `mpz_sizeinbase (|z|, 2)`: 1 for zero.
fn bit_size(z: &BigInt) -> i64 {
    (z.bits() as i64).max(1)
}

/// `frac_to_double`: N / D rounded once to the nearest double, ties to even.
pub(crate) fn frac_to_double(n: &BigInt, d: &BigInt) -> f64 {
    if let (Some(ni), Some(di)) = (n.to_i64(), d.to_i64()) {
        if ni % di == 0 {
            return (ni / di) as f64;
        }
    }
    let mut scale = bit_size(d) - bit_size(n) + MANT_DIG;
    let (n, d) = if scale < 0 {
        (n.clone(), d << ((-scale) as usize))
    } else {
        scale = scale.min(MAX_SCALE);
        (n << (scale as usize), d.clone())
    };
    let (q, r) = n.div_rem(&d);
    let incr: u32 = if bit_size(&q) <= MANT_DIG {
        let twice_r: BigInt = r.abs() * 2;
        match twice_r.cmp(&d.abs()) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Equal if q.is_odd() => 1,
            _ => 0,
        }
    } else {
        let lo_2digits = (q.abs() % 4u32).to_u32().unwrap_or(0);
        if lo_2digits % 2 == 1 && ((lo_2digits / 2) & 1 == 1 || !r.is_zero()) {
            2
        } else {
            0
        }
    };
    let q = if n.is_negative() { q - incr } else { q + incr };
    // `mpz_get_d` truncates toward zero; `scalbn` is then exact.
    let mag = q.abs();
    let drop = (bit_size(&mag) - MANT_DIG).max(0);
    let top = (mag >> drop as usize).to_f64().unwrap_or(0.0);
    let x = crate::builtins::scalbn(top, drop - scale);
    if q.is_negative() {
        -x
    } else {
        x
    }
}

/// TIME as a double (`float_time`); a float is returned as is.
pub(crate) fn float_seconds(h: &ElispHost, v: Option<&Value>) -> Result<f64, String> {
    if let Some(Value::Float(f)) = v {
        return Ok(*f);
    }
    let t = decode(h, v, false)?;
    Ok(frac_to_double(&t.ticks, &t.hz))
}

/// `floor (TICKS / HZ)` as a `time_t`, or `time_overflow`.
pub(crate) fn floor_seconds(t: &TicksHz) -> Result<i64, String> {
    t.ticks.div_floor(&t.hz).to_i64().ok_or_else(overflow)
}

/// `lisp_seconds_argument`.
pub(crate) fn seconds_argument(h: &ElispHost, v: Option<&Value>) -> Result<i64, String> {
    floor_seconds(&decode(h, v, true)?)
}

/// `(time--decode TIME)`: TIME as the exact `(TICKS . HZ)` pair — the
/// prelude's time arithmetic is built on this.
pub(crate) fn time_decode_fn(h: &mut ElispHost, a: &[Value]) -> Result<Value, String> {
    let t = decode(h, a.first(), false)?;
    let ticks = h.make_integer(t.ticks);
    let hz = h.make_integer(t.hz);
    Ok(h.cons(ticks, hz))
}
