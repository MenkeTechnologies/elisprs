//! Offline generator for `docs/reference.html` — the full `elisp --lsp` doc
//! corpus rendered as a static HTML page using the same cyberpunk styling as
//! `docs/index.html`. Run with `cargo run --bin gen-docs` before pushing to
//! GitHub Pages.
//!
//! Source of truth: `elisprs::lsp::SPECIAL_FORMS` and `elisprs::lsp::SUBRS`
//! (the exact `Entry` table that drives LSP hover / completion / signature
//! help). Every builtin registered in `builtins::install` has an `Entry`, so
//! this page covers the whole language surface. Entries are grouped into
//! ordered chapters by `category_of`; anything not explicitly categorized
//! lands in a synthetic "Other" chapter so nothing silently vanishes.
//!
//! The markdown → HTML converter is intentionally minimal (in-house, no crate
//! dependency): it handles what the hover docs actually use — fenced `elisp`
//! code blocks, inline backticks, paragraph breaks, `###` headings, and bullet
//! lists. Anything weirder falls through as escaped text.

use std::fs;
use std::path::PathBuf;

use elisprs::lsp::{Entry, Kind, SPECIAL_FORMS, SUBRS};

fn main() {
    let out_path = PathBuf::from("docs/reference.html");
    let html = build_page();
    fs::write(&out_path, html).expect("write docs/reference.html");
    println!("wrote {}", out_path.display());
}

/// Ordered chapter list. "Special Forms" first, then the function categories
/// in a deliberate teaching order, with "Other" as the catch-all tail.
const CHAPTER_ORDER: &[&str] = &[
    "Special Forms",
    "Arithmetic & Numbers",
    "Predicates & Type Tests",
    "Cons, Lists & Sequences",
    "Symbols, Cells & Binding",
    "Strings & Characters",
    "I/O, Print & Format",
    "Control & Functional",
    "Association Lists & Hash Tables",
    "Other",
];

/// Map an `Entry` to its chapter. Special forms are grouped wholesale; every
/// function name is explicitly categorized, and any function that isn't listed
/// falls into "Other" so it is never dropped.
fn category_of(e: &Entry) -> &'static str {
    if let Kind::SpecialForm = e.kind {
        return "Special Forms";
    }
    match e.name {
        "%" | "*" | "+" | "-" | "/" | "1+" | "1-" | "<" | "<=" | "=" | ">" | ">=" | "abs"
        | "acos" | "ash" | "asin" | "atan" | "ceiling" | "copysign" | "cos" | "exp" | "expt"
        | "fceiling" | "ffloor" | "float" | "floor" | "frexp" | "fround" | "ftruncate"
        | "ldexp" | "log" | "logand" | "logb" | "logcount" | "logior" | "lognot" | "logxor"
        | "lsh" | "mod" | "random" | "round" | "sin" | "sqrt" | "string-to-number" | "tan"
        | "truncate" => "Arithmetic & Numbers",
        "atom" | "char-equal" | "char-or-string-p" | "char-uppercase-p" | "cl-struct-p"
        | "consp" | "eq" | "eql" | "equal" | "floatp" | "functionp" | "integerp" | "isnan"
        | "listp" | "macrop" | "not" | "null" | "numberp" | "recordp" | "special-form-p"
        | "special-variable-p" | "stringp" | "subrp" | "symbolp" | "type-of" | "vectorp"
        | "zerop" => "Predicates & Type Tests",
        "append" | "aref" | "aset" | "car" | "cdr" | "cons" | "fillarray" | "length" | "list"
        | "make-vector" | "nth" | "reverse" | "setcar" | "setcdr" | "string-to-list"
        | "string-to-vector" | "vconcat" | "vector" => "Cons, Lists & Sequences",
        "boundp" | "fboundp" | "fset" | "indirect-function" | "intern" | "intern-soft"
        | "make-symbol" | "makunbound" | "set" | "symbol-function" | "symbol-name"
        | "symbol-value" => "Symbols, Cells & Binding",
        "char-to-string"
        | "compare-strings"
        | "concat"
        | "downcase"
        | "make-string"
        | "match-beginning"
        | "match-data"
        | "match-end"
        | "match-string"
        | "number-to-string"
        | "regexp-quote"
        | "replace-regexp-in-string"
        | "save-match-data"
        | "set-match-data"
        | "split-string"
        | "string"
        | "string-distance"
        | "string-empty-p"
        | "string-join"
        | "string-match"
        | "string-match-p"
        | "string-prefix-p"
        | "string-search"
        | "string-suffix-p"
        | "string-to-char"
        | "substring"
        | "upcase" => "Strings & Characters",
        "--pop-output-capture--"
        | "--push-output-capture--"
        | "format"
        | "message"
        | "prin1"
        | "prin1-to-string"
        | "princ"
        | "print"
        | "read"
        | "read-from-string"
        | "terpri" => "I/O, Print & Format",
        "apply" | "error" | "eval" | "func-arity" | "funcall" | "identity" | "pcase" | "signal"
        | "subr-arity" | "throw" | "user-error" => "Control & Functional",
        "clrhash" | "copy-hash-table" | "gethash" | "hash-table-count" | "hash-table-keys"
        | "hash-table-p" | "hash-table-test" | "hash-table-values" | "make-hash-table"
        | "puthash" | "remhash" => "Association Lists & Hash Tables",
        _ => "Other",
    }
}

