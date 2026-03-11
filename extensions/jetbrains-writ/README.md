# Writ for JetBrains IDEs

Language support for the [Writ](https://github.com/forge18/writ) scripting language in IntelliJ IDEA, CLion, and other JetBrains IDEs.

> **Alpha** — Writ is under active development. Expect breaking changes.

## Requirements

- IntelliJ IDEA 2025.3 or later (any JetBrains IDE based on the IntelliJ Platform 2025.3+)
- `writ-lsp` on your `$PATH` for LSP features (diagnostics, completions, hover, go-to-definition)

## Installation

Build from source:

```sh
cd extensions/jetbrains-writ
./gradlew buildPlugin
```

Then install the resulting ZIP from **Settings → Plugins → Install Plugin from Disk**.

## Features

- **Syntax highlighting** — powered by the same TextMate grammar used by the VS Code extension
- **LSP integration** — diagnostics, completions, hover, go-to-definition, and rename via `writ-lsp`
- **Comment toggling** — `Cmd+/` (macOS) or `Ctrl+/` (Linux/Windows) for line comments, block comment support
- **File type recognition** — `.writ` files detected automatically with a dedicated file icon
