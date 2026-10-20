-- lua/lib/window.lua
-- Window module providing lifecycle signals and window management for Neoweb (Luakit API parity)

local lousy = require("lousy")
local _M = {}

local windows = {}

--- Register a signal callback for window lifecycle events (e.g. "close", "init")
function _M.add_signal(name, func)
    lousy.signal.add(_M, name, func)
    if _G.w then
        lousy.signal.add(_G.w, name, func)
    end
end

--- Emit a window lifecycle signal
function _M.emit_signal(name, ...)
    lousy.signal.emit(name, _M, ...)
    if _G.w then
        lousy.signal.emit(name, _G.w, ...)
    end
end

--- Initialize window instance
function _M.init(w)
    w = w or _G.w
    if not w then return end
    table.insert(windows, w)
    lousy.signal.setup(w)
    return w
end

return _M
