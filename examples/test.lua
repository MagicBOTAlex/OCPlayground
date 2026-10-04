-- Demo program: exercises colors, color depths and the text buffer.
local term = require("term")
local gpu = require("component").gpu

term.clear()

local w, h = gpu.getResolution()
print("ocplay demo on a " .. w .. "x" .. h .. " screen")
print("max depth: " .. gpu.maxDepth())

local max = gpu.maxDepth()
for _, depth in ipairs({ 1, 4, 8 }) do
  if depth <= max then
    local ok = pcall(gpu.setDepth, depth)
    if ok then
      io.write("depth " .. depth .. ": ")
      local count = (depth == 1) and 2 or 16
      for i = 0, count - 1 do
        gpu.setForeground(i, depth > 1)
        io.write("\u{2588}")
      end
      io.write("\n")
    end
  end
end

gpu.setDepth(max)
gpu.setForeground(0xFFFFFF)
gpu.setBackground(0x000000)
print("")
print("Done. Shutting down.")
