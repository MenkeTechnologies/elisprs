//! backquote.el: the expander behind the `` ` `` macro.
//!
//! The reader turns `` `X ``, `,X` and `,@X` into `` (\` X) ``, `(\, X)` and
//! `(\,@ X)`, exactly as Emacs's reader does, so a backquote is ordinary data
//! until it is macroexpanded: `'`(a ,b)` is a three-level list, nested
//! backquotes keep their levels, and `pcase` sees the `` \` `` pattern. The
//! prelude defines `` \` `` and `backquote` as macros over
//! `backquote-process`, which is this module — a line-by-line port of GNU Emacs
//! 31.1's `lisp/emacs-lisp/backquote.el` (`backquote-process`,
//! `backquote-delay-process`, `backquote-listify`). It lives in Rust because
//! the prelude's own macros are written with backquote, so the expander has to
//! exist before the first of them is read.

use crate::host::{ElispHost, Obj};
use fusevm::Value;

/// backquote.el's tags: the code is constant (0), must be evaluated (1), or
/// evaluates to a list to be spliced into its container (2).
type Tagged = (u8, Value);

fn is_nil(v: &Value) -> bool {
    matches!(v, Value::Undef | Value::Bool(false))
}

fn head_is(h: &ElispHost, v: &Value, name: &str) -> bool {
    match h.obj(v) {
        Some(Obj::Cons(car, _)) => h.sym_name(car).as_deref() == Some(name),
        _ => false,
    }
}

fn car_cdr(h: &ElispHost, v: &Value) -> Option<(Value, Value)> {
    match h.obj(v) {
        Some(Obj::Cons(a, d)) => Some((a.clone(), d.clone())),
        _ => None,
    }
}

fn quote(h: &mut ElispHost, v: Value) -> Value {
    let q = h.intern("quote");
    h.list_from(vec![q, v])
}

/// `(eval FORM)` for a form `backquote-process` tagged constant: a `(quote X)`
/// or a self-evaluating atom.
fn const_value(h: &ElispHost, form: &Value) -> Value {
    if head_is(h, form, "quote") {
        if let Some((_, rest)) = car_cdr(h, form) {
            if let Some((x, _)) = car_cdr(h, &rest) {
                return x;
            }
        }
    }
    form.clone()
}

fn tag_of(h: &ElispHost, expr: Value) -> Tagged {
    (if head_is(h, &expr, "quote") { 0 } else { 1 }, expr)
}

/// `backquote-delay-process`: a quote construct nested inside the backquote is
/// rebuilt as data, its body processed at the adjusted LEVEL.
fn delay_process(h: &mut ElispHost, s: &Value, level: i64) -> Result<Tagged, String> {
    let (head, rest) = car_cdr(h, s).expect("caller checked consp");
    let head_q = quote(h, head);
    let rest_p = process(h, &rest, level)?;
    let exp = listify(h, vec![(0, head_q)], rest_p);
    Ok(tag_of(h, exp))
}

