---
title: IDE Extensions
description: Syntax highlighting, LSP, debugging, and hot reload for Writ in your editor.
---

## VS Code

The official VS Code extension provides full language support for `.writ` files.

### Installation

Install from the VS Code Marketplace by searching for **Writ**, or install the `.vsix` directly:

```sh
code --install-extension writ-lang-0.1.0.vsix
```

The `.vsix` file is available on the [GitHub releases page](https://github.com/forge18/writ/releases).

### Features

**Syntax highlighting** — full grammar coverage for all language constructs.

**Language server (LSP)**
- Diagnostics (type errors, undefined names) as you type
- Completions for fields, methods, and stdlib functions
- Hover documentation
- Go to definition

**Debugging** — breakpoints, step through, inspect variables via the Debug Adapter Protocol. Add a launch configuration to `.vscode/launch.json`:

```json
{
  "type": "writ",
  "request": "launch",
  "name": "Debug Writ Script",
  "program": "${file}"
}
```

**Hot reload** — saving a `.writ` file automatically pushes updated bytecode to a running VM. Requires your host to listen for reload requests (see the [Embedding guide](/writ/advanced/embedding/)).

### Settings

| Setting | Default | Description |
|---|---|---|
| `writ.lspPath` | `"writ-lsp"` | Path to the language server binary. Leave as default to use the bundled binary. |
| `writ.hotReload.enabled` | `true` | Enable hot reload on save. |
| `writ.hotReload.mechanism` | `"socket"` | How reload requests are sent: `socket`, `pipe`, or `file`. |
| `writ.hotReload.address` | `"127.0.0.1:7777"` | Address for socket-based hot reload. |

## Other editors

No other editor extensions exist yet. The language server (`writ-lsp`) speaks standard LSP and can be connected to any editor that supports it — Neovim, Helix, Zed, etc. — with manual configuration.
