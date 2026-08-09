//! Two things in elisprs are keyed by a hand-written identifier that nothing
//! else validates, and a duplicate in either one is silently absorbed:
//!
//! 1. **Extension-op IDs** (`pub mod ops` in `src/host.rs`) are hand-assigned
//!    `u16` constants. The compiler emits them and `ext_dispatch` matches on
//!    them. Two constants sharing a number make the *second* `match` arm dead
//!    code: every site that emits the later op is then executed by the earlier
//!    op's handler. `rustc` only warns (`unreachable_patterns`), and a warning
//!    in a 32k-line crate is not a gate.
//!
//! 2. **Subr names.** `ElispHost::defsubr` (`src/host.rs:1435`) ends in
//!    `set_function(name, subr)`, which overwrites the symbol's function cell
//!    with no complaint. Registering `"length"` twice leaves whichever call ran
//!    last installed and drops the other handler entirely.
//!
//! Both failure modes survive a `git merge` without a conflict marker: two
//! branches that each append a registration to a different part of the file
//! merge cleanly and the collision only exists in the merged result. The same
//! defect was found in two sibling fusevm frontends (`scalars` had
//! `MAKE_ORDERING`/`MAKE_QUEUE` both at 754; `phplang` had `INDEX_ISSET`
//! colliding with `LIST_ELEM_GET` at 105), in both cases after a merge.
//!
//! This test reads the identifiers out of the source text rather than out of
//! the compiled crate, because that is the only place they exist as a *set*:
//! `ops::*` are separate constants with no registry, and `defsubr` keeps no
//! record of what it replaced.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(name: &str) -> String {
    let p = src_dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// The body of `pub mod ops { … }` in `src/host.rs`.
fn ops_module(host: &str) -> &str {
    let start = host
        .find("pub mod ops {")
        .expect("src/host.rs no longer has `pub mod ops {`; update this test");
    let body = &host[start..];
    // The module is flat (constants only), so the first line that is a lone `}`
    // at column 0 closes it.
    let end = body
        .find("\n}\n")
        .expect("unterminated `pub mod ops` in src/host.rs");
    &body[..end]
}

/// `(NAME, VALUE, line)` for every `pub const NAME: u16 = VALUE;` in `ops`.
fn op_constants(host: &str) -> Vec<(String, u64, usize)> {
    let module = ops_module(host);
    let base = host[..host.find(module).unwrap()].lines().count();
    let mut out = Vec::new();
    for (i, line) in module.lines().enumerate() {
        let Some(rest) = line.trim().strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once(':') else {
            continue;
        };
        let Some((_ty, val)) = rest.split_once('=') else {
            continue;
        };
        // Trailing `// comment` first, then the statement's `;`.
        let val = val.split("//").next().unwrap_or(val);
        let val = val.trim().trim_end_matches(';').trim();
        let parsed = val.replace('_', "").parse::<u64>().unwrap_or_else(|e| {
            panic!("ops::{} has a non-literal value {val:?}: {e}", name.trim())
        });
        out.push((name.trim().to_string(), parsed, base + i + 1));
    }
    assert!(
        out.len() >= 12,
        "found only {} ops constants; the parser is broken, not the source",
        out.len()
    );
    out
}

/// Every subr name installed from Rust, as `(name, "file:line")`.
///
/// Two spellings reach `defsubr`: the `s("name", …)` closure in
/// `builtins::install`, and direct `h.defsubr("name", …)` calls elsewhere.
fn registered_subrs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(src_dir())
        .expect("cannot list src/")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = read(&name);
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            let arg = if let Some(r) = t.strip_prefix("s(\"") {
                r
            } else if let Some(r) = t.strip_prefix("h.defsubr(\"") {
                r
            } else if let Some(r) = t.strip_prefix("self.defsubr(\"") {
                r
            } else {
                continue;
            };
            // The subr name is a plain literal in every call site; a `\"` inside
            // one would mean a name containing a quote, which elisp has none of.
            let Some(end) = arg.find('"') else { continue };
            out.push((arg[..end].to_string(), format!("{name}:{}", i + 1)));
        }
    }
    assert!(
        out.len() >= 300,
        "found only {} subr registrations; the parser is broken, not the source",
        out.len()
    );
    out
}

/// No two `ops::*` constants share a number.
///
/// A duplicate makes the later constant's `ext_dispatch` arm unreachable, so
/// every bytecode site emitting it silently runs the earlier op's handler.
#[test]
fn extension_op_ids_are_unique() {
    let host = read("host.rs");
    let mut by_value: HashMap<u64, Vec<(String, usize)>> = HashMap::new();
    for (name, value, line) in op_constants(&host) {
        by_value.entry(value).or_default().push((name, line));
    }
    let mut clashes: Vec<String> = by_value
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(value, v)| {
            let who: Vec<String> = v
                .iter()
                .map(|(n, l)| format!("ops::{n} (host.rs:{l})"))
                .collect();
            format!("  id {value}: {}", who.join(", "))
        })
        .collect();
    clashes.sort();
    assert!(
        clashes.is_empty(),
        "extension-op IDs collide; the later arm in ext_dispatch is dead code:\n{}",
        clashes.join("\n")
    );
}

/// Constant *names* are unique too — a repeated name in the same module is a
/// hard error in rustc, but a repeated name across a merge that also renamed
/// one of them is not, and the pair would then be two spellings of one op.
#[test]
fn extension_op_names_are_unique() {
    let host = read("host.rs");
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut dups = Vec::new();
    for (name, _value, line) in op_constants(&host) {
        if let Some(first) = seen.insert(name.clone(), line) {
            dups.push(format!("  ops::{name}: host.rs:{first} and host.rs:{line}"));
        }
    }
    assert!(
        dups.is_empty(),
        "duplicate extension-op constant names:\n{}",
        dups.join("\n")
    );
}

/// Every `ops::*` constant is dispatched exactly once.
///
/// A constant with no arm is an op the compiler can emit and the VM will not
/// understand; a constant with two arms is a collision that the value check
/// above cannot see (two names, same handler intent, one of them dead).
#[test]
fn every_extension_op_has_exactly_one_dispatch_arm() {
    let host = read("host.rs");
    let dispatch = {
        let at = host
            .find("pub fn ext_dispatch(")
            .expect("src/host.rs no longer defines `ext_dispatch`; update this test");
        &host[at..]
    };
    let mut wrong = Vec::new();
    for (name, _value, line) in op_constants(&host) {
        let arms = dispatch
            .lines()
            .filter(|l| l.trim().starts_with(&format!("ops::{name} =>")))
            .count();
        if arms != 1 {
            wrong.push(format!(
                "  ops::{name} (host.rs:{line}) has {arms} dispatch arms, expected 1"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "ext_dispatch does not cover the op set one-to-one:\n{}",
        wrong.join("\n")
    );
}

/// No subr name is registered twice.
///
/// `defsubr` overwrites the function cell, so the second registration wins and
/// the first handler becomes unreachable without any diagnostic.
#[test]
fn subr_names_are_registered_once() {
    let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
    for (name, site) in registered_subrs() {
        by_name.entry(name).or_default().push(site);
    }
    let mut clashes: Vec<String> = by_name
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(name, v)| format!("  {name:?}: {}", v.join(", ")))
        .collect();
    clashes.sort();
    assert!(
        clashes.is_empty(),
        "a subr is registered more than once; only the last registration survives:\n{}",
        clashes.join("\n")
    );
}
