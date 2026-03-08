local M = {}

local pipe_path = vim.fn.expand("~/.local/share/cortex/editor.pipe")

--- Write a JSON event to the cortex editor pipe.
--- @param event_type string
--- @param path string
local function send_event(event_type, path)
  if path == "" then
    return
  end

  local abs_path = vim.fn.fnamemodify(path, ":p")
  local timestamp = os.date("!%Y-%m-%dT%H:%M:%SZ")

  local payload = vim.fn.json_encode({
    type = event_type,
    path = abs_path,
    timestamp = timestamp,
  })

  -- Non-blocking write: open, write, close in one shot
  local fd = vim.loop.fs_open(pipe_path, "a", tonumber("644", 8))
  if fd then
    vim.loop.fs_write(fd, payload .. "\n", -1)
    vim.loop.fs_close(fd)
  end
end

function M.setup()
  local group = vim.api.nvim_create_augroup("CortexEditor", { clear = true })

  vim.api.nvim_create_autocmd("BufEnter", {
    group = group,
    callback = function(args)
      local path = vim.api.nvim_buf_get_name(args.buf)
      send_event("file_open", path)
    end,
  })

  vim.api.nvim_create_autocmd("BufWritePost", {
    group = group,
    callback = function(args)
      local path = vim.api.nvim_buf_get_name(args.buf)
      send_event("file_save", path)
    end,
  })

  vim.api.nvim_create_autocmd("BufDelete", {
    group = group,
    callback = function(args)
      local path = vim.api.nvim_buf_get_name(args.buf)
      send_event("file_delete", path)
    end,
  })
end

return M
