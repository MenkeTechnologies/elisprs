package com.menketechnologies.elisprs

import com.intellij.lexer.LexerBase
import com.intellij.psi.TokenType
import com.intellij.psi.tree.IElementType

/**
 * Hand-rolled Emacs Lisp lexer. Recognizes:
 *
 *   * `;` line comments (run to end-of-line)
 *   * `"..."` strings with backslash escapes
 *   * `?c` / `?\n` character literals
 *   * numbers — decimal ints, floats (`1.5`, `2e-3`), and `#x`/`#o`/`#b` radix
 *   * `(` `)` parens and `[` `]` vector brackets (paired by the brace matcher)
 *   * the reader prefixes `'` (quote), `` ` `` (backquote), `,` / `,@`
 *     (unquote), and `#'` (function quote)
 *   * `:keyword` symbols and the `t` / `nil` constants
 *   * special forms (`defun`/`let`/`if`/`lambda`/…) as keywords, with the
 *     name after a `def…` form lexed as a declaration
 *   * core subrs (`car`/`cons`/`mapcar`/`+`/…) when in call-head position
 *
 * The LSP server (`elisp --lsp`) overlays semantic tokens; this lexer gives
 * instant feedback before the LSP turn-around. (Several VimL-era token slots in
 * [ElisprsTokenTypes] are unused by elisp and simply never emitted.)
 */
class ElisprsLexer : LexerBase() {
    private var buf: CharSequence = ""
    private var endOffset = 0
    private var pos = 0
    private var tokenStart = 0
    private var tokenEnd = 0
    private var tokenType: IElementType? = null
    private var state = 0

    /// Set after `(`; the next symbol is in call-head position, so a special
    /// form colors as KEYWORD and a known subr as BUILTIN_FUNCTION. Cleared
    /// once any non-whitespace token is consumed.
    private var afterOpen = false

    /// Set after a `def…` form keyword; the next symbol is the declared name
    /// and colors as FUNCTION_DECL. Cleared once that name is consumed.
    private var expectName = false

    override fun start(buffer: CharSequence, startOffset: Int, endOffset: Int, initialState: Int) {
        buf = buffer
        this.endOffset = endOffset
        pos = startOffset
        state = initialState
        afterOpen = false
        expectName = false
        advance()
    }

    override fun getState(): Int = state
    override fun getTokenType(): IElementType? = tokenType
    override fun getTokenStart(): Int = tokenStart
    override fun getTokenEnd(): Int = tokenEnd
    override fun getBufferSequence(): CharSequence = buf
    override fun getBufferEnd(): Int = endOffset

    override fun advance() {
        tokenStart = pos
        if (pos >= endOffset) {
            tokenType = null
            tokenEnd = pos
            return
        }
        val c = buf[pos]
        when {
            c == ';' -> consumeLineComment()
            c == ' ' || c == '\t' || c == '\n' || c == '\r' -> consumeWhitespace()
            c == '"' -> consumeString()
            c == '?' -> consumeCharLiteral()
            c == '(' -> { emit(1, ElisprsTokenTypes.LPAREN); afterOpen = true }
            c == ')' -> emit(1, ElisprsTokenTypes.RPAREN)
            c == '[' -> emit(1, ElisprsTokenTypes.LBRACKET)
            c == ']' -> emit(1, ElisprsTokenTypes.RBRACKET)
            c == '\'' || c == '`' -> emit(1, ElisprsTokenTypes.OPERATOR)
            c == ',' && peek(1) == '@' -> emit(2, ElisprsTokenTypes.OPERATOR)
            c == ',' -> emit(1, ElisprsTokenTypes.OPERATOR)
            c == '#' && peek(1) == '\'' -> emit(2, ElisprsTokenTypes.OPERATOR)
            c == '#' && (peek(1) == 'x' || peek(1) == 'X' || peek(1) == 'o' ||
                peek(1) == 'O' || peek(1) == 'b' || peek(1) == 'B') -> consumeRadixNumber()
            c.isDigit() -> consumeNumber()
            (c == '-' || c == '+') && peek(1).isDigit() -> consumeNumber()
            isSymbolChar(c) -> consumeSymbol()
            else -> emit(1, TokenType.BAD_CHARACTER)
        }
    }

