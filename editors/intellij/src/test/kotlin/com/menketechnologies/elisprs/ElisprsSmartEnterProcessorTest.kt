package com.menketechnologies.elisprs

import com.menketechnologies.elisprs.ElisprsSmartEnterProcessor.Companion.computePlan
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Pure JUnit 4 tests for [ElisprsSmartEnterProcessor.computePlan].
 *
 * In every test the `src` parameter uses `|` to mark the user's caret position
 * (stripped before the planner is called); `expected` inserts a `|` where the
 * caret should land after the plan applies.
 */
class ElisprsSmartEnterProcessorTest {
    private fun caretOf(src: String): Pair<String, Int> {
        val i = src.indexOf('|')
        require(i >= 0) { "test fixture must contain '|' for caret: $src" }
        return src.removeRange(i, i + 1) to i
    }

    private fun lineAt(text: String, caret: Int): Pair<String, Int> {
        val lineStart = text.lastIndexOf('\n', caret - 1).let { if (it < 0) 0 else it + 1 }
        val lineEnd = text.indexOf('\n', caret).let { if (it < 0) text.length else it }
        return text.substring(lineStart, lineEnd) to lineStart
    }

    private fun applyPlan(src: String): String {
        val (text, caret) = caretOf(src)
        val (line, lineStart) = lineAt(text, caret)
        val plan = computePlan(line, lineStart) ?: error("expected a plan for: $src")
        val sb = StringBuilder(text)
        sb.insert(plan.offset, plan.insert)
        sb.insert(plan.offset + plan.caretRel, "|")
        return sb.toString()
    }

    private fun assertPlan(src: String, expected: String) = assertEquals(expected, applyPlan(src))

    private fun assertNoPlan(src: String) {
        val (text, caret) = caretOf(src)
        val (line, lineStart) = lineAt(text, caret)
        assertNull("expected no plan for: $src", computePlan(line, lineStart))
    }

    @Test fun unclosed_paren_closes() {
        assertPlan("(message 1|", "(message 1)|")
    }

    @Test fun unclosed_vector_bracket_closes() {
        assertPlan("[1 2|", "[1 2]|")
    }

    @Test fun nested_parens_close_in_order() {
        assertPlan("(+ a (* b|", "(+ a (* b))|")
    }

    @Test fun balanced_line_is_noop() {
        assertNoPlan("(+ 1 2)|")
    }

    @Test fun comment_line_is_noop() {
        assertNoPlan(";; a comment coming|")
    }

    @Test fun paren_inside_string_is_ignored() {
        assertPlan("(message \"(\"|", "(message \"(\")|")
    }

    @Test fun paren_in_char_literal_is_ignored() {
        assertPlan("(eq c ?(|", "(eq c ?()|")
    }

    @Test fun closers_land_before_trailing_comment() {
        assertPlan("(foo bar  ; note|", "(foo bar)|  ; note")
    }
}
