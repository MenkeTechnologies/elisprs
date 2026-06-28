package com.menketechnologies.elisprs.run

import com.intellij.execution.configurations.ConfigurationFactory
import com.intellij.execution.configurations.ConfigurationType
import com.intellij.execution.configurations.RunConfiguration
import com.intellij.openapi.project.Project
import com.menketechnologies.elisprs.ElisprsIcons
import javax.swing.Icon

class ElisprsRunConfigurationType : ConfigurationType {
    override fun getDisplayName(): String = "elisprs"
    override fun getConfigurationTypeDescription(): String = "Run a viml script with elisprs"
    override fun getIcon(): Icon = ElisprsIcons.FILE
    override fun getId(): String = "ELISPRS_RUN_CONFIGURATION"
    override fun getConfigurationFactories(): Array<ConfigurationFactory> = arrayOf(factory)

    val factory = object : ConfigurationFactory(this) {
        override fun getId(): String = "elisprs"
        override fun createTemplateConfiguration(project: Project): RunConfiguration =
            ElisprsRunConfiguration(project, this, "elisprs")
        override fun getOptionsClass(): Class<ElisprsRunConfigurationOptions> =
            ElisprsRunConfigurationOptions::class.java
    }

    companion object {
        fun getInstance(): ElisprsRunConfigurationType =
            com.intellij.execution.configurations.ConfigurationTypeUtil
                .findConfigurationType(ElisprsRunConfigurationType::class.java)
    }
}
