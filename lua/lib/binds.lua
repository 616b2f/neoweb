local modes = require("modes")
local last_search_backward = false

-- Normal Mode Bindings
modes.add_binds("normal", {
    { "j", "Scroll down", function (w, count) w:scroll_by(0, 50 * (count or 1)) end },
    { "k", "Scroll up", function (w, count) w:scroll_by(0, -50 * (count or 1)) end },
    { "h", "Scroll left", function (w, count) w:scroll_by(-40 * (count or 1), 0) end },
    { "l", "Scroll right", function (w, count) w:scroll_by(40 * (count or 1), 0) end },
    { "gg", "Scroll to top", function (w, count) w:scroll_to(count or 0) end },
    { "G", "Scroll to bottom", function (w, count) w:scroll_to(count or 100) end },
    { "d", "Close tab", function (w, count) for _ = 1, (count or 1) do w:close_tab() end end },
    { "gt", "Next tab", function (w, count) for _ = 1, (count or 1) do w:next_tab() end end },
    { "gT", "Previous tab", function (w, count) for _ = 1, (count or 1) do w:prev_tab() end end },
    { "r", "Reload current page", function (w) w:reload(false) end },
    { "R", "Reload bypassing cache", function (w) w:reload(true) end },
    { "<C-o>", "Go back in history", function (w, count) for _ = 1, (count or 1) do w:back() end end },
    { "<C-i>", "Go forward in history", function (w, count) for _ = 1, (count or 1) do w:forward() end end },
    { "H", "Go back in history (alias)", function (w, count) for _ = 1, (count or 1) do w:back() end end },
    { "L", "Go forward in history (alias)", function (w, count) for _ = 1, (count or 1) do w:forward() end end },
    { "zi", "Zoom in", function (w) w:zoom_in() end },
    { "zo", "Zoom out", function (w) w:zoom_out() end },
    { "zz", "Reset zoom", function (w) w:zoom_reset() end },
    { "yd", "Copy domain of current tab", function (w)
        local uri = w:uri()
        local domain = uri:match("^%a+://([^/]+)")
        if domain then
            luakit.selection.clipboard = domain
            w:notify("Copied domain: " .. domain)
        end
    end },
    { "y", "Copy URL of current tab", function (w)
        local uri = w:uri()
        if uri and uri ~= "" then
            luakit.selection.clipboard = uri
            w:notify("Yanked: " .. uri)
        end
    end },
    { "p", "Open URL from clipboard", function (w)
        local clip = luakit.selection.clipboard
        if clip and clip ~= "" then
            local uri_mod = require("lousy.uri")
            w:navigate(uri_mod.parse(clip))
        end
    end },
    { "P", "Open URL from clipboard in new tab", function (w)
        local clip = luakit.selection.clipboard
        if clip and clip ~= "" then
            local uri_mod = require("lousy.uri")
            w:new_tab(uri_mod.parse(clip))
        end
    end },
    { ":", "Enter command mode", function (w) modes.set_prompt(":"); modes.set_mode(w, "command") end },
    { "o", "Open URL in current tab", function (w) modes.set_prompt(":open "); modes.set_mode(w, "command") end },
    { "t", "Open URL in new tab", function (w) modes.set_prompt(":tabopen "); modes.set_mode(w, "command") end },
    { "w", "Open URL in new tab (alias)", function (w) modes.set_prompt(":tabopen "); modes.set_mode(w, "command") end },
    { "O", "Edit current URL", function (w)
        local u = w:uri()
        local cur = (u and u ~= "" and u ~= "about:neoweb") and u or ""
        modes.set_prompt(":open " .. cur)
        modes.set_mode(w, "command")
    end },
    { "T", "Edit current URL in new tab", function (w)
        local u = w:uri()
        local cur = (u and u ~= "" and u ~= "about:neoweb") and u or ""
        modes.set_prompt(":tabopen " .. cur)
        modes.set_mode(w, "command")
    end },
    { "f", "Follow link hints", function (w) modes.set_mode(w, "follow", "current") end },
    { "F", "Follow link hints in new tab", function (w) modes.set_mode(w, "follow", "tab") end },
    { ";d", "Follow link and download target", function (w) modes.set_mode(w, "follow", "download") end },
    { "i", "Enter insert mode", function (w) modes.set_mode(w, "insert") end },
    { "<C-z>", "Enter passthrough mode", function (w) modes.set_mode(w, "passthrough") end },
    { "ZZ", "Exit Neoweb (save session)", function (w) luakit.quit({ save = true }) end },
    { "ZQ", "Exit Neoweb without saving", function (w) luakit.quit({ save = false }) end },
    { "gh", "Open home page", function (w) w:navigate("about:neoweb") end },
    { "0", "Scroll far left", function (w) w:scroll_by(-100, 0) end },
    { "/", "Search forward on page", function (w) modes.set_prompt("/"); modes.set_mode(w, "search", "/") end },
    { "?", "Search backward on page", function (w) modes.set_prompt("?"); modes.set_mode(w, "search", "?") end },
    { "n", "Find next search match", function (w, count) for _ = 1, (count or 1) do w:search_next({ backward = last_search_backward }) end end },
    { "N", "Find previous search match", function (w, count) for _ = 1, (count or 1) do w:search_next({ backward = not last_search_backward }) end end },
})

