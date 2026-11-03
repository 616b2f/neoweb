-- lua/lib/ui/dialog.lua
-- Declarative JavaScript dialog UI component for Neoweb

local ui = require("ui")

local _M = {}

--- Build declarative dialog box element
-- @param data table: { id, type = "alert"|"confirm"|"prompt", message, input }
-- @param w window userdata
function _M.build(data, w)
    w = w or _G.w
    if not data or not data.id then
        return nil
    end

    local dtype = data.type or "alert"
    local badge_text = "[ALERT]"
    local badge_color = "yellow"
    if dtype == "confirm" then
        badge_text = "[CONFIRM]"
        badge_color = "blue"
    elseif dtype == "prompt" then
        badge_text = "[PROMPT]"
        badge_color = "magenta"
    end

    local ok_cb = ui.register_callback(function()
        local d = package.loaded["dialog"]
        if d and d.confirm then
            d.confirm(w)
        end
    end)

    local cancel_cb = ui.register_callback(function()
        local d = package.loaded["dialog"]
        if d and d.cancel then
            d.cancel(w)
        end
    end)

    local row_children = {
        ui.text(badge_text .. " ", { size = 12, color = badge_color }),
        ui.text(data.message or "", { size = 12, color = "primary" }),
    }

    if dtype == "prompt" then
        table.insert(row_children, ui.space({ width = 8 }))
        local input_text = data.input or ""
        table.insert(row_children, ui.container(
            ui.row({
                ui.text(input_text, { size = 12, color = "primary" }),
                ui.text("▌", { size = 12, color = "yellow" }),
            }, { spacing = 0, align = "center" }),
            { padding = { 2, 6 }, style = "prompt_bar" }
        ))
    end

    table.insert(row_children, ui.space({ width = 12 }))

    -- OK Button
    table.insert(row_children, ui.button(
        ui.text(" OK ", { size = 11, color = "background" }),
        { callback_id = ok_cb, style = "active" }
    ))

    -- Cancel Button for confirm & prompt
    if dtype ~= "alert" then
        table.insert(row_children, ui.space({ width = 6 }))
        table.insert(row_children, ui.button(
            ui.text(" Cancel ", { size = 11, color = "primary" }),
            { callback_id = cancel_cb, style = "item_normal" }
        ))
    end

    local dialog_row = ui.row(row_children, { spacing = 4, align = "center" })

    return ui.container(dialog_row, {
        padding = { 6, 10 },
        width = "fill",
        style = "dark",
    })
end

return _M
