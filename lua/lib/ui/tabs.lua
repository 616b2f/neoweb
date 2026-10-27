-- lua/lib/ui/tabs.lua
-- Declarative tab strip UI component in Lua for Neoweb

local ui = require("ui")

local _M = {}

_M.HEIGHT = 28

--- Get tab strip height in pixels
function _M.get_height()
    assert(_M.HEIGHT, "ui.tabs: HEIGHT must be defined")
    return _M.HEIGHT
end

--- Set tab strip height in pixels
function _M.set_height(height)
    local h = tonumber(height)
    assert(h and h > 0, "ui.tabs: height must be a positive number")
    _M.HEIGHT = h
end

local function utf8_len(str)
    if not str then return 0 end
    local count = 0
    local byte_pos = 1
    local len = #str
    while byte_pos <= len do
        local b = str:byte(byte_pos)
        if not b then break end
        if b < 128 then
            byte_pos = byte_pos + 1
        elseif b < 224 then
            byte_pos = byte_pos + 2
        elseif b < 240 then
            byte_pos = byte_pos + 3
        else
            byte_pos = byte_pos + 4
        end
        count = count + 1
    end
    return count
end

local function utf8_truncate(str, max_chars)
    if not str or max_chars <= 0 then return "" end
    local count = 0
    local byte_pos = 1
    local len = #str
    while byte_pos <= len and count < max_chars do
        local b = str:byte(byte_pos)
        if not b then break end
        if b < 128 then
            byte_pos = byte_pos + 1
        elseif b < 224 then
            byte_pos = byte_pos + 2
        elseif b < 240 then
            byte_pos = byte_pos + 3
        else
            byte_pos = byte_pos + 4
        end
        count = count + 1
    end
    if byte_pos > len then
        return str
    end
    return str:sub(1, byte_pos - 1)
end

function _M.build(data, w)
    w = w or _G.w
    data = data or {}
    _M.current = data
    local tab_height = _M.get_height()
    local active_idx = data.active_index or 1
    local tabs = data.tabs or {}
    local num_tabs = math.max(1, #tabs)
    local total_width = tonumber(data.width) or 1280
    local tab_tile_width = total_width / num_tabs

    local tab_items = {}

    for _, t in ipairs(tabs) do
        local is_active = (t.index == active_idx)
        local raw_title = (t.title and t.title ~= "") and t.title or "New Tab"

        -- Dynamically calculate available width for title inside this tab tile:
        -- tab_pill horizontal padding: 16px ({0, 8})
        -- tab_inner spacing: 4px (only when close button is shown)
        -- close_btn: 16px (same size as favicon, only when tab has focus)
        -- num_text: ~20px
        -- fav_widget: 16px
        -- space: 6px
        local close_btn_w = is_active and 16 or 0
        local inner_spacing = is_active and 4 or 0
        local non_title_width = 16 + inner_spacing + close_btn_w + ((#tostring(t.index) + 2) * 7.5) + 16 + 6
        local avail_title_width = math.max(0, tab_tile_width - non_title_width)
        local char_width = 7.5
        local max_chars = math.max(1, math.floor(avail_title_width / char_width))

        local title = raw_title
        local ulen = utf8_len(raw_title)
        if ulen > max_chars then
            if max_chars <= 3 then
                title = utf8_truncate(raw_title, max_chars) .. "..."
            else
                title = utf8_truncate(raw_title, max_chars - 3) .. "..."
            end
        end

        local num_text = ui.text(tostring(t.index) .. ": ", {
            size = 12,
            color = is_active and "blue" or "muted",
            wrapping = "none",
        })

        local fav_widget = nil
        if t.is_loading and not (t.favicon and t.favicon.is_custom) then
            fav_widget = ui.spinner({
                width = 16,
                height = 16,
                circle_radius = 1.5,
            })
        elseif t.favicon then
            if t.favicon.data and #t.favicon.data > 0 then
                fav_widget = ui.image({
                    data = t.favicon.data,
                    width = 16,
                    height = 16,
                })
            elseif t.favicon.glyph and t.favicon.glyph ~= "" then
                fav_widget = ui.text(t.favicon.glyph, {
                    size = 13,
                    wrapping = "none",
                })
            end
        end
        if not fav_widget then
            fav_widget = ui.text("◆", { size = 13, color = "blue", wrapping = "none" })
        end

        local title_text = ui.text(title, {
            size = 13,
            color = is_active and "primary" or "muted",
            width = "fill",
            wrapping = "none",
        })

        local tab_idx = t.index
        local select_btn = ui.button(
            ui.row({
                num_text,
                fav_widget,
                ui.space({ width = 6 }),
                title_text,
            }, { spacing = 0, align = "center", width = "fill", height = tab_height }),
            {
                padding = { 0, 0 },
                width = "fill",
                height = tab_height,
                style = "none",
                on_press = function ()
                    if w and w.goto_tab then
                        w:goto_tab(tab_idx - 1)
                    end
                end,
            }
        )

        local inner_children = { select_btn }

        if is_active then
            local close_glyph = ui.text("×", {
                size = 12,
                color = "muted",
                wrapping = "none",
                align = "center",
                width = 16,
                height = 16,
            })
            local close_btn = ui.button(
                close_glyph,
                {
                    padding = { 0, 0 },
                    width = 16,
                    height = 16,
                    style = "close_btn",
                    on_press = function ()
                        if w and w.close_tab then
                            w:close_tab(tab_idx - 1)
                        end
                    end,
                }
            )
            table.insert(inner_children, close_btn)
        end

        local tab_inner = ui.row(inner_children, {
            spacing = inner_spacing,
            align = "center",
            width = "fill",
            height = tab_height,
        })

        local tab_pill = ui.container(tab_inner, {
            padding = { 0, 8 },
            width = "fill",
            height = tab_height,
            style = is_active and "tab_active" or "tab_inactive",
        })

        table.insert(tab_items, tab_pill)
    end

    local row = ui.row(tab_items, { spacing = 0, align = "center", width = "fill", height = tab_height })
    return ui.container(row, {
        padding = 0,
        width = "fill",
        height = tab_height,
        style = "tab_strip",
    })
end

--- Return current tabs data table { active_index = ..., tabs = { ... } }
function _M.get_tabs()
    if _M.current and _M.current.tabs and #_M.current.tabs > 0 then
        return _M.current
    end
    local w = _G.w
    local uri = (w and w.uri and w:uri()) or "about:neoweb"
    local title = (w and w.title and w:title()) or "Neoweb"
    return {
        active_index = 1,
        tabs = {
            { index = 1, uri = uri, title = title }
        }
    }
end

return _M
