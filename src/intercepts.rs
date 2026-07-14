//! AOP command-intercept / advice machinery — an elisprs extension ported
//! faithfully from zshrs `src/extensions/intercepts.rs`.
//!
//! This is **distinct from elisp's native per-symbol advice** (nadvice:
//! `advice-add` / `defadvice`). The distinguishing feature ported from zshrs is
//! GLOB/PATTERN matching across *many* function names at once — one registration
//! fires on every symbol whose name matches a glob such as `"forward-*"`, `"_*"`,
//! or the catch-alls `"*"` / `"all"` — with `before`/`after`/`around` advice and a
//! timing/`proceed` protocol. nadvice can only attach to a single named symbol, so
//! this layer is additive, not a replacement.
//!
//! Join point: every elisp function call flows through [`crate::host::call_function`];
//! the pattern layer fires there, on the symbol-function dispatch, before the
//! callee is resolved. Advice bodies are ordinary elisp *forms* (passed quoted to
//! `intercept`), evaluated in the current host via the same `eval` intrinsic as
//! everything else — no subprocess, no fork.
//!
//! There is no C zsh counterpart to the pattern layer; zshrs's own note records
//! that C zsh's closest analog is the function-wrapper hook in `Src/module.c`
//! (`addwrapper()`), but per-function before/after/around AOP intercepts are
//! unique to the zshrs lineage that this module continues.

use crate::host::{call_function, with_host, ElispHost, Obj};
use fusevm::Value;
use std::time::{Duration, Instant};

/// AOP advice type — before, after, or around. zshrs-original (no C counterpart);
/// distinct from elisp nadvice's `:before`/`:after`/`:around` combinators, which
/// attach per-symbol rather than per-glob.
#[derive(Debug, Clone)]
pub enum AdviceKind {
    /// Run the advice form before the function executes.
    Before,
    /// Run the advice form after the function executes. `intercept-ms`/
    /// `intercept-us` are bound to the elapsed time.
    After,
    /// Wrap the function. The advice form must call `(intercept-proceed)` to run
    /// the original; the intercept returns the advice form's own value.
    Around,
}

impl AdviceKind {
    /// The advice kind's elisp keyword name (`before` / `after` / `around`).
    pub fn as_str(&self) -> &'static str {
        match self {
            AdviceKind::Before => "before",
            AdviceKind::After => "after",
            AdviceKind::Around => "around",
        }
    }
}

/// One AOP intercept registered against a function-name pattern. zshrs-original.
/// `code` is an elisp form (heap handle) evaluated when the pattern matches — the
/// arena is append-only, so a stored handle stays valid for the host's lifetime.
#[derive(Debug, Clone)]
pub struct Intercept {
    /// Pattern to match function names. Supports glob: `"forward-*"`, `"_*"`,
    /// `"*"`, `"all"`.
    pub pattern: String,
    /// What kind of advice.
    pub kind: AdviceKind,
    /// The elisp advice form to evaluate.
    pub code: Value,
    /// Unique ID for removal.
    pub id: u32,
}

/// Match an intercept pattern against a function name or the full "name arg…"
/// string. Supports: exact match, glob (`"forward-*"`, `"_*"`, `"*"`), or `"all"`.
/// Ported verbatim from zshrs `intercept_matches`; the glob crate is replaced by
/// the dependency-free [`GlobPat`] (elisprs vendors only fusevm).
pub(crate) fn intercept_matches(pattern: &str, cmd_name: &str, full_cmd: &str) -> bool {
    if pattern == "*" || pattern == "all" {
        return true;
    }
    if pattern == cmd_name {
        return true;
    }
    if pattern.contains('*') || pattern.contains('?') {
        if let Ok(pat) = GlobPat::new(pattern) {
            return pat.matches(cmd_name) || pat.matches(full_cmd);
        }
    }
    false
}