-- Command Mode Bindings (handled completely in Lua using Neovim key notation)
modes.add_binds("command", {
    { "<CR>", "Execute command line", function (w)
        local comp = package.loaded["completion"]
        if comp and comp.clear then comp.clear() end
        local text = modes.get_prompt()
        modes.set_prompt("")
        modes.set_mode(w, "normal")
        local cmd, args = text:match("^%s*:?%s*(%S+)%s*(.-)%s*$")
        if cmd and cmd ~= "" then
            if luakit and luakit.run_cmd then
                local handled = luakit.run_cmd(cmd, args)
                if not handled then
                    if text:find("%.") or text:find("^https?://") then
                        w:navigate(text:gsub("^%s*:%s*", ""))
                    else
                        w:warning("Unknown command: " .. cmd)
                    end
                end
            end
        end
    end },
    { "<BS>", "Delete character or exit command mode", function (w)
        local comp = package.loaded["completion"]
        if comp and comp.clear then comp.clear() end
        local text = modes.get_prompt()
        if #text <= 1 then
            modes.set_prompt("")
            modes.set_mode(w, "normal")
        else
            modes.set_prompt(text:sub(1, -2))
        end
    end },
    { "<Tab>", "Complete next command or argument", function (w)
        local comp = require("completion")
        local text = modes.get_prompt()
        local repl = comp.tab_complete(text)
        if repl then
            modes.set_prompt(":" .. repl)
        end
    end },
    { "<C-n>", "Complete next command or argument", function (w)
        local comp = require("completion")
        local text = modes.get_prompt()
        local repl = comp.tab_complete(text)
        if repl then
            modes.set_prompt(":" .. repl)
        end
    end },
    { "<c-n>", "Complete next command or argument (alias)", function (w)
        local comp = require("completion")
        local text = modes.get_prompt()
        local repl = comp.tab_complete(text)
        if repl then
            modes.set_prompt(":" .. repl)
        end
    end },
    { "<S-Tab>", "Complete previous command or argument", function (w)
        local comp = require("completion")
        local repl = comp.tab_complete_prev()
        if repl then
            modes.set_prompt(":" .. repl)
        end
    end },
    { "<C-p>", "Complete previous command or argument", function (w)
        local comp = require("completion")
        local repl = comp.tab_complete_prev()
        if repl then
            modes.set_prompt(":" .. repl)
        end
    end },
    { "<c-p>", "Complete previous command or argument (alias)", function (w)
        local comp = require("completion")
        local repl = comp.tab_complete_prev()
        if repl then
            modes.set_prompt(":" .. repl)
        end
    end },
    { "<C-u>", "Clear prompt line", function (w)
        local comp = package.loaded["completion"]
        if comp and comp.clear then comp.clear() end
        modes.set_prompt(":")
    end },
    { "<C-w>", "Delete last word", function (w)
        local text = modes.get_prompt()
        local trimmed = text:gsub("%s+$", "")
        local prefix = trimmed:match("^(.*%s)%S+$")
        if prefix then
            modes.set_prompt(prefix)
        else
            if text:sub(1, 1) == ":" then
                modes.set_prompt(":")
            else
                modes.set_prompt("")
            end
        end
    end },
})

