-- lua/lib/downloads.lua
-- File download management module for Neoweb (Luakit API parity)

local _M = {}

_M.default_dir = os.getenv("XDG_DOWNLOAD_DIR")
    or (os.getenv("HOME") and (os.getenv("HOME") .. "/Downloads"))
    or "."

--- Trigger download of a URL using the active or provided window
function _M.download(w, uri, filename)
    w = w or _G.w
    if not uri or uri == "" then
        uri = (w and w.uri and w:uri()) or ""
    end
    if not uri or uri == "" or uri:find("^about:") then
        if w and w.warning then
            w:warning("Cannot download " .. tostring(uri))
        end
        return false
    end
    if w and w.download then
        w:download(uri, filename)
        return true
    end
    return false
end

return _M
