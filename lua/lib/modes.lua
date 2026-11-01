-- lua/lib/modes.lua
-- Modal state machine and key dispatch (Luakit compatible)

local _M = {}
local bind = require("lousy.bind")
local binds_table = {}
local modes_table = {}
local current_mode = "normal"
local prompt_text = ""
local follow_target = "current"
local follow_query = ""

--- Register a new mode with lifecycle hooks and metadata
-- @param name string: Mode identifier (e.g. "normal", "command", "insert", "visual")
-- @param spec table: Optional mode specification:
--   - description string: Human-readable mode description
--   - badge table: { text = "...", color = "..." } or string
--   - prompt_prefix string: e.g. ":", "/", "?" or nil
--   - passthrough boolean: true if raw keys pass to browser engine
--   - modal_input boolean: true if modal input buffers into prompt
--   - enter function(w, ...): called when entering this mode
--   - leave function(w): called when exiting this mode
--   - activate function(w): called when window/tab is focused in this mode
function _M.new_mode(name, spec)
    assert(type(name) == "string", "Mode name must be a string")
    spec = spec or {}
    local badge = spec.badge
    if type(badge) == "string" then
        badge = { text = badge, color = "primary" }
    elseif type(badge) ~= "table" then
        badge = { text = name:upper(), color = "primary" }
    end

    modes_table[name] = {
        name = name,
        description = spec.description or "",
        badge = badge,
        prompt_prefix = spec.prompt_prefix,
        passthrough = spec.passthrough or false,
        modal_input = spec.modal_input or false,
        enter = spec.enter,
        leave = spec.leave,
        activate = spec.activate,
    }
    return modes_table[name]
end

--- Get mode metadata table
function _M.get_mode_info(name)
    name = name or current_mode
    return modes_table[name]
end

--- Get all registered modes
function _M.get_modes()
    return modes_table
end

function _M.get_mode()
    return current_mode
end

function _M.get_prompt()
    return prompt_text
end

function _M.set_prompt(text)
    prompt_text = text or ""
end

function _M.is_passthrough(name)
    name = name or current_mode
    local info = modes_table[name]
    return (info and info.passthrough) or false
end

function _M.is_modal_input(name)
    name = name or current_mode
    local info = modes_table[name]
    return (info and info.modal_input) or false
end

--- Transition between modes with lifecycle execution
function _M.set_mode(w, target_mode, ...)
    w = w or _G.w
    if current_mode == target_mode then
        return
    end

    local prev_mode_name = current_mode
    local prev_spec = modes_table[prev_mode_name]

    -- 1. Execute leave hook on previous mode
    if prev_spec and type(prev_spec.leave) == "function" then
        local ok, err = pcall(prev_spec.leave, w)
        if not ok then
            print(string.format("[Neoweb] Error in mode '%s' leave hook: %s", prev_mode_name, tostring(err)))
        end
    end

    -- 2. Transition state
    current_mode = target_mode
    bind.clear()

    local new_spec = modes_table[target_mode]

    -- 3. Execute enter hook on new mode
    if new_spec and type(new_spec.enter) == "function" then
        local ok, err = pcall(new_spec.enter, w, ...)
        if not ok then
            print(string.format("[Neoweb] Error in mode '%s' enter hook: %s", target_mode, tostring(err)))
        end
    end

    -- 4. Notify Rust host
    if w and w.set_mode then
        w:set_mode(target_mode)
    end
end

function _M.add_binds(mode, binds)
    binds_table[mode] = binds_table[mode] or {}
    for _, b in ipairs(binds) do
        table.insert(binds_table[mode], b)
    end
end

function _M.get_binds(mode)
    return binds_table[mode] or {}
end

