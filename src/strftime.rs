//! `nstrftime`: the time formatter behind `format-time-string`, ported from
//! gnulib's `lib/strftime.c` as GNU Emacs 31.1 builds it.
//!
//! The control flow, flag/width/modifier grammar, padding rules and
//! bad-format recovery follow the C one for one. Two parts of it defer to the
//! platform, exactly as Emacs does on a system without `_NL_CURRENT`: the
//! locale-dependent directives (`%a %A %b %B %h %c %x %X %p`) are produced by
//! libc's `strftime`.

use std::ffi::CString;

/// gnulib's `enum pad_style`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pad {
    /// Default: pad with 0 unless the directive says otherwise.
    Zero,
    /// `0` flag.
    AlwaysZero,
    /// `+` flag.
    Sign,
    /// `_` flag.
    Space,
    /// `-` flag.
    No,
}

/// Directive widths past this are refused: the C code would exhaust memory.
const MAX_WIDTH: i64 = 1 << 28;

/// Failure: the result does not fit (`ERANGE`/`EOVERFLOW` in the C).
pub(crate) struct Overflow;

static LOCALE_INIT: std::sync::Once = std::sync::Once::new();

/// Adopt the environment's `LC_TIME`, as Emacs's `main` does with
/// `setlocale (LC_ALL, "")`, before libc's `strftime` is first asked.
fn init_time_locale() {
    LOCALE_INIT.call_once(|| {
        let empty = CString::new("").expect("no NUL");
        unsafe { libc::setlocale(libc::LC_TIME, empty.as_ptr()) };
    });
}

/// `%DIRECTIVE` through libc (`underlying_strftime`), without modifier.
fn underlying(modifier: u8, format_char: u8, tm: &libc::tm) -> Vec<u8> {
    init_time_locale();
    let mut spec = vec![b'%'];
    if modifier != 0 {
        spec.push(modifier);
    }
    spec.push(format_char);
    let fmt = CString::new(spec).expect("no NUL");
    let mut buf = vec![0u8; 1024];
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            fmt.as_ptr(),
            tm,
        )
    };
    buf.truncate(n);
    buf
}

/// `width_add`: pad S to WIDTH (zeros for the zero pad styles, else spaces)
/// and apply the ASCII case conversion of `cpy`.
fn put(out: &mut Vec<u8>, pad: Pad, width: i64, s: &[u8], low: bool, up: bool) {
    let n = s.len() as i64;
    let w = if pad == Pad::No || width < 0 {
        0
    } else {
        width
    };
    if n < w {
        let fill = if matches!(pad, Pad::AlwaysZero | Pad::Sign) {
            b'0'
        } else {
            b' '
        };
        out.extend(std::iter::repeat_n(fill, (w - n) as usize));
    }
    out.extend(s.iter().map(|&b| {
        if low {
            b.to_ascii_lowercase()
        } else if up {
            b.to_ascii_uppercase()
        } else {
            b
        }
    }));
}

/// `iso_week_days`: days from the start of the ISO week 1 of this year.
fn iso_week_days(yday: i32, wday: i32) -> i32 {
    // ISO_WEEK1_WDAY (Thursday) - ISO_WEEK_START_WDAY (Monday) = 3.
    let big_enough_multiple_of_7 = (366 / 7 + 2) * 7;
    yday - (yday - wday + 4 + big_enough_multiple_of_7) % 7 + 3
}

/// What a numeric directive hands to the shared sign-and-padding tail.
struct Number {
    digits: i64,
    negative: bool,
    magnitude: u64,
    always_sign: bool,
    tz_colon_mask: u32,
}

/// The broken-down time every directive reads, plus the exact epoch seconds `%s`
/// prints and the nanoseconds `%N` prints.
#[derive(Clone, Copy)]
struct Moment<'a> {
    tm: &'a libc::tm,
    secs: i64,
    ns: i64,
}

/// Format TM per FORMAT; SECS and NS are the epoch seconds and nanoseconds `%s`
/// and `%N` print.
pub(crate) fn format(
    format: &[u8],
    tm: &libc::tm,
    secs: i64,
    ns: i64,
) -> Result<Vec<u8>, Overflow> {
    let mut out = Vec::new();
    let moment = Moment { tm, secs, ns };
    internal(&mut out, format, moment, false, Pad::Zero, -1)?;
    Ok(out)
}

