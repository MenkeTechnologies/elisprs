package com.menketechnologies.elisprs.lsp

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.application.PathManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.SystemInfo
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor
import com.intellij.platform.lsp.api.customization.LspCodeActionsSupport
import com.intellij.platform.lsp.api.customization.LspCompletionSupport
import com.intellij.platform.lsp.api.customization.LspDiagnosticsSupport
import com.intellij.platform.lsp.api.customization.LspFormattingSupport
import com.intellij.platform.lsp.api.customization.LspSemanticTokensSupport
import com.menketechnologies.elisprs.ElisprsColors
import com.menketechnologies.elisprs.ElisprsSettings
import java.io.File

class ElisprsLspServerDescriptor(project: Project) :
    ProjectWideLspServerDescriptor(project, "elisprs") {

    override fun isSupportedFile(file: VirtualFile): Boolean =
        ElisprsSettings.getInstance().isSupportedFile(file.name, file.extension)

    // ── Explicit feature opt-ins ──────────────────────────────────────────
    // The default `LspSemanticTokensSupport()` returns null from
    // `getTextAttributesKey` — so even if the server sends semantic tokens,
    // the IDE has no color slot to apply and the overlay is silently
    // dropped. Map every standard LSP token type our server emits to a
    // elisprs color key.

    override val lspSemanticTokensSupport: LspSemanticTokensSupport = object : LspSemanticTokensSupport() {
        override fun getTextAttributesKey(
            tokenType: String,
            tokenModifiers: List<String>,
        ): com.intellij.openapi.editor.colors.TextAttributesKey? = when (tokenType) {
            "keyword" -> ElisprsColors.KEYWORD
            "function" -> ElisprsColors.BUILTIN_FUNCTION
            "method" -> ElisprsColors.BUILTIN_FUNCTION
            "variable" -> ElisprsColors.IDENTIFIER
            "parameter" -> ElisprsColors.IDENTIFIER
            "string" -> ElisprsColors.STRING_DQ
            "number" -> ElisprsColors.NUMBER
            "comment" -> ElisprsColors.COMMENT
            "operator" -> ElisprsColors.OPERATOR
            "macro" -> ElisprsColors.FUNCTION_DECL
            "type" -> ElisprsColors.OPTION
            "class" -> ElisprsColors.OPTION
            "property" -> ElisprsColors.SCOPE_VAR
            "namespace" -> ElisprsColors.OPTION
            else -> null
        }
    }

    override val lspCodeActionsSupport: LspCodeActionsSupport = LspCodeActionsSupport()
    override val lspDiagnosticsSupport: LspDiagnosticsSupport = LspDiagnosticsSupport()
    /// Re-trigger the completion popup after inserting an item whose LSP
    /// `command` is `editor.action.triggerSuggest`. The Platform LSP API's
    /// default `LspCompletionSupport` doesn't honor the `command` field on
    /// completion items; this subclass adds that behavior so chained
    /// completions (e.g. autoload `#` segments) keep the popup open.
    override val lspCompletionSupport: LspCompletionSupport = object : LspCompletionSupport() {
        override fun createLookupElement(
            parameters: com.intellij.codeInsight.completion.CompletionParameters,
            item: org.eclipse.lsp4j.CompletionItem,
        ): com.intellij.codeInsight.lookup.LookupElement? {
            val base = super.createLookupElement(parameters, item) ?: return null
            val cmd = item.command ?: return base
            if (cmd.command != "editor.action.triggerSuggest") return base
            val editor = parameters.editor
            val proj = editor.project ?: project
            return com.intellij.codeInsight.lookup.LookupElementDecorator
                .withDelegateInsertHandler<com.intellij.codeInsight.lookup.LookupElement>(
                    base,
                ) { ctx, _ ->
                    base.handleInsert(ctx)
                    ctx.setLaterRunnable {
                        com.intellij.codeInsight.AutoPopupController
                            .getInstance(proj)
                            .scheduleAutoPopup(editor)
                    }
                }
        }
    }
    override val lspFormattingSupport: LspFormattingSupport = LspFormattingSupport()
    override val lspHoverSupport: Boolean = true
    override val lspGoToDefinitionSupport: Boolean = true

    override fun createCommandLine(): GeneralCommandLine {
        val settings = ElisprsSettings.getInstance()
        val exe = resolveExe()
        LOG.info("Starting elisprs LSP: $exe --lsp ${settings.extraLspArgs}")
        com.menketechnologies.elisprs.ElisprsDebugLog.log(
            "lsp",
            "createCommandLine exe=$exe args=--lsp ${settings.extraLspArgs} cwd=${project.basePath}",
        )
        val cmd = GeneralCommandLine(exe)
            .withParameters("--lsp")
            .withWorkDirectory(project.basePath ?: PathManager.getHomePath())
            .withEnvironment("RUST_BACKTRACE", "1")
        splitArgs(settings.extraLspArgs).forEach { cmd.addParameter(it) }
        for (kv in splitArgs(settings.lspEnv)) {
            val i = kv.indexOf('=')
            if (i > 0) cmd.withEnvironment(kv.substring(0, i), kv.substring(i + 1))
        }
        if (settings.logLspToFile && settings.lspLogPath.isNotBlank()) {
            cmd.withEnvironment("ELISPRS_LSP_LOG", settings.lspLogPath)
            com.menketechnologies.elisprs.ElisprsDebugLog.log(
                "lsp",
                "ELISPRS_LSP_LOG=${settings.lspLogPath}",
            )
        }
        return cmd
    }

    private fun resolveExe(): String {
        val settings = ElisprsSettings.getInstance()
        settings.elisprsExecutable
            ?.takeIf { it.isNotBlank() && File(it).canExecute() }
            ?.let { return it }
        return findOnPath("elisp") ?: "elisp"
    }

    private fun findOnPath(name: String): String? {
        val pathEnv = System.getenv("PATH") ?: return null
        val sep = File.pathSeparator
        val suffixes = if (SystemInfo.isWindows) listOf(".exe", ".bat", ".cmd", "") else listOf("")
        for (dir in pathEnv.split(sep)) {
            for (suf in suffixes) {
                val f = File(dir, name + suf)
                if (f.canExecute()) return f.absolutePath
            }
        }
        return null
    }

    private fun splitArgs(s: String): List<String> {
        if (s.isBlank()) return emptyList()
        val out = mutableListOf<String>()
        val sb = StringBuilder()
        var quote: Char? = null
        for (c in s) {
            when {
                quote != null && c == quote -> quote = null
                quote != null -> sb.append(c)
                c == '"' || c == '\'' -> quote = c
                c.isWhitespace() -> if (sb.isNotEmpty()) { out += sb.toString(); sb.clear() }
                else -> sb.append(c)
            }
        }
        if (sb.isNotEmpty()) out += sb.toString()
        return out
    }

    companion object {
        private val LOG = Logger.getInstance(ElisprsLspServerDescriptor::class.java)
    }
}
