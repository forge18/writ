package com.writ.plugin

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object WritFileType : LanguageFileType(WritLanguage) {
    override fun getName(): String = "Writ"
    override fun getDescription(): String = "Writ scripting language"
    override fun getDefaultExtension(): String = "wrt"
    override fun getIcon(): Icon? = null // TODO: Add icon
}
