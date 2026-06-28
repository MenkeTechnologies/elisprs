package com.menketechnologies.elisprs

import com.intellij.psi.PsiElement
import com.intellij.spellchecker.tokenizer.SpellcheckingStrategy
import com.intellij.spellchecker.tokenizer.Tokenizer

/**
 * Disable the platform's `TypoInspection` for Emacs Lisp literal-bearing tokens.
 *
 * The default platform behavior is to spell-check every string-literal /
 * comment-like token via the built-in `TextTokenizer`. In a Emacs Lisp file
 * that flags every option name, key-notation (`<C-R>`, `<leader>`), regex,
 * highlight group, and banner divider as a typo. Users see the red squiggle
 * wave on `nnoremap`, `ctermfg`, `noremap`, none of which are typos.
 *
 * Strategy: return `EMPTY_TOKENIZER` for STRING_DQ / STRING_SQ / COMMENT /
 * SHEBANG. The token still renders with its color from
 * [ElisprsSyntaxHighlighter]; only the spell-check pass is suppressed.
 *
 * Variables, identifiers, and command names are NOT suppressed — those are
 * where a real typo *would* matter (`fuction` vs `function`), and the
 * platform's word splitter already plays nicely with camel/snake.
 */
class ElisprsSpellcheckingStrategy : SpellcheckingStrategy() {
    override fun getTokenizer(element: PsiElement): Tokenizer<*> {
        val node = element.node ?: return super.getTokenizer(element)
        return when (node.elementType) {
            ElisprsTokenTypes.STRING_DQ,
            ElisprsTokenTypes.STRING_SQ,
            ElisprsTokenTypes.COMMENT,
            ElisprsTokenTypes.SHEBANG -> EMPTY_TOKENIZER
            else -> super.getTokenizer(element)
        }
    }
}
