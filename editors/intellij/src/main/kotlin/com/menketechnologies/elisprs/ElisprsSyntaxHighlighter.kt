package com.menketechnologies.elisprs

import com.intellij.lexer.Lexer
import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterBase
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.psi.TokenType
import com.intellij.psi.tree.IElementType

class ElisprsSyntaxHighlighter : SyntaxHighlighterBase() {
    override fun getHighlightingLexer(): Lexer = ElisprsLexer()

    override fun getTokenHighlights(type: IElementType): Array<TextAttributesKey> {
        val key: TextAttributesKey? = when (type) {
            ElisprsTokenTypes.COMMENT -> ElisprsColors.COMMENT
            ElisprsTokenTypes.SHEBANG -> ElisprsColors.SHEBANG
            ElisprsTokenTypes.STRING_DQ -> ElisprsColors.STRING_DQ
            ElisprsTokenTypes.STRING_SQ -> ElisprsColors.STRING_SQ
            ElisprsTokenTypes.NUMBER -> ElisprsColors.NUMBER

            ElisprsTokenTypes.KEYWORD -> ElisprsColors.KEYWORD
            ElisprsTokenTypes.COMMAND -> ElisprsColors.COMMAND
            ElisprsTokenTypes.BUILTIN_FUNCTION -> ElisprsColors.BUILTIN_FUNCTION
            ElisprsTokenTypes.FUNCTION_DECL -> ElisprsColors.FUNCTION_DECL
            ElisprsTokenTypes.IDENTIFIER -> ElisprsColors.IDENTIFIER

            ElisprsTokenTypes.SCOPE_VAR -> ElisprsColors.SCOPE_VAR
            ElisprsTokenTypes.SPECIAL_VAR -> ElisprsColors.SPECIAL_VAR
            ElisprsTokenTypes.OPTION -> ElisprsColors.OPTION
            ElisprsTokenTypes.ENV_VAR -> ElisprsColors.ENV_VAR
            ElisprsTokenTypes.REGISTER -> ElisprsColors.REGISTER

            ElisprsTokenTypes.OPERATOR -> ElisprsColors.OPERATOR
            ElisprsTokenTypes.ASSIGN_OP -> ElisprsColors.ASSIGN_OP
            ElisprsTokenTypes.BAR -> ElisprsColors.BAR
            ElisprsTokenTypes.LINE_CONTINUATION -> ElisprsColors.LINE_CONTINUATION

            ElisprsTokenTypes.PAREN -> ElisprsColors.PAREN
            ElisprsTokenTypes.LPAREN -> ElisprsColors.PAREN
            ElisprsTokenTypes.RPAREN -> ElisprsColors.PAREN
            ElisprsTokenTypes.BRACE -> ElisprsColors.BRACE
            ElisprsTokenTypes.LBRACE -> ElisprsColors.BRACE
            ElisprsTokenTypes.RBRACE -> ElisprsColors.BRACE
            ElisprsTokenTypes.BRACKET -> ElisprsColors.BRACKET
            ElisprsTokenTypes.LBRACKET -> ElisprsColors.BRACKET
            ElisprsTokenTypes.RBRACKET -> ElisprsColors.BRACKET
            ElisprsTokenTypes.COMMA -> ElisprsColors.COMMA

            TokenType.BAD_CHARACTER -> ElisprsColors.BAD_CHAR
            else -> null
        }
        return if (key == null) emptyArray() else arrayOf(key)
    }
}

class ElisprsSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    override fun getSyntaxHighlighter(project: Project?, virtualFile: VirtualFile?): SyntaxHighlighter =
        ElisprsSyntaxHighlighter()
}
