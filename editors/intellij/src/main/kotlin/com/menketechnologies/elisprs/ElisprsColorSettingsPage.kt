package com.menketechnologies.elisprs

import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.options.colors.AttributesDescriptor
import com.intellij.openapi.options.colors.ColorDescriptor
import com.intellij.openapi.options.colors.ColorSettingsPage
import javax.swing.Icon

class ElisprsColorSettingsPage : ColorSettingsPage {
    private val attrs = arrayOf(
        AttributesDescriptor("Comments//Line comment (\")", ElisprsColors.COMMENT),
        AttributesDescriptor("Comments//Shebang (#!)", ElisprsColors.SHEBANG),
        AttributesDescriptor("Strings//Double-quoted (\"…\")", ElisprsColors.STRING_DQ),
        AttributesDescriptor("Strings//Single-quoted ('…')", ElisprsColors.STRING_SQ),
        AttributesDescriptor("Numbers//Integer / float / hex", ElisprsColors.NUMBER),

        AttributesDescriptor("Keywords//Statement / control (if/function/let/try)", ElisprsColors.KEYWORD),
        AttributesDescriptor("Keywords//Ex command (set/autocmd/nnoremap/syntax)", ElisprsColors.COMMAND),

        AttributesDescriptor("Names//Builtin function (len/has/printf)", ElisprsColors.BUILTIN_FUNCTION),
        AttributesDescriptor("Names//Function declaration / autoload", ElisprsColors.FUNCTION_DECL),
        AttributesDescriptor("Names//Identifier", ElisprsColors.IDENTIFIER),

        AttributesDescriptor("Variables//Scope-prefixed (g: s: b: l: a:)", ElisprsColors.SCOPE_VAR),
        AttributesDescriptor("Variables//Special (v:true v:count v:shell_error)", ElisprsColors.SPECIAL_VAR),
        AttributesDescriptor("Variables//Option (&name &l:name)", ElisprsColors.OPTION),
        AttributesDescriptor("Variables//Environment (\$NAME)", ElisprsColors.ENV_VAR),
        AttributesDescriptor("Variables//Register (@x)", ElisprsColors.REGISTER),

        AttributesDescriptor("Operators//Generic operator", ElisprsColors.OPERATOR),
        AttributesDescriptor("Operators//Assignment (= += -= .=)", ElisprsColors.ASSIGN_OP),
        AttributesDescriptor("Operators//Bar (|)", ElisprsColors.BAR),
        AttributesDescriptor("Operators//Line continuation (\\)", ElisprsColors.LINE_CONTINUATION),

        AttributesDescriptor("Punctuation//Parentheses ( )", ElisprsColors.PAREN),
        AttributesDescriptor("Punctuation//Braces { }", ElisprsColors.BRACE),
        AttributesDescriptor("Punctuation//Brackets [ ]", ElisprsColors.BRACKET),
        AttributesDescriptor("Punctuation//Comma", ElisprsColors.COMMA),

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
        // Every grammar/syntax category appears at least once so each color
        // slot has a live preview in Settings → Editor → Color Scheme → elisprs.
        private val DEMO = """
            #!/usr/bin/env elisprs
            " demo.vim — every token category for color tweaking.
            " A leading double-quote begins a comment to end-of-line.

            " ── options + settings ──
            set number expandtab shiftwidth=4
            setlocal textwidth=80
            let &l:foldlevel = 2

            " ── declarations + scope vars + specials ──
            let g:loaded_demo = v:true
            let s:cache = {}
            let b:counter = 0x1F
            const PI = 3.14159

            " ── function declaration + builtins + env + register ──
            function! s:Greet(name) abort
                let l:msg = printf("hello, %s (pid=%d)", a:name, getpid())
                echomsg l:msg
                let @a = $HOME .. '/.vimrc'
                if has('nvim') && len(l:msg) > 0
                    return v:true
                endif
                return v:false
            endfunction

            " ── for / while / try ──
            for item in range(1, 10)
                call s:Greet('world ' .. item)
            endfor

            while line('.') < 100 && !empty(getline('.'))
                normal! j
            endwhile

            try
                source ~/.vim/extra.vim
            catch /E484/
                echoerr 'cannot open file: ' .. v:exception
            finally
                echo 'done'
            endtry

            " ── autoload call + mappings + autocmd ──
            call plug#begin('~/.vim/plugged')
            nnoremap <silent> <leader>w :write<CR>
            augroup demo
                autocmd!
                autocmd BufWritePre *.vim call s:Greet('save')
            augroup END

            highlight Comment ctermfg=green guifg=#5f8700
        """.trimIndent()
    }
}