--- Dispatch a key event to the active mode bindings
function _M.handle_key(w, key)
    w = w or _G.w
    if key == "<Esc>" or key == "<Escape>" then
        local comp = package.loaded["completion"]
        if comp and comp.get_state and comp.get_state().active then
            comp.clear()
            if comp.original_prompt and comp.original_prompt ~= "" then
                prompt_text = comp.original_prompt
            end
            return true
        end
        bind.clear()
        if current_mode ~= "normal" then
            if current_mode == "search" and w and w.clear_search then
                w:clear_search()
            end
            _M.set_mode(w, "normal")
            return true
        end
        return false
    end

    local mode_info = modes_table[current_mode]
    local is_modal = (mode_info and mode_info.modal_input) or false
    local is_passthrough = (mode_info and mode_info.passthrough) or false

    if current_mode == "follow" then
        if key == "<BS>" or key == "<C-h>" then
            if #follow_query > 0 then
                follow_query = follow_query:sub(1, -2)
                if w and w.follow_hint then
                    w:follow_hint(follow_query)
                end
                return true
            end
            return true
        end
        if #key == 1 and (key:match("%a") or key:match("%d")) then
            follow_query = follow_query .. key:lower()
            if w and w.follow_hint then
                w:follow_hint(follow_query)
                return true
            end
        end
        return false
    end

    if is_modal then
        -- In modal input modes (command, search):
        -- Check exact trigger bindings first (like <CR>, <BS>, <Tab>, <C-w>, <C-u>, etc.)
        local active_binds = _M.get_binds(current_mode)
        for _, b in ipairs(active_binds) do
            local trigger = b[1] or b.trigger or b.pattern
            if trigger == key then
                local func = b[3] or b.func or b.action
                if func then
                    local ok, err = pcall(func, w)
                    if not ok then
                        print("Error executing keybind: " .. tostring(err))
                    end
                end
                return true
            end
        end

        -- If not intercepted by a bind, buffer typed character into prompt
        local char = (key == "<Space>") and " " or key
        if not (char:len() > 1 and char:sub(1, 1) == "<" and char:sub(-1) == ">") and not char:find("%c") then
            local comp = package.loaded["completion"]
            if comp and comp.clear then comp.clear() end
            prompt_text = prompt_text .. char
            return true
        end

        return false
    end

    if is_passthrough then
        -- Check mode-specific bindings first (e.g. custom binds registered in insert mode)
        local active_binds = _M.get_binds(current_mode)
        if active_binds and #active_binds > 0 then
            local hit = bind.hit(w, active_binds, key)
            if hit then
                return true
            end
        end

        -- Explicitly forward unintercepted keys to the webview
        if w and w.send_key then
            w:send_key(key)
            return true
        end

        return false
    end

    local active_binds = _M.get_binds(current_mode)
    local hit = bind.hit(w, active_binds, key)
    if hit then
        return true
    end

    return false
end

function _M.get_buffer()
    return bind.get_buffer()
end

function _M.clear_buffer()
    bind.clear()
end

function _M.add_cmds(mode, cmds)
    if type(mode) == "table" and not cmds then
        cmds = mode
        mode = "command"
    end
    for _, item in ipairs(cmds or {}) do
        local trigger = item[1] or item.name or item.trigger
        local desc = item[2] or item.desc or item.description or ""
        local func = item[3] or item.func or item.action
        if trigger and func and luakit and luakit.add_cmd then
            local name = tostring(trigger):gsub("^:+", "")
            luakit.add_cmd(name, desc, func)
        end
    end
end

-- =========================================================================
-- Built-in Core Modes Registration
-- =========================================================================

_M.new_mode("normal", {
    description = "Normal navigation mode",
    badge = { text = "NORMAL", color = "green" },
    passthrough = false,
    enter = function (w)
        prompt_text = ""
    end,
})

_M.new_mode("command", {
    description = "Command-line entry mode",
    badge = { text = "COMMAND", color = "yellow" },
    prompt_prefix = ":",
    modal_input = true,
    enter = function (w, text)
        if text then
            prompt_text = text
        elseif prompt_text == "" then
            prompt_text = ":"
        end
    end,
    leave = function (w)
        local comp = package.loaded["completion"]
        if comp and comp.clear then
            comp.clear()
        end
        prompt_text = ""
    end,
})

_M.new_mode("search", {
    description = "In-page text search mode",
    badge = { text = "SEARCH", color = "blue" },
    prompt_prefix = "/",
    modal_input = true,
    enter = function (w, text)
        if text then
            prompt_text = text
        elseif prompt_text == "" then
            prompt_text = "/"
        end
    end,
    leave = function (w)
        prompt_text = ""
    end,
})

_M.new_mode("insert", {
    description = "Direct webview input mode",
    badge = { text = "INSERT", color = "purple" },
    passthrough = true,
    enter = function (w)
        prompt_text = ""
    end,
    leave = function (w)
        prompt_text = ""
    end,
})

_M.new_mode("passthrough", {
    description = "Raw input passthrough mode",
    badge = { text = "PASSTHROUGH", color = "yellow" },
    passthrough = true,
    enter = function (w)
        prompt_text = ""
    end,
})

_M.new_mode("follow", {
    description = "Link hint following mode",
    badge = { text = "FOLLOW", color = "red" },
    enter = function (w, target)
        prompt_text = ""
        follow_query = ""
        follow_target = target or "current"
    end,
    leave = function (w)
        prompt_text = ""
        follow_query = ""
        follow_target = "current"
        if w and w.clear_hints then
            w:clear_hints()
        end
    end,
})

function _M.get_follow_target()
    return follow_target
end

function _M.is_follow_tab()
    return current_mode == "follow" and follow_target == "tab"
end

function _M.get_follow_query()
    return follow_query
end

function _M.pop_follow_query()
    if #follow_query > 0 then
        follow_query = follow_query:sub(1, -2)
    end
end

return _M
