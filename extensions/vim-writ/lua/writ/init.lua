-- Writ LSP configuration for Neovim
-- Usage: require('writ').setup({ lsp_cmd = "writ-lsp" })

local M = {}

M.defaults = {
  lsp_cmd = "writ-lsp",
  hot_reload = {
    enabled = true,
    mechanism = "socket",
    address = "127.0.0.1:7777",
  },
}

function M.setup(opts)
  opts = vim.tbl_deep_extend("force", M.defaults, opts or {})

  -- Register LSP configuration
  vim.api.nvim_create_autocmd("FileType", {
    pattern = "writ",
    callback = function(args)
      vim.lsp.start({
        name = "writ-lsp",
        cmd = { opts.lsp_cmd },
        root_dir = vim.fs.root(args.buf, { ".git", "Cargo.toml" }),
      })
    end,
  })

  -- Hot reload on save
  if opts.hot_reload.enabled then
    vim.api.nvim_create_autocmd("BufWritePost", {
      pattern = "*.writ",
      callback = function(args)
        M._send_reload(opts.hot_reload, args.file)
      end,
    })
  end
end

function M._send_reload(config, file_path)
  local relative = vim.fn.fnamemodify(file_path, ":.")
  local payload = vim.json.encode({ type = "reload", file = relative })

  if config.mechanism == "socket" then
    local host, port = config.address:match("^(.+):(%d+)$")
    if not host or not port then
      vim.notify("Writ: invalid hot reload address: " .. config.address, vim.log.levels.ERROR)
      return
    end

    local handle = vim.uv.new_tcp()
    handle:connect(host, tonumber(port), function(err)
      if err then
        vim.schedule(function()
          vim.notify("Writ: reload failed - " .. err, vim.log.levels.WARN)
        end)
        handle:close()
        return
      end

      handle:write(payload .. "\n", function(write_err)
        if write_err then
          vim.schedule(function()
            vim.notify("Writ: reload write failed - " .. write_err, vim.log.levels.WARN)
          end)
        else
          vim.schedule(function()
            vim.notify("Writ: reloaded " .. relative, vim.log.levels.INFO)
          end)
        end
        handle:close()
      end)
    end)
  elseif config.mechanism == "file" then
    local f = io.open(config.address, "w")
    if f then
      f:write(relative)
      f:close()
      vim.notify("Writ: reloaded " .. relative, vim.log.levels.INFO)
    else
      vim.notify("Writ: reload failed - cannot write sentinel", vim.log.levels.WARN)
    end
  end
end

return M
