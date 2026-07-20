//! elisp wiring for inline Rust FFI (`rust { ... }` blocks).
//!
//! A `rust { ... }` block is not valid s-expression syntax, so it is rewritten
//! at the raw source level — BEFORE the reader runs — into an ordinary elisp
//! call `(__rust-compile "<b64>" LINE)`. The reader, compiler, and VM only ever
//! see that call; it dispatches to [`fusevm::ffi`] (compile the block to a
//! cdylib, `dlopen` it, register every `pub extern "C"` export). Each exported
//! function is then callable by bareword — `(add 21 21)` — resolved in
//! [`crate::host::call_function`]'s `void-function` fallback (a user `defun`
//! still shadows it).
//!
//! The heavy lifting (scan/rewrite, compile, marshal) lives in fusevm; this
//! module only supplies the elisp-flavored [`fusevm::RustSugar`] config and the
//! [`desugar`] entry the execution path calls.

use fusevm::RustSugar;

/// Emit the elisp form a `rust { ... }` block desugars to: a call to the
/// `__rust-compile` builtin carrying the base64-encoded block body and its
/// source line. `__rust-compile` is a valid elisp symbol (hyphens allowed in
/// symbol names), and the emitted form is valid top-level elisp.
fn emit(b64: &str, line: usize) -> String {
    format!("(__rust-compile \"{b64}\" {line})")
}

/// elisp desugar config. Comments start with `;`; elisp has no block comment.
/// `newline_boundary` is `true` so a top-level `rust {` on its own line is
/// recognized: `;` line comments are skipped first, `(`/`)` set a non-boundary
/// (fine — a real FFI block sits at top level on its own lines), and the reader
/// would otherwise choke on `{` (elisp has no brace syntax), so a match only
/// ever fires on an intended FFI block.
pub const SUGAR: RustSugar = RustSugar {
    keyword: "rust",
    line_comments: &[";"],
    block_comment: None,
    newline_boundary: true,
    emit,
};

/// Rewrite every top-level `rust { ... }` block in elisp source into a
/// `(__rust-compile ...)` call, before reading. No-op (single substring scan on
/// the fast path) when the source contains no `rust` token.
pub fn desugar(src: &str) -> String {
    SUGAR.desugar(src)
}

#[cfg(test)]
mod tests {
    #[test]
    fn desugars_block_on_its_own_line() {
        let src = "rust { pub extern \"C\" fn add(a: i64, b: i64) -> i64 { a + b } }\n(message \"%d\" (add 2 3))\n";
        let out = super::desugar(src);
        assert!(out.contains("(__rust-compile "), "no builtin call: {out}");
        assert!(!out.contains("pub extern"), "Rust body leaked: {out}");
        assert!(out.contains("(add 2 3)"), "trailing form lost: {out}");
    }

    #[test]
    fn leaves_ordinary_elisp_untouched() {
        let src = "(defun sq (x) (* x x))\n(message \"%d\" (sq 5))\n";
        assert_eq!(super::desugar(src), src);
    }

    #[test]
    fn keyword_in_line_comment_not_expanded() {
        // A `;`-comment mentioning the keyword must not be desugared.
        let src = "; rust { not a block }\n(setq x 1)\n";
        assert_eq!(super::desugar(src), src);
    }
}
