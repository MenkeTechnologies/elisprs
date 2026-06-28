package com.menketechnologies.elisprs.run

import com.intellij.execution.Executor
import com.intellij.execution.configurations.CommandLineState
import com.intellij.execution.configurations.ConfigurationFactory
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.configurations.LocatableConfigurationBase
import com.intellij.execution.configurations.RunConfiguration
import com.intellij.execution.configurations.RuntimeConfigurationException
import com.intellij.execution.process.KillableColoredProcessHandler
import com.intellij.execution.process.ProcessHandler
import com.intellij.execution.process.ProcessTerminatedListener
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.io.FileUtil
import com.menketechnologies.elisprs.ElisprsSettings
import java.io.File

class ElisprsRunConfiguration(
    project: Project,
    factory: ConfigurationFactory,
    name: String,
) : LocatableConfigurationBase<ElisprsRunConfigurationOptions>(project, factory, name) {

    public override fun getOptions(): ElisprsRunConfigurationOptions =
        super.getOptions() as ElisprsRunConfigurationOptions

    override fun getConfigurationEditor(): SettingsEditor<out RunConfiguration> =
        ElisprsRunConfigurationEditor()

    override fun checkConfiguration() {
        val script = options.scriptPath.orEmpty()
        if (script.isBlank()) throw RuntimeConfigurationException("Script path is required")
        if (!File(script).isFile) throw RuntimeConfigurationException("Script not found: $script")
    }

    override fun getState(executor: Executor, env: ExecutionEnvironment): CommandLineState =
        object : CommandLineState(env) {
            override fun startProcess(): ProcessHandler {
                val exe = ElisprsSettings.getInstance().elisprsExecutable
                    ?.takeIf { it.isNotBlank() } ?: "elisp"
                val cmd = GeneralCommandLine()
                    .withExePath(exe)
                    .withCharset(Charsets.UTF_8)

                // elisprs runs a script via a POSITIONAL file argument — no
                // `-f` flag and no subcommand: `elisprs [--disasm] FILE …`.
                if (options.disasm) cmd.addParameter("--disasm")
                splitArgs(options.interpreterArgs.orEmpty()).forEach { cmd.addParameter(it) }
                cmd.addParameter(options.scriptPath.orEmpty())
                splitArgs(options.scriptArgs.orEmpty()).forEach { cmd.addParameter(it) }

                val wd = options.workingDirectory?.takeIf { it.isNotBlank() }
                    ?: FileUtil.toSystemDependentName(project.basePath ?: ".")
                cmd.withWorkDirectory(wd)

                val handler = KillableColoredProcessHandler(cmd)
                ProcessTerminatedListener.attach(handler)
                return handler
            }
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
}