    private fun peek(off: Int): Char = if (pos + off in 0 until endOffset) buf[pos + off] else ' '

    /// Emit a fixed-length token and clear the call-head flag (so only the
    /// symbol immediately after `(` is treated as a call head). The `(` case
    /// re-sets `afterOpen` after calling this.
    private fun emit(len: Int, tt: IElementType) {
        tokenEnd = (pos + len).coerceAtMost(endOffset)
        pos = tokenEnd
        tokenType = tt
        afterOpen = false
    }

    private fun consumeLineComment() {
        var p = pos
        while (p < endOffset && buf[p] != '\n') p++
        tokenEnd = p; pos = p
        tokenType = ElisprsTokenTypes.COMMENT
        afterOpen = false
    }

    private fun consumeWhitespace() {
        var p = pos
        while (p < endOffset && (buf[p] == ' ' || buf[p] == '\t' || buf[p] == '\n' || buf[p] == '\r')) p++
        tokenEnd = p; pos = p
        tokenType = TokenType.WHITE_SPACE
        // whitespace does NOT clear afterOpen — `(  car …)` still heads on car.
    }

    private fun consumeString() {
        var p = pos + 1
        while (p < endOffset) {
            val ch = buf[p]
            if (ch == '\\' && p + 1 < endOffset) { p += 2; continue }
            if (ch == '"') { p++; break }
            p++
        }
        tokenEnd = p; pos = p
        tokenType = ElisprsTokenTypes.STRING_DQ
        afterOpen = false
    }

    /// `?c`, `?\n`, `?\C-a` — a character literal. We consume `?` + one char,
    /// or `?\` + an escape char (plus following `-`/word chars for named/ctrl
    /// escapes like `\C-a`, kept loose on purpose).
    private fun consumeCharLiteral() {
        var p = pos + 1
        if (p < endOffset && buf[p] == '\\') {
            p++
            if (p < endOffset) p++
            while (p < endOffset && (buf[p] == '-' || buf[p].isLetterOrDigit())) p++
        } else if (p < endOffset) {
            p++
        }
        tokenEnd = p; pos = p
        tokenType = ElisprsTokenTypes.NUMBER
        afterOpen = false
    }

    private fun consumeRadixNumber() {
        var p = pos + 2 // past `#x`/`#o`/`#b`
        while (p < endOffset && buf[p].isLetterOrDigit()) p++
        tokenEnd = p; pos = p
        tokenType = ElisprsTokenTypes.NUMBER
        afterOpen = false
    }

    private fun consumeNumber() {
        var p = pos
        if (buf[p] == '-' || buf[p] == '+') p++
        val digitsStart = p
        while (p < endOffset && buf[p].isDigit()) p++
        if (p < endOffset && buf[p] == '.' && p + 1 < endOffset && buf[p + 1].isDigit()) {
            p++
            while (p < endOffset && buf[p].isDigit()) p++
        }
        if (p < endOffset && (buf[p] == 'e' || buf[p] == 'E')) {
            var q = p + 1
            if (q < endOffset && (buf[q] == '+' || buf[q] == '-')) q++
            if (q < endOffset && buf[q].isDigit()) {
                p = q
                while (p < endOffset && buf[p].isDigit()) p++
            }
        }
        // A lone `-`/`+` with no digits is the subtraction/addition subr, not a
        // number — fall back to symbol lexing.
        if (p == digitsStart) {
            consumeSymbol()
            return
        }
        tokenEnd = p; pos = p
        tokenType = ElisprsTokenTypes.NUMBER
        afterOpen = false
    }

