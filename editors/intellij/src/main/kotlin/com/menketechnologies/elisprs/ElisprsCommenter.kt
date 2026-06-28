package com.menketechnologies.elisprs

import com.intellij.lang.Commenter

/**
 * Emacs Lisp line comments start with `;` (the convention is `;;` for
 * code-level comments). Cmd/Ctrl-`/` toggles `;; `.
 *
 * Emacs Lisp has NO block-comment form — so the block-comment hooks return null
 * and Cmd+Opt+/ falls back to line-by-line `;;` commenting.
 */
class ElisprsCommenter : Commenter {
    override fun getLineCommentPrefix(): String = ";; "
    override fun getBlockCommentPrefix(): String? = null
    override fun getBlockCommentSuffix(): String? = null
    override fun getCommentedBlockCommentPrefix(): String? = null
    override fun getCommentedBlockCommentSuffix(): String? = null
}
