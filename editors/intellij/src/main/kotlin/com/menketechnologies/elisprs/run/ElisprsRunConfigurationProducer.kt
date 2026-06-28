package com.menketechnologies.elisprs.run

import com.intellij.execution.actions.ConfigurationContext
import com.intellij.execution.actions.LazyRunConfigurationProducer
import com.intellij.execution.configurations.ConfigurationFactory
import com.intellij.openapi.util.Ref
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.menketechnologies.elisprs.ElisprsSettings

class ElisprsRunConfigurationProducer : LazyRunConfigurationProducer<ElisprsRunConfiguration>() {

    override fun getConfigurationFactory(): ConfigurationFactory =
        ElisprsRunConfigurationType.getInstance().factory

    override fun setupConfigurationFromContext(
        config: ElisprsRunConfiguration,
        context: ConfigurationContext,
        sourceElement: Ref<PsiElement>,
    ): Boolean {
        val file: PsiFile = context.psiLocation?.containingFile ?: return false
        val vf = file.virtualFile ?: return false
        if (!ElisprsSettings.getInstance().isSupportedFile(vf.name, vf.extension)) return false
        config.options.scriptPath = vf.path
        config.name = vf.nameWithoutExtension.ifBlank { vf.name }
        if (config.options.workingDirectory.isNullOrBlank()) {
            config.options.workingDirectory = vf.parent?.path ?: ""
        }
        return true
    }

    override fun isConfigurationFromContext(
        config: ElisprsRunConfiguration,
        context: ConfigurationContext,
    ): Boolean {
        val vf = context.psiLocation?.containingFile?.virtualFile ?: return false
        return ElisprsSettings.getInstance().isSupportedFile(vf.name, vf.extension) &&
            config.options.scriptPath == vf.path
    }
}
