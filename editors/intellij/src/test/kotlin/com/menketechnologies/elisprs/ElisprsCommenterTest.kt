package com.menketechnologies.elisprs

import org.junit.Assert.*
import org.junit.Test

/**
 * The Commenter contract is what IntelliJ uses for Cmd+/ and Cmd+Opt+/.
 * Wrong prefixes here mean the editor's comment shortcuts silently produce
 * broken Emacs Lisp.
 */
class ElisprsCommenterTest {
    private val c = ElisprsCommenter()

    @Test fun `line comment prefix is double-quote-space`() {
        assertEquals("\" ", c.lineCommentPrefix)
    }

    @Test fun `there is no block comment form`() {
        // Emacs Lisp has no block-comment delimiters — the hooks must be null so
        // Cmd+Opt+/ degrades to line-by-line `"` commenting.
        assertNull(c.blockCommentPrefix)
        assertNull(c.blockCommentSuffix)
        assertNull(c.commentedBlockCommentPrefix)
        assertNull(c.commentedBlockCommentSuffix)
    }
}
