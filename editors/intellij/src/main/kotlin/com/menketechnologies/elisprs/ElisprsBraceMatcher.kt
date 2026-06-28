package com.menketechnologies.elisprs

import com.intellij.lang.BracePair
import com.intellij.lang.PairedBraceMatcher
import com.intellij.psi.PsiFile
import com.intellij.psi.tree.IElementType

/**
 * Brace pairing for Emacs Lisp. Powers auto-insertion of `)`/`]` when typing
 * `(`/`[`, AND structural highlighting when the cursor sits next to a paired
 * delimiter. Emacs Lisp uses `(` lists and `[` vectors — there is no `{}`.
 */
class ElisprsBraceMatcher : PairedBraceMatcher {
    private val pairs = arrayOf(
        BracePair(ElisprsTokenTypes.LPAREN, ElisprsTokenTypes.RPAREN, true),
        BracePair(ElisprsTokenTypes.LBRACKET, ElisprsTokenTypes.RBRACKET, false),
    )

    override fun getPairs(): Array<BracePair> = pairs

    override fun isPairedBracesAllowedBeforeType(
        lbraceType: IElementType,
        contextType: IElementType?,
    ): Boolean = true

    override fun getCodeConstructStart(file: PsiFile?, openingBraceOffset: Int): Int =
        openingBraceOffset
}
