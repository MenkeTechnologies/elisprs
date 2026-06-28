package com.menketechnologies.elisprs.run

import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.ui.components.JBCheckBox
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBTextField
import com.intellij.util.ui.FormBuilder
import com.intellij.util.ui.JBUI
import javax.swing.JComponent
import javax.swing.JPanel

class ElisprsRunConfigurationEditor : SettingsEditor<ElisprsRunConfiguration>() {
    private val scriptField = TextFieldWithBrowseButton().apply {
        addBrowseFolderListener(
            "Emacs Lisp Script",
            "Choose a Emacs Lisp script to run",
            null,
            FileChooserDescriptorFactory.createSingleFileNoJarsDescriptor(),
        )
    }
    private val scriptArgsField = JBTextField()
    private val interpreterArgsField = JBTextField()
    private val workDirField = TextFieldWithBrowseButton().apply {
        addBrowseFolderListener(
            "Working Directory",
            "Choose the run working directory",
            null,
            FileChooserDescriptorFactory.createSingleFolderDescriptor(),
        )
    }
    private val disasmCheck = JBCheckBox("--disasm (fusevm bytecode disassembly)")

    private val panel: JPanel = FormBuilder.createFormBuilder()
        .addComponent(header("Program"))
        .addLabeledComponent("Script:", scriptField)
        .addLabeledComponent("Script arguments:", scriptArgsField)
        .addLabeledComponent("Interpreter arguments:", interpreterArgsField)
        .addLabeledComponent("Working directory:", workDirField)

        .addComponent(header("Tracing / debug"))
        .addComponent(disasmCheck)

        .addComponentFillVertically(JPanel(), 0)
        .panel.apply { border = JBUI.Borders.empty(8) }

    private fun header(title: String) =
        JBLabel("<html><b>$title</b></html>").apply { border = JBUI.Borders.emptyTop(8) }

    override fun createEditor(): JComponent = panel

    override fun resetEditorFrom(s: ElisprsRunConfiguration) {
        scriptField.text = s.options.scriptPath.orEmpty()
        scriptArgsField.text = s.options.scriptArgs.orEmpty()
        interpreterArgsField.text = s.options.interpreterArgs.orEmpty()
        workDirField.text = s.options.workingDirectory.orEmpty()
        disasmCheck.isSelected = s.options.disasm
    }

    override fun applyEditorTo(s: ElisprsRunConfiguration) {
        s.options.scriptPath = scriptField.text
        s.options.scriptArgs = scriptArgsField.text
        s.options.interpreterArgs = interpreterArgsField.text
        s.options.workingDirectory = workDirField.text
        s.options.disasm = disasmCheck.isSelected
    }
}
