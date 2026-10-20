-- lua/lib/lousy/bind.lua
-- Key sequence matching, trie lookup, and numeric count parsing (Luakit compatible)

local _M = {}

local buffer = ""
local count = nil

--- Returns the current buffer display string (e.g. "5g" or "")
function _M.get_buffer()
    local disp = ""
    if count then
        disp = tostring(count)
    end
    disp = disp .. buffer
    return disp
end

--- Clear the key and count buffer
function _M.clear()
    buffer = ""
    count = nil
end

--- Match a key against a list of mode bindings
-- Returns:
--   "match", func, count
--   "pending"
--   "nomatch"
function _M.match_key(binds, key, allow_count)
    if allow_count == nil then allow_count = true end

    -- Handle digit count prefix when buffer is empty and count parsing is enabled
    if allow_count and buffer == "" and key:match("^%d$") then
        if key ~= "0" or count ~= nil then
            count = (count or 0) * 10 + tonumber(key)
            return "pending"
        end
    end

    buffer = buffer .. key

    local exact_match = nil
    local has_prefix = false

    for _, bind in ipairs(binds) do
        local trigger = bind[1] or bind.trigger or bind.pattern
        if trigger then
            if trigger == buffer then
                exact_match = bind
            elseif #trigger > #buffer and trigger:sub(1, #buffer) == buffer then
                has_prefix = true
            end
        end
    end

    if has_prefix then
        return "pending"
    elseif exact_match then
        local current_count = count
        _M.clear()
        local func = exact_match[3] or exact_match.func or exact_match.action
        return "match", func, current_count
    else
        _M.clear()
        return "nomatch"
    end
end

--- Process a keypress against mode bindings
function _M.hit(w, binds, key, allow_count)
    local status, func, current_count = _M.match_key(binds, key, allow_count)
    if status == "match" and func then
        local ok, err = pcall(func, w, current_count)
        if not ok then
            print("Error executing keybind: " .. tostring(err))
        end
        return true
    elseif status == "pending" then
        return true
    end
    return false
end

return _M
