package com.menketechnologies.elisprs

import com.intellij.lang.Language

object ElisprsLanguage : Language("elisprs") {
    private fun readResolve(): Any = ElisprsLanguage
    override fun getDisplayName(): String = "elisprs"
    override fun isCaseSensitive(): Boolean = true
}
