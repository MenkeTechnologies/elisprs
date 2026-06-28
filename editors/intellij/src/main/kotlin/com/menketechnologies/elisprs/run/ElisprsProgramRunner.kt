package com.menketechnologies.elisprs.run

import com.intellij.execution.configurations.RunProfile
import com.intellij.execution.executors.DefaultRunExecutor
import com.intellij.execution.runners.DefaultProgramRunner

class ElisprsProgramRunner : DefaultProgramRunner() {
    override fun getRunnerId(): String = "ElisprsProgramRunner"

    override fun canRun(executorId: String, profile: RunProfile): Boolean {
        if (profile !is ElisprsRunConfiguration) return false
        return executorId == DefaultRunExecutor.EXECUTOR_ID
    }
}