fn build_page() -> String {
    // Group entries into chapters, preserving the corpus (registration) order
    // within each chapter. Special forms come from SPECIAL_FORMS, functions
    // from SUBRS — the same tables the LSP serves.
    let all: Vec<&Entry> = SPECIAL_FORMS.iter().chain(SUBRS).collect();
    let mut chapters: Vec<(&str, Vec<&Entry>)> = CHAPTER_ORDER
        .iter()
        .map(|&c| (c, Vec::<&Entry>::new()))
        .collect();
    for e in &all {
        let cat = category_of(e);
        let slot = chapters
            .iter_mut()
            .find(|(c, _)| *c == cat)
            .expect("category in CHAPTER_ORDER");
        slot.1.push(e);
    }
    chapters.retain(|(_, rows)| !rows.is_empty());

    let total_entries: usize = chapters.iter().map(|(_, r)| r.len()).sum();
    let chapter_count = chapters.len();

    // ── render ──────────────────────────────────────────────────────────
    let mut out = String::with_capacity(1_000_000);
    out.push_str(HEAD);
    out.push_str(&format!(
        r#"  <header class="tutorial-header">
    <div class="tutorial-header-inner">
      <div>
        <h1 class="tutorial-brand">// ELISPRS — FULL REFERENCE</h1>
        <nav class="tutorial-crumbs" aria-label="Breadcrumb">
          <a href="index.html">Docs</a>
          <span class="sep">/</span>
          <span class="current">Reference</span>
          <span class="sep">/</span>
          <a href="https://github.com/MenkeTechnologies/elisprs" target="_blank" rel="noopener noreferrer">GitHub</a>
        </nav>
        <p class="docs-build-line">elisprs v{version} · {total_entries} builtins &amp; special forms · {chapter_count} chapters · generated from <code>elisprs/lsp.rs</code></p>
      </div>
      <div class="tutorial-toolbar">
        <button type="button" class="btn btn-secondary" id="btnTheme" title="Toggle light/dark">Theme</button>
        <button type="button" class="btn btn-secondary active" id="btnCrt" title="CRT scanline overlay">CRT</button>
        <button type="button" class="btn btn-secondary active" id="btnNeon" title="Neon border pulse">Neon</button>
        <a class="btn btn-secondary" href="index.html">Hub</a>
        <a class="btn btn-secondary" href="https://github.com/MenkeTechnologies/elisprs" target="_blank" rel="noopener noreferrer">GitHub</a>
      </div>
    </div>
  </header>

  <div class="hub-scheme-strip">
    <div class="hub-scheme-strip-inner">
      <span class="hud-scheme-label">// Color scheme</span>
      <div class="scheme-grid" id="hudSchemeGrid"></div>
    </div>
  </div>

  <main class="tutorial-main">
    <h2 class="tutorial-title"><span class="step-hash">&gt;_</span>LANGUAGE REFERENCE</h2>
    <p class="tutorial-subtitle">Every special form and builtin subr with an LSP hover doc — rendered from the exact metadata that <code>elisp --lsp</code> serves on hover and completion. Jump via the chapter index, or <kbd>Ctrl+F</kbd> for a specific name.</p>
"#,
        version = env!("CARGO_PKG_VERSION"),
        total_entries = total_entries,
        chapter_count = chapter_count,
    ));

    // Chapter index
    out.push_str(
        r#"    <section class="tutorial-section">
      <h2>Chapters</h2>
      <ul class="chapter-index">
"#,
    );
    for (chapter, rows) in &chapters {
        let slug = slugify(chapter);
        out.push_str(&format!(
            "        <li><a href=\"#ch-{slug}\">{chapter}</a> <span class=\"chapter-count\">{n}</span></li>\n",
            slug = slug,
            chapter = html_escape(chapter),
            n = rows.len(),
        ));
    }
    out.push_str("      </ul>\n    </section>\n");

    // Chapters and their entries
    for (chapter, rows) in &chapters {
        let slug = slugify(chapter);
        out.push_str(&format!(
            r#"    <section class="tutorial-section" id="ch-{slug}">
      <h2>{chapter}</h2>
      <p class="chapter-meta">{n} entries</p>
"#,
            slug = slug,
            chapter = html_escape(chapter),
            n = rows.len(),
        ));
        for e in rows {
            let topic_slug = slugify(e.name);
            let topic_escaped = html_escape(e.name);
            let md = format!("```elisp\n{}\n```\n\n{}", e.sig, e.doc);
            out.push_str("      <article class=\"doc-entry\" id=\"doc-");
            out.push_str(&topic_slug);
            out.push_str("\">\n        <h3><a class=\"doc-anchor\" href=\"#doc-");
            out.push_str(&topic_slug);
            out.push_str("\">#</a> <code>");
            out.push_str(&topic_escaped);
            out.push_str("</code></h3>\n");
            out.push_str(&markdown_to_html(&md));
            out.push_str("      </article>\n");
        }
        out.push_str("    </section>\n");
    }

    out.push_str(FOOT);
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Minimal markdown → HTML converter. Scope: what the hover corpus actually
// uses. Blocks: fenced code, `### heading`, blank-line-separated paragraphs,
// `-`/`*` bullet lists. Inlines: `backtick code`. Everything else is HTML-
// escaped and passes through as plain text.
// ─────────────────────────────────────────────────────────────────────────
fn markdown_to_html(md: &str) -> String {
    let mut out = String::with_capacity(md.len() + md.len() / 4);
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Fenced code block: ```LANG … ```
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            let lang = rest.trim().to_string();
            let lang_attr = if lang.is_empty() {
                String::new()
            } else {
                format!(" class=\"lang-{}\"", html_escape(&lang))
            };
            out.push_str(&format!("        <pre><code{lang_attr}>"));
            i += 1;
            while i < lines.len() {
                let l = lines[i];
                if l.trim_start().starts_with("```") {
                    i += 1;
                    break;
                }
                out.push_str(&html_escape(l));
                out.push('\n');
                i += 1;
            }
            out.push_str("</code></pre>\n");
            continue;
        }

        // Heading: `###` (the only level the corpus uses).
        if let Some(body) = line.strip_prefix("### ") {
            out.push_str(&format!("        <h4>{}</h4>\n", inline(body)));
            i += 1;
            continue;
        }
        if let Some(body) = line.strip_prefix("## ") {
            out.push_str(&format!("        <h4>{}</h4>\n", inline(body)));
            i += 1;
            continue;
        }

        // Bullet list.
        if line.trim_start().starts_with("- ") || line.trim_start().starts_with("* ") {
            out.push_str("        <ul>\n");
            while i < lines.len() {
                let l = lines[i];
                let t = l.trim_start();
                let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) else {
                    break;
                };
                out.push_str(&format!("          <li>{}</li>\n", inline(item)));
                i += 1;
            }
            out.push_str("        </ul>\n");
            continue;
        }

        // Blank line → paragraph boundary.
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Paragraph: accumulate contiguous non-blank, non-block lines.
        let mut para = String::new();
        while i < lines.len() {
            let l = lines[i];
            let t = l.trim_start();
            if l.trim().is_empty()
                || t.starts_with("```")
                || t.starts_with("### ")
                || t.starts_with("## ")
                || t.starts_with("- ")
                || t.starts_with("* ")
            {
                break;
            }
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(l.trim());
            i += 1;
        }
        if !para.is_empty() {
            out.push_str(&format!("        <p>{}</p>\n", inline(&para)));
        }
    }
    out
}