// ── glob matcher (drop-in for zshrs's `glob::Pattern`) ─────────────────────────
// Supports `*` (any run, incl. empty), `?` (one char), and `[...]` classes with
// `!`/`^` negation and `a-z` ranges. `new` errors on an unterminated `[`, so an
// invalid pattern makes `intercept_matches` fall through to `false` exactly as a
// `glob::Pattern::new` error would.

enum ClassItem {
    Ch(char),
    Range(char, char),
}

enum GTok {
    Star,
    Any,
    Ch(char),
    Class { neg: bool, items: Vec<ClassItem> },
}

/// A compiled glob pattern. Mirrors the subset of `glob::Pattern` semantics the
/// zshrs intercept layer relies on.
struct GlobPat {
    toks: Vec<GTok>,
}

impl GlobPat {
    fn new(pat: &str) -> Result<GlobPat, ()> {
        let chars: Vec<char> = pat.chars().collect();
        let mut toks = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '*' => {
                    toks.push(GTok::Star);
                    i += 1;
                }
                '?' => {
                    toks.push(GTok::Any);
                    i += 1;
                }
                '[' => {
                    // Parse a character class up to the matching `]`.
                    let mut j = i + 1;
                    let neg = matches!(chars.get(j), Some('!') | Some('^'));
                    if neg {
                        j += 1;
                    }
                    let mut items = Vec::new();
                    // A `]` immediately after `[`/`[!` is a literal `]` (glob rule).
                    if matches!(chars.get(j), Some(']')) {
                        items.push(ClassItem::Ch(']'));
                        j += 1;
                    }
                    let mut closed = false;
                    while j < chars.len() {
                        if chars[j] == ']' {
                            closed = true;
                            break;
                        }
                        // `a-z` range (but a trailing `-` before `]` is literal).
                        if chars.get(j + 1) == Some(&'-')
                            && chars.get(j + 2).is_some_and(|c| *c != ']')
                        {
                            items.push(ClassItem::Range(chars[j], chars[j + 2]));
                            j += 3;
                        } else {
                            items.push(ClassItem::Ch(chars[j]));
                            j += 1;
                        }
                    }
                    if !closed {
                        return Err(());
                    }
                    toks.push(GTok::Class { neg, items });
                    i = j + 1;
                }
                c => {
                    toks.push(GTok::Ch(c));
                    i += 1;
                }
            }
        }
        Ok(GlobPat { toks })
    }

    fn matches(&self, text: &str) -> bool {
        let text: Vec<char> = text.chars().collect();
        // Classic wildcard match with `*` backtracking.
        let (mut ti, mut xi) = (0usize, 0usize);
        let (mut star_ti, mut star_xi): (Option<usize>, usize) = (None, 0);
        while xi < text.len() {
            let matched = ti < self.toks.len()
                && match &self.toks[ti] {
                    GTok::Any => true,
                    GTok::Ch(c) => text[xi] == *c,
                    GTok::Class { neg, items } => class_match(*neg, items, text[xi]),
                    GTok::Star => {
                        star_ti = Some(ti);
                        star_xi = xi;
                        ti += 1;
                        continue;
                    }
                };
            if matched {
                ti += 1;
                xi += 1;
            } else if let Some(sti) = star_ti {
                // Backtrack: let the last `*` swallow one more char.
                ti = sti + 1;
                star_xi += 1;
                xi = star_xi;
            } else {
                return false;
            }
        }
        while ti < self.toks.len() && matches!(self.toks[ti], GTok::Star) {
            ti += 1;
        }
        ti == self.toks.len()
    }
}

fn class_match(neg: bool, items: &[ClassItem], c: char) -> bool {
    let hit = items.iter().any(|it| match it {
        ClassItem::Ch(x) => *x == c,
        ClassItem::Range(a, b) => *a <= c && c <= *b,
    });
    hit != neg
}

// ── runtime: firing intercepts on the call_function join point ─────────────────