-- Search Mode Bindings (unified forward & backward search)
modes.add_binds("search", {
    { "<CR>", "Execute search", function (w)
        local text = modes.get_prompt()
        local backward = (text:sub(1, 1) == "?")
        last_search_backward = backward
        modes.set_prompt("")
        modes.set_mode(w, "normal")
        local query = text:gsub("^[/?]", "")
        if query and query ~= "" then
            w:search(query, { backward = backward })
        end
    end },
    { "<BS>", "Delete character or exit search mode", function (w)
        local text = modes.get_prompt()
        if #text <= 1 then
            modes.set_prompt("")
            modes.set_mode(w, "normal")
            w:clear_search()
        else
            modes.set_prompt(text:sub(1, -2))
        end
    end },
    { "<C-u>", "Clear search query", function (w)
        local text = modes.get_prompt()
        local prefix = (text:sub(1, 1) == "?") and "?" or "/"
        modes.set_prompt(prefix)
    end },
    { "<C-w>", "Delete last word in search query", function (w)
        local text = modes.get_prompt()
        local prefix = (text:sub(1, 1) == "?") and "?" or "/"
        local body = text:gsub("^[/?]", "")
        local trimmed = body:gsub("%s+$", "")
        local remaining = trimmed:match("^(.*%s)%S+$")
        if remaining then
            modes.set_prompt(prefix .. remaining)
        else
            modes.set_prompt(prefix)
        end
    end },
})

