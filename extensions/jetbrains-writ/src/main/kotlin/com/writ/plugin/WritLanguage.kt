package com.writ.plugin

import com.intellij.lang.Language

object WritLanguage : Language("Writ") {
    private fun readResolve(): Any = WritLanguage
}
