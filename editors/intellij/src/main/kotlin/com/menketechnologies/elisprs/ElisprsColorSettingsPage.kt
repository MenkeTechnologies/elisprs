package com.menketechnologies.elisprs

import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.options.colors.AttributesDescriptor
import com.intellij.openapi.options.colors.ColorDescriptor
import com.intellij.openapi.options.colors.ColorSettingsPage
import javax.swing.Icon

class ElisprsColorSettingsPage : ColorSettingsPage {
    // Only the categories the Emacs Lisp lexer actually emits get a slot. (Other
    // keys remain defined in [ElisprsColors] for the highlighter's fallbacks.)
    private val attrs = arrayOf(
        AttributesDescriptor("Comments//Line comment (;)", ElisprsColors.COMMENT),
        AttributesDescriptor("Strings//Double-quoted (\"…\")", ElisprsColors.STRING_DQ),
        AttributesDescriptor("Numbers//Integer / float / char / radix", ElisprsColors.NUMBER),

        AttributesDescriptor("Keywords//Special form (defun/let/if/lambda)", ElisprsColors.KEYWORD),

        AttributesDescriptor("Names//Builtin subr (car/cons/+)", ElisprsColors.BUILTIN_FUNCTION),
        AttributesDescriptor("Names//Definition name (defun/defvar)", ElisprsColors.FUNCTION_DECL),
        AttributesDescriptor("Names//Identifier", ElisprsColors.IDENTIFIER),

        AttributesDescriptor("Constants//Keyword / t / nil (:foo t nil)", ElisprsColors.SPECIAL_VAR),

        AttributesDescriptor("Operators//Reader prefix (' ` , #')", ElisprsColors.OPERATOR),

        AttributesDescriptor("Punctuation//Parentheses ( )", ElisprsColors.PAREN),
        AttributesDescriptor("Punctuation//Vector brackets [ ]", ElisprsColors.BRACKET),

        AttributesDescriptor("Errors//Bad character", ElisprsColors.BAD_CHAR),
    )

    override fun getIcon(): Icon = ElisprsIcons.FILE
    override fun getHighlighter(): SyntaxHighlighter = ElisprsSyntaxHighlighter()
    override fun getDemoText(): String = DEMO
    override fun getAdditionalHighlightingTagToDescriptorMap(): MutableMap<String, TextAttributesKey>? = null
    override fun getAttributeDescriptors(): Array<AttributesDescriptor> = attrs
    override fun getColorDescriptors(): Array<ColorDescriptor> = ColorDescriptor.EMPTY_ARRAY
    override fun getDisplayName(): String = "elisprs"

    companion object {
        // Every emitted token category appears at least once so each color slot
        // has a live preview in Settings → Editor → Color Scheme → elisprs.
        private val DEMO = """
            ;;; demo.el --- every token category  -*- lexical-binding: t; -*-
            ;; A leading semicolon begins a comment to end-of-line.

            (defvar my-count 0
              "A demo variable.")

            (defconst my-pi 3.14159)

            (defun greet (name &optional loud)
              "Say hello to NAME; shout when LOUD."
              (let ((msg (format "hello, %s (#%d)" name (1+ my-count)))
                    (tag :greeting))
                (when (and loud (> (length name) 0))
                  (setq msg (upcase msg)))
                (message "%s" msg)
                (if loud t nil)))

            (dolist (who '("a" "b" "c"))
              (greet who))

            (aref [?A ?B ?C] 0)
            (mapcar #'greet (vector "x" "y"))
        """.trimIndent()
    }
}
