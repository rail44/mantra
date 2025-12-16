-- Mantra LSP for init.lua
-- Usage: Add to your init.lua or source this file

local mantra_bin = vim.fn.expand("~/src/github.com/rail44/mantra/rust/target/debug/mantra")

local function start_mantra()
  return vim.lsp.start({
    name = "mantra",
    cmd = { mantra_bin },
    cmd_env = { RUST_LOG = "mantra=debug" },
    root_dir = vim.fn.expand("%:p:h"),
  })
end

-- Auto-start for Go files
vim.api.nvim_create_autocmd("FileType", {
  pattern = "go",
  callback = start_mantra,
})

-- Restart command (use after cargo build)
vim.api.nvim_create_user_command("MantraRestart", function()
  for _, client in ipairs(vim.lsp.get_clients({ name = "mantra" })) do
    vim.lsp.stop_client(client.id, true)
  end
  vim.defer_fn(function()
    start_mantra()
    print("Mantra restarted")
  end, 100)
end, {})

-- Start if current buffer is Go
if vim.bo.filetype == "go" then
  start_mantra()
end
