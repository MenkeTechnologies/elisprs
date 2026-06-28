package com.menketechnologies.elisprs

import com.intellij.codeInsight.editorActions.smartEnter.SmartEnterProcessor
import com.intellij.openapi.editor.Editor
import com.intellij.openapi.editor.ScrollType
import com.intellij.openapi.project.Project
import com.intellij.psi.PsiFile

/**
 * Complete Current Statement (Cmd-Shift-Enter) for Emacs Lisp source.
 *
 * Emacs Lisp has no keyword-bracketed blocks — structure is entirely
 * parenthesized — so the one strategy is **bracket balance**: any unclosed
 * `(` / `[` on the current line gets its matching `)` / `]` appended (before a
 * trailing `;` comment), and the caret lands just after them. Strings (`"…"`
 * with backslash escapes), `;` comments, and `?c` char literals are skipped so
 * a `(` inside them doesn't count.
 *
 * Skipped (return false → platform default Enter): comment lines and balanced
 * lines. The pure planner lives in [Companion.computePlan] so the test suite can
 * exercise it without a platform fixture — see [ElisprsSmartEnterProcessorTest].
 */
class ElisprsSmartEnterProcessor : SmartEnterProcessor() {
    override fun process(project: Project, editor: Editor, file: PsiFile): Boolean {
        if (file.fileType !is ElisprsFileType) return false

        val doc = editor.document
        val caret = editor.caretModel.offset
        val text = doc.charsSequence

        val lineNum = doc.getLineNumber(caret)
        val lineStart = doc.getLineStartOffset(lineNum)
        val lineEnd = doc.getLineEndOffset(lineNum)
        val line = text.subSequence(lineStart, lineEnd).toString()

        val plan = computePlan(line, lineStart) ?: return false

        doc.insertString(plan.offset, plan.insert)
        commit(editor)
        editor.caretModel.moveToOffset(plan.offset + plan.caretRel)
        editor.scrollingModel.scrollToCaret(ScrollType.RELATIVE)
        return true
    }

    companion object {
        /** Computed edit: insert [insert] at [offset], caret to [offset]+[caretRel]. */
        data class Plan(val offset: Int, val insert: String, val caretRel: Int)

        /**
         * Pure function: given an Emacs Lisp source line, return the edit plan
         * that closes its unbalanced parens/brackets, or `null` if the line is a
         * comment or already balanced. Offsets are absolute (`lineStart`-based).
         */
        fun computePlan(line: String, lineStart: Int): Plan? {
            if (line.trimStart().startsWith(";")) return null
            return tryBracketBalance(line, lineStart)
        }

        private fun tryBracketBalance(line: String, lineStart: Int): Plan? {
            val stack = ArrayDeque<Char>()
            var i = 0
            while (i < line.length) {
                when (line[i]) {
                    '(' -> stack.addLast(')')
                    '[' -> stack.addLast(']')
                    ')', ']' -> if (stack.lastOrNull() == line[i]) stack.removeLast()
                    ';' -> break // rest of line is a comment
                    '?' -> { // char literal: ?c or ?\c — skip the escaped char
                        i += if (i + 1 < line.length && line[i + 1] == '\\') 2 else 1
                    }
                    '"' -> {
                        i++
                        while (i < line.length && line[i] != '"') {
                            if (line[i] == '\\' && i + 1 < line.length) i++
                            i++
                        }
                    }
                }
                i++
            }
            if (stack.isEmpty()) return null
            val closers = stack.reversed().joinToString("")
            val anchor = lineStart + lengthBeforeTrailingComment(line)
            return Plan(anchor, closers, closers.length)
        }

        /** Length of [line] up to the start of any trailing `;` comment. */
        private fun lengthBeforeTrailingComment(line: String): Int {
            var i = 0
            while (i < line.length) {
                when (line[i]) {
                    ';' -> {
                        var j = i
                        while (j > 0 && (line[j - 1] == ' ' || line[j - 1] == '\t')) j--
                        return j
                    }
                    '?' -> i += if (i + 1 < line.length && line[i + 1] == '\\') 2 else 1
                    '"' -> {
                        i++
                        while (i < line.length && line[i] != '"') {
                            if (line[i] == '\\' && i + 1 < line.length) i++
                            i++
                        }
                    }
                }
                i++
            }
            return line.trimEnd().length
        }
    }
}
