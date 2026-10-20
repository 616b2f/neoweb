-- lua/lib/ui/item_list.lua
-- Generic item list view built in Lua using Iced primitives

local ui = require("ui")
local _M = {}

--- Build an interactive list view element using Iced primitives
-- @param props table:
--   items: array of { id, title, description, badge, selected }
--   selected_index: optional number (1-based index)
--   max_visible: optional number (default 8)
--   title_width: optional number (default 130)
--   on_select: callback function(item, index)
function _M.build(props)
    props = props or {}
    local items = props.items or {}
    if #items == 0 then
        return ui.space()
    end

    local max_visible = props.max_visible or 8
    local title_width = props.title_width or 160
    local on_select = props.on_select or function () end
    local total = #items

    -- Find selected index
    local selected_idx = props.selected_index or 1
    for i, it in ipairs(items) do
        if it.selected then
            selected_idx = i
            break
        end
    end

    -- Compute slice bounds for windowing
    local start_idx = 1
    if total > max_visible and selected_idx > math.floor(max_visible / 2) then
        start_idx = math.min(selected_idx - math.floor(max_visible / 2), total - max_visible + 1)
    end
    local end_idx = math.min(start_idx + max_visible - 1, total)

    local rows = {}

    for i = start_idx, end_idx do
        local it = items[i]
        local is_sel = (i == selected_idx) or (it.selected == true)

        -- Row content: optional badge + title (left aligned) + optional description
        local cols = {}

        -- 1. Optional badge
        if it.badge and it.badge ~= "" then
            table.insert(cols, ui.container(
                ui.text(it.badge, { size = 10, color = is_sel and "yellow" or "purple" }),
                { padding = { 1, 4 }, style = "surface", width = nil }
            ))
            table.insert(cols, ui.space({ width = 6 }))
        end

        -- 2. Title (first column - left aligned)
        table.insert(cols, ui.text(it.title or it.label or "", {
            size = 12,
            color = is_sel and "yellow" or "blue",
            width = title_width,
            wrapping = "none",
        }))

        -- 3. Space between columns
        table.insert(cols, ui.space({ width = 12 }))

        -- 4. Description (second column)
        if it.description and it.description ~= "" then
            table.insert(cols, ui.text(it.description, {
                size = 11,
                color = is_sel and "primary" or "muted",
                width = "fill",
                wrapping = "none",
            }))
        end

        local row_elem = ui.row(cols, { spacing = 0, align = "center", width = "fill" })

        -- Wrap in interactive button with click callback
        local btn_elem = ui.button(row_elem, {
            padding = { 3, 8 },
            width = "fill",
            style = is_sel and "item_selected" or "item_normal",
            on_press = function ()
                on_select(it, i)
            end,
        })

        table.insert(rows, btn_elem)
    end

    -- Optional footer counter
    if total > max_visible then
        local counter_txt = string.format("  [%d of %d items]", selected_idx, total)
        table.insert(rows, ui.container(
            ui.text(counter_txt, { size = 10, color = "muted" }),
            { padding = { 2, 6 } }
        ))
    end

    -- Return full styled container with column of rows
    return ui.container(
        ui.column(rows, { spacing = 1 }),
        { padding = { 4, 6 }, style = "surface", width = "fill" }
    )
end

return _M
