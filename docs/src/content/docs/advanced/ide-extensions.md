---
title: IDE Extensions
description: Syntax highlighting, LSP, debugging, and hot reload for Writ in your editor.
---

Writ ships with extensions for three editors: **VS Code**, **Neovim/Vim**, and **JetBrains IDEs**. All extensions live under `extensions/` in the repository.

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

**Syntax highlighting** — full TextMate grammar coverage for all language constructs.

**Language server (LSP)** — powered by `writ-lsp` via `vscode-languageclient`.
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
  "program": "${file}",
  "host": "127.0.0.1",
  "port": 7778
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

---

## Neovim / Vim

The `vim-writ` plugin provides syntax highlighting, filetype detection, LSP integration, and hot reload for Neovim. Traditional Vim gets syntax highlighting and filetype settings.

### Installation

Use your preferred plugin manager. For example, with **lazy.nvim**:

```lua
{
  "forge18/writ",
  config = function()
    require("writ").setup()
  end,
}
```

Or clone `extensions/vim-writ` into your Vim runtimepath manually.

### Features

**Syntax highlighting** — Vim syntax file covering keywords, types, strings (with interpolation), comments, operators, and number literals.

**Filetype detection** — `.writ` files are automatically recognized.

**Filetype settings** — `commentstring`, `tabstop`/`shiftwidth` (4 spaces), and `suffixesadd` configured out of the box.

**Language server (LSP)** *(Neovim only)* — auto-starts `writ-lsp` via `vim.lsp.start()` when a `.writ` file is opened. No `lspconfig` dependency required.

**Hot reload** *(Neovim only)* — on save, sends a reload payload to a running host over TCP socket or file sentinel, matching the VS Code behavior.

### Configuration

Pass options to `require("writ").setup()`:

```lua
require("writ").setup({
  lsp_cmd = "writ-lsp",        -- path to the language server binary
  hot_reload = {
    enabled = true,             -- enable hot reload on save
    mechanism = "socket",       -- "socket" or "file"
    address = "127.0.0.1:7777", -- host:port for socket, or file path for file
  },
})
```

---

## JetBrains IDEs

The `jetbrains-writ` plugin provides Writ support for IntelliJ IDEA, CLion, and other JetBrains IDEs (2025.3+).

### Installation

Build the plugin from source:

```sh
cd extensions/jetbrains-writ
./gradlew buildPlugin
```

Then install the resulting ZIP from **Settings → Plugins → Install Plugin from Disk**.

### Features

**Syntax highlighting** — powered by the same TextMate grammar used by the VS Code extension.

**LSP integration** — diagnostics, completions, hover, go-to-definition, and rename via `writ-lsp` (must be on `$PATH`).

**Comment toggling** — `Cmd+/` (macOS) or `Ctrl+/` (Linux/Windows) for line and block comments.

**File type recognition** — `.writ` files detected automatically with a dedicated file icon.
