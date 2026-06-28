package com.menketechnologies.elisprs

import org.junit.Assert.*
import org.junit.Test

/**
 * Pure-logic tests for [ElisprsSettings]. The `getInstance()` path touches
 * the IntelliJ ApplicationManager and must run under `gradle :test`
 * (BasePlatformTestCase); but the small parsers (`supportedExtensions`,
 * `isSupportedFile`) are pure on a freshly-constructed instance and can be
 * tested with plain JUnit.
 */
class ElisprsSettingsTest {

    private fun fresh(ext: String): ElisprsSettings {
        val s = ElisprsSettings()
        s.fileExtensions = ext
        return s
    }

    @Test fun `default extension set contains vim`() {
        val s = fresh("vim")
        assertTrue("vim" in s.supportedExtensions())
    }

    @Test fun `supportedExtensions parses comma list`() {
        val s = fresh("vim, vimrc,nvim; vimscript")
        val got = s.supportedExtensions().toSet()
        assertEquals(setOf("vim", "vimrc", "nvim", "vimscript"), got)
    }

    @Test fun `supportedExtensions strips leading dots`() {
        val s = fresh(".vim, .nvim")
        assertEquals(listOf("vim", "nvim"), s.supportedExtensions())
    }

    @Test fun `supportedExtensions ignores blanks`() {
        val s = fresh("  ,  vim ,,, ")
        assertEquals(listOf("vim"), s.supportedExtensions())
    }

    @Test fun `isSupportedFile matches by extension`() {
        val s = fresh("el")
        assertTrue(s.isSupportedFile("greet.el", "el"))
        assertTrue(s.isSupportedFile("init.el", "el"))
        assertFalse(s.isSupportedFile("readme.md", "md"))
    }

    @Test fun `isSupportedFile recognizes Emacs init dotfiles without extension`() {
        val s = fresh("el")
        for (name in listOf(".emacs", "_emacs", ".gnus", ".spacemacs", ".viper")) {
            assertTrue("$name should be supported", s.isSupportedFile(name, null))
        }
    }

    @Test fun `isSupportedFile rejects unrelated dotfiles`() {
        val s = fresh("el")
        assertFalse(s.isSupportedFile(".bashrc", null))
        assertFalse(s.isSupportedFile(".zshrc", null))
        assertFalse(s.isSupportedFile(".profile", null))
    }
}