impl ElispHost {
    /// The "name arg…" string an intercept pattern can glob against (mirrors
    /// zshrs's `full_cmd`, so `"forward-*"` can match on the callee name and a
    /// pattern like `"foo bar"` can match on the printed call).
    pub(crate) fn intercept_full_cmd(&self, name: &str, args: &[Value]) -> String {
        if args.is_empty() {
            return name.to_string();
        }
        let mut s = String::from(name);
        for a in args {
            s.push(' ');
            s.push_str(&self.print(a, false));
        }
        s
    }

    /// Bind the AOP context variables read by advice bodies (`intercept-name`,
    /// `intercept-args`, `intercept-cmd`). Mirrors zshrs's `INTERCEPT_NAME` /
    /// `INTERCEPT_ARGS` / `INTERCEPT_CMD`, adapted to elisp names + types
    /// (`intercept-args` is the real argument *list*, not a joined string).
    pub(crate) fn set_intercept_context(&mut self, name: &str, args: &[Value], full: &str) {
        let arglist = self.list_from(args.to_vec());
        self.bind_intercept_var("intercept-name", Value::str(name));
        self.bind_intercept_var("intercept-args", arglist);
        self.bind_intercept_var("intercept-cmd", Value::str(full.to_string()));
    }

    /// Bind the timing context variables read by `after` advice (`intercept-ms`
    /// float milliseconds, `intercept-us` integer microseconds). Mirrors zshrs's
    /// `INTERCEPT_MS` / `INTERCEPT_US`.
    pub(crate) fn set_intercept_timing(&mut self, elapsed: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.0;
        self.bind_intercept_var("intercept-ms", Value::Float(ms));
        self.bind_intercept_var("intercept-us", Value::Int((ms * 1000.0) as i64));
    }

    /// Void the AOP context variables so they do not leak past the intercept.
    pub(crate) fn clear_intercept_context(&mut self) {
        for n in [
            "intercept-name",
            "intercept-args",
            "intercept-cmd",
            "intercept-ms",
            "intercept-us",
        ] {
            let sym = self.intern(n);
            if let Value::Obj(id) = sym {
                if let Some(Obj::Symbol(sd)) = self.arena.get_mut(id as usize) {
                    sd.value = None;
                }
            }
        }
    }

    fn bind_intercept_var(&mut self, name: &str, val: Value) {
        let sym = self.intern(name);
        self.set_special(&sym);
        let _ = self.set_raw_global(&sym, val);
    }
}

/// Reset the intercept-processing guard and clear the per-call context. Always
/// runs before `run_intercepts` returns.
fn intercept_finish(h: &mut ElispHost) {
    h.intercept_active = false;
    h.intercept_current = None;
    h.clear_intercept_context();
    h.intercept_proceeded = false;
}

/// Evaluate an advice form in the current host. Reuses the `eval` intrinsic (so
/// advice runs in an empty lexical environment with the AOP context variables
/// visible dynamically) and holds no host borrow across the nested run.
fn eval_advice(form: &Value) -> Result<Value, String> {
    let ev = with_host(|h| h.intern("eval"));
    call_function(&ev, std::slice::from_ref(form))
}

