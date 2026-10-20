-- lua/lib/ui/prompt.lua
-- Declarative command and search prompt bar UI component in Lua for Neoweb

local ui = require("ui")

local _M = {}

--- Build prompt bar UI element using Iced primitives
-- @param props table:
--   prefix: prompt symbol prefix (e.g. ":", "/", "?")
--   text: input string (e.g. ":tabnew")
--   completion_view: optional pre-rendered completion LuaUiNode
--   show_completion: optional boolean (default true)
function _M.build(props, w)
    props = props or {}
    local prefix = props.prefix or ":"
    local input = props.text or ""

    local display_val = input
    if display_val == "" then
        display_val = prefix
    end

    local prompt_sym = ":"
    local rest = display_val
    if display_val:sub(1, 1) == ":" then
        prompt_sym = ":"
        rest = display_val:sub(2)
    elseif display_val:sub(1, 1) == "/" then
        prompt_sym = "/"
        rest = display_val:sub(2)
    elseif display_val:sub(1, 1) == "?" then
        prompt_sym = "?"
        rest = display_val:sub(2)
    end

    local prompt_row = ui.row({
        ui.text(prompt_sym, { size = 13, color = "yellow" }),
        ui.space({ width = 4 }),
        ui.text(rest, { size = 13, color = "primary" }),
        ui.text("▌", { size = 13, color = "yellow" }),
    }, { spacing = 0, align = "center" })

    local prompt_box = ui.container(prompt_row, {
        padding = { 4, 8 },
        width = "fill",
        style = "prompt_bar",
    })

    -- Optional completion list view
    local comp_view = props.completion_view
    if not comp_view and props.show_completion ~= false then
        local comp = package.loaded["completion"]
        if comp and comp.get_view then
            comp_view = comp.get_view()
        end
    end

    if comp_view then
        local combined = ui.column({
            comp_view,
            prompt_box,
        }, { spacing = 0, width = "fill" })
        return ui.container(combined, {
            width = "fill",
            style = "none",
        })
    end

    return prompt_box
end

return _M