/// `(backquote-process S &optional LEVEL)`.
pub fn process(h: &mut ElispHost, s: &Value, level: i64) -> Result<Tagged, String> {
    if let Some(Obj::Vector(items)) = h.obj(s) {
        let items = items.clone();
        let as_list = h.list_from(items.to_vec());
        let (tag, code) = process(h, &as_list, level)?;
        if tag == 0 {
            return Ok((0, s.clone()));
        }
        let form = match car_cdr(h, &code) {
            None if !is_nil(&code) => {
                let vc = h.intern("vconcat");
                h.list_from(vec![vc, code])
            }
            Some((op, args)) if h.sym_name(&op).as_deref() == Some("list") => {
                let v = h.intern("vector");
                h.cons(v, args)
            }
            Some((op, args)) if h.sym_name(&op).as_deref() == Some("append") => {
                let v = h.intern("vconcat");
                h.cons(v, args)
            }
            Some(_) | None => {
                let apply = h.intern("apply");
                let function = h.intern("function");
                let vector = h.intern("vector");
                let fv = h.list_from(vec![function, vector]);
                h.list_from(vec![apply, fv, code])
            }
        };
        return Ok((1, form));
    }
    let Some((head, _)) = car_cdr(h, s) else {
        // An atom: nil, t and non-symbols stand for themselves; a symbol is
        // quoted.
        let self_evaluating = is_nil(s)
            || matches!(s, Value::Bool(true))
            || !matches!(h.obj(s), Some(Obj::Symbol(_)));
        return Ok((
            0,
            if self_evaluating {
                s.clone()
            } else {
                quote(h, s.clone())
            },
        ));
    };
    match h.sym_name(&head).as_deref() {
        Some(",") => {
            if level <= 0 {
                let parts = h.list_vec(s).unwrap_or_default();
                if parts.len() > 2 {
                    return Err(multiple_args(h, ",", s));
                }
                let x = parts.get(1).cloned().unwrap_or(Value::Undef);
                return Ok(tag_of(h, x));
            }
            return delay_process(h, s, level - 1);
        }
        Some(",@") => {
            if level <= 0 {
                let parts = h.list_vec(s).unwrap_or_default();
                if parts.len() > 2 {
                    return Err(multiple_args(h, ",@", s));
                }
                return Ok((2, parts.get(1).cloned().unwrap_or(Value::Undef)));
            }
            return delay_process(h, s, level - 1);
        }
        Some("`") => return delay_process(h, s, level + 1),
        _ => {}
    }
    // A list: LISTS collects (backwards) forms that each produce a run of
    // elements; LIST holds the non-spliced items since the last splice, most
    // recent first; FIRSTLIST the ones before the first splice.
    let mut rest = s.clone();
    let mut firstlist: Vec<Tagged> = Vec::new();
    let mut list: Vec<Tagged> = Vec::new();
    let mut lists: Vec<Value> = Vec::new();
    while let Some((car, cdr)) = car_cdr(h, &rest) {
        // Stop at a dotted `,X' / `` `X '' tail: that cdr is itself a quote
        // construct and goes through `backquote-process' whole.
        if matches!(h.sym_name(&car).as_deref(), Some(",") | Some("`")) {
            break;
        }
        let item = process(h, &car, level)?;
        if item.0 == 2 {
            if lists.is_empty() {
                firstlist = std::mem::take(&mut list);
            }
            if !list.is_empty() {
                let l = std::mem::take(&mut list);
                let form = listify(h, l, (0, Value::Undef));
                lists.push(form);
            }
            lists.push(item.1);
        } else {
            list.insert(0, item);
        }
        rest = cdr;
    }
    if !is_nil(&rest) || !list.is_empty() {
        let tail = process(h, &rest, level)?;
        let l = std::mem::take(&mut list);
        let form = listify(h, l, tail);
        lists.push(form);
    }
    let mut expression = if lists.len() > 1 || lists.first().is_some_and(|f| head_is(h, f, ",@")) {
        let append = h.intern("append");
        let mut v = vec![append];
        v.extend(lists);
        h.list_from(v)
    } else {
        lists.pop().unwrap_or(Value::Undef)
    };
    if !firstlist.is_empty() {
        expression = listify(h, firstlist, (1, expression));
    }
    Ok(tag_of(h, expression))
}

/// `backquote-listify`: combine tagged items (LIST, most recent first) with the
/// processed tail OLD-TAIL into `list` / `cons` / `backquote-list*`, folding
/// the constant trailing items into a quoted tail.
fn listify(h: &mut ElispHost, list: Vec<Tagged>, old_tail: Tagged) -> Value {
    let mut heads: Vec<Value> = Vec::new(); // in order
    let old_tail_constant = old_tail.0 == 0;
    let mut tail = if old_tail_constant {
        const_value(h, &old_tail.1)
    } else {
        old_tail.1.clone()
    };
    for (tag, form) in list {
        if !heads.is_empty() || !old_tail_constant || tag != 0 {
            heads.insert(0, form);
        } else {
            let v = const_value(h, &form);
            tail = h.cons(v, tail);
        }
    }
    if !is_nil(&tail) {
        if old_tail_constant {
            tail = quote(h, tail);
        }
        if heads.is_empty() {
            return tail;
        }
        let use_list_star = heads.len() > 1 || head_is(h, &heads[0], ",@");
        let op = h.intern(if use_list_star {
            "backquote-list*"
        } else {
            "cons"
        });
        let mut v = vec![op];
        v.extend(heads);
        v.push(tail);
        return h.list_from(v);
    }
    let op = h.intern("list");
    let mut v = vec![op];
    v.extend(heads);
    h.list_from(v)
}

fn multiple_args(h: &mut ElispHost, op: &str, s: &Value) -> String {
    let msg = format!(
        "Multiple args to {op} are not supported: {}",
        h.print(s, true)
    );
    let text = h.new_string(msg.clone());
    let sym = h.intern("error");
    let data = h.list_from(vec![text]);
    let obj = h.cons(sym, data);
    h.set_pending_error(&format!("error: {msg}"), obj);
    format!("error: {msg}")
}
