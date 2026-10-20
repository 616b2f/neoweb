local _M = {}
local store = {}

_M.defaults = {
    ["webview.zoom_level"] = 1.0,
    ["window.home_page"] = "about:neoweb",
    ["window.search_engine"] = "https://duckduckgo.com/html/?q=%s",
    ["history.max_entries"] = 1000,
    ["follow.pattern"] = "asdfghjkl",
}

for k, v in pairs(_M.defaults) do
    store[k] = v
end

function _M.get(key)
    return store[key]
end

function _M.set(key, value)
    store[key] = value
end

return _M
