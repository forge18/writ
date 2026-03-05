package com.writ.plugin

import com.intellij.extapi.psi.PsiFileBase
import com.intellij.openapi.fileTypes.FileType
import com.intellij.psi.FileViewProvider

class WritFile(viewProvider: FileViewProvider) : PsiFileBase(viewProvider, WritLanguage) {
    override fun getFileType(): FileType = WritFileType
}
