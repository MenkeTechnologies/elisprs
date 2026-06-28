package com.menketechnologies.elisprs

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object ElisprsFileType : LanguageFileType(ElisprsLanguage) {
    override fun getName(): String = "Emacs Lisp"
    override fun getDescription(): String = "Emacs Lisp source file"
    override fun getDefaultExtension(): String = "el"
    override fun getIcon(): Icon = ElisprsIcons.FILE
}
