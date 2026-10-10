//! Text-property intervals, following `intervals.c` / `textprop.c`.
//!
//! A string (or buffer) keeps one plist per character plus a set of *boundary*
//! positions. The intervals Emacs holds are the runs between boundaries, where
//! a boundary is either recorded in that set or implied by two neighbouring
//! plists that differ. Recording matters because Emacs never merges intervals
//! that merely became equal: `(put-text-property 1 3 'p 1 s)` followed by
//! `(put-text-property 3 5 'p 1 s)` leaves the intervals `[1,3)` and `[3,5)`
//! apart, and `prin1` prints them as two. Each primitive here splits where the
//! C one splits, and nowhere else.

use crate::host::{el_truthy, ElispHost, Obj};
use fusevm::Value;

/// How a property already present is combined with a new value
/// (`enum property_set_type`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SetType {
    Replace,
    Prepend,
    Append,
}

/// One interval: the half-open character range and its plist.
pub type Run = (usize, usize);

impl ElispHost {
    /// Whether two neighbouring plists are one interval's worth of the same
    /// properties.
    pub(crate) fn tp_same(&self, a: &Value, b: &Value) -> bool {
        self.values_eq(a, b) || self.plist_struct_eq(a, b)
    }

    /// The intervals of a text of `props.len()` characters: the runs between
    /// the recorded boundaries and the places two neighbours differ.
    pub(crate) fn tp_runs(&self, props: &[Value], bounds: &[u32]) -> Vec<Run> {
        let n = props.len();
        let mut out = Vec::new();
        let mut start = 0;
        let mut b = bounds.iter().copied().peekable();
        for i in 1..n {
            while b.peek().is_some_and(|&p| (p as usize) < i) {
                b.next();
            }
            let recorded = b.peek().is_some_and(|&p| p as usize == i);
            if recorded || !self.tp_same(&props[i - 1], &props[i]) {
                out.push((start, i));
                start = i;
            }
        }
        if n > 0 {
            out.push((start, n));
        }
        out
    }

    /// Record a boundary at char index `p` (0 and the end are not boundaries).
    fn tp_split(bounds: &mut Vec<u32>, p: usize, len: usize) {
        if p == 0 || p >= len {
            return;
        }
        let p = p as u32;
        if let Err(i) = bounds.binary_search(&p) {
            bounds.insert(i, p);
        }
    }

    /// The runs a derived text keeps from `src`'s intervals clipped to
    /// `[from, from + len)`, placed at offset `at` of the result: a propertized
    /// source interval puts a boundary at each of its clipped edges.
    pub(crate) fn tp_clip(
        &self,
        props: &[Value],
        bounds: &[u32],
        span: (usize, usize, usize, usize),
        out_bounds: &mut Vec<u32>,
    ) {
        let (from, len, at, total) = span;
        for (rs, re) in self.tp_runs(props, bounds) {
            if re <= from || rs >= from + len {
                continue;
            }
            if !el_truthy(&props[rs]) {
                continue;
            }
            let a = rs.max(from) - from + at;
            let b = re.min(from + len) - from + at;
            Self::tp_split(out_bounds, a, total);
            Self::tp_split(out_bounds, b, total);
        }
    }

    /// `copy_properties`: the characters `[from, to)` get one fresh copy of the
    /// plist they share, as the new interval a split creates does.
    fn tp_detach(&mut self, props: &mut [Value], from: usize, to: usize) {
        if from >= to || !el_truthy(&props[from]) {
            return;
        }
        let items = self.list_vec(&props[from]).unwrap_or_default();
        let copy = self.list_from(items);
        for p in &mut props[from..to] {
            *p = copy.clone();
        }
    }

    /// `interval_has_all_properties`: every PAIR is present with an `eq` value.
    fn tp_has_all(&self, plist: &Value, pairs: &[(Value, Value)]) -> bool {
        pairs.iter().all(|(k, v)| {
            self.plist_lookup(plist, k)
                .is_some_and(|found| self.values_eq(&found, v))
        })
    }

    /// `interval_has_some_properties`: some NAME is present.
    fn tp_has_some(&self, plist: &Value, names: &[Value]) -> bool {
        names.iter().any(|k| self.plist_lookup(plist, k).is_some())
    }