#[allow(clippy::useless_conversion)]
fn internal(
    out: &mut Vec<u8>,
    format: &[u8],
    moment: Moment,
    upcase: bool,
    yr_spec: Pad,
    width_param: i64,
) -> Result<(), Overflow> {
    let Moment { tm, secs, ns } = moment;
    let hour12 = match tm.tm_hour {
        h if h > 12 => h - 12,
        0 => 12,
        h => h,
    };
    let at = |i: usize| -> u8 { format.get(i).copied().unwrap_or(0) };
    let mut width = width_param;
    let mut f = 0usize;
    while f < format.len() && format[f] != 0 {
        // The `for` increment: width resets for every conversion, also when
        // the body `continue`s.
        let this_width = width;
        width = -1;
        if format[f] != b'%' {
            put(out, Pad::Zero, this_width, &format[f..f + 1], false, false);
            f += 1;
            continue;
        }
        let mut width_now = this_width;
        let percent = f;
        let mut pad = Pad::Zero;
        let mut to_low = false;
        let mut to_up = upcase;
        let mut change_case = false;
        loop {
            f += 1;
            match at(f) {
                b'_' => pad = Pad::Space,
                b'-' => pad = Pad::No,
                b'+' => pad = Pad::Sign,
                b'0' => pad = Pad::AlwaysZero,
                b'^' => to_up = true,
                b'#' => change_case = true,
                _ => break,
            }
        }
        if at(f).is_ascii_digit() {
            let mut w: i64 = 0;
            while at(f).is_ascii_digit() {
                w = w * 10 + i64::from(at(f) - b'0');
                if w > i64::from(i32::MAX) {
                    return Err(Overflow);
                }
                f += 1;
            }
            width_now = w;
        }
        if width_now > MAX_WIDTH {
            return Err(Overflow);
        }
        let modifier = match at(f) {
            m @ (b'E' | b'O') => {
                f += 1;
                m
            }
            _ => 0,
        };

        // Outcome of the directive: bytes to `cpy`, a sub-format, a number,
        // or a bad format.
        enum Act {
            Cpy(Vec<u8>),
            Sub(&'static str, Pad, i64),
            Num(Number),
            /// Raw bytes through `add`-style padding without case change.
            Bad,
            BadPercent,
            Nothing,
        }
        let fc = at(f);
        let ye = |n: i32| -> i64 { i64::from(n) };
        let tm_year = tm.tm_year;
        let act: Act = 'dir: {
            macro_rules! bad_if {
                ($c:expr) => {
                    if $c {
                        break 'dir Act::Bad;
                    }
                };
            }
            macro_rules! number {
                ($digits:expr, $v:expr) => {{
                    let v: i64 = $v;
                    break 'dir Act::Num(Number {
                        digits: $digits,
                        negative: v < 0,
                        magnitude: v.unsigned_abs(),
                        always_sign: false,
                        tz_colon_mask: 0,
                    });
                }};
            }
            match fc {
                b'%' => {
                    if f - 1 != percent {
                        break 'dir Act::BadPercent;
                    }
                    break 'dir Act::Cpy(vec![b'%']);
                }
                b'a' | b'A' | b'b' | b'h' | b'B' => {
                    bad_if!(modifier != 0 && matches!(fc, b'a' | b'A'));
                    bad_if!(modifier == b'E' && matches!(fc, b'b' | b'h' | b'B'));
                    if change_case {
                        to_up = true;
                        to_low = false;
                    }
                    let fcn = if fc == b'h' { b'b' } else { fc };
                    break 'dir Act::Cpy(underlying(0, fcn, tm));
                }
                b'c' => {
                    bad_if!(modifier == b'O');
                    break 'dir Act::Cpy(underlying(modifier, b'c', tm));
                }
                b'x' | b'X' => {
                    bad_if!(modifier == b'O');
                    break 'dir Act::Cpy(underlying(modifier, fc, tm));
                }
                b'C' => {
                    // Without `_NL_CURRENT` (macOS), an `E` modifier hands the
                    // directive to the C library; with it, the era lookup fails
                    // in the C locale and the plain number is printed.
                    if cfg!(target_os = "macos") && modifier == b'E' {
                        break 'dir Act::Cpy(underlying(modifier, fc, tm));
                    }
                    let negative_year = tm_year < -1900;
                    let zero_thru_1899 = !negative_year && tm_year < 0;
                    let century = (tm_year - 99 * i32::from(zero_thru_1899)) / 100 + 1900 / 100;
                    let mut pad2 = pad;
                    if pad2 == Pad::Zero {
                        pad2 = yr_spec;
                    }
                    pad = pad2;
                    break 'dir Act::Num(Number {
                        digits: 2,
                        negative: negative_year,
                        magnitude: ye(century).unsigned_abs(),
                        always_sign: pad == Pad::Sign
                            && (99 < ye(century).unsigned_abs() || 2 < width_now),
                        tz_colon_mask: 0,
                    });
                }
                b'D' => {
                    bad_if!(modifier != 0);
                    break 'dir Act::Sub("%m/%d/%y", pad, -1);
                }
                b'd' => {
                    bad_if!(modifier == b'E');
                    number!(2, ye(tm.tm_mday));
                }
                b'e' | b'k' | b'l' => {
                    bad_if!(modifier == b'E');
                    let v = match fc {
                        b'e' => tm.tm_mday,
                        b'k' => tm.tm_hour,
                        _ => hour12,
                    };
                    if pad == Pad::Zero {
                        pad = Pad::Space;
                    }
                    number!(2, ye(v));
                }
                b'F' => {
                    bad_if!(modifier != 0);
                    if pad == Pad::Zero && width_now < 0 {
                        break 'dir Act::Sub("%Y-%m-%d", Pad::Sign, 4);
                    }
                    break 'dir Act::Sub("%Y-%m-%d", pad, (width_now - 6).max(0));
                }
                b'H' => {
                    bad_if!(modifier == b'E');
                    number!(2, ye(tm.tm_hour));
                }
                b'I' => {
                    bad_if!(modifier == b'E');
                    number!(2, ye(hour12));
                }
                b'j' => {
                    bad_if!(modifier == b'E');
                    break 'dir Act::Num(Number {
                        digits: 3,
                        negative: tm.tm_yday < -1,
                        magnitude: (i64::from(tm.tm_yday) + 1).unsigned_abs(),
                        always_sign: false,
                        tz_colon_mask: 0,
                    });
                }
                b'M' => {
                    bad_if!(modifier == b'E');
                    number!(2, ye(tm.tm_min));
                }
                b'm' => {
                    bad_if!(modifier == b'E');
                    break 'dir Act::Num(Number {
                        digits: 2,
                        negative: tm.tm_mon < -1,
                        magnitude: (i64::from(tm.tm_mon) + 1).unsigned_abs(),
                        always_sign: false,
                        tz_colon_mask: 0,
                    });
                }
                b'N' => {
                    bad_if!(modifier == b'E');
                    let mut n = ns;
                    let ns_digits = 9i64;
                    let mut w = width_now;
                    if w <= 0 {
                        w = ns_digits;
                    }
                    let mut ndigs = ns_digits;
                    while w < ndigs || (1 < ndigs && n % 10 == 0) {
                        ndigs -= 1;
                        n /= 10;
                    }
                    let mut digs = vec![b'0'; ndigs as usize];
                    for j in (0..ndigs as usize).rev() {
                        digs[j] = b'0' + (n % 10) as u8;
                        n /= 10;
                    }
                    if pad == Pad::Zero {
                        pad = Pad::AlwaysZero;
                    }
                    // width_cpy (0, ndigs, buf); width_add (width - ndigs, 0, ...)
                    put(out, pad, 0, &digs, to_low, to_up);
                    put(out, pad, w - ndigs, &[], false, false);
                    break 'dir Act::Nothing;
                }
                b'n' => break 'dir Act::Cpy(vec![b'\n']),
                b't' => break 'dir Act::Cpy(vec![b'\t']),
                b'P' | b'p' => {
                    if fc == b'P' {
                        to_low = true;
                    }
                    if change_case {
                        to_up = false;
                        to_low = true;
                    }
                    break 'dir Act::Cpy(underlying(0, b'p', tm));
                }
                b'q' => {
                    break 'dir Act::Num(Number {
                        digits: 1,
                        negative: false,
                        magnitude: (((tm.tm_mon * 11) >> 5) + 1) as u64,
                        always_sign: false,
                        tz_colon_mask: 0,
                    });
                }
                b'R' => break 'dir Act::Sub("%H:%M", pad, -1),
                b'r' => break 'dir Act::Sub("%I:%M:%S %p", pad, -1),
                b'S' => {
                    bad_if!(modifier == b'E');
                    number!(2, ye(tm.tm_sec));
                }
                b's' => {
                    break 'dir Act::Num(Number {
                        digits: 1,
                        negative: secs < 0,
                        magnitude: secs.unsigned_abs(),
                        always_sign: false,
                        tz_colon_mask: 0,
                    });
                }
                b'T' => break 'dir Act::Sub("%H:%M:%S", pad, -1),
                b'u' => number!(1, ye((tm.tm_wday - 1 + 7) % 7 + 1)),
                b'U' => {
                    bad_if!(modifier == b'E');
                    number!(2, ye((tm.tm_yday - tm.tm_wday + 7) / 7));
                }
                b'V' | b'g' | b'G' => {
                    bad_if!(modifier == b'E');
                    let leap = |y: i32| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
                    let year = tm_year
                        + if tm_year < 0 {
                            1900 % 400
                        } else {
                            1900 % 400 - 400
                        };
                    let mut year_adjust = 0;
                    let mut days = iso_week_days(tm.tm_yday, tm.tm_wday);
                    if days < 0 {
                        year_adjust = -1;
                        days = iso_week_days(
                            tm.tm_yday + (365 + i32::from(leap(year - 1))),
                            tm.tm_wday,
                        );
                    } else {
                        let d =
                            iso_week_days(tm.tm_yday - (365 + i32::from(leap(year))), tm.tm_wday);
                        if d >= 0 {
                            year_adjust = 1;
                            days = d;
                        }
                    }
                    match fc {
                        b'g' => {
                            let yy = (tm_year % 100 + year_adjust) % 100;
                            let v = if yy >= 0 {
                                yy
                            } else if tm_year < -1900 - year_adjust {
                                -yy
                            } else {
                                yy + 100
                            };
                            if pad == Pad::Zero {
                                pad = yr_spec;
                            }
                            break 'dir Act::Num(Number {
                                digits: 2,
                                negative: false,
                                magnitude: ye(v).unsigned_abs(),
                                always_sign: pad == Pad::Sign && (99 < v || 2 < width_now),
                                tz_colon_mask: 0,
                            });
                        }
                        b'G' => {
                            let negative = tm_year < -1900 - year_adjust;
                            let value = ye(tm_year) + 1900 + ye(year_adjust);
                            if pad == Pad::Zero {
                                pad = yr_spec;
                            }
                            break 'dir Act::Num(Number {
                                digits: 4,
                                negative,
                                magnitude: value.unsigned_abs(),
                                always_sign: pad == Pad::Sign
                                    && (9999 < value.unsigned_abs() || 4 < width_now),
                                tz_colon_mask: 0,
                            });
                        }
                        _ => number!(2, ye(days / 7 + 1)),
                    }
                }
                b'W' => {
                    bad_if!(modifier == b'E');
                    number!(2, ye((tm.tm_yday - (tm.tm_wday - 1 + 7) % 7 + 7) / 7));
                }
                b'w' => {
                    bad_if!(modifier == b'E');
                    number!(1, ye(tm.tm_wday));
                }
                b'Y' => {
                    if cfg!(target_os = "macos") && modifier == b'E' {
                        break 'dir Act::Cpy(underlying(modifier, fc, tm));
                    }
                    if pad == Pad::Zero {
                        pad = yr_spec;
                    }
                    let value = ye(tm_year) + 1900;
                    break 'dir Act::Num(Number {
                        digits: 4,
                        negative: tm_year < -1900,
                        magnitude: value.unsigned_abs(),
                        always_sign: pad == Pad::Sign
                            && (9999 < value.unsigned_abs() || 4 < width_now),
                        tz_colon_mask: 0,
                    });
                }
                b'y' => {
                    if cfg!(target_os = "macos") && modifier == b'E' {
                        break 'dir Act::Cpy(underlying(modifier, fc, tm));
                    }
                    let mut yy = tm_year % 100;
                    if yy < 0 {
                        yy = if tm_year < -1900 { -yy } else { yy + 100 };
                    }
                    if pad == Pad::Zero {
                        pad = yr_spec;
                    }
                    break 'dir Act::Num(Number {
                        digits: 2,
                        negative: false,
                        magnitude: ye(yy).unsigned_abs(),
                        always_sign: pad == Pad::Sign && (99 < yy || 2 < width_now),
                        tz_colon_mask: 0,
                    });
                }
                b'Z' => {
                    if change_case {
                        to_up = false;
                        to_low = true;
                    }
                    let zone = if tm.tm_zone.is_null() {
                        Vec::new()
                    } else {
                        unsafe { std::ffi::CStr::from_ptr(tm.tm_zone) }
                            .to_bytes()
                            .to_vec()
                    };
                    break 'dir Act::Cpy(zone);
                }
                b':' | b'z' => {
                    let mut colons = 0usize;
                    if fc == b':' {
                        colons = 1;
                        while at(f + colons) == b':' {
                            colons += 1;
                        }
                        if at(f + colons) != b'z' {
                            break 'dir Act::Bad;
                        }
                        f += colons;
                    }
                    if tm.tm_isdst < 0 {
                        break 'dir Act::Nothing;
                    }
                    let diff = i64::from(tm.tm_gmtoff);
                    let mut negative = diff < 0;
                    if diff == 0 && !tm.tm_zone.is_null() {
                        negative = unsafe { *tm.tm_zone } as u8 == b'-';
                    }
                    let hour = diff / 3600;
                    let min = diff / 60 % 60;
                    let sec = diff % 60;
                    let (digits, mask, v) = match colons {
                        0 => (5, 0, hour * 100 + min),
                        1 => (6, 0o4, hour * 100 + min),
                        2 => (9, 0o24, hour * 10000 + min * 100 + sec),
                        3 if sec != 0 => (9, 0o24, hour * 10000 + min * 100 + sec),
                        3 if min != 0 => (6, 0o4, hour * 100 + min),
                        3 => (3, 0, hour),
                        _ => break 'dir Act::Bad,
                    };
                    break 'dir Act::Num(Number {
                        digits,
                        negative,
                        magnitude: v.unsigned_abs(),
                        always_sign: true,
                        tz_colon_mask: mask,
                    });
                }
                0 => break 'dir Act::BadPercent,
                _ => break 'dir Act::Bad,
            }
        };

        match act {
            Act::Cpy(bytes) => put(out, pad, width_now, &bytes, to_low, to_up),
            Act::Sub(subfmt, sub_pad, subwidth) => {
                let mut tmp = Vec::new();
                internal(
                    &mut tmp,
                    subfmt.as_bytes(),
                    moment,
                    to_up,
                    sub_pad,
                    subwidth,
                )?;
                put(out, pad, width_now, &tmp, false, false);
            }
            Act::Num(num) => {
                let mut mask = num.tz_colon_mask;
                let mut v = num.magnitude;
                let mut rev: Vec<u8> = Vec::new();
                loop {
                    if mask & 1 == 1 {
                        rev.push(b':');
                    }
                    mask >>= 1;
                    rev.push(b'0' + (v % 10) as u8);
                    v /= 10;
                    if v == 0 && mask == 0 {
                        break;
                    }
                }
                rev.reverse();
                let mut pad = pad;
                if pad == Pad::Zero {
                    pad = Pad::AlwaysZero;
                }
                let mut w = if width_now < 0 { num.digits } else { width_now };
                let sign = if num.negative {
                    Some(b'-')
                } else if num.always_sign {
                    Some(b'+')
                } else {
                    None
                };
                let shortage = w - i64::from(sign.is_some()) - rev.len() as i64;
                let padding = if pad == Pad::No || shortage <= 0 {
                    0
                } else {
                    shortage
                };
                if let Some(s) = sign {
                    if pad == Pad::Space {
                        out.extend(std::iter::repeat_n(b' ', padding as usize));
                        w -= padding;
                    }
                    out.push(s);
                    w -= 1;
                }
                put(out, pad, w, &rev, to_low, to_up);
            }
            Act::Bad => {
                put(
                    out,
                    pad,
                    width_now,
                    &format[percent..=f.min(format.len() - 1)],
                    to_low,
                    to_up,
                );
            }
            Act::BadPercent => {
                // `--f`: the copied text ends before the offending character.
                f = f.saturating_sub(1);
                put(
                    out,
                    pad,
                    width_now,
                    &format[percent..=f.min(format.len() - 1)],
                    to_low,
                    to_up,
                );
            }
            Act::Nothing => {}
        }
        f += 1;
    }
    Ok(())
}
