# Writ for Vim / Neovim

Language support for the [Writ](https://github.com/forge18/writ) scripting language in Vim and Neovim.

> **Alpha** — Writ is under active development. Expect breaking changes.

## Features

| Feature | Vim | Neovim |
|---|---|---|
| Syntax highlighting | Yes | Yes |
| Filetype detection (`.writ`) | Yes | Yes |
| Filetype settings (comments, indentation) | Yes | Yes |
| LSP (diagnostics, completions, hover, go-to-definition) | — | Yes |
| Hot reload on save | — | Yes |

## Installation

### lazy.nvim

```lua
{
  "forge18/writ",
  config = function()
    require("writ").setup()
  end,
}
```

### packer.nvim

```lua
use {
  "forge18/writ",
  config = function()
    require("writ").setup()
  end,
}
```

### vim-plug

```vim
Plug 'forge18/writ'
```

For Neovim, add to your `init.lua`:

```lua
require("writ").setup()
```

### Manual

Clone `extensions/vim-writ` into your Vim runtimepath, or symlink it:

```sh
ln -s /path/to/writ/extensions/vim-writ ~/.vim/pack/writ/start/vim-writ    # Vim
ln -s /path/to/writ/extensions/vim-writ ~/.local/share/nvim/site/pack/writ/start/vim-writ  # Neovim
```

## Configuration

Pass options to `require("writ").setup()` (Neovim only):

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

## Requirements

- Neovim 0.10+ for LSP and hot reload features
- `writ-lsp` on your `$PATH` for LSP features
- Traditional Vim supports syntax highlighting and filetype settings only
