package com.menketechnologies.elisprs

import com.intellij.codeInsight.editorActions.QuoteHandler
import com.intellij.openapi.editor.Editor
import com.intellij.openapi.editor.highlighter.HighlighterIterator
import com.intellij.psi.tree.IElementType

/**
 * Auto-pair `"` strings in Emacs Lisp source via char-only scanning.
 *
 * Only `"` opens a string in Emacs Lisp (with backslash escapes); `'` is the
 * `quote` reader macro, not a string delimiter, so it is intentionally not
 * treated as a quote char here.
 */
class ElisprsQuoteHandler : QuoteHandler {
    override fun isClosingQuote(iterator: HighlighterIterator, offset: Int): Boolean {
        val ch = charAt(iterator, offset) ?: return false
        if (!isQuoteChar(ch)) return false
        return matchingOpenBefore(iterator, offset, ch)
    }

    override fun isOpeningQuote(iterator: HighlighterIterator, offset: Int): Boolean {
        val ch = charAt(iterator, offset) ?: return false
        if (!isQuoteChar(ch)) return false
        return !matchingOpenBefore(iterator, offset, ch)
    }

    override fun hasNonClosedLiteral(
        editor: Editor,
        iterator: HighlighterIterator,
        offset: Int,
    ): Boolean = true

    override fun isInsideLiteral(iterator: HighlighterIterator): Boolean {
        val tt: IElementType? = iterator.tokenType
        return tt == ElisprsTokenTypes.STRING_DQ
    }

    private fun isQuoteChar(c: Char): Boolean = c == '"'

    private fun charAt(iterator: HighlighterIterator, offset: Int): Char? {
        val doc = iterator.document ?: return null
        if (offset < 0 || offset >= doc.textLength) return null
        return doc.charsSequence[offset]
    }

    private fun matchingOpenBefore(
        iterator: HighlighterIterator,
        offset: Int,
        quote: Char,
    ): Boolean {
        val doc = iterator.document ?: return false
        val text = doc.charsSequence
        var i = offset - 1
        while (i >= 0) {
            val c = text[i]
            if (c == '\n') return false
            // Single quotes have no backslash escaping in Emacs Lisp; double
            // quotes do. Only honor backslash-escape for `"`.
            if (c == quote && !(quote == '"' && isEscaped(text, i))) return true
            i--
        }
        return false
    }

    private fun isEscaped(text: CharSequence, idx: Int): Boolean {
        var n = 0
        var i = idx - 1
        while (i >= 0 && text[i] == '\\') {
            n++; i--
        }
        return n % 2 == 1
    }
}
