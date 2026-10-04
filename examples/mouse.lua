-- Interactive mouse/touch test.
--
-- Run with:
--   ocplay --interactive examples/computer.yaml examples/mouse.lua
--
-- Click or drag to paint the cell under the pointer, scroll to report the
-- wheel, press any key or right-click to quit. Requires `components.mouse: true`.

local event = require("event")
local term = require("term")
local gpu = require("component").gpu
local computer = require("computer")

local colors = { 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0x00FFFF, 0xFF00FF }

term.clear()
gpu.setBackground(0x000000)
gpu.setForeground(0xFFFFFF)
print("ocplay mouse test")
print("click/drag = paint, scroll = report, key/right-click = quit")

local color = 0
local start = computer.uptime()

local function status(text)
  gpu.setBackground(0x000000)
  gpu.setForeground(0xFFFFFF)
  gpu.set(1, 1, text .. string.rep(" ", gpu.getResolution() - #text))
end

while true do
  local e = table.pack(event.pull(120))
  if e.n == 0 then
    -- Nothing arrived for two minutes; exit so a non-interactive run cannot hang.
    break
  end
  local name, x, y, data = e[1], e[3], e[4], e[5]

  if name == "key_down" or ((name == "touch" or name == "drop") and data == 1) then
    break
  elseif name == "scroll" then
    status(string.format("scroll %s,%s delta %s", tostring(x), tostring(y), tostring(data)))
  elseif name == "touch" or name == "drag" or name == "drop" then
    color = color % #colors + 1
    gpu.setBackground(colors[color])
    gpu.set(x, y, " ")
    status(string.format("%s %s,%s button %s", name, tostring(x), tostring(y), tostring(data)))
  end
end

gpu.setBackground(0x000000)
gpu.setForeground(0xFFFFFF)
term.clear()
print(string.format("mouse test finished after %ds", math.floor(computer.uptime() - start)))
computer.shutdown()
