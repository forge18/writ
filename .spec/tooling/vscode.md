# VS Code Extension

> **Status:** Draft

## 1. Purpose

This spec defines the Writ VS Code extension — the primary editor integration. The extension hosts the LSP server and adds game-dev-specific tooling on top.

## 2. Dependencies

| Depends On                      | Relationship                                                  |
|---------------------------------|---------------------------------------------------------------|
| [lsp.md](lsp.md)                | Extension launches the LSP server as a subprocess             |
| [debug.md](../runtime/debug.md) | Breakpoints and hot reload are surfaced through the extension |

---

## 3. Features

### 3.1 Syntax Highlighting

TextMate grammar for `.writ` files providing syntax highlighting for:

- Keywords (`class`, `func`, `trait`, `enum`, `import`, `export`, `start`, `yield`, `when`, `let`, `var`, `const`, `public`, `private`, `extends`, `with`)
- Primitive types (`int`, `float`, `string`, `bool`, etc.)
- String interpolation (`$name`, `${expression}`)
- Operators and punctuation
- Comments (`//`, `/* */`)
- Doc comments
- Numeric literals
- String literals including multiline `"""`

### 3.2 LSP Integration

The extension launches `writ-lsp` as a background process on workspace open and connects to it via stdin/stdout. All LSP features are surfaced through VS Code's standard interfaces:

- Autocomplete via IntelliSense
- Go to Definition via F12
- Find References via Shift+F12
- Inline error squiggles and the Problems panel
- Hover documentation on mouseover
- Rename via F2

### 3.3 Debugger Integration

The extension integrates with VS Code's Debug Adapter Protocol to expose Writ's VM breakpoints.

**Workflow:**

1. Developer sets a breakpoint by clicking the gutter in a `.writ` file
2. Extension sends the breakpoint to the host application via Debug Adapter Protocol
3. Host application registers the breakpoint on its VM instance
4. VM pauses when the breakpoint is hit
5. Host application notifies the extension via DAP
6. Extension highlights the paused line and shows local variables

This requires the host application to implement the DAP bridge. The extension provides the VS Code side.

**Supported debug actions:**

- Set / remove breakpoints
- Continue
- Step Over
- Step Into
- View call stack
- View local variables at the paused frame

### 3.4 Hot Reload

The extension watches `.writ` files for changes and triggers hot reload on save.

**Workflow:**

1. Developer saves a `.writ` file
2. Extension detects the file change
3. Extension sends a hot reload request to the host application via a configurable mechanism (TCP socket, named pipe, or file sentinel)
4. Host application calls `vm.reload(path)` on its VM
5. Extension shows a status bar notification: `Writ: reloaded entities/player.writ`

The reload mechanism is configurable because different host applications have different inter-process communication setups.

### 3.5 Error Display

Compile errors from the LSP are displayed as:

- Red squiggles in the editor at the error location
- Entries in the Problems panel
- Hover tooltips on the squiggled text

Error messages follow the format defined in [error-messages.md](error-messages.md).

---

## 4. Configuration

The extension adds the following settings to VS Code:

| Setting                    | Default            | Description                                              |
|----------------------------|--------------------|----------------------------------------------------------|
| `writ.lspPath`             | `writ-lsp`         | Path to the LSP server binary                            |
| `writ.hotReload.enabled`   | `true`             | Enable hot reload on save                                |
| `writ.hotReload.mechanism` | `"socket"`         | Communication mechanism (`"socket"`, `"pipe"`, `"file"`) |
| `writ.hotReload.address`   | `"127.0.0.1:7777"` | Address for socket-based hot reload                      |

---

## 5. File Association

The extension registers `.writ` as the Writ language ID. All VS Code language features (search, diff, encoding) apply to `.writ` files automatically.

---

## 6. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
