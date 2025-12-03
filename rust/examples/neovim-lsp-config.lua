-- Neovim LSP configuration for mantra
-- Usage: RUST_LOG=mantra=debug nvim --clean -c "luafile examples/neovim-lsp-config.lua" examples/go/simple.go

-- Update this path to your mantra binary
local mantra_bin = vim.fn.expand("~/src/github.com/rail44/mantra/rust/target/debug/mantra")

-- Enable filetype detection (disabled with -u NONE)
vim.cmd("filetype on")
vim.cmd("filetype plugin on")

-- Basic settings
vim.opt.number = true
vim.opt.signcolumn = "yes"

-- Start mantra LSP for Go files
vim.api.nvim_create_autocmd("FileType", {
  pattern = "go",
  callback = function()
    -- Use the directory of the current file as root_dir to find mantra.toml
    local file_dir = vim.fn.expand("%:p:h")
    local client_id = vim.lsp.start({
      name = "mantra",
      cmd = { mantra_bin, "lsp" },
      root_dir = file_dir,
    })
    if client_id then
      print("mantra LSP started with client_id: " .. client_id)
    else
      print("Failed to start mantra LSP")
    end
  end,
})

-- Debug commands
vim.api.nvim_create_user_command("MantraClients", function()
  print(vim.inspect(vim.lsp.get_clients()))
end, {})

vim.api.nvim_create_user_command("MantraDiag", function()
  print(vim.inspect(vim.diagnostic.get(0)))
end, {})

-- Keymaps for testing
vim.keymap.set("n", "<leader>c", function()
  print(vim.inspect(vim.lsp.get_clients()))
end, { desc = "Show LSP clients" })

vim.keymap.set("n", "<leader>d", function()
  vim.diagnostic.open_float()
end, { desc = "Show diagnostics" })

vim.keymap.set("n", "<leader>a", function()
  vim.lsp.buf.code_action()
end, { desc = "Code action" })

-- Start LSP for current buffer if it's a Go file (for when config is loaded after file is opened)
if vim.bo.filetype == "go" then
  local file_dir = vim.fn.expand("%:p:h")
  local client_id = vim.lsp.start({
    name = "mantra",
    cmd = { mantra_bin, "lsp" },
    root_dir = file_dir,
  })
  if client_id then
    print("mantra LSP started for current buffer (client_id: " .. client_id .. ")")
  end
end

print("mantra LSP config loaded. Commands: :MantraClients, :MantraDiag")
