-- lua/lib/ui/status.lua
-- Declarative status bar UI component in Lua for Neoweb

local ui = require("ui")

local _M = {}

_M.HEIGHT = 28

--- Get status bar height in pixels
function _M.get_height()
    assert(_M.HEIGHT, "ui.status: HEIGHT must be defined")
    return _M.HEIGHT
end

--- Set status bar height in pixels
function _M.set_height(height)
    local h = tonumber(height)
    assert(h and h > 0, "ui.status: height must be a positive number")
    _M.HEIGHT = h
end

local mode_badges = {
    ["normal"] = { "NORMAL", "green" },
    ["command"] = { "COMMAND", "yellow" },
    ["insert"] = { "INSERT", "purple" },
    ["follow"] = { "FOLLOW", "red" },
    ["passthrough"] = { "PASSTHROUGH", "yellow" },
    ["search"] = { "SEARCH", "blue" },
}

function _M.build(data, w)
    w = w or _G.w
    data = data or {}
    local mode = data.mode or "normal"
    local modes_mod = package.loaded["modes"]
    local mode_info = (modes_mod and modes_mod.get_mode_info) and modes_mod.get_mode_info(mode)
    local badge_text = (mode_info and mode_info.badge and mode_info.badge.text)
        or (mode_badges[mode] and mode_badges[mode][1])
        or mode:upper()
    if mode == "follow" and modes_mod and modes_mod.is_follow_tab and modes_mod.is_follow_tab() then
        badge_text = "FOLLOW-TAB"
    end
    local badge_color = (mode_info and mode_info.badge and mode_info.badge.color)
        or (mode_badges[mode] and mode_badges[mode][2])
        or "primary"

    local mode_badge = ui.container(
        ui.text(badge_text, { size = 11, color = badge_color }),
        { padding = { 2, 6 }, width = "shrink" }
    )

    local display_uri = data.uri or "about:blank"
    if data.hover_uri and data.hover_uri ~= "" then
        display_uri = "🔗 " .. data.hover_uri
    elseif display_uri == "" then
        display_uri = "about:blank"
    end

    local progress_str = ""
    local prog = tonumber(data.progress or 1.0)
    if prog < 1.0 then
        progress_str = string.format(" [%.0f%%]", prog * 100)
    end

    local left_items = {
        mode_badge,
        ui.space({ width = 8 }),
        ui.text(display_uri, { size = 12, color = "primary" }),
    }
    if progress_str ~= "" then
        table.insert(left_items, ui.text(progress_str, { size = 12, color = "blue" }))
    end

    local left_section = ui.row(left_items, { spacing = 0, align = "center" })

    -- SSL indicator
    local ssl = "⚙ INTERNAL"
    if display_uri:find("^https://") then
        ssl = "🔒 HTTPS"
    elseif display_uri:find("^http://") then
        ssl = "🔓 HTTP"
    end

    -- Scroll text
    local scroll_val = tonumber(data.scroll_pct or 0.0)
    local scroll_text = string.format("%.0f%%", scroll_val)
    if scroll_val <= 0.5 then
        scroll_text = "Top"
    elseif scroll_val >= 99.0 then
        scroll_text = "Bot"
    end

    local tab_counter = string.format("[%d/%d]", (data.tab_index or 0) + 1, data.total_tabs or 1)
    local key_buf = data.key_buffer or ""

    local right_items = {}
    if key_buf ~= "" then
        table.insert(right_items, ui.text(key_buf, { size = 12, color = "yellow" }))
        table.insert(right_items, ui.space({ width = 8 }))
    end
    table.insert(right_items, ui.text(ssl, { size = 11, color = "muted" }))
    table.insert(right_items, ui.space({ width = 8 }))
    table.insert(right_items, ui.text(tab_counter, { size = 11, color = "muted" }))
    table.insert(right_items, ui.space({ width = 8 }))
    table.insert(right_items, ui.text(scroll_text, { size = 11, color = "primary" }))

    local right_section = ui.row(right_items, { spacing = 0, align = "center" })

    local full_row = ui.row({
        left_section,
        ui.space({ width = "fill" }),
        right_section,
    }, { spacing = 0, align = "center" })

    return ui.container(full_row, {
        padding = { 4, 8 },
        width = "fill",
        height = _M.get_height(),
        style = "status_bar",
    })
end

return _M
