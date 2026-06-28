package com.menketechnologies.elisprs

import com.intellij.psi.tree.IElementType

class ElisprsTokenType(debugName: String) : IElementType(debugName, ElisprsLanguage)

/**
 * Fine-grained Emacs Lisp (Emacs Lisp) token types. Each maps to its own
 * [ElisprsColors] entry so any category can be recolored independently.
 * When adding a token here, also add the matching case in
 * [ElisprsSyntaxHighlighter.getTokenHighlights] and the matching entry in
 * [ElisprsColorSettingsPage.attrs].
 */
object ElisprsTokenTypes {
    // ── Trivia / literals ──────────────────────────────────────────────
    /// `"` in command position runs to end-of-line — the classic Emacs Lisp
    /// comment form. `#!` on line 1 is also a comment (shebang).
    @JvmField val COMMENT = ElisprsTokenType("ELISPRS_COMMENT")
    @JvmField val SHEBANG = ElisprsTokenType("ELISPRS_SHEBANG")
    /// `"..."` double-quoted string (in expression position) — honors
    /// backslash escapes (`\n`, `\t`, `\"`, `\<Esc>`).
    @JvmField val STRING_DQ = ElisprsTokenType("ELISPRS_STRING_DQ")
    /// `'...'` literal string — only `''` escapes a single quote;
    /// backslashes are literal.
    @JvmField val STRING_SQ = ElisprsTokenType("ELISPRS_STRING_SQ")
    @JvmField val NUMBER = ElisprsTokenType("ELISPRS_NUMBER")

    // ── Keywords / commands ────────────────────────────────────────────
    /// Statement / control keywords: `if` / `endif` / `while` /
    /// `function` / `let` / `call` / `echo` / `try` / `return` etc.
    @JvmField val KEYWORD = ElisprsTokenType("ELISPRS_KEYWORD")
    /// Common ex commands with their own color slot: `set` / `autocmd` /
    /// `nnoremap` / `highlight` / `syntax` / `source` / `silent` etc.
    @JvmField val COMMAND = ElisprsTokenType("ELISPRS_COMMAND")
    /// Built-in function name — only colored when immediately followed
    /// by `(`: `len(`, `has(`, `printf(`, `substitute(`.
    @JvmField val BUILTIN_FUNCTION = ElisprsTokenType("ELISPRS_BUILTIN_FUNCTION")
    /// User function declaration / autoload call name: `Foo` after
    /// `function`, or `plug#begin(` autoload-style names.
    @JvmField val FUNCTION_DECL = ElisprsTokenType("ELISPRS_FUNCTION_DECL")
    @JvmField val IDENTIFIER = ElisprsTokenType("ELISPRS_IDENTIFIER")

    // ── Variables ──────────────────────────────────────────────────────
    /// Scope-prefixed name — `g:` `s:` `b:` `w:` `t:` `l:` `a:` `v:`
    /// followed by an identifier, lexed as ONE token (`g:loaded_foo`).
    @JvmField val SCOPE_VAR = ElisprsTokenType("ELISPRS_SCOPE_VAR")
    /// Predefined `v:` variables — `v:true` / `v:false` / `v:null` /
    /// `v:count` / `v:val` / `v:shell_error` etc.
    @JvmField val SPECIAL_VAR = ElisprsTokenType("ELISPRS_SPECIAL_VAR")
    /// `&name` option reference (and `&l:name` / `&g:name`).
    @JvmField val OPTION = ElisprsTokenType("ELISPRS_OPTION")
    /// `$NAME` environment-variable reference.
    @JvmField val ENV_VAR = ElisprsTokenType("ELISPRS_ENV_VAR")
    /// `@x` register reference (`@"`, `@a`, `@+`).
    @JvmField val REGISTER = ElisprsTokenType("ELISPRS_REGISTER")

    // ── Operators ──────────────────────────────────────────────────────
    @JvmField val OPERATOR = ElisprsTokenType("ELISPRS_OPERATOR")
    /// `=` `+=` `-=` `*=` `/=` `%=` `.=` `..=`.
    @JvmField val ASSIGN_OP = ElisprsTokenType("ELISPRS_ASSIGN_OP")
    /// `|` — the command separator / bar.
    @JvmField val BAR = ElisprsTokenType("ELISPRS_BAR")
    /// `\` at the start of a continued line (`:h line-continuation`).
    @JvmField val LINE_CONTINUATION = ElisprsTokenType("ELISPRS_LINE_CONTINUATION")

    // ── Punctuation ────────────────────────────────────────────────────
    // Split L/R variants so `lang.braceMatcher` can pair them; the umbrella
    // `PAREN`/`BRACE`/`BRACKET` names stay for the color slot fallback.
    @JvmField val PAREN = ElisprsTokenType("ELISPRS_PAREN")
    @JvmField val LPAREN = ElisprsTokenType("ELISPRS_LPAREN")
    @JvmField val RPAREN = ElisprsTokenType("ELISPRS_RPAREN")
    @JvmField val BRACE = ElisprsTokenType("ELISPRS_BRACE")
    @JvmField val LBRACE = ElisprsTokenType("ELISPRS_LBRACE")
    @JvmField val RBRACE = ElisprsTokenType("ELISPRS_RBRACE")
    @JvmField val BRACKET = ElisprsTokenType("ELISPRS_BRACKET")
    @JvmField val LBRACKET = ElisprsTokenType("ELISPRS_LBRACKET")
    @JvmField val RBRACKET = ElisprsTokenType("ELISPRS_RBRACKET")
    @JvmField val COMMA = ElisprsTokenType("ELISPRS_COMMA")

    // ── Errors ─────────────────────────────────────────────────────────
    @JvmField val BAD = ElisprsTokenType("ELISPRS_BAD")
}
