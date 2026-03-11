package com.writ.plugin

import com.intellij.openapi.fileTypes.LanguageFileType
import com.intellij.openapi.util.IconLoader
import javax.swing.Icon

object WritFileType : LanguageFileType(WritLanguage) {
    override fun getName(): String = "Writ"
    override fun getDescription(): String = "Writ scripting language"
    override fun getDefaultExtension(): String = "writ"
    override fun getIcon(): Icon = IconLoader.getIcon("/icons/writ-file.svg", WritFileType::class.java)
}
