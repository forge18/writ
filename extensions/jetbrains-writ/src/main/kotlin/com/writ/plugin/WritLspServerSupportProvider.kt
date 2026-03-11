package com.writ.plugin

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspServerSupportProvider
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor

internal class WritLspServerSupportProvider : LspServerSupportProvider {
    override fun fileOpened(
        project: Project,
        file: VirtualFile,
        serverStarter: LspServerSupportProvider.LspServerStarter,
    ) {
        if (file.extension == "writ") {
            serverStarter.ensureServerStarted(WritLspServerDescriptor(project))
        }
    }
}

private class WritLspServerDescriptor(project: Project) :
    ProjectWideLspServerDescriptor(project, "Writ") {

    override fun isSupportedFile(file: VirtualFile): Boolean =
        file.extension == "writ"

    override fun createCommandLine(): GeneralCommandLine =
        GeneralCommandLine("writ-lsp")
}
