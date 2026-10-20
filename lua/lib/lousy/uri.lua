local _M = {}

_M.default_search_engine = "https://duckduckgo.com/html/?q=%s"

_M.search_engines = {
    g = "https://www.google.com/search?q=%s",
    ddg = "https://duckduckgo.com/html/?q=%s",
    w = "https://en.wikipedia.org/wiki/Special:Search?search=%s",
    crates = "https://crates.io/search?q=%s",
    gh = "https://github.com/search?q=%s",
}

function _M.parse(input)
    if not input or input == "" then
        return "about:blank"
    end

    -- Check if scheme exists
    if input:match("^%a+://") or input:match("^about:") or input:match("^file:") then
        return input
    end

    -- Check search engine prefix (e.g. "g servo engine")
    local prefix, query = input:match("^(%a+)%s+(.*)$")
    if prefix and _M.search_engines[prefix] then
        local encoded = query:gsub("%s+", "+")
        return string.format(_M.search_engines[prefix], encoded)
    end

    -- Check if it looks like a domain name
    if input:find("%.") and not input:find("%s") then
        return "https://" .. input
    end

    -- Fallback to default search
    local encoded = input:gsub("%s+", "+")
    return string.format(_M.default_search_engine, encoded)
end

return _M
