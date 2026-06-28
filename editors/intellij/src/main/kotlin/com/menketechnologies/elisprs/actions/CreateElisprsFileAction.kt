package com.menketechnologies.elisprs.actions

import com.intellij.ide.actions.CreateFileFromTemplateAction
import com.intellij.ide.actions.CreateFileFromTemplateDialog
import com.intellij.openapi.project.Project
import com.intellij.psi.PsiDirectory
import com.intellij.psi.PsiFile
import com.intellij.psi.PsiFileFactory
import com.menketechnologies.elisprs.ElisprsFileType
import com.menketechnologies.elisprs.ElisprsIcons

/// File > New > Emacs Lisp File. Hands the user a name dialog with a few
/// canonical starting templates (script, library, empty). All templates resolve
/// to `ElisprsFileType` so the new buffer immediately picks up syntax
/// highlighting, LSP, commenter, etc. without an IDE reload.
///
/// Implemented via the platform's `CreateFileFromTemplateAction` so we inherit
/// the standard New-File dialog (name field, template picker, undoable PSI
/// write). Templates are inline string literals here so the plugin stays
/// single-jar with no resource extraction at runtime.
class CreateElisprsFileAction :
    CreateFileFromTemplateAction("Emacs Lisp File", "Create new Emacs Lisp script", ElisprsIcons.FILE) {

    override fun getActionName(directory: PsiDirectory?, newName: String, templateName: String?): String =
        "Create Emacs Lisp File"

    override fun buildDialog(
        project: Project,
        directory: PsiDirectory,
        builder: CreateFileFromTemplateDialog.Builder,
    ) {
        builder
            .setTitle("New Emacs Lisp File")
            .addKind("Script (#!/usr/bin/env elisp)", ElisprsIcons.FILE, TPL_SCRIPT)
            .addKind("Library",                       ElisprsIcons.FILE, TPL_LIBRARY)
            .addKind("Empty",                         ElisprsIcons.FILE, TPL_EMPTY)
    }

    override fun createFile(name: String, templateName: String, dir: PsiDirectory): PsiFile? {
        val fileName = if (name.contains('.')) name else "$name.el"
        val feature = fileName.removeSuffix(".el")
        val body = when (templateName) {
            TPL_SCRIPT  -> SCRIPT_BODY
            TPL_LIBRARY -> libraryBody(feature)
            else        -> ""
        }
        val file = PsiFileFactory.getInstance(dir.project)
            .createFileFromText(fileName, ElisprsFileType, body)
        return dir.add(file) as? PsiFile
    }

    companion object {
        private const val TPL_SCRIPT  = "Script"
        private const val TPL_LIBRARY = "Library"
        private const val TPL_EMPTY   = "Empty"

        private val SCRIPT_BODY = """
            |#!/usr/bin/env elisp
            |;;; -*- lexical-binding: t; -*-
            |
            |(defun main ()
            |  (message "hello from elisprs"))
            |
            |(main)
            |""".trimMargin()

        /// A conventional library file, with the feature `provide`d under the
        /// file's base name so `(require 'NAME)` works.
        private fun libraryBody(feature: String): String = """
            |;;; $feature.el --- one-line summary  -*- lexical-binding: t; -*-
            |
            |;;; Commentary:
            |;; Describe the library here.
            |
            |;;; Code:
            |
            |(defun $feature-greet (who)
            |  "Say hello to WHO."
            |  (message "hello, %s" who))
            |
            |(provide '$feature)
            |;;; $feature.el ends here
            |""".trimMargin()
    }
}