/// Fire the intercepts matching `name` for a function call. Returns:
/// * `Ok(None)` — no intercept fully handled the call; the caller must run the
///   original dispatch normally (no match, or only `before` advice fired).
/// * `Ok(Some(v))` — the call was handled here (an `around` advice, or the
///   original was run to feed `after` advice); `v` is the result.
/// * `Err(_)` — an advice form (or the proceeded original) signalled.
///
/// Ported from zshrs `ShellExecutor::run_intercepts`. The `intercept_active`
/// guard makes advice bodies (and the proceeded original) dispatch normally so a
/// pattern that matches inside its own advice does not recurse forever.
pub fn run_intercepts(f: &Value, name: &str, args: &[Value]) -> Result<Option<Value>, String> {
    // Snapshot the matching intercepts under a short borrow (clone to avoid
    // borrow issues while advice runs, mirroring the zshrs original).
    let (matching, full_cmd) = with_host(|h| {
        if h.intercept_active {
            return (Vec::new(), String::new());
        }
        let full = h.intercept_full_cmd(name, args);
        let m: Vec<Intercept> = h
            .intercepts
            .iter()
            .filter(|i| intercept_matches(&i.pattern, name, &full))
            .cloned()
            .collect();
        (m, full)
    });

    if matching.is_empty() {
        return Ok(None);
    }

    // Enter the guarded region and publish the AOP context.
    with_host(|h| {
        h.intercept_active = true;
        h.intercept_current = Some((f.clone(), args.to_vec()));
        h.intercept_proceeded = false;
        h.set_intercept_context(name, args, &full_cmd);
    });

    // `before` advice.
    for advice in matching
        .iter()
        .filter(|i| matches!(i.kind, AdviceKind::Before))
    {
        if let Err(e) = eval_advice(&advice.code) {
            with_host(intercept_finish);
            return Err(e);
        }
    }

    // `around` advice wins first-match, as in zshrs.
    let around = matching
        .iter()
        .find(|i| matches!(i.kind, AdviceKind::Around));
    let has_after = matching.iter().any(|i| matches!(i.kind, AdviceKind::After));

    let t0 = Instant::now();
    let result: Value = if let Some(advice) = around {
        // The around advice runs the original itself via `(intercept-proceed)`;
        // the intercept returns the advice form's own value (zshrs returns the
        // advice result whether or not it proceeded).
        match eval_advice(&advice.code) {
            Ok(v) => v,
            Err(e) => {
                with_host(intercept_finish);
                return Err(e);
            }
        }
    } else if !has_after {
        // Only `before` advice fired — let normal dispatch run the original.
        with_host(intercept_finish);
        return Ok(None);
    } else {
        // `after` advice with no `around`: run the original ourselves so the
        // result exists for the `after` forms. The guard makes this dispatch
        // normally (no re-trigger).
        match call_function(f, args) {
            Ok(v) => v,
            Err(e) => {
                with_host(intercept_finish);
                return Err(e);
            }
        }
    };
    let elapsed = t0.elapsed();
    with_host(|h| h.set_intercept_timing(elapsed));

    // `after` advice.
    for advice in matching
        .iter()
        .filter(|i| matches!(i.kind, AdviceKind::After))
    {
        if let Err(e) = eval_advice(&advice.code) {
            with_host(intercept_finish);
            return Err(e);
        }
    }

    with_host(intercept_finish);
    Ok(Some(result))
}

/// `(intercept-proceed)` — called from `around` advice to run the original
/// function. VM-re-entrant, so it is dispatched as an intrinsic from
/// [`crate::host::call_function`] (not a plain subr, which would hold a host
/// borrow and deadlock the nested call). Mirrors zshrs `builtin_intercept_proceed`.
pub fn intrinsic_intercept_proceed() -> Result<Value, String> {
    let ctx = with_host(|h| {
        h.intercept_proceeded = true;
        h.intercept_current.clone()
    });
    match ctx {
        Some((f, args)) => call_function(&f, &args),
        // Called outside an around advice — nothing to proceed to.
        None => Ok(Value::Undef),
    }
}

// ── user-facing subrs ──────────────────────────────────────────────────────────

/// `(intercept KIND PATTERN FORM)` — register AOP advice. KIND is `before`,
/// `after`, or `around` (symbol or string); PATTERN is a glob string; FORM is the
/// (quoted) advice form. Returns the new intercept's integer ID.
fn intercept_subr(h: &mut ElispHost, args: &[Value]) -> Result<Value, String> {
    let kind_name = h
        .sym_name(&args[0])
        .or_else(|| match &args[0] {
            Value::Str(s) => Some(s.to_string()),
            _ => None,
        })
        .ok_or("intercept: KIND must be a symbol or string")?;
    let kind = match kind_name.as_str() {
        "before" => AdviceKind::Before,
        "after" => AdviceKind::After,
        "around" => AdviceKind::Around,
        other => {
            return Err(format!(
                "intercept: unknown advice kind `{other}` (use before, after, or around)"
            ))
        }
    };
    let pattern = match &args[1] {
        Value::Str(s) => s.to_string(),
        _ => return Err("intercept: PATTERN must be a string".to_string()),
    };
    let code = args[2].clone();
    let id = h.intercepts.iter().map(|i| i.id).max().unwrap_or(0) + 1;
    h.intercepts.push(Intercept {
        pattern,
        kind,
        code,
        id,
    });
    Ok(Value::Int(id as i64))
}

