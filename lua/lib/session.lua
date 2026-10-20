-- lua/lib/session.lua
-- Pure-Lua Session Management and Tab Restoration for Neoweb

local lousy = require("lousy")
local tabs_mod = require("ui.tabs")

local _M = {}

local config_dir = (luakit and luakit.config_dir) or (os.getenv("HOME") and (os.getenv("HOME") .. "/.config/neoweb")) or "."
_M.default_file = config_dir .. "/session.json"
_M.sessions_dir = config_dir .. "/sessions"

--- Resolve session file path from optional name or path
function _M.get_session_path(name)
    if not name or name == "" or name == "default" then
        return _M.default_file
    end
    if name:find("%.json$") or name:find("/") then
        return name
    end
    return _M.sessions_dir .. "/" .. name .. ".json"
end

--- Save current window tabs and active tab index to a session file
function _M.save(w, name)
    local target_file = _M.get_session_path(name)
    local win = w or _G.w

    local tab_info = tabs_mod.get_tabs()
    if not tab_info or not tab_info.tabs or #tab_info.tabs == 0 then
        return false, "No tabs to save"
    end

    local session_data = {
        active_index = tab_info.active_index or 1,
        tabs = {}
    }

    for _, t in ipairs(tab_info.tabs) do
        table.insert(session_data.tabs, {
            uri = t.uri or "about:blank",
            title = (t.title and t.title ~= "") and t.title or (t.uri or "Neoweb"),
        })
    end

    -- Ensure target directory exists
    if luakit and luakit.mkdir then
        local dir = target_file:match("^(.*)/[^/]+$")
        if dir and dir ~= "" then
            luakit.mkdir(dir)
        end
    end

    if not luakit or not luakit.json or not luakit.json.encode then
        return false, "luakit.json.encode is not available"
    end

    local json_str, err = luakit.json.encode(session_data)
    if not json_str then
        if win and win.error then win:error("Failed to encode session: " .. tostring(err)) end
        return false, err
    end

    local f, io_err = io.open(target_file, "w")
    if not f then
        if win and win.error then win:error("Failed to write session: " .. tostring(io_err)) end
        return false, io_err
    end

    f:write(json_str)
    f:close()

    return true, target_file
end

--- Restore tabs from a session file into the given window
function _M.restore(w, name)
    local target_file = _M.get_session_path(name)
    local win = w or _G.w
    if not win then
        return false, "No window available"
    end

    local f = io.open(target_file, "r")
    if not f then
        if name and name ~= "" and name ~= "default" and win.warning then
            win:warning("Session not found: " .. name)
        end
        return false, "Session file not found"
    end

    local content = f:read("*a")
    f:close()

    if not luakit or not luakit.json or not luakit.json.decode then
        return false, "luakit.json.decode is not available"
    end

    local session_data, err = luakit.json.decode(content)
    if not session_data or not session_data.tabs or #session_data.tabs == 0 then
        if name and name ~= "" and win.warning then
            win:warning("Invalid or empty session in " .. target_file)
        end
        return false, "Invalid session data: " .. tostring(err)
    end

    local cur_tabs = tabs_mod.get_tabs()
    local initial_count = (cur_tabs and cur_tabs.tabs and #cur_tabs.tabs) or 1

    -- 1. Navigate first tab to first session tab
    local first_tab = session_data.tabs[1]
    if first_tab and first_tab.uri and first_tab.uri ~= "" then
        win:navigate(first_tab.uri)
    end

    -- 2. Open any subsequent session tabs
    for i = 2, #session_data.tabs do
        local t = session_data.tabs[i]
        if t and t.uri and t.uri ~= "" then
            win:new_tab(t.uri)
        end
    end

    -- 3. If replacing multiple existing tabs, close the pushed old tabs
    if initial_count > 1 then
        for _ = 1, initial_count - 1 do
            win:close_tab(1)
        end
    end

    -- 4. Focus saved active tab
    local active_idx = session_data.active_index or 1
    if active_idx < 1 then active_idx = 1 end
    if active_idx > #session_data.tabs then active_idx = #session_data.tabs end
    win:goto_tab(active_idx - 1)

    if win.notify and name and name ~= "" and name ~= "default" then
        win:notify("Restored session '" .. name .. "' (" .. #session_data.tabs .. " tabs)")
    end

    return true
end

--- List all available named sessions
function _M.list()
    local sessions = {}

    -- Check default session
    local f_def = io.open(_M.default_file, "r")
    if f_def then
        f_def:close()
        table.insert(sessions, "default")
    end

    -- Check named sessions
    if luakit and luakit.list_dir then
        local files = luakit.list_dir(_M.sessions_dir) or {}
        for _, file in ipairs(files) do
            local name = file:match("^(.*)%.json$")
            if name and name ~= "" and name ~= "default" then
                table.insert(sessions, name)
            end
        end
    end

    table.sort(sessions)
    return sessions
end

--- Delete a named session
function _M.delete(w, name)
    local win = w or _G.w
    if not name or name == "" then
        if win and win.warning then win:warning("Usage: :session-delete <name>") end
        return false, "No name specified"
    end

    local path = _M.get_session_path(name)
    if luakit and luakit.remove_file then
        luakit.remove_file(path)
    else
        os.remove(path)
    end

    if win and win.notify then
        win:notify("Deleted session: " .. name)
    end
    return true
end

-- Auto-save default session on window close signal (unless save == false / nosave)
local window = require("window")
window.add_signal("close", function (win, opts)
    if type(opts) == "table" and opts.save == false then
        return
    elseif opts == false or opts == "nosave" or opts == "!" then
        return
    end
    _M.save(win)
end)

return _M