    private fun consumeSymbol() {
        val head = afterOpen
        var p = pos
        while (p < endOffset && isSymbolChar(buf[p])) p++
        if (p == pos) p++ // never stall
        val word = buf.subSequence(pos, p).toString()
        tokenEnd = p; pos = p
        afterOpen = false
        tokenType = classify(word, head)
    }

    private fun classify(word: String, headPosition: Boolean): IElementType {
        if (expectName) {
            expectName = false
            return ElisprsTokenTypes.FUNCTION_DECL
        }
        if (word == "t" || word == "nil") return ElisprsTokenTypes.SPECIAL_VAR
        if (word.startsWith(':')) return ElisprsTokenTypes.SPECIAL_VAR
        if (word in SPECIAL_FORMS) {
            if (word in DEFINING_FORMS) expectName = true
            return ElisprsTokenTypes.KEYWORD
        }
        if (headPosition && word in BUILTIN_FUNCTIONS) return ElisprsTokenTypes.BUILTIN_FUNCTION
        return ElisprsTokenTypes.IDENTIFIER
    }

    companion object {
        const val STATE_NORMAL = 0

        /// Symbol constituents: anything that isn't whitespace, a paren/bracket,
        /// a string quote, a comment `;`, or a reader prefix.
        private fun isSymbolChar(c: Char): Boolean = when (c) {
            ' ', '\t', '\n', '\r', '(', ')', '[', ']', '"', ';', '\'', '`', ',' -> false
            else -> true
        }

        /// Special forms + the ubiquitous defining macros (treated as keywords).
        private val SPECIAL_FORMS = setOf(
            "quote", "function", "lambda",
            "if", "when", "unless", "cond", "and", "or", "not",
            "while", "dolist", "dotimes",
            "progn", "prog1", "prog2",
            "let", "let*", "letrec",
            "setq", "setq-default", "setf", "push", "pop",
            "defun", "defmacro", "defvar", "defconst", "defcustom",
            "defsubst", "defalias", "defgroup", "defface", "define-minor-mode",
            "condition-case", "unwind-protect", "catch", "throw",
            "save-excursion", "save-restriction", "save-match-data",
            "with-current-buffer", "with-temp-buffer", "ignore-errors",
            "interactive", "declare", "require", "provide",
            "cl-defun", "cl-defmacro", "cl-let", "cl-loop", "cl-case",
        )

        /// The `def…` forms whose next symbol is the declared name.
        private val DEFINING_FORMS = setOf(
            "defun", "defmacro", "defvar", "defconst", "defcustom",
            "defsubst", "defalias", "defgroup", "defface", "define-minor-mode",
            "cl-defun", "cl-defmacro",
        )

        /// Core subrs — colored only in call-head position. A representative
        /// canonical subset; the LSP refines the rest via semantic tokens.
        private val BUILTIN_FUNCTIONS = setOf(
            "car", "cdr", "cons", "list", "append", "length", "nth", "nthcdr",
            "reverse", "member", "memq", "assoc", "assq", "mapcar", "mapc",
            "mapconcat", "nconc", "delete", "delq", "remove", "last", "elt",
            "eq", "eql", "equal", "null", "atom", "consp", "listp", "symbolp",
            "stringp", "numberp", "integerp", "floatp", "vectorp", "functionp",
            "zerop", "boundp", "fboundp",
            "+", "-", "*", "/", "%", "mod", "=", "/=", "<", ">", "<=", ">=",
            "1+", "1-", "abs", "max", "min", "expt", "sqrt", "float", "truncate",
            "message", "format", "princ", "prin1", "print", "error", "concat",
            "symbol-name", "symbol-value", "intern", "make-symbol", "set",
            "setcar", "setcdr", "funcall", "apply", "identity", "vector",
            "make-vector", "aref", "aset",
            "string=", "string<", "string-equal", "string-match", "substring",
            "split-string", "number-to-string", "string-to-number", "upcase",
            "downcase",
        )
    }
}