    /// The value PROP has in PLIST, or `None` when PROP is absent (a present nil
    /// value is `Some`).
    pub(crate) fn plist_lookup(&self, plist: &Value, prop: &Value) -> Option<Value> {
        let mut cur = plist.clone();
        while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
            let (k, d) = (k.clone(), d.clone());
            let Some(Obj::Cons(v, rest)) = self.obj(&d) else {
                return None;
            };
            if self.values_eq(&k, prop) {
                return Some(v.clone());
            }
            cur = rest.clone();
        }
        None
    }

    /// `add_properties`: a fresh copy of PLIST with every PAIR applied. A
    /// property already present keeps its position and takes the new value (or
    /// is combined with it for `face` under SET_TYPE); a new one is prepended.
    fn tp_apply_add(
        &mut self,
        plist: &Value,
        pairs: &[(Value, Value)],
        set_type: SetType,
    ) -> Value {
        let mut items: Vec<Value> = Vec::new();
        let mut cur = plist.clone();
        while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
            let (k, d) = (k.clone(), d.clone());
            let Some(Obj::Cons(v, rest)) = self.obj(&d) else {
                break;
            };
            items.push(k);
            items.push(v.clone());
            cur = rest.clone();
        }
        for (prop, val) in pairs {
            let slot = (0..items.len())
                .step_by(2)
                .find(|&i| self.values_eq(&items[i], prop));
            match slot {
                Some(i) => {
                    let old = items[i + 1].clone();
                    if self.values_eq(&old, val) {
                        continue;
                    }
                    items[i + 1] = match set_type {
                        SetType::Replace => val.clone(),
                        _ => self.tp_combine(prop, &old, val, set_type),
                    };
                }
                None => {
                    items.insert(0, val.clone());
                    items.insert(0, prop.clone());
                }
            }
        }
        self.list_from(items)
    }

    /// `add_properties`' PREPEND / APPEND arms: the previous value becomes a
    /// list that VAL is added to the front or back of — except an anonymous face
    /// (a plist starting with a keyword), which is a single value.
    fn tp_combine(&mut self, prop: &Value, old: &Value, val: &Value, set_type: SetType) -> Value {
        let is_face = self.sym_name(prop).as_deref() == Some("face");
        let old_is_list = matches!(self.obj(old), Some(Obj::Cons(..))) && {
            let head_is_keyword = match self.obj(old) {
                Some(Obj::Cons(car, _)) => {
                    let car = car.clone();
                    self.sym_name(&car).is_some_and(|n| n.starts_with(':'))
                }
                _ => false,
            };
            !(is_face && head_is_keyword)
        };
        let mut items = if old_is_list {
            self.list_vec(old).unwrap_or_default()
        } else {
            vec![old.clone()]
        };
        match set_type {
            SetType::Prepend => items.insert(0, val.clone()),
            _ => items.push(val.clone()),
        }
        self.list_from(items)
    }

    /// `add_text_properties_1`: apply PAIRS over `[s, e)` of a text whose
    /// plists are PROPS. Returns whether anything changed.
    pub(crate) fn tp_add(
        &mut self,
        props: &mut [Value],
        bounds: &mut Vec<u32>,
        s: usize,
        e: usize,
        pairs: &[(Value, Value)],
        set_type: SetType,
    ) -> bool {
        let n = props.len();
        if s >= e || e > n || pairs.is_empty() {
            return false;
        }
        let runs = self.tp_runs(props, bounds);
        let Some(mut i) = runs.iter().position(|&(rs, re)| rs <= s && s < re) else {
            return false;
        };
        // Intervals that already have everything are skipped, unsplit.
        while self.tp_has_all(&props[runs[i].0], pairs) {
            if runs[i].1 >= e {
                return false;
            }
            i += 1;
        }
        let first_start = runs[i].0;
        let mut cursor = first_start.max(s);
        if first_start < s {
            Self::tp_split(bounds, s, n);
            self.tp_detach(props, s, runs[i].1);
        }
        let mut changed = false;
        loop {
            let (_, re) = runs[i];
            let lacks = !self.tp_has_all(&props[runs[i].0], pairs);
            if re >= e {
                if !lacks {
                    return changed;
                }
                if re > e {
                    Self::tp_split(bounds, e, n);
                }
                let new = self.tp_apply_add(&props[runs[i].0].clone(), pairs, set_type);
                for p in &mut props[cursor..e] {
                    *p = new.clone();
                }
                return true;
            }
            let new = self.tp_apply_add(&props[runs[i].0].clone(), pairs, set_type);
            for p in &mut props[cursor..re] {
                *p = new.clone();
            }
            changed = true;
            cursor = re;
            i += 1;
        }
    }

    /// `remove_properties`: a fresh copy of PLIST without NAMES.
    fn tp_apply_remove(&mut self, plist: &Value, names: &[Value]) -> Value {
        let mut items: Vec<Value> = Vec::new();
        let mut cur = plist.clone();
        while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
            let (k, d) = (k.clone(), d.clone());
            let Some(Obj::Cons(v, rest)) = self.obj(&d) else {
                break;
            };
            if !names.iter().any(|n| self.values_eq(n, &k)) {
                items.push(k);
                items.push(v.clone());
            }
            cur = rest.clone();
        }
        self.list_from(items)
    }

    /// `Fremove_text_properties`' walk over `[s, e)`.
    pub(crate) fn tp_remove(
        &mut self,
        props: &mut [Value],
        bounds: &mut Vec<u32>,
        s: usize,
        e: usize,
        names: &[Value],
    ) -> bool {
        let n = props.len();
        if s >= e || e > n || names.is_empty() {
            return false;
        }
        let runs = self.tp_runs(props, bounds);
        let Some(mut i) = runs.iter().position(|&(rs, re)| rs <= s && s < re) else {
            return false;
        };
        while !self.tp_has_some(&props[runs[i].0], names) {
            if runs[i].1 >= e {
                return false;
            }
            i += 1;
        }
        let first_start = runs[i].0;
        let mut cursor = first_start.max(s);
        if first_start < s {
            Self::tp_split(bounds, s, n);
            self.tp_detach(props, s, runs[i].1);
        }
        let mut changed = false;
        loop {
            let re = runs[i].1;
            let has = self.tp_has_some(&props[runs[i].0], names);
            if re >= e {
                if !has {
                    return changed;
                }
                if re > e {
                    Self::tp_split(bounds, e, n);
                }
                let new = self.tp_apply_remove(&props[runs[i].0].clone(), names);
                for p in &mut props[cursor..e] {
                    *p = new.clone();
                }
                return true;
            }
            if has {
                let new = self.tp_apply_remove(&props[runs[i].0].clone(), names);
                for p in &mut props[cursor..re] {
                    *p = new.clone();
                }
                changed = true;
            }
            cursor = re;
            i += 1;
        }
    }

    /// `set_text_properties`: `[s, e)` becomes one interval holding a copy of
    /// PLIST, bounded by `s` and `e`; the boundaries inside it are gone.
    pub(crate) fn tp_set(
        &mut self,
        props: &mut [Value],
        bounds: &mut Vec<u32>,
        s: usize,
        e: usize,
        plist: &Value,
    ) {
        let n = props.len();
        if s >= e || e > n {
            return;
        }
        let runs = self.tp_runs(props, bounds);
        if let Some(&(rs, re)) = runs.iter().find(|&&(rs, re)| rs < s && s < re) {
            // The piece after the range is the one a right-split creates, so it
            // holds its own copy of the plist.
            if e < re {
                self.tp_detach(props, e, re);
            }
            let _ = rs;
        }
        bounds.retain(|&p| (p as usize) <= s || (p as usize) >= e);
        Self::tp_split(bounds, s, n);
        Self::tp_split(bounds, e, n);
        let items = self.list_vec(plist).unwrap_or_default();
        let copy = if items.is_empty() {
            Value::Undef
        } else {
            self.list_from(items)
        };
        for p in &mut props[s..e] {
            *p = copy.clone();
        }
    }
}

