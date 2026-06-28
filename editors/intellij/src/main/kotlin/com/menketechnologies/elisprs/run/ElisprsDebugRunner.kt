package com.menketechnologies.elisprs.run

import com.intellij.execution.ExecutionException
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.configurations.RunProfile
import com.intellij.execution.configurations.RunProfileState
import com.intellij.execution.executors.DefaultDebugExecutor
import com.intellij.execution.process.ProcessHandler
import com.intellij.execution.process.ProcessOutputTypes
import com.intellij.execution.runners.DefaultProgramRunner
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.execution.ui.RunContentDescriptor
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.util.io.FileUtil
import com.intellij.xdebugger.XDebugProcess
import com.intellij.xdebugger.XDebugProcessStarter
import com.intellij.xdebugger.XDebugSession
import com.intellij.xdebugger.XDebuggerManager
import com.menketechnologies.elisprs.ElisprsSettings
import com.menketechnologies.elisprs.dap.ElisprsDebugProcess
import java.io.OutputStream

/**
 * Debug executor for [ElisprsRunConfiguration]. Spawns `elisprs --dap` (a DAP
 * server over **stdio**), then constructs an [XDebugProcess] that speaks DAP
 * over the launched process's stdout / stdin while the debuggee's program
 * output flows back as DAP `output` events into the Debug Console.
 *
 * elisprs's DAP server is stdio-only (`elisprs --dap`, no host:port) — there
 * is no TCP loopback to accept. The launched process's stdout carries the
 * protocol frames exclusively, so the process handler here deliberately does
 * NOT decode stdout (that would steal the stream the DAP client reads and
 * dump raw JSON into the console); only stderr is pumped to the console.
 */
class ElisprsDebugRunner : DefaultProgramRunner() {
    override fun getRunnerId(): String = "ElisprsDebugRunner"

    override fun canRun(executorId: String, profile: RunProfile): Boolean =
        executorId == DefaultDebugExecutor.EXECUTOR_ID && profile is ElisprsRunConfiguration

    @Throws(ExecutionException::class)
    override fun doExecute(state: RunProfileState, env: ExecutionEnvironment): RunContentDescriptor? {
        val cfg = env.runProfile as ElisprsRunConfiguration
        val exe = ElisprsSettings.getInstance().elisprsExecutable
            ?.takeIf { it.isNotBlank() } ?: "elisp"

        val cmd = GeneralCommandLine()
            .withExePath(exe)
            .withCharset(Charsets.UTF_8)
            .withParameters("--dap")
        val wd = cfg.options.workingDirectory?.takeIf { it.isNotBlank() }
            ?: FileUtil.toSystemDependentName(env.project.basePath ?: ".")
        cmd.withWorkDirectory(wd)

        val process: Process = cmd.createProcess()
        val handler = ElisprsDapProcessHandler(process, cmd.commandLineString)

        val session: XDebugSession = XDebuggerManager.getInstance(env.project).startSession(
            env,
            object : XDebugProcessStarter() {
                override fun start(session: XDebugSession): XDebugProcess {
                    val args = splitArgs(cfg.options.scriptArgs.orEmpty())
                    return ElisprsDebugProcess(
                        session = session,
                        processHandler = handler,
                        dapInput = process.inputStream,   // the server's stdout (protocol frames)
                        dapOutput = process.outputStream,  // the server's stdin
                        programPath = cfg.options.scriptPath.orEmpty(),
                        programArgs = args,
                        workingDirectory = wd,
                    )
                }
            },
        )

        return getDescriptorWithoutSplitDebuggerWarning(session)
            ?: @Suppress("DEPRECATION") session.runContentDescriptor
    }

    private fun getDescriptorWithoutSplitDebuggerWarning(session: XDebugSession): RunContentDescriptor? {
        return try {
            val m = session.javaClass.methods.firstOrNull {
                it.name == "getMockRunContentDescriptorIfInitialized" && it.parameterCount == 0
            } ?: return null
            m.isAccessible = true
            m.invoke(session) as? RunContentDescriptor
        } catch (e: Throwable) {
            LOG.debug("getMockRunContentDescriptorIfInitialized reflection failed", e)
            null
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

    companion object {
        private val LOG = Logger.getInstance(ElisprsDebugRunner::class.java)
    }
}

/**
 * Lifecycle-only process handler for the `elisprs --dap` server. Pumps
 * **stderr** into the Debug Console and reports termination, but never
 * touches stdout — that is the DAP protocol stream owned by the DAP client.
 * The console's actual program output arrives via DAP `output` events
 * (see [ElisprsDebugProcess.handleEvent]).
 */
private class ElisprsDapProcessHandler(
    private val process: Process,
    private val commandLine: String,
) : ProcessHandler() {
    init {
        Thread({
            try {
                process.errorStream.bufferedReader().forEachLine {
                    notifyTextAvailable(it + "\n", ProcessOutputTypes.STDERR)
                }
            } catch (_: Exception) { /* stream closed on exit */ }
        }, "elisprs-DAP-stderr").apply { isDaemon = true; start() }

        Thread({
            try {
                val code = process.waitFor()
                notifyProcessTerminated(code)
            } catch (_: InterruptedException) { /* shutting down */ }
        }, "elisprs-DAP-waiter").apply { isDaemon = true; start() }
    }

    override fun destroyProcessImpl() {
        process.destroy()
        // The waiter thread reports termination once the process exits.
    }

    override fun detachProcessImpl() {
        notifyProcessDetached()
    }

    override fun detachIsDefault(): Boolean = false

    override fun getProcessInput(): OutputStream? = null
}
