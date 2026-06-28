package com.menketechnologies.elisprs.dap

import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.xdebugger.breakpoints.XLineBreakpointTypeBase
import com.menketechnologies.elisprs.ElisprsSettings

/**
 * Line-breakpoint type for viml files. The runtime decides at execution time
 * whether a line is reachable; we accept any line of a supported file so the
 * gutter stays uniform.
 */
class ElisprsBreakpointType : XLineBreakpointTypeBase(
    "elisprs-line",
    "elisprs Line Breakpoint",
    ElisprsDebuggerEditorsProvider(),
) {
    override fun canPutAt(file: VirtualFile, line: Int, project: Project): Boolean =
        ElisprsSettings.getInstance().isSupportedFile(file.name, file.extension)

    override fun getPriority(): Int = 100
}