impl ElispHost {
    /// `graft_intervals_into_buffer`: the intervals of string S land at char
    /// index `at` of the current buffer. Each propertized source interval gets
    /// a fresh plist copy; every source boundary, and the two edges of the
    /// inserted text, become buffer boundaries.
    pub(crate) fn buffer_graft_string_props(
        &mut self,
        s: &std::sync::Arc<String>,
        at: usize,
        len: usize,
    ) {
        let Some(props) = self.string_props_vec(s) else {
            return;
        };
        let sb = self.string_bounds_vec(s);
        let runs = self.tp_runs(&props, &sb);
        let bi = self.current;
        let mut staged: Vec<(usize, usize, Value)> = Vec::new();
        for &(rs, re) in &runs {
            if rs >= len || !el_truthy(&props[rs]) {
                continue;
            }
            let items = self.list_vec(&props[rs]).unwrap_or_default();
            let copy = self.list_from(items);
            staged.push((rs, re.min(len), copy));
        }
        if staged.is_empty() {
            return;
        }
        let b = &mut self.buffers[bi];
        b.has_intervals = true;
        for (rs, re, plist) in staged {
            for slot in &mut b.props[at + rs..at + re] {
                *slot = plist.clone();
            }
        }
        let total = b.props.len();
        let mut edges: Vec<usize> = vec![at, at + len];
        edges.extend(runs.iter().map(|&(rs, _)| at + rs));
        for edge in edges {
            if edge > 0 && edge < total {
                let e = edge as u32;
                if let Err(i) = b.prop_bounds.binary_search(&e) {
                    b.prop_bounds.insert(i, e);
                }
            }
        }
    }