/// `(intercept-list)` — return the registered intercepts as a list of
/// `(ID KIND PATTERN FORM)` entries (newest registration last), or nil if none.
fn intercept_list_subr(h: &mut ElispHost, _args: &[Value]) -> Result<Value, String> {
    let items = h.intercepts.clone();
    let mut out = Vec::with_capacity(items.len());
    for i in &items {
        let kind = h.intern(i.kind.as_str());
        let entry = h.list_from(vec![
            Value::Int(i.id as i64),
            kind,
            Value::str(i.pattern.clone()),
            i.code.clone(),
        ]);
        out.push(entry);
    }
    Ok(h.list_from(out))
}

/// `(intercept-remove ID)` — remove the intercept with the given integer ID.
/// Returns t if one was removed, nil otherwise.
fn intercept_remove_subr(h: &mut ElispHost, args: &[Value]) -> Result<Value, String> {
    let id = match &args[0] {
        Value::Int(n) => *n as u32,
        _ => return Err("intercept-remove: ID must be an integer".to_string()),
    };
    let before = h.intercepts.len();
    h.intercepts.retain(|i| i.id != id);
    Ok(if h.intercepts.len() < before {
        Value::Bool(true)
    } else {
        Value::Undef
    })
}

/// `(intercept-clear)` — remove all intercepts. Returns the count removed.
fn intercept_clear_subr(h: &mut ElispHost, _args: &[Value]) -> Result<Value, String> {
    let count = h.intercepts.len();
    h.intercepts.clear();
    Ok(Value::Int(count as i64))
}

