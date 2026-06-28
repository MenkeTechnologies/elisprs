package com.menketechnologies.elisprs

import com.intellij.openapi.editor.DefaultLanguageHighlighterColors as Defaults
import com.intellij.openapi.editor.HighlighterColors
import com.intellij.openapi.editor.colors.TextAttributesKey

/**
 * Stable, plugin-owned [TextAttributesKey]s for every Emacs Lisp token category.
 * Each key inherits a sensible default but lives in its own `ELISPRS_*`
 * namespace so users can rebind any of them in
 * *Settings → Editor → Color Scheme → elisprs* without affecting the rest of
 * the IDE.
 */
object ElisprsColors {
    @JvmField val COMMENT = mk("ELISPRS_COMMENT", Defaults.LINE_COMMENT)
    @JvmField val SHEBANG = mk("ELISPRS_SHEBANG", Defaults.LINE_COMMENT)
    @JvmField val STRING_DQ = mk("ELISPRS_STRING_DQ", Defaults.STRING)
    @JvmField val STRING_SQ = mk("ELISPRS_STRING_SQ", Defaults.STRING)
    @JvmField val NUMBER = mk("ELISPRS_NUMBER", Defaults.NUMBER)

    @JvmField val KEYWORD = mk("ELISPRS_KEYWORD", Defaults.KEYWORD)
    // Ex commands get their own slot (defaults to METADATA) so `set`,
    // `autocmd`, `nnoremap`, `highlight`, `syntax` visually separate
    // from control-flow keywords.
    @JvmField val COMMAND = mk("ELISPRS_COMMAND", Defaults.METADATA)
    @JvmField val BUILTIN_FUNCTION = mk("ELISPRS_BUILTIN_FUNCTION", Defaults.STATIC_METHOD)
    @JvmField val FUNCTION_DECL = mk("ELISPRS_FUNCTION_DECL", Defaults.FUNCTION_DECLARATION)
    @JvmField val IDENTIFIER = mk("ELISPRS_IDENTIFIER", Defaults.IDENTIFIER)

    @JvmField val SCOPE_VAR = mk("ELISPRS_SCOPE_VAR", Defaults.GLOBAL_VARIABLE)
    @JvmField val SPECIAL_VAR = mk("ELISPRS_SPECIAL_VAR", Defaults.PREDEFINED_SYMBOL)
    @JvmField val OPTION = mk("ELISPRS_OPTION", Defaults.CONSTANT)
    @JvmField val ENV_VAR = mk("ELISPRS_ENV_VAR", Defaults.GLOBAL_VARIABLE)
    @JvmField val REGISTER = mk("ELISPRS_REGISTER", Defaults.INSTANCE_FIELD)

    @JvmField val OPERATOR = mk("ELISPRS_OPERATOR", Defaults.OPERATION_SIGN)
    @JvmField val ASSIGN_OP = mk("ELISPRS_ASSIGN_OP", Defaults.OPERATION_SIGN)
    @JvmField val BAR = mk("ELISPRS_BAR", Defaults.LABEL)
    @JvmField val LINE_CONTINUATION = mk("ELISPRS_LINE_CONTINUATION", Defaults.OPERATION_SIGN)

    @JvmField val PAREN = mk("ELISPRS_PAREN", Defaults.PARENTHESES)
    @JvmField val BRACE = mk("ELISPRS_BRACE", Defaults.BRACES)
    @JvmField val BRACKET = mk("ELISPRS_BRACKET", Defaults.BRACKETS)
    @JvmField val COMMA = mk("ELISPRS_COMMA", Defaults.COMMA)

    @JvmField val BAD_CHAR = mk("ELISPRS_BAD_CHAR", HighlighterColors.BAD_CHARACTER)

    private fun mk(name: String, fallback: TextAttributesKey): TextAttributesKey =
        TextAttributesKey.createTextAttributesKey(name, fallback)
}
