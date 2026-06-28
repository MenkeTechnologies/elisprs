package com.menketechnologies.elisprs

import com.intellij.psi.TokenType
import com.intellij.psi.tree.IElementType
import org.junit.Assert.*
import org.junit.Test

/**
 * Unit tests for [ElisprsLexer]. These run under `./gradlew test` and exercise
 * the hand-rolled tokenizer that feeds the syntax highlighter before the LSP
 * semantic-tokens response lands.
 */
class ElisprsLexerTest {

    private fun tokens(src: String): List<Pair<IElementType?, String>> {
        val lex = ElisprsLexer()
        lex.start(src, 0, src.length, 0)
        val out = mutableListOf<Pair<IElementType?, String>>()
        while (lex.tokenType != null) {
            val t = lex.tokenType
            val s = src.substring(lex.tokenStart, lex.tokenEnd)
            out += t to s
            lex.advance()
        }
        return out
    }

    private fun nonWs(src: String) = tokens(src).filter { it.first != TokenType.WHITE_SPACE }

    @Test fun `semicolon runs a comment to end of line`() {
        val toks = nonWs("; a comment with \"quotes\" inside\n(message \"hi\")\n")
        assertEquals(ElisprsTokenTypes.COMMENT, toks[0].first)
        assertTrue(toks[0].second.contains("inside"))
        // Code on the next line still lexes.
        assertTrue(toks.any { it.first == ElisprsTokenTypes.BUILTIN_FUNCTION && it.second == "message" })
    }

    @Test fun `double-quoted string with escapes is one token`() {
        val toks = nonWs("(setq s \"he\\\"llo\")")
        assertTrue(
            "expected a STRING_DQ token: $toks",
            toks.any { it.first == ElisprsTokenTypes.STRING_DQ && it.second.startsWith("\"he") },
        )
    }

    @Test fun `char literals lex as numbers`() {
        assertEquals(ElisprsTokenTypes.NUMBER, nonWs("?A")[0].first)
        assertEquals(ElisprsTokenTypes.NUMBER, nonWs("?\\n")[0].first)
    }

    @Test fun `numbers decimal float and radix`() {
        assertEquals(ElisprsTokenTypes.NUMBER, nonWs("42")[0].first)
        assertEquals("3.14", nonWs("3.14")[0].second)
        assertEquals("2e-3", nonWs("2e-3")[0].second)
        assertEquals(ElisprsTokenTypes.NUMBER, nonWs("#xFF")[0].first)
        assertEquals(ElisprsTokenTypes.NUMBER, nonWs("#o17")[0].first)
        assertEquals(ElisprsTokenTypes.NUMBER, nonWs("#b1010")[0].first)
    }

    @Test fun `special forms classify as KEYWORD`() {
        for ((tt, w) in nonWs("if when unless cond and or while let let* progn lambda setq")) {
            assertEquals("expected KEYWORD for $w", ElisprsTokenTypes.KEYWORD, tt)
        }
    }

    @Test fun `t nil and keywords classify as SPECIAL_VAR`() {
        for (w in listOf("t", "nil", ":foo", ":use-package")) {
            assertEquals("expected SPECIAL_VAR for $w", ElisprsTokenTypes.SPECIAL_VAR, nonWs(w)[0].first)
        }
    }

    @Test fun `subr in call-head position is BUILTIN_FUNCTION`() {
        val toks = nonWs("(car x)")
        assertEquals(ElisprsTokenTypes.LPAREN, toks[0].first)
        assertEquals(ElisprsTokenTypes.BUILTIN_FUNCTION, toks[1].first)
        assertEquals("car", toks[1].second)
        // the `+` subr also heads
        assertEquals(ElisprsTokenTypes.BUILTIN_FUNCTION, nonWs("(+ 1 2)")[1].first)
    }

    @Test fun `subr not in head position stays identifier`() {
        // `car` as an argument (not call head) is a plain identifier.
        val toks = nonWs("(funcall car x)")
        assertTrue(
            "non-head car should be IDENTIFIER: $toks",
            toks.any { it.first == ElisprsTokenTypes.IDENTIFIER && it.second == "car" },
        )
    }

    @Test fun `defun name colors as FUNCTION_DECL`() {
        val toks = nonWs("(defun fact (n) n)")
        assertEquals(ElisprsTokenTypes.KEYWORD, toks[1].first)
        assertEquals("defun", toks[1].second)
        assertEquals(ElisprsTokenTypes.FUNCTION_DECL, toks[2].first)
        assertEquals("fact", toks[2].second)
    }

    @Test fun `defvar name colors as FUNCTION_DECL`() {
        val toks = nonWs("(defvar my-var 10)")
        assertTrue(
            "declared name should be FUNCTION_DECL: $toks",
            toks.any { it.first == ElisprsTokenTypes.FUNCTION_DECL && it.second == "my-var" },
        )
    }

    @Test fun `reader prefixes lex as operators`() {
        for (p in listOf("'", "`", ",", ",@", "#'")) {
            val toks = tokens(p + "x")
            assertEquals("expected OPERATOR for $p", ElisprsTokenTypes.OPERATOR, toks[0].first)
            assertEquals(p, toks[0].second)
        }
    }

    @Test fun `parens and vector brackets lex as distinct L and R tokens`() {
        val toks = nonWs("(list [1 2])")
        val types = toks.map { it.first }
        assertTrue(types.contains(ElisprsTokenTypes.LPAREN))
        assertTrue(types.contains(ElisprsTokenTypes.RPAREN))
        assertTrue(types.contains(ElisprsTokenTypes.LBRACKET))
        assertTrue(types.contains(ElisprsTokenTypes.RBRACKET))
    }

    @Test fun `hyphenated symbol is one identifier and minus is a subr`() {
        // `foo-bar` is a single symbol; `-` alone is the subtraction subr.
        assertEquals("foo-bar", nonWs("foo-bar")[0].second)
        assertEquals(ElisprsTokenTypes.IDENTIFIER, nonWs("foo-bar")[0].first)
        assertEquals(ElisprsTokenTypes.BUILTIN_FUNCTION, nonWs("(- 5 1)")[1].first)
    }

    @Test fun `representative sample produces no bad characters`() {
        val src = """
            ;;; greet.el --- a sample  -*- lexical-binding: t; -*-
            (defun greet (who)
              "Say hi to WHO."
              (message "hi %s (%d)" who (1+ (length who))))
            (let ((names '("a" "b")))
              (mapcar #'greet names))
        """.trimIndent() + "\n"
        val toks = tokens(src)
        assertFalse(
            "sample produced BAD_CHARACTER tokens: ${toks.filter { it.first == TokenType.BAD_CHARACTER }}",
            toks.any { it.first == TokenType.BAD_CHARACTER },
        )
    }
}