/// Inline pass: `backtick code` spans and `**bold**` spans, otherwise
/// HTML-escape. Single `*em*` is intentionally not supported because the
/// corpus contains lisp-syntax text like `*foo*` earmuffs that would generate
/// false matches; bold uses doubled `**` which avoids that collision.
fn inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        if bytes[i] == b'`' {
            // Find matching backtick.
            let start = i + 1;
            let mut j = start;
            while j < s.len() && bytes[j] != b'`' {
                j += 1;
            }
            if j < s.len() {
                out.push_str("<code>");
                out.push_str(&html_escape(&s[start..j]));
                out.push_str("</code>");
                i = j + 1;
                continue;
            }
        }
        // `**bold**`. Require the closing `**` to also exist; otherwise fall
        // through and treat the literal `**` as text.
        if i + 1 < s.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            let start = i + 2;
            let mut j = start;
            while j + 1 < s.len() {
                if bytes[j] == b'*' && bytes[j + 1] == b'*' {
                    break;
                }
                j += 1;
            }
            if j + 1 < s.len() && bytes[j] == b'*' && bytes[j + 1] == b'*' {
                out.push_str("<strong>");
                out.push_str(&inline(&s[start..j]));
                out.push_str("</strong>");
                i = j + 2;
                continue;
            }
        }
        // Default: html-escape this one char.
        let c = &s[i..i + char_len(bytes, i)];
        out.push_str(&html_escape(c));
        i += c.len();
    }
    out
}

