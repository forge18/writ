package com.writ.plugin

import com.intellij.lexer.Lexer
import com.intellij.openapi.editor.DefaultLanguageHighlighterColors
import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterBase
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.psi.tree.IElementType

// TODO: Implement WritLexer that wraps the TextMate-style tokenization
// or build a JFlex-based lexer for JetBrains PSI integration.
// For now this is a placeholder that registers the highlighter factory.

class WritSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    override fun getSyntaxHighlighter(project: Project?, virtualFile: VirtualFile?): SyntaxHighlighter {
        return WritSyntaxHighlighter()
    }
}

class WritSyntaxHighlighter : SyntaxHighlighterBase() {
    companion object {
        val KEYWORD = TextAttributesKey.createTextAttributesKey(
            "WRIT_KEYWORD", DefaultLanguageHighlighterColors.KEYWORD
        )
        val STRING = TextAttributesKey.createTextAttributesKey(
            "WRIT_STRING", DefaultLanguageHighlighterColors.STRING
        )
        val NUMBER = TextAttributesKey.createTextAttributesKey(
            "WRIT_NUMBER", DefaultLanguageHighlighterColors.NUMBER
        )
        val COMMENT = TextAttributesKey.createTextAttributesKey(
            "WRIT_COMMENT", DefaultLanguageHighlighterColors.LINE_COMMENT
        )
    }

    override fun getHighlightingLexer(): Lexer {
        // TODO: Return a JFlex-based lexer for Writ tokens
        throw UnsupportedOperationException("WritLexer not yet implemented")
    }

    override fun getTokenHighlights(tokenType: IElementType?): Array<TextAttributesKey> {
        // TODO: Map Writ token types to highlight attributes
        return emptyArray()
    }
}
