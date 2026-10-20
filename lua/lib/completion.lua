-- lua/lib/completion.lua
-- Command, bookmark, and history completion engine in Lua (Luakit compatible)

local _M = {}

_M.current_items = {}
_M.current_index = 0
_M.is_active = false
_M.original_prompt = ""

--- Compute completions for the command bar input string
function _M.complete(input)
    local results = {}
    local trimmed = input or ""
    trimmed = trimmed:gsub("^%s*:%s*", "") -- strip leading ':'

    -- 1. If typing command name (before any space)
    if not trimmed:find("%s") then
        local cmds = (luakit and luakit.commands) or {}
        for name, entry in pairs(cmds) do
            local clean_name = name:gsub("^:+", "")
            if trimmed == "" or clean_name:sub(1, #trimmed) == trimmed or clean_name:find(trimmed, 1, true) then
                table.insert(results, {
                    label = clean_name,
                    description = (type(entry) == "table" and entry.desc) or "",
                    replacement = clean_name .. " ",
                })
            end
        end
        table.sort(results, function (a, b)
            local a_prefix = (trimmed ~= "" and a.label:sub(1, #trimmed) == trimmed)
            local b_prefix = (trimmed ~= "" and b.label:sub(1, #trimmed) == trimmed)
            if a_prefix ~= b_prefix then
                return a_prefix
            end
            return a.label < b.label
        end)
        return results
    end

    -- 2. If typing command arguments for navigation commands
    local cmd, query = trimmed:match("^(%S+)%s*(.*)$")
    cmd = cmd or ""
    query = query or ""

    if cmd == "open" or cmd == "tabopen" or cmd == "o" or cmd == "t" then
        -- Query bookmarks
        if luakit and luakit.get_bookmarks then
            local bms = luakit.get_bookmarks(query) or {}
            for _, b in ipairs(bms) do
                table.insert(results, {
                    label = "★ " .. (b.title or b.uri or ""),
                    description = b.uri or "",
                    replacement = cmd .. " " .. (b.uri or ""),
                })
                if #results >= 5 then break end
            end
        end

        -- Query history
        if luakit and luakit.get_history and #results < 8 then
            local hist = luakit.get_history(query, 8 - #results) or {}
            for _, h in ipairs(hist) do
                table.insert(results, {
                    label = h.title or h.uri or "",
                    description = h.uri or "",
                    replacement = cmd .. " " .. (h.uri or ""),
                })
            end
        end
    end

    if cmd == "session" then
        local subcmd, rest = query:match("^(%S*)%s*(.*)$")
        subcmd = subcmd or ""
        rest = rest or ""

        if not query:find("%s") then
            local subcmds = {
                { name = "save", desc = "Save current session" },
                { name = "restore", desc = "Restore a saved session" },
                { name = "list", desc = "List all saved sessions" },
                { name = "delete", desc = "Delete a saved session" },
            }
            for _, sc in ipairs(subcmds) do
                if subcmd == "" or sc.name:sub(1, #subcmd) == subcmd then
                    table.insert(results, {
                        label = sc.name,
                        description = sc.desc,
                        replacement = "session " .. sc.name .. " ",
                    })
                end
            end
        elseif subcmd == "restore" or subcmd == "open" or subcmd == "delete" or subcmd == "del" then
            local session = package.loaded["session"] or require("session")
            if session and session.list then
                local names = session.list()
                for _, sname in ipairs(names) do
                    if rest == "" or sname:sub(1, #rest) == rest then
                        table.insert(results, {
                            label = "🗂 " .. sname,
                            description = "Saved session",
                            replacement = "session " .. subcmd .. " " .. sname,
                        })
                    end
                end
            end
        end
    end

    return results
end

--- Start or cycle next completion item
function _M.tab_complete(prompt)
    if not _M.is_active or #_M.current_items == 0 then
        _M.original_prompt = prompt or ":"
        local items = _M.complete(prompt)
        if items and #items > 0 then
            _M.current_items = items
            _M.current_index = 1
            _M.is_active = true
            return items[1].replacement
        else
            _M.clear()
            return nil
        end
    else
        _M.current_index = (_M.current_index % #_M.current_items) + 1
        return _M.current_items[_M.current_index].replacement
    end
end

--- Cycle previous completion item
function _M.tab_complete_prev()
    if _M.is_active and #_M.current_items > 0 then
        _M.current_index = _M.current_index - 1
        if _M.current_index < 1 then
            _M.current_index = #_M.current_items
        end
        return _M.current_items[_M.current_index].replacement
    end
    return nil
end

--- Clear completion state
function _M.clear()
    _M.current_items = {}
    _M.current_index = 0
    _M.is_active = false
    _M.original_prompt = ""
end

--- Get current completion state for UI rendering
function _M.get_state()
    return {
        active = _M.is_active,
        index = _M.current_index,
        items = _M.current_items,
    }
end

--- Get declarative Iced primitive view for current completion items
function _M.get_view(on_select)
    if not _M.is_active or #_M.current_items == 0 then
        return nil
    end
    local item_list = require("ui.item_list")
    return item_list.build({
        items = _M.current_items,
        selected_index = _M.current_index,
        max_visible = 8,
        title_width = 160,
        on_select = on_select or function (item)
            local modes = package.loaded["modes"]
            if modes and item and item.replacement then
                modes.set_prompt(":" .. item.replacement)
                _M.clear()
            end
        end,
    })
end

return _M