fn char_len(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    if b < 0x80 {
        1
    } else if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

const HEAD: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="color-scheme" content="dark light">
  <meta name="description" content="elisprs full reference — every special form and builtin subr with its LSP hover doc rendered as a static page.">
  <title>elisprs — Reference</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Orbitron:wght@400;600;700;900&amp;family=Share+Tech+Mono&amp;display=swap" rel="stylesheet">
  <link rel="stylesheet" href="hud-static.css">
  <link rel="stylesheet" href="tutorial.css">
  <style>
    .tutorial-main { max-width: 72rem; }
    .docs-build-line {
      margin: 0.35rem 0 0;
      font-family: 'Share Tech Mono', ui-monospace, monospace;
      font-size: 11px; color: var(--text-dim);
      letter-spacing: 0.03em; max-width: 42rem; opacity: 0.75;
    }
    .hub-scheme-strip {
      border-bottom: 1px dashed var(--border);
      background: color-mix(in srgb, var(--bg-secondary) 85%, transparent);
      padding: 0.55rem 1.5rem 0.65rem; position: relative;
    }
    .hub-scheme-strip-inner {
      max-width: 72rem; margin: 0 auto;
      display: flex; align-items: center; gap: 0.85rem;
    }
    .hub-scheme-strip .hud-scheme-label {
      flex: 0 0 auto;
      font-family: 'Orbitron', sans-serif; font-size: 9px; font-weight: 700;
      letter-spacing: 2px; text-transform: uppercase; color: var(--accent);
    }
    .hub-scheme-strip .scheme-grid {
      flex: 1 1 auto;
      display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 6px;
    }
    @media (max-width: 720px) {
      .hub-scheme-strip-inner { flex-direction: column; align-items: stretch; }
      .hub-scheme-strip .scheme-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    }

    .chapter-index {
      list-style: none; padding: 0; margin: 0;
      display: grid; grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
      gap: 0.3rem;
    }
    .chapter-index li {
      border: 1px solid var(--border); padding: 0.45rem 0.65rem; border-radius: 2px;
      background: color-mix(in srgb, var(--bg-card) 92%, transparent);
      display: flex; justify-content: space-between; align-items: baseline;
    }
    .chapter-index li a {
      color: var(--cyan); text-decoration: none; font-size: 13px;
      font-family: 'Share Tech Mono', ui-monospace, monospace;
    }
    .chapter-index li a:hover { color: var(--accent-light); }
    .chapter-count {
      font-size: 10px; color: var(--text-muted);
      font-family: 'Share Tech Mono', ui-monospace, monospace;
    }
    .chapter-meta {
      font-size: 11px; color: var(--text-muted); margin: -0.3rem 0 0.8rem;
      font-family: 'Share Tech Mono', ui-monospace, monospace;
    }

    .doc-entry {
      margin: 1rem 0 1.4rem;
      padding: 0.75rem 0.9rem 0.5rem;
      border-left: 2px solid var(--cyan);
      background: color-mix(in srgb, var(--bg) 94%, transparent);
      border-radius: 2px;
    }
    .doc-entry h3 {
      margin: 0 0 0.45rem;
      font-family: 'Orbitron', sans-serif;
      font-size: 13px; font-weight: 700; letter-spacing: 1.5px;
      text-transform: uppercase; color: var(--cyan);
    }
    .doc-entry h3 code {
      color: var(--accent-light); background: transparent; border: none;
      padding: 0; font-size: 1em; letter-spacing: 0.5px;
    }
    .doc-entry .doc-anchor {
      color: var(--text-muted); font-size: 0.85em; margin-right: 0.25rem;
      text-decoration: none;
    }
    .doc-entry .doc-anchor:hover { color: var(--accent); }
    .doc-entry h4 {
      font-family: 'Orbitron', sans-serif;
      font-size: 11px; font-weight: 700; letter-spacing: 1.5px;
      text-transform: uppercase; color: var(--accent-light);
      margin: 0.8rem 0 0.3rem;
    }
    .doc-entry p {
      font-size: 13px; line-height: 1.6; color: var(--text-dim);
      margin: 0.35rem 0;
    }
    .doc-entry p code, .doc-entry li code {
      color: var(--accent-light); font-size: 12px;
    }
    .doc-entry ul { margin: 0.3rem 0 0.5rem; padding-left: 1.25rem; }
    .doc-entry li { font-size: 13px; color: var(--text-dim); line-height: 1.55; margin: 0.2rem 0; }
    .doc-entry pre {
      font-family: 'Share Tech Mono', ui-monospace, monospace;
      font-size: 12px;
      background: var(--bg); border: 1px solid var(--border);
      border-radius: 2px;
      padding: 0.7rem 0.9rem; overflow-x: auto;
      color: var(--text); margin: 0.5rem 0;
      box-shadow: inset 0 0 18px rgba(0, 0, 0, 0.35);
    }
    .doc-entry pre code { color: var(--text); background: transparent; border: none; padding: 0; }
    [data-theme="light"] .doc-entry pre { box-shadow: inset 0 0 10px rgba(0, 0, 0, 0.05); }

    kbd {
      font-family: 'Share Tech Mono', ui-monospace, monospace;
      font-size: 11px;
      padding: 1px 6px;
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-bottom-width: 2px;
      border-radius: 3px;
      color: var(--cyan);
    }
  </style>
</head>
<body>
  <div class="app tutorial-app" id="docsApp">
    <div class="crt-scanline" id="crtH" aria-hidden="true"></div>
    <div class="crt-scanline-v" id="crtV" aria-hidden="true"></div>
"##;

const FOOT: &str = r#"  </main>
  </div>
  <script src="hud-theme.js"></script>
</body>
</html>
"#;
