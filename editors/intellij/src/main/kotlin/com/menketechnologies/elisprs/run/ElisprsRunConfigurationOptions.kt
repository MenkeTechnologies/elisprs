package com.menketechnologies.elisprs.run

import com.intellij.execution.configurations.LocatableRunConfigurationOptions

class ElisprsRunConfigurationOptions : LocatableRunConfigurationOptions() {
    var scriptPath: String? by string()
    var scriptArgs: String? by string()
    var interpreterArgs: String? by string()
    var workingDirectory: String? by string()
    var disasm: Boolean by property(false)         // --disasm (fusevm bytecode listing)
}
