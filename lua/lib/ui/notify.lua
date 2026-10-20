-- lua/lib/ui/notify.lua
-- Declarative notification bar UI component in Lua for Neoweb

local ui = require("ui")

local _M = {}

local level_colors = {
    ["error"] = "red",
    ["warning"] = "yellow",
    ["info"] = "blue",
}

_M.current = nil

--- Store active notification state
function _M.notify(message, level)
    if not message or message == "" then
        _M.current = nil
        return
    end
    _M.current = {
        level = level or "info",
        message = tostring(message),
    }
end

--- Clear active notification
function _M.clear()
    _M.current = nil
end

--- Get current notification data
function _M.get_current()
    return _M.current
end

--- Build declarative notification bar UI element
function _M.build(data, w)
    w = w or _G.w
    data = data or _M.current
    if not data or not data.message or data.message == "" then
        return nil
    end

    local level = data.level or "info"
    local color = level_colors[level] or "blue"
    local badge = string.format("[%s] ", level:upper())

    local content = ui.row({
        ui.text(badge, { size = 11, color = color }),
        ui.text(data.message, { size = 12, color = "primary" }),
    }, { spacing = 0, align = "center" })

    return ui.container(content, {
        padding = { 3, 8 },
        width = "fill",
        style = "dark",
    })
end

--- Get declarative UI node for active notification
function _M.get_view(w)
    if not _M.current then
        return nil
    end
    return _M.build(_M.current, w)
end

return _M
