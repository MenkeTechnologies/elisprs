package com.menketechnologies.elisprs.dap

import com.intellij.xdebugger.frame.XExecutionStack
import com.intellij.xdebugger.frame.XStackFrame
import com.intellij.xdebugger.frame.XSuspendContext

class ElisprsSuspendContext(private val stack: ElisprsExecutionStack) : XSuspendContext() {
    override fun getActiveExecutionStack(): XExecutionStack = stack
}

class ElisprsExecutionStack : XExecutionStack("Main") {

    @Volatile private var frames: List<ElisprsStackFrame> = emptyList()

    fun setFrames(newFrames: List<ElisprsStackFrame>) {
        frames = newFrames
    }

    override fun getTopFrame(): XStackFrame? = frames.firstOrNull()

    override fun computeStackFrames(firstFrameIndex: Int, container: XStackFrameContainer) {
        val slice = if (firstFrameIndex <= 0) frames else frames.drop(firstFrameIndex)
        container.addStackFrames(slice, true)
    }
}
