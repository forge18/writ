package com.writ.plugin

import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import java.nio.file.Path

class WritTextMateBundleProvider : TextMateBundleProvider {
    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> {
        val bundleUrl = this::class.java.classLoader.getResource("textmate/writ")
            ?: return emptyList()
        return listOf(TextMateBundleProvider.PluginBundle("Writ", Path.of(bundleUrl.toURI())))
    }
}