    /// `copy_intervals_to_string`: string S gets the buffer plists over the
    /// 0-based range `[from, to)`, one fresh copy per interval, and the
    /// interval boundaries that fall inside it.
    pub(crate) fn string_from_buffer_props(
        &mut self,
        s: &std::sync::Arc<String>,
        from: usize,
        to: usize,
    ) {
        let bi = self.current;
        let props = self.buffers[bi].props.clone();
        let bounds = self.buffers[bi].prop_bounds.clone();
        if !props[from..to].iter().any(el_truthy) {
            return;
        }
        let mut vec: Vec<Value> = vec![Value::Undef; to - from];
        let mut out_bounds: Vec<u32> = Vec::new();
        for (rs, re) in self.tp_runs(&props, &bounds) {
            if re <= from || rs >= to {
                continue;
            }
            let a = rs.max(from) - from;
            let b = re.min(to) - from;
            if el_truthy(&props[rs]) {
                let items = self.list_vec(&props[rs]).unwrap_or_default();
                let copy = self.list_from(items);
                for slot in &mut vec[a..b] {
                    *slot = copy.clone();
                }
            }
            Self::tp_split(&mut out_bounds, a, to - from);
            Self::tp_split(&mut out_bounds, b, to - from);
        }
        self.string_set_props_bounded(s, vec, out_bounds);
    }
}

impl ElispHost {
    /// Whether the characters on both sides of 1-based buffer position POS are
    /// one interval, so that text inserted there extends it.
    pub(crate) fn buffer_inside_interval(&self, pos: usize) -> bool {
        let b = &self.buffers[self.current];
        if pos <= 1 || pos > b.props.len() {
            return false;
        }
        let at = pos - 1;
        if b.prop_bounds.binary_search(&(at as u32)).is_ok() {
            return false;
        }
        self.tp_same(&b.props[at - 1], &b.props[at])
    }

    /// `insert-and-inherit`'s tail: the text `[start, end)` (1-based) just
    /// inserted takes INHERITED, plus whatever properties it carried that
    /// INHERITED does not name. It then joins the interval to its left or right
    /// when their plists are the same, as `adjust_intervals_for_insertion` does.
    pub(crate) fn buffer_inherit_props(
        &mut self,
        start: usize,
        end: usize,
        inherited: &Value,
        inside: bool,
    ) {
        let bi = self.current;
        let (lo, hi) = (start - 1, end - 1);
        if lo >= hi {
            return;
        }
        let inherited_items = self.list_vec(inherited).unwrap_or_default();
        let runs: Vec<(usize, usize)> = {
            let b = &self.buffers[bi];
            self.tp_runs(&b.props[lo..hi], &[])
        };
        for (rs, re) in runs {
            let own = self.buffers[bi].props[lo + rs].clone();
            let mut items = inherited_items.clone();
            let mut cur = own;
            while let Some(Obj::Cons(k, d)) = self.obj(&cur) {
                let (k, d) = (k.clone(), d.clone());
                let Some(Obj::Cons(v, rest)) = self.obj(&d) else {
                    break;
                };
                let named = (0..items.len())
                    .step_by(2)
                    .any(|i| self.values_eq(&items[i], &k));
                if !named {
                    items.push(k);
                    items.push(v.clone());
                }
                cur = rest.clone();
            }
            let plist = if items.is_empty() {
                Value::Undef
            } else {
                self.list_from(items)
            };
            for slot in &mut self.buffers[bi].props[lo + rs..lo + re] {
                *slot = plist.clone();
            }
        }
        // Boundaries: the insertion put one at each edge; inheriting removes the
        // ones where the new text simply continues a neighbour.
        let (left, right) = {
            let b = &self.buffers[bi];
            (
                (lo > 0).then(|| b.props[lo - 1].clone()),
                (hi < b.props.len()).then(|| b.props[hi].clone()),
            )
        };
        let new_first = self.buffers[bi].props[lo].clone();
        let new_last = self.buffers[bi].props[hi - 1].clone();
        let joins_left = inside || left.as_ref().is_some_and(|l| self.tp_same(l, &new_first));
        let joins_right = inside || right.as_ref().is_some_and(|r| self.tp_same(r, &new_last));
        let b = &mut self.buffers[bi];
        if joins_left {
            b.prop_bounds.retain(|&p| p as usize != lo);
        }
        if joins_right {
            b.prop_bounds.retain(|&p| p as usize != hi);
        }
    }
}