/// Register the intercept subrs and mark the AOP context variables special so
/// free references in advice forms compile to dynamic reads. Called from
/// `builtins::install`. `intercept-proceed` is intentionally *not* a subr — it is
/// dispatched as an intrinsic in `call_function` because it re-enters the VM.
pub fn install(h: &mut ElispHost) {
    h.defsubr("intercept", 3, Some(3), intercept_subr);
    h.defsubr("intercept-list", 0, Some(0), intercept_list_subr);
    h.defsubr("intercept-remove", 1, Some(1), intercept_remove_subr);
    h.defsubr("intercept-clear", 0, Some(0), intercept_clear_subr);
    for v in [
        "intercept-name",
        "intercept-args",
        "intercept-cmd",
        "intercept-ms",
        "intercept-us",
    ] {
        let sym = h.intern(v);
        h.set_special(&sym);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── intercept_matches (ported verbatim from zshrs) ──

    #[test]
    fn star_matches_anything() {
        assert!(intercept_matches("*", "anything", "anything --here"));
        assert!(intercept_matches("*", "", ""));
    }

    #[test]
    fn all_matches_anything() {
        assert!(intercept_matches("all", "ls", "ls -la"));
        assert!(intercept_matches("all", "git", "git status"));
    }

    #[test]
    fn exact_match_on_cmd_name() {
        assert!(intercept_matches("git", "git", "git push"));
        assert!(intercept_matches("ls", "ls", "ls -la"));
    }

    #[test]
    fn exact_pattern_does_not_match_different_name() {
        assert!(!intercept_matches("git", "svn", "svn diff"));
        assert!(!intercept_matches("ls", "lsof", "lsof -p 1"));
    }

    #[test]
    fn glob_star_matches_prefix() {
        // "git *" should match the full command line like "git push origin".
        assert!(intercept_matches("git *", "git", "git push origin"));
    }

    #[test]
    fn glob_star_underscore_prefix_matches_completion_funcs() {
        // "_*" is the canonical zsh pattern for completion functions.
        assert!(intercept_matches("_*", "_files", "_files"));
        assert!(intercept_matches("_*", "_describe", "_describe"));
    }

    #[test]
    fn glob_star_does_not_match_non_prefix() {
        assert!(!intercept_matches("_*", "files", "files"));
    }

    #[test]
    fn question_mark_glob_matches_single_char() {
        assert!(intercept_matches("l?", "ls", "ls"));
        assert!(!intercept_matches("l?", "lsof", "lsof"));
    }

    #[test]
    fn unmatched_pattern_without_glob_chars_returns_false() {
        assert!(!intercept_matches("nope", "git", "git push"));
    }

    #[test]
    fn invalid_glob_pattern_returns_false() {
        // `[` with no `*`/`?` never reaches glob parsing (no glob chars), so it
        // falls through to `false`.
        assert!(!intercept_matches("[invalid", "git", "git push"));
        // With a `*` present the class IS parsed; an unterminated `[` errors and
        // `intercept_matches` falls through to false (matching glob::Pattern::new).
        assert!(!intercept_matches("a*[bad", "axbad", "axbad"));
    }

    #[test]
    fn empty_pattern_does_not_match_non_empty_cmd() {
        assert!(!intercept_matches("", "ls", "ls -la"));
    }

    #[test]
    fn empty_pattern_matches_empty_cmd_exactly() {
        assert!(intercept_matches("", "", ""));
    }

    // ── elisp-specific glob shapes (function names, not shell words) ──

    #[test]
    fn glob_star_matches_elisp_function_prefix() {
        assert!(intercept_matches(
            "forward-*",
            "forward-char",
            "forward-char"
        ));
        assert!(intercept_matches(
            "forward-*",
            "forward-word",
            "forward-word 2"
        ));
        assert!(!intercept_matches(
            "forward-*",
            "backward-char",
            "backward-char"
        ));
    }

    #[test]
    fn glob_mid_and_suffix_stars() {
        assert!(intercept_matches("*-mode", "text-mode", "text-mode"));
        assert!(intercept_matches(
            "save-*-excursion",
            "save-window-excursion",
            "x"
        ));
        assert!(!intercept_matches("*-mode", "mode-line", "mode-line"));
    }

    #[test]
    fn char_class_matches() {
        // Faithful to zshrs: a `[...]` class only activates when the pattern also
        // contains `*` or `?` (those are the sole triggers for glob parsing), so a
        // `?` accompanies the class here.
        assert!(intercept_matches("cadr?", "cadr1", "cadr1"));
        assert!(intercept_matches("[cm]a?", "car", "car"));
        assert!(intercept_matches("[cm]a?", "mar", "mar"));
        assert!(!intercept_matches("[cm]a?", "bar", "bar"));
        assert!(intercept_matches("[!x]a?", "car", "car"));
        assert!(!intercept_matches("[!x]a?", "xar", "xar"));
        // A bracket class with no `*`/`?` is treated literally (never globbed).
        assert!(!intercept_matches("[cm]ar", "car", "car"));
    }

    #[test]
    fn char_class_range_matches() {
        assert!(intercept_matches("f[a-z]?", "foo", "foo"));
        assert!(!intercept_matches("f[a-z]?", "f0o", "f0o"));
    }

    // ── data structures ──

    #[test]
    fn advice_kind_as_str_round_trips() {
        assert_eq!(AdviceKind::Before.as_str(), "before");
        assert_eq!(AdviceKind::After.as_str(), "after");
        assert_eq!(AdviceKind::Around.as_str(), "around");
    }

    #[test]
    fn intercept_struct_clone_preserves_fields() {
        let i = Intercept {
            pattern: "forward-*".into(),
            kind: AdviceKind::Before,
            code: Value::Undef,
            id: 42,
        };
        let c = i.clone();
        assert_eq!(c.pattern, "forward-*");
        assert!(matches!(c.kind, AdviceKind::Before));
        assert_eq!(c.id, 42);
    }
}
