package com.writ.plugin

import com.intellij.lang.ASTNode
import com.intellij.lang.ParserDefinition
import com.intellij.lang.PsiParser
import com.intellij.lexer.Lexer
import com.intellij.openapi.project.Project
import com.intellij.psi.FileViewProvider
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.intellij.psi.tree.IFileElementType
import com.intellij.psi.tree.TokenSet

// TODO: Implement full PSI parser or wire LSP for semantic features.
// This is a scaffold — the JetBrains plugin requires a ParserDefinition
// to register the language, even if semantic analysis comes from the LSP.

class WritParserDefinition : ParserDefinition {
    companion object {
        val FILE = IFileElementType(WritLanguage)
    }

    override fun createLexer(project: Project?): Lexer {
        // TODO: Return WritLexer
        throw UnsupportedOperationException("WritLexer not yet implemented")
    }

    override fun createParser(project: Project?): PsiParser {
        // TODO: Return WritParser
        throw UnsupportedOperationException("WritParser not yet implemented")
    }

    override fun getFileNodeType(): IFileElementType = FILE

    override fun getCommentTokens(): TokenSet = TokenSet.EMPTY

    override fun getStringLiteralElements(): TokenSet = TokenSet.EMPTY

    override fun createElement(node: ASTNode?): PsiElement {
        throw UnsupportedOperationException("PSI elements not yet implemented")
    }

    override fun createFile(viewProvider: FileViewProvider): PsiFile {
        return WritFile(viewProvider)
    }
}
