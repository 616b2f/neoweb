-- lua/init.lua: Neoweb Main Lua Environment & Initialization
-- Driven by LuaJIT 2.1 following Luakit's micro-browser architecture

print("[Neoweb] Initializing Lua runtime from lua/init.lua...")

-- Load core modules
local lousy = require("lousy")
local modes = require("modes")
local binds = require("binds")
local completion = require("completion")
local ui = require("ui")
local window = require("window")
window.init(_G.w)
local session = require("session")
local downloads = require("downloads")
local dialog = require("dialog")
if luakit then
    luakit.downloads = downloads
    luakit.dialog = dialog
end

-- Luakit Application Lifecycle Management
if luakit then
    local raw_exit = luakit.exit or luakit.quit
    function luakit.quit(opts)
        local save = true
        if type(opts) == "table" and opts.save ~= nil then
            save = opts.save
        elseif opts == false or opts == "nosave" or opts == "!" then
            save = false
        end

        local win_mod = package.loaded["window"] or window
        if win_mod and win_mod.emit_signal then
            win_mod.emit_signal("close", { save = save })
        end

        raw_exit()
    end
end

-- Check if command-line arguments contain a URL
local has_cli_url = false
if luakit and luakit.args and #luakit.args > 0 then
    local uri_mod = require("lousy.uri")
    for _, arg in ipairs(luakit.args) do
        if not arg:match("^%-") and (arg:match("^%a+://") or arg:match("%.") or arg:match("^about:")) then
            if not has_cli_url then
                has_cli_url = true
                _G.w:navigate(uri_mod.parse(arg))
            else
                _G.w:new_tab(uri_mod.parse(arg))
            end
        end
    end
end

if not has_cli_url then
    session.restore(_G.w)
end


-- Load optional user configuration
local userconf_paths = {
    os.getenv("XDG_CONFIG_HOME") and (os.getenv("XDG_CONFIG_HOME") .. "/neoweb/userconf.lua"),
    os.getenv("HOME") and (os.getenv("HOME") .. "/.config/neoweb/userconf.lua"),
    "lua/userconf.lua",
}

for _, path in ipairs(userconf_paths) do
    local f = io.open(path, "r")
    if f then
        f:close()
        print("[Neoweb] Loading user configuration from " .. path)
        dofile(path)
        break
    end
end

print("[Neoweb] Lua environment initialized successfully.")
