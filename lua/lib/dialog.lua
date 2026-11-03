-- lua/lib/dialog.lua
-- JavaScript simple dialog delegation for Neoweb (alert, confirm, prompt)

local ui_dialog = require("ui.dialog")

local _M = {}

_M.current = nil

--- Show a JavaScript dialog request
function _M.show(w, id, dtype, message, default_val)
    w = w or _G.w
    _M.current = {
        id = id,
        type = dtype or "alert",
        message = tostring(message or ""),
        default = tostring(default_val or ""),
        input = tostring(default_val or ""),
    }

    local modes = package.loaded["modes"]
    if modes and modes.set_mode then
        modes.set_mode(w, "dialog")
    end

    if w and w.emit_signal then
        w:emit_signal("dialog::show", _M.current)
    end
end

--- Dismiss a dialog request if matching
function _M.dismiss(w, id)
    w = w or _G.w
    if _M.current and (not id or _M.current.id == id) then
        local old_id = _M.current.id
        _M.current = nil
        local modes = package.loaded["modes"]
        if modes and modes.get_mode and modes.get_mode() == "dialog" then
            modes.set_mode(w, "normal")
        end
        if w and w.emit_signal then
            w:emit_signal("dialog::dismiss", old_id)
        end
    end
end

--- Confirm the active dialog
function _M.confirm(w)
    w = w or _G.w
    if not _M.current then
        return
    end

    local cur = _M.current
    local val = (cur.type == "prompt") and cur.input or nil

    if w and w.confirm_dialog then
        w:confirm_dialog(cur.id, val)
    elseif w and w.respond_dialog then
        w:respond_dialog(cur.id, true, val)
    end

    _M.current = nil
    local modes = package.loaded["modes"]
    if modes and modes.get_mode and modes.get_mode() == "dialog" then
        modes.set_mode(w, "normal")
    end
end

--- Cancel the active dialog
function _M.cancel(w)
    w = w or _G.w
    if not _M.current then
        return
    end

    local cur = _M.current
    if w and w.cancel_dialog then
        w:cancel_dialog(cur.id)
    elseif w and w.respond_dialog then
        w:respond_dialog(cur.id, false, nil)
    end

    _M.current = nil
    local modes = package.loaded["modes"]
    if modes and modes.get_mode and modes.get_mode() == "dialog" then
        modes.set_mode(w, "normal")
    end
end

--- Get declarative UI node for the active dialog
function _M.get_view(w)
    w = w or _G.w
    if not _M.current then
        return nil
    end
    return ui_dialog.build(_M.current, w)
end

return _M
