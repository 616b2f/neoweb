-- lua/lib/ui.lua
-- Declarative Iced UI primitives in Lua for Neoweb

local _M = {}

local callbacks = {}
local next_callback_id = 1

--- Register a Lua callback and get a numeric ID
function _M.register_callback(fn)
    if type(fn) ~= "function" then
        return nil
    end
    local id = next_callback_id
    next_callback_id = (next_callback_id % 10000) + 1
    callbacks[id] = fn
    return id
end

--- Invoke a registered callback by ID
function _M.invoke_callback(id)
    local fn = callbacks[tonumber(id)]
    if fn then
        local ok, err = pcall(fn)
        if not ok then
            print("[Neoweb UI] Error invoking callback " .. tostring(id) .. ": " .. tostring(err))
        end
        return true
    end
    return false
end

--- Clear callback registry
function _M.clear_callbacks()
    callbacks = {}
    next_callback_id = 1
end

--- Text primitive: iced::widget::text
function _M.text(content, opts)
    opts = opts or {}
    return {
        type = "text",
        content = tostring(content or ""),
        size = opts.size or 12,
        color = opts.color or "primary", -- "primary", "muted", "blue", "yellow", "green", "red", "purple"
        width = opts.width, -- number or "fill"
        height = opts.height,
        wrapping = opts.wrapping or "none",
        align = opts.align,
    }
end

--- Space primitive: iced::widget::Space
function _M.space(opts)
    opts = opts or {}
    return {
        type = "space",
        width = opts.width,
        height = opts.height,
    }
end

--- Image primitive: iced::widget::image
function _M.image(opts)
    opts = opts or {}
    return {
        type = "image",
        data = opts.data,
        path = opts.path,
        width = opts.width or 16,
        height = opts.height or 16,
    }
end

--- Spinner primitive: iced_aw::Spinner
function _M.spinner(opts)
    opts = opts or {}
    return {
        type = "spinner",
        width = opts.width or 16,
        height = opts.height or 16,
        circle_radius = opts.circle_radius or 1.5,
    }
end

--- Row primitive: iced::widget::row
function _M.row(children, opts)
    opts = opts or {}
    return {
        type = "row",
        children = children or {},
        spacing = opts.spacing or 0,
        align = opts.align or "center", -- "center", "start", "end"
        width = opts.width,
        height = opts.height,
    }
end

--- Column primitive: iced::widget::column
function _M.column(children, opts)
    opts = opts or {}
    return {
        type = "column",
        children = children or {},
        spacing = opts.spacing or 0,
        align = opts.align or "start",
        width = opts.width,
        height = opts.height,
    }
end

--- Container primitive: iced::widget::container
function _M.container(child, opts)
    opts = opts or {}
    return {
        type = "container",
        child = child,
        padding = opts.padding, -- number or { v, h }
        width = opts.width or "fill",
        height = opts.height,
        style = opts.style or "none", -- "surface", "active", "dark", "none"
    }
end

--- Button primitive: iced::widget::button
function _M.button(child, opts)
    opts = opts or {}
    local cb_id = nil
    if opts.on_press then
        cb_id = _M.register_callback(opts.on_press)
    end
    return {
        type = "button",
        child = child,
        padding = opts.padding or { 3, 6 },
        width = opts.width or "shrink",
        height = opts.height,
        style = opts.style or "item_normal", -- "item_normal", "item_selected", "tab_active", "tab_inactive", "close_btn", "default"
        callback_id = cb_id,
    }
end

function _M.render_tabs(data, w)
    local tabs_mod = package.loaded["ui.tabs"] or require("ui.tabs")
    return tabs_mod.build(data, w)
end

function _M.render_status(data, w)
    local status_mod = package.loaded["ui.status"] or require("ui.status")
    return status_mod.build(data, w)
end

function _M.render_notification(data, w)
    local notify_mod = package.loaded["ui.notify"] or require("ui.notify")
    return notify_mod.build(data, w)
end

function _M.render_prompt(data, w)
    local prompt_mod = package.loaded["ui.prompt"] or require("ui.prompt")
    return prompt_mod.build(data, w)
end

--- Query mandatory tab strip height from ui.tabs without fallback
function _M.get_tab_strip_height()
    local tabs_mod = package.loaded["ui.tabs"] or require("ui.tabs")
    assert(tabs_mod and tabs_mod.get_height, "ui.tabs module must export get_height")
    return tabs_mod.get_height()
end

--- Query mandatory bottom bar height from ui.status without fallback
function _M.get_bottom_bar_height()
    local status_mod = package.loaded["ui.status"] or require("ui.status")
    assert(status_mod and status_mod.get_height, "ui.status module must export get_height")
    return status_mod.get_height()
end

return _M
