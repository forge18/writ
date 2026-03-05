# LSP

> **Crate:** `writ-lsp` | **Status:** Draft

## 1. Purpose

This spec defines Writ's Language Server Protocol implementation. The LSP server exposes the compiler pipeline's knowledge — types, definitions, references, errors — to editors via the standard LSP protocol.

## 2. Dependencies

| Depends On                                   | Relationship                                        |
|----------------------------------------------|-----------------------------------------------------|
| [type-system.md](../language/type-system.md) | All LSP features derive from type checker output    |
| [syntax.md](../language/syntax.md)           | Parser provides AST for hover and go-to             |
| [error-messages.md](error-messages.md)       | Inline errors use the same format as compile errors |

---

## 3. Architecture

The LSP server reuses the full compiler pipeline — lexer, parser, and type checker — on every file change. It does not maintain a separate analysis pass.

```
Editor keystroke
  → LSP server receives file change
  → Lexer + Parser → AST
  → Type Checker → typed AST + errors
  → LSP responds with completions / errors / hover / etc.
```

The type checker runs incrementally where possible — only re-checking files affected by the change.

---

## 4. Features

### 4.1 Autocomplete

Triggered on `.`, `::`, and identifier prefix. The type checker knows the type of every expression, so completions are fully type-aware.

**Completion sources:**

- Fields and methods on the current type
- Imported names from `import { ... } from "..."`
- Wildcard namespace members (`enemy::`)
- Global host-registered functions and types
- Standard library functions
- Local variables in scope
- Keywords

### 4.2 Go to Definition

Resolves the definition of any identifier — variable, function, class, trait, enum, or imported name. Supports cross-file navigation.

Works for:

- Script-defined names (navigates to `.writ` source)
- Host-registered names (reports that the definition is in host Rust code)

### 4.3 Find References

Lists all usages of a symbol across all `.writ` files in the project. Powered by the type checker's name resolution graph.

### 4.4 Inline Errors

Compile errors and type errors are published to the editor as diagnostics on the affected line and column. Uses the same error format as the command-line compiler. See [error-messages.md](error-messages.md).

Errors are refreshed on every file change.

### 4.5 Hover Documentation

Hovering over any identifier shows:

- Its fully resolved type
- The doc comment attached to its declaration (if any)
- For host-registered types and functions: the registered description (if provided)

### 4.6 Rename Refactoring

Renames a symbol and all its references across all `.writ` files. The type checker's reference graph provides the full set of usages. Read-only references (host-registered names) cannot be renamed.

---

## 5. Protocol

The LSP server communicates via `stdin`/`stdout` using the standard JSON-RPC LSP protocol. It is launched as a subprocess by the editor extension.

**Supported LSP methods:**

| Method                            | Feature             |
|-----------------------------------|---------------------|
| `textDocument/completion`         | Autocomplete        |
| `textDocument/definition`         | Go to definition    |
| `textDocument/references`         | Find references     |
| `textDocument/publishDiagnostics` | Inline errors       |
| `textDocument/hover`              | Hover documentation |
| `textDocument/rename`             | Rename refactoring  |
| `textDocument/didOpen`            | File opened         |
| `textDocument/didChange`          | File changed        |
| `textDocument/didClose`           | File closed         |
| `initialize`                      | Handshake           |
| `shutdown`                        | Shutdown            |

---

## 6. Edge Cases

1. **Given** a file has a parse error, **then** the LSP provides error diagnostics and best-effort completions based on the partial AST.
2. **Given** a file imports from a path that does not exist, **then** the import produces an error diagnostic and the imported names are treated as unknown types.
3. **Given** the same name is registered by multiple host types, **then** the LSP reports the ambiguity and shows all candidates.
4. **Given** a file is very large, **then** incremental re-checking applies — only the changed function body is re-type-checked where possible.

---

## 7. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
