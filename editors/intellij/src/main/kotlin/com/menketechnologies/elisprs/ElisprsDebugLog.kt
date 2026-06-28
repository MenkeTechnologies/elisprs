package com.menketechnologies.elisprs

import java.io.File
import java.io.FileWriter
import java.io.PrintWriter
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter

/**
 * Append-only debug log written under the standard elisprs state dir
 * (`~/.elisprs/elisprs-plugin.log`, or `$ELISPRS_HOME/elisprs-plugin.log` when
 * that env var is set). Tail with `tail -f ~/.elisprs/elisprs-plugin.log`.
 *
 * Used by the LSP descriptor, refactoring handler, DAP client, and
 * breakpoint handler to surface "did this code path actually fire?"
 * without forcing the user to dig through 60 MB of idea.log.
 *
 * Mirrors the stryke plugin's `StrykeDebugLog`, but routed through
 * `~/.elisprs/` so it shares a directory with the server's `elisprs.log`
 * and rotation archives (`elisprs.log.1`, …).
 */
object ElisprsDebugLog {
    private val LOG_FILE: File by lazy { resolveLogFile() }
    private val FMT = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS")
    private val LOCK = Any()

    /**
     * Resolve the log destination. Honors `$ELISPRS_HOME` so users who
     * relocate the elisprs state dir via env get a single coherent set
     * of paths. Falls back to `~/.elisprs/elisprs-plugin.log`. Creates the
     * parent directory if it doesn't exist; failures fall back to
     * `/tmp/elisprs-plugin.log` for diagnostic continuity.
     */
    private fun resolveLogFile(): File {
        val envHome = System.getenv("ELISPRS_HOME")
        val base = if (!envHome.isNullOrBlank()) {
            File(envHome)
        } else {
            File(System.getProperty("user.home"), ".elisprs")
        }
        return try {
            if (!base.exists()) base.mkdirs()
            File(base, "elisprs-plugin.log")
        } catch (_: Exception) {
            File("/tmp/elisprs-plugin.log")
        }
    }

    fun log(tag: String, msg: String) {
        synchronized(LOCK) {
            try {
                PrintWriter(FileWriter(LOG_FILE, true)).use { w ->
                    w.println("[${LocalDateTime.now().format(FMT)}] [$tag] $msg")
                }
            } catch (_: Exception) {
                // Silent — debug log failures shouldn't propagate.
            }
        }
    }

    /** Path the next [log] call will append to. Useful for status / about UIs. */
    fun path(): String = LOG_FILE.absolutePath
}
