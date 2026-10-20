local _M = {}
local signals = {}

function _M.setup(obj)
    signals[obj] = signals[obj] or {}
end

function _M.add(obj, name, func)
    signals[obj] = signals[obj] or {}
    signals[obj][name] = signals[obj][name] or {}
    table.insert(signals[obj][name], func)
end

function _M.emit(name, obj, ...)
    local target = obj or "global"
    if signals[target] and signals[target][name] then
        for _, func in ipairs(signals[target][name]) do
            local status, err = pcall(func, obj, ...)
            if not status then
                print(string.format("Signal error [%s]: %s", name, tostring(err)))
            end
        end
    end
end

return _M
