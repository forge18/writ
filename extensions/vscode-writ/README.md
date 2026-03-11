# Writ for VS Code

Language support for the [Writ](https://github.com/forge18/writ) scripting language.

> **Alpha** — Writ is under active development. Expect breaking changes.

## Features

- **Syntax highlighting** — full TextMate grammar coverage for all language constructs
- **Language server (LSP)** — diagnostics, completions, hover documentation, and go-to-definition via `writ-lsp`
- **Debugging** — breakpoints, step through, and variable inspection via the Debug Adapter Protocol
- **Hot reload** — saving a `.writ` file pushes updated bytecode to a running VM

## Installation

Search for **Writ** in the VS Code Marketplace, or install the `.vsix` directly:

```sh
code --install-extension writ-lang-0.1.0.vsix
```

The `.vsix` file is available on the [GitHub releases page](https://github.com/forge18/writ/releases).

## Debugging

Add a launch configuration to `.vscode/launch.json`:

```json
{
  "type": "writ",
  "request": "launch",
  "name": "Debug Writ Script",
  "program": "${file}",
  "host": "127.0.0.1",
  "port": 7778
}
```

## Settings

| Setting | Default | Description |
|---|---|---|
| `writ.lspPath` | `"writ-lsp"` | Path to the language server binary. Leave as default to use the bundled binary. |
| `writ.hotReload.enabled` | `true` | Enable hot reload on save. |
| `writ.hotReload.mechanism` | `"socket"` | How reload requests are sent: `socket`, `pipe`, or `file`. |
| `writ.hotReload.address` | `"127.0.0.1:7777"` | Address for socket-based hot reload. |

## Requirements

- `writ-lsp` on your `$PATH` (or configure `writ.lspPath`)
- For hot reload: a host application listening for reload requests (see the [Embedding guide](https://forge18.github.io/writ/advanced/embedding/))