-- Default Commands registered in Lua
modes.add_cmds({
    { "search", "Search text on current page", function (w, arg)
        if arg and arg ~= "" then
            w:search(arg, { backward = false })
        end
    end },
    { "nohlsearch", "Clear in-page search highlighting", function (w)
        w:clear_search()
    end },
    { "noh", "Clear in-page search highlighting (alias)", function (w)
        w:clear_search()
    end },
    { "open", "Open URL or search query in current tab", function (w, arg)
        if arg and arg ~= "" then
            local uri_mod = require("lousy.uri")
            w:navigate(uri_mod.parse(arg))
        end
    end },
    { "o", "Open URL or search query (alias)", function (w, arg)
        if arg and arg ~= "" then
            local uri_mod = require("lousy.uri")
            w:navigate(uri_mod.parse(arg))
        end
    end },
    { "tabopen", "Open URL or search query in new tab", function (w, arg)
        local target = "about:neoweb"
        if arg and arg ~= "" then
            local uri_mod = require("lousy.uri")
            target = uri_mod.parse(arg)
        end
        w:new_tab(target)
    end },
    { "t", "Open URL or search query in new tab (alias)", function (w, arg)
        local target = "about:neoweb"
        if arg and arg ~= "" then
            local uri_mod = require("lousy.uri")
            target = uri_mod.parse(arg)
        end
        w:new_tab(target)
    end },
    { "close", "Close the active tab", function (w) w:close_tab() end },
    { "c", "Close the active tab (alias)", function (w) w:close_tab() end },
    { "quit", "Exit Neoweb", function (w, arg) luakit.quit({ save = (arg ~= "!" and arg ~= "nosave") }) end },
    { "q", "Exit Neoweb (alias)", function (w, arg) luakit.quit({ save = (arg ~= "!" and arg ~= "nosave") }) end },
    { "quit!", "Exit Neoweb without saving", function (w) luakit.quit({ save = false }) end },
    { "q!", "Exit Neoweb without saving (alias)", function (w) luakit.quit({ save = false }) end },
    { "reload", "Reload current page", function (w, arg) w:reload(arg == "!") end },
    { "r", "Reload current page (alias)", function (w, arg) w:reload(arg == "!") end },
    { "back", "Navigate backward in history", function (w) w:back() end },
    { "forward", "Navigate forward in history", function (w) w:forward() end },
    { "download", "Download current page or URL (:download [uri] [path])", function (w, arg)
        local uri, path = (arg or ""):match("^%s*(%S+)%s*(.-)%s*$")
        if not uri or uri == "" then
            uri = w:uri()
        end
        if path == "" then
            path = nil
        end
        local dl_mod = package.loaded["downloads"] or require("downloads")
        dl_mod.download(w, uri, path)
    end },
    { "dl", "Download current page or URL (alias)", function (w, arg)
        local uri, path = (arg or ""):match("^%s*(%S+)%s*(.-)%s*$")
        if not uri or uri == "" then
            uri = w:uri()
        end
        if path == "" then
            path = nil
        end
        local dl_mod = package.loaded["downloads"] or require("downloads")
        dl_mod.download(w, uri, path)
    end },
    { "help", "Show Neoweb guide and keybindings", function (w) w:navigate("about:help") end },
    { "lua", "Evaluate arbitrary Lua code", function (w, arg)
        if arg and arg ~= "" then
            local fn, err = loadstring("return " .. arg)
            if not fn then fn, err = loadstring(arg) end
            if fn then
                local ok, res = pcall(fn)
                if ok then
                    w:notify("Lua: " .. tostring(res))
                else
                    w:error("Lua error: " .. tostring(res))
                end
            else
                w:error("Lua compile error: " .. tostring(err))
            end
        end
    end },
    { "bookmark", "Save current page as bookmark in Chromium format", function (w, folder)
        local uri = w:uri()
        local title = w:title()
        if uri and uri ~= "" and not uri:find("^about:") then
            if luakit and luakit.add_bookmark then
                local ok, err = pcall(luakit.add_bookmark, uri, title ~= "" and title or uri, folder)
                if ok then
                    w:notify("Bookmarked: " .. (title ~= "" and title or uri))
                else
                    w:error("Bookmark error: " .. tostring(err))
                end
            end
        else
            w:warning("Cannot bookmark " .. tostring(uri))
        end
    end },
    { "b", "Save current page as bookmark (alias)", function (w, folder)
        local cmd = luakit and luakit.commands and luakit.commands["bookmark"]
        if cmd then
            local fn = type(cmd) == "function" and cmd or cmd.func
            if fn then fn(w, folder) end
        end
    end },
    { "session", "Manage browser sessions (:session save|restore|list|delete [name])", function (w, arg)
        local session = require("session")
        local subcmd, rest = (arg or ""):match("^%s*(%S+)%s*(.-)%s*$")
        subcmd = subcmd or ""
        local name = (rest ~= "") and rest or nil

        if subcmd == "save" or subcmd == "write" then
            local ok, path_or_err = session.save(w, name)
            if ok then
                w:notify("Session saved" .. (name and (": " .. name) or ""))
            else
                w:error("Failed to save session: " .. tostring(path_or_err))
            end
        elseif subcmd == "restore" or subcmd == "open" or subcmd == "load" then
            session.restore(w, name)
        elseif subcmd == "list" or subcmd == "ls" then
            local list = session.list()
            if #list > 0 then
                w:notify("Saved sessions: " .. table.concat(list, ", "))
            else
                w:notify("No saved sessions found")
            end
        elseif subcmd == "delete" or subcmd == "del" or subcmd == "rm" then
            if name then
                session.delete(w, name)
            else
                w:warning("Usage: :session delete <name>")
            end
        else
            w:warning("Usage: :session <save|restore|list|delete> [name]")
        end
    end },
    { "write", "Save session", function (w, arg)
        local session = require("session")
        local name = arg and arg:match("^%s*(%S+)")
        local ok, path_or_err = session.save(w, name)
        if ok then
            w:notify("Session saved" .. (name and (": " .. name) or ""))
        else
            w:error("Failed to save session: " .. tostring(path_or_err))
        end
    end },
    { "w", "Save session (alias)", function (w, arg)
        local session = require("session")
        local name = arg and arg:match("^%s*(%S+)")
        local ok, path_or_err = session.save(w, name)
        if ok then
            w:notify("Session saved" .. (name and (": " .. name) or ""))
        else
            w:error("Failed to save session: " .. tostring(path_or_err))
        end
    end },
    { "wq", "Save session and exit", function (w, arg)
        local session = require("session")
        local name = arg and arg:match("^%s*(%S+)")
        session.save(w, name)
        luakit.quit()
    end },
    { "x", "Save session and exit (alias)", function (w, arg)
        local session = require("session")
        local name = arg and arg:match("^%s*(%S+)")
        session.save(w, name)
        luakit.quit()
    end },
})

return modes
