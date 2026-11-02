pub mod bridge;

use bridge::{LuaSender, LuaToRustMsg};
use mlua::{Function, Lua, Table, UserData, UserDataMethods};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

#[derive(Clone)]
pub struct LuaWindow {
    sender: LuaSender,
    pub title: Arc<Mutex<String>>,
    pub uri: Arc<Mutex<String>>,
    pub progress: Arc<Mutex<f32>>,
}

impl UserData for LuaWindow {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("notify", |lua, this, msg: String| {
            let code = r#"
                local msg = ...
                local notify = package.loaded["ui.notify"] or require("ui.notify")
                if notify and notify.notify then
                    notify.notify(msg, "info")
                end
            "#;
            if let Ok(func) = lua.load(code).into_function() {
                let _ = func.call::<()>(msg.as_str());
            }
            this.sender.send(LuaToRustMsg::Notify {
                level: "info".to_string(),
                message: msg,
            });
            Ok(())
        });

        methods.add_method("warning", |lua, this, msg: String| {
            let code = r#"
                local msg = ...
                local notify = package.loaded["ui.notify"] or require("ui.notify")
                if notify and notify.notify then
                    notify.notify(msg, "warning")
                end
            "#;
            if let Ok(func) = lua.load(code).into_function() {
                let _ = func.call::<()>(msg.as_str());
            }
            this.sender.send(LuaToRustMsg::Notify {
                level: "warning".to_string(),
                message: msg,
            });
            Ok(())
        });

        methods.add_method("error", |lua, this, msg: String| {
            let code = r#"
                local msg = ...
                local notify = package.loaded["ui.notify"] or require("ui.notify")
                if notify and notify.notify then
                    notify.notify(msg, "error")
                end
            "#;
            if let Ok(func) = lua.load(code).into_function() {
                let _ = func.call::<()>(msg.as_str());
            }
            this.sender.send(LuaToRustMsg::Notify {
                level: "error".to_string(),
                message: msg,
            });
            Ok(())
        });

        methods.add_method("navigate", |_, this, url: String| {
            this.sender.send(LuaToRustMsg::Navigate(url));
            Ok(())
        });

        methods.add_method("new_tab", |_, this, url: Option<String>| {
            let u = url.unwrap_or_else(|| "about:neoweb".to_string());
            this.sender.send(LuaToRustMsg::NewTab(u));
            Ok(())
        });

        methods.add_method("close_tab", |_, this, index: Option<usize>| {
            this.sender.send(LuaToRustMsg::CloseTab(index));
            Ok(())
        });

        methods.add_method("next_tab", |_, this, ()| {
            this.sender.send(LuaToRustMsg::NextTab);
            Ok(())
        });

        methods.add_method("prev_tab", |_, this, ()| {
            this.sender.send(LuaToRustMsg::PrevTab);
            Ok(())
        });

        methods.add_method("scroll_by", |_, this, (dx, dy): (f32, f32)| {
            this.sender.send(LuaToRustMsg::Scroll(dx, dy));
            Ok(())
        });

        methods.add_method("scroll_to", |_, this, pct: f32| {
            this.sender.send(LuaToRustMsg::ScrollPct(pct));
            Ok(())
        });

        methods.add_method("zoom_in", |_, this, ()| {
            this.sender.send(LuaToRustMsg::Zoom(0.1));
            Ok(())
        });

        methods.add_method("zoom_out", |_, this, ()| {
            this.sender.send(LuaToRustMsg::Zoom(-0.1));
            Ok(())
        });

        methods.add_method("zoom_reset", |_, this, ()| {
            this.sender.send(LuaToRustMsg::ZoomReset);
            Ok(())
        });

        methods.add_method("set_mode", |lua, this, mode: String| {
            let code = r#"
                local mode = ...
                local modes = package.loaded["modes"]
                if modes and modes.get_mode and modes.get_mode() ~= mode then
                    modes.set_mode(nil, mode)
                end
            "#;
            if let Ok(func) = lua.load(code).into_function() {
                let _ = func.call::<()>(mode.as_str());
            }
            this.sender.send(LuaToRustMsg::SetMode(mode));
            Ok(())
        });

        methods.add_method("mode", |lua, _this, ()| {
            let code = r#"
                local modes = package.loaded["modes"]
                if modes and modes.get_mode then
                    return modes.get_mode() or "normal"
                end
                return "normal"
            "#;
            Ok(lua
                .load(code)
                .eval::<String>()
                .unwrap_or_else(|_| "normal".to_string()))
        });

        methods.add_method("reload", |_, this, bypass_cache: Option<bool>| {
            this.sender
                .send(LuaToRustMsg::Reload(bypass_cache.unwrap_or(false)));
            Ok(())
        });

        methods.add_method("back", |_, this, ()| {
            this.sender.send(LuaToRustMsg::Back);
            Ok(())
        });

        methods.add_method("forward", |_, this, ()| {
            this.sender.send(LuaToRustMsg::Forward);
            Ok(())
        });

        methods.add_method("goto_tab", |_, this, index: usize| {
            this.sender.send(LuaToRustMsg::GotoTab(index));
            Ok(())
        });

        methods.add_method("eval_js", |_, this, script: String| {
            this.sender.send(LuaToRustMsg::ExecuteJs(script));
            Ok(())
        });

        methods.add_method("uri", |_, this, ()| {
            let uri = this.uri.lock().unwrap().clone();
            Ok(uri)
        });

        methods.add_method("title", |_, this, ()| {
            let title = this.title.lock().unwrap().clone();
            Ok(title)
        });

        methods.add_method("progress", |_, this, ()| {
            let progress = *this.progress.lock().unwrap();
            Ok(progress)
        });

        methods.add_method(
            "search",
            |_, this, (query, opts_val): (Option<String>, Option<mlua::Value>)| {
                let mut opts = crate::engine::SearchOptions::default();
                if let Some(mlua::Value::Table(tbl)) = opts_val {
                    if let Ok(b) = tbl.get::<bool>("backward") {
                        opts.backward = b;
                    }
                    if let Ok(fnxt) = tbl
                        .get::<bool>("find_next")
                        .or_else(|_| tbl.get::<bool>("next"))
                    {
                        opts.find_next = fnxt;
                    }
                    if let Ok(w) = tbl.get::<bool>("wrap") {
                        opts.wrap = w;
                    }
                    if let Ok(cs) = tbl.get::<bool>("case_sensitive") {
                        opts.case_sensitive = cs;
                    }
                } else if let Some(mlua::Value::Boolean(b)) = opts_val {
                    opts.backward = b;
                }
                this.sender.send(LuaToRustMsg::Search { query, opts });
                Ok(())
            },
        );

        methods.add_method("search_next", |_, this, opts_val: Option<mlua::Value>| {
            let mut opts = crate::engine::SearchOptions {
                find_next: true,
                ..Default::default()
            };
            if let Some(mlua::Value::Table(tbl)) = opts_val {
                if let Ok(b) = tbl.get::<bool>("backward") {
                    opts.backward = b;
                }
                if let Ok(w) = tbl.get::<bool>("wrap") {
                    opts.wrap = w;
                }
                if let Ok(cs) = tbl.get::<bool>("case_sensitive") {
                    opts.case_sensitive = cs;
                }
            } else if let Some(mlua::Value::Boolean(b)) = opts_val {
                opts.backward = b;
            }
            this.sender.send(LuaToRustMsg::Search { query: None, opts });
            Ok(())
        });

        methods.add_method("clear_search", |_, this, ()| {
            this.sender.send(LuaToRustMsg::ClearSearch);
            Ok(())
        });

        methods.add_method("send_key", |_, this, key: String| {
            this.sender.send(LuaToRustMsg::SendKey(key));
            Ok(())
        });

        methods.add_method("follow_hint", |_, this, key: String| {
            this.sender.send(LuaToRustMsg::FollowHint(key));
            Ok(())
        });

        methods.add_method(
            "download",
            |_, this, (url, path): (String, Option<String>)| {
                this.sender.send(LuaToRustMsg::Download { url, path });
                Ok(())
            },
        );

        methods.add_method(
            "add_signal",
            |lua, _this, (name, func): (String, Function)| {
                let lousy: Table = lua.load("require('lousy')").eval()?;
                let signal: Table = lousy.get("signal")?;
                let add_fn: Function = signal.get("add")?;
                let w_val: mlua::Value = lua.globals().get("w")?;
                add_fn.call::<()>((w_val, name, func))?;
                Ok(())
            },
        );

        methods.add_method(
            "emit_signal",
            |lua, _this, (name, arg): (String, Option<mlua::Value>)| {
                let lousy: Table = lua.load("require('lousy')").eval()?;
                let signal: Table = lousy.get("signal")?;
                let emit_fn: Function = signal.get("emit")?;
                let w_val: mlua::Value = lua.globals().get("w")?;
                emit_fn.call::<()>((name, w_val, arg.unwrap_or(mlua::Value::Nil)))?;
                Ok(())
            },
        );
    }
}

fn lua_value_to_json(val: mlua::Value) -> Result<serde_json::Value, mlua::Error> {
    match val {
        mlua::Value::Nil => Ok(serde_json::Value::Null),
        mlua::Value::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        mlua::Value::Integer(i) => Ok(serde_json::Value::Number(i.into())),
        mlua::Value::Number(n) => {
            if let Some(num) = serde_json::Number::from_f64(n) {
                Ok(serde_json::Value::Number(num))
            } else {
                Ok(serde_json::Value::Null)
            }
        }
        mlua::Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        mlua::Value::Table(tbl) => {
            let len = tbl.raw_len();
            if len > 0 {
                let mut vec = Vec::with_capacity(len);
                for i in 1..=len {
                    let item: mlua::Value = tbl.raw_get(i)?;
                    vec.push(lua_value_to_json(item)?);
                }
                Ok(serde_json::Value::Array(vec))
            } else {
                let mut map = serde_json::Map::new();
                for pair in tbl.pairs::<mlua::Value, mlua::Value>() {
                    let (k, v) = pair?;
                    let key_str = match k {
                        mlua::Value::String(s) => s.to_str()?.to_string(),
                        mlua::Value::Integer(i) => i.to_string(),
                        mlua::Value::Number(n) => n.to_string(),
                        _ => continue,
                    };
                    map.insert(key_str, lua_value_to_json(v)?);
                }
                Ok(serde_json::Value::Object(map))
            }
        }
        _ => Ok(serde_json::Value::Null),
    }
}

fn json_to_lua_value(lua: &Lua, val: serde_json::Value) -> Result<mlua::Value, mlua::Error> {
    match val {
        serde_json::Value::Null => Ok(mlua::Value::Nil),
        serde_json::Value::Bool(b) => Ok(mlua::Value::Boolean(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(mlua::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(mlua::Value::Number(f))
            } else {
                Ok(mlua::Value::Nil)
            }
        }
        serde_json::Value::String(s) => {
            let lua_str = lua.create_string(&s)?;
            Ok(mlua::Value::String(lua_str))
        }
        serde_json::Value::Array(arr) => {
            let tbl = lua.create_table_with_capacity(arr.len(), 0)?;
            for (idx, item) in arr.into_iter().enumerate() {
                tbl.raw_set(idx + 1, json_to_lua_value(lua, item)?)?;
            }
            Ok(mlua::Value::Table(tbl))
        }
        serde_json::Value::Object(obj) => {
            let tbl = lua.create_table_with_capacity(0, obj.len())?;
            for (k, v) in obj {
                tbl.set(k, json_to_lua_value(lua, v)?)?;
            }
            Ok(mlua::Value::Table(tbl))
        }
    }
}

pub struct LuaRuntime {
    lua: Lua,
    pub window_proxy: LuaWindow,
}

impl LuaRuntime {
    pub fn new(
        sender: LuaSender,
        bookmarks: Arc<Mutex<crate::bookmarks::BookmarkStore>>,
        storage: Arc<Mutex<crate::storage::Storage>>,
    ) -> Result<Self, mlua::Error> {
        Self::with_config_dir(sender, bookmarks, storage, None)
    }

    pub fn with_config_dir(
        sender: LuaSender,
        bookmarks: Arc<Mutex<crate::bookmarks::BookmarkStore>>,
        storage: Arc<Mutex<crate::storage::Storage>>,
        custom_config_dir: Option<PathBuf>,
    ) -> Result<Self, mlua::Error> {
        let lua = Lua::new();

        let window_proxy = LuaWindow {
            sender: sender.clone(),
            title: Arc::new(Mutex::new("Neoweb".to_string())),
            uri: Arc::new(Mutex::new("about:neoweb".to_string())),
            progress: Arc::new(Mutex::new(1.0)),
        };

        // Configure Lua package.path
        let package_table: Table = lua.globals().get("package")?;
        let current_path: String = package_table.get("path")?;
        let lua_dir_path = "lua/?.lua;lua/?/init.lua;lua/lib/?.lua;lua/lib/?/init.lua;./?.lua;";
        package_table.set("path", format!("{}{}", lua_dir_path, current_path))?;

        // Global `luakit` table
        let luakit = lua.create_table()?;

        let sender_quit = sender.clone();
        luakit.set(
            "quit",
            lua.create_function(move |lua, ()| {
                let code = r#"
                    local lousy = package.loaded["lousy"] or package.loaded["lib.lousy"]
                    if lousy and lousy.signal and _G.w then
                        lousy.signal.emit("close", _G.w, "")
                    end
                "#;
                let _ = lua.load(code).exec();
                sender_quit.send(LuaToRustMsg::Quit);
                Ok(())
            })?,
        )?;

        let config_dir = custom_config_dir.unwrap_or_else(|| {
            directories::ProjectDirs::from("org", "neoweb", "neoweb")
                .map(|p| p.config_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        });
        luakit.set("config_dir", config_dir.to_string_lossy().to_string())?;

        let data_dir = directories::ProjectDirs::from("org", "neoweb", "neoweb")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        luakit.set("data_dir", data_dir.to_string_lossy().to_string())?;

        let args_tbl = lua.create_table()?;
        let raw_args: Vec<String> = std::env::args().skip(1).collect();
        for (i, arg) in raw_args.into_iter().enumerate() {
            args_tbl.set(i + 1, arg)?;
        }
        luakit.set("args", args_tbl)?;

        luakit.set(
            "mkdir",
            lua.create_function(|_, path: String| {
                std::fs::create_dir_all(path)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                Ok(true)
            })?,
        )?;

        luakit.set(
            "list_dir",
            lua.create_function(|lua, path: String| {
                let tbl = lua.create_table()?;
                if let Ok(entries) = std::fs::read_dir(path) {
                    let mut i = 1;
                    for entry in entries.flatten() {
                        if let Ok(name) = entry.file_name().into_string() {
                            tbl.set(i, name)?;
                            i += 1;
                        }
                    }
                }
                Ok(tbl)
            })?,
        )?;

        luakit.set(
            "remove_file",
            lua.create_function(|_, path: String| {
                let _ = std::fs::remove_file(path);
                Ok(true)
            })?,
        )?;

        // JSON encode/decode via serde_json
        let json_encode =
            lua.create_function(|_, val: mlua::Value| match lua_value_to_json(val) {
                Ok(j) => match serde_json::to_string_pretty(&j) {
                    Ok(s) => Ok((Some(s), None::<String>)),
                    Err(e) => Ok((None, Some(e.to_string()))),
                },
                Err(e) => Ok((None, Some(e.to_string()))),
            })?;

        let json_decode = lua.create_function(|lua, s: String| {
            match serde_json::from_str::<serde_json::Value>(&s) {
                Ok(j) => match json_to_lua_value(lua, j) {
                    Ok(v) => Ok((Some(v), None::<String>)),
                    Err(e) => Ok((None, Some(e.to_string()))),
                },
                Err(e) => Ok((None, Some(e.to_string()))),
            }
        })?;

        luakit.set("json_encode", json_encode.clone())?;
        luakit.set("json_decode", json_decode.clone())?;

        let json_table = lua.create_table()?;
        json_table.set("encode", json_encode)?;
        json_table.set("decode", json_decode)?;
        luakit.set("json", json_table)?;

        luakit.set(
            "spawn",
            lua.create_function(|_, cmd: String| {
                info!("Spawning external command: {}", cmd);
                let _ = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
                Ok(())
            })?,
        )?;

        // Selection / clipboard table
        let selection = lua.create_table()?;
        let sender_clip = sender.clone();

        // Metatable for luakit.selection.clipboard
        let clip_meta = lua.create_table()?;
        let sender_clip_set = sender_clip.clone();
        clip_meta.set(
            "__newindex",
            lua.create_function(move |_, (_tbl, key, val): (Table, String, String)| {
                if key == "clipboard" {
                    sender_clip_set.send(LuaToRustMsg::SetClipboard(val));
                }
                Ok(())
            })?,
        )?;

        clip_meta.set(
            "__index",
            lua.create_function(|_, (_tbl, key): (Table, String)| {
                if key == "clipboard"
                    && let Ok(mut clip) = arboard::Clipboard::new()
                {
                    return Ok(clip.get_text().unwrap_or_default());
                }
                Ok(String::new())
            })?,
        )?;

        let _ = selection.set_metatable(Some(clip_meta));
        luakit.set("selection", selection)?;

        let commands_table = lua.create_table()?;
        luakit.set("commands", commands_table)?;

        luakit.set(
            "add_cmd",
            lua.create_function(|lua, (name, desc, func): (String, String, Function)| {
                let globals = lua.globals();
                let luakit_tbl: Table = globals.get("luakit")?;
                let cmds_tbl: Table = luakit_tbl.get("commands")?;
                let entry = lua.create_table()?;
                entry.set("desc", desc)?;
                entry.set("func", func)?;
                cmds_tbl.set(name, entry)?;
                Ok(())
            })?,
        )?;

        luakit.set(
            "run_cmd",
            lua.create_function(|lua, (name, args): (String, String)| {
                let globals = lua.globals();
                let luakit_tbl: Table = globals.get("luakit")?;
                let cmds_tbl: Table = luakit_tbl.get("commands")?;
                let entry_val: mlua::Value = cmds_tbl.get(name.as_str())?;
                let func_opt = match entry_val {
                    mlua::Value::Function(f) => Some(f),
                    mlua::Value::Table(tbl) => tbl.get::<Function>("func").ok(),
                    _ => None,
                };
                if let Some(func) = func_opt {
                    let w_val: mlua::Value = globals.get("w")?;
                    func.call::<()>((w_val, args))?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            })?,
        )?;

        // Chromium File-Based Bookmarks Glue
        let bm_add = bookmarks.clone();
        luakit.set(
            "add_bookmark",
            lua.create_function(
                move |_, (uri, title, folder): (String, String, Option<String>)| {
                    bm_add
                        .lock()
                        .unwrap()
                        .add_bookmark(&uri, &title, folder.as_deref())
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
                },
            )?,
        )?;

        let bm_get = bookmarks;
        luakit.set(
            "get_bookmarks",
            lua.create_function(move |lua, query: Option<String>| {
                let q = query.unwrap_or_default();
                let entries = bm_get.lock().unwrap().search(&q);
                let tbl = lua.create_table()?;
                for (i, entry) in entries.into_iter().enumerate() {
                    let item = lua.create_table()?;
                    item.set("uri", entry.uri)?;
                    item.set("title", entry.title)?;
                    item.set("folder", entry.folder)?;
                    tbl.set(i + 1, item)?;
                }
                Ok(tbl)
            })?,
        )?;

        let storage_get_hist = storage;
        luakit.set(
            "get_history",
            lua.create_function(
                move |lua, (query, limit): (Option<String>, Option<usize>)| {
                    let q = query.unwrap_or_default();
                    let lim = limit.unwrap_or(10);
                    let entries = storage_get_hist
                        .lock()
                        .unwrap()
                        .search_history(&q, lim)
                        .unwrap_or_default();
                    let tbl = lua.create_table()?;
                    for (i, entry) in entries.into_iter().enumerate() {
                        let item = lua.create_table()?;
                        item.set("uri", entry.uri)?;
                        item.set("title", entry.title)?;
                        tbl.set(i + 1, item)?;
                    }
                    Ok(tbl)
                },
            )?,
        )?;

        lua.globals().set("luakit", luakit)?;

        // Global `window` table / current window instance `w`
        let window_obj = window_proxy.clone();
        lua.globals().set("w", window_obj)?;

        Ok(Self { lua, window_proxy })
    }

    pub fn run_command(&self, name: &str, args: &str) -> Result<bool, String> {
        let globals = self.lua.globals();
        if let Ok(luakit_tbl) = globals.get::<Table>("luakit")
            && let Ok(run_func) = luakit_tbl.get::<Function>("run_cmd")
        {
            return run_func
                .call::<(bool,)>((name, args))
                .map(|(success,)| success)
                .map_err(|e| e.to_string());
        }
        Ok(false)
    }

    pub fn load_init_script(&self) {
        let candidate_paths = [
            PathBuf::from("lua/init.lua"),
            PathBuf::from("lua/rc.lua"),
            directories::ProjectDirs::from("org", "neoweb", "neoweb")
                .map(|p| p.config_dir().join("init.lua"))
                .unwrap_or_else(|| PathBuf::from("init.lua")),
            directories::ProjectDirs::from("org", "neoweb", "neoweb")
                .map(|p| p.config_dir().join("rc.lua"))
                .unwrap_or_else(|| PathBuf::from("rc.lua")),
        ];

        for path in &candidate_paths {
            if path.exists() {
                info!("Loading Lua config from {:?}", path);
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        if let Err(e) = self.lua.load(&content).exec() {
                            error!("Error executing {:?}: {}", path, e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to read {:?}: {}", path, e);
                    }
                }
                break;
            }
        }
    }

    pub fn sync_state(&self, uri: &str, title: &str, progress: f32) {
        if let Ok(mut u) = self.window_proxy.uri.lock()
            && *u != uri
        {
            *u = uri.to_string();
        }
        if let Ok(mut t) = self.window_proxy.title.lock()
            && *t != title
        {
            *t = title.to_string();
        }
        if let Ok(mut p) = self.window_proxy.progress.lock()
            && (*p - progress).abs() > 0.001
        {
            *p = progress;
        }
    }

    pub fn emit_signal(&self, signal_name: &str, arg: &str) {
        let code = r#"
            local name, val = ...
            local lousy = package.loaded["lousy"] or package.loaded["lib.lousy"]
            if lousy and lousy.signal then
                lousy.signal.emit(name, w, val)
            end
        "#;
        if let Ok(func) = self.lua.load(code).into_function() {
            let _ = func.call::<()>((signal_name, arg));
        }
    }

    pub fn handle_key(&self, key_str: &str) -> bool {
        let code = r#"
            local key = ...
            local modes = package.loaded["modes"] or require("modes")
            if modes and modes.handle_key then
                return modes.handle_key(w, key) or false
            end
            return false
        "#;
        if let Ok(func) = self.lua.load(code).into_function() {
            func.call::<bool>(key_str).unwrap_or(false)
        } else {
            false
        }
    }

    pub fn get_key_buffer(&self) -> String {
        let code = r#"
            local modes = package.loaded["modes"] or package.loaded["lib.modes"]
            if modes and modes.get_buffer then
                return modes.get_buffer() or ""
            end
            return ""
        "#;
        self.lua.load(code).eval::<String>().unwrap_or_default()
    }

    pub fn get_prompt_text(&self) -> String {
        let code = r#"
            local modes = package.loaded["modes"] or package.loaded["lib.modes"]
            if modes and modes.get_prompt then
                return modes.get_prompt() or ""
            end
            return ""
        "#;
        self.lua.load(code).eval::<String>().unwrap_or_default()
    }

    pub fn get_mode(&self) -> String {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.get_mode then
                return modes.get_mode() or "normal"
            end
            return "normal"
        "#;
        self.lua
            .load(code)
            .eval::<String>()
            .unwrap_or_else(|_| "normal".to_string())
    }

    pub fn set_mode(&self, mode: &str) {
        let code = r#"
            local mode = ...
            local modes = package.loaded["modes"]
            if modes and modes.set_mode then
                modes.set_mode(w, mode)
            end
        "#;
        if let Ok(func) = self.lua.load(code).into_function() {
            let _ = func.call::<()>(mode);
        }
    }

    pub fn is_passthrough_mode(&self) -> bool {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.is_passthrough then
                return modes.is_passthrough()
            end
            return false
        "#;
        self.lua.load(code).eval::<bool>().unwrap_or(false)
    }

    pub fn is_modal_input_mode(&self) -> bool {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.is_modal_input then
                return modes.is_modal_input()
            end
            return false
        "#;
        self.lua.load(code).eval::<bool>().unwrap_or(false)
    }

    pub fn is_follow_tab(&self) -> bool {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.is_follow_tab then
                return modes.is_follow_tab()
            end
            return false
        "#;
        self.lua.load(code).eval::<bool>().unwrap_or(false)
    }

    pub fn get_follow_target(&self) -> String {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.get_follow_target then
                return modes.get_follow_target() or "current"
            end
            return "current"
        "#;
        self.lua
            .load(code)
            .eval::<String>()
            .unwrap_or_else(|_| "current".to_string())
    }

    pub fn get_follow_query(&self) -> String {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.get_follow_query then
                return modes.get_follow_query() or ""
            end
            return ""
        "#;
        self.lua.load(code).eval::<String>().unwrap_or_default()
    }

    pub fn pop_follow_query(&self) {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.pop_follow_query then
                modes.pop_follow_query()
            end
        "#;
        let _ = self.lua.load(code).exec();
    }

    pub fn get_prompt_prefix(&self) -> Option<String> {
        let code = r#"
            local modes = package.loaded["modes"]
            if modes and modes.get_mode_info then
                local info = modes.get_mode_info()
                if info and info.prompt_prefix then
                    return info.prompt_prefix
                end
            end
            return nil
        "#;
        self.lua.load(code).eval::<Option<String>>().unwrap_or(None)
    }

    pub fn get_completions(&self) -> Vec<CompletionItem> {
        let code = r#"
            local comp = package.loaded["completion"]
            if comp and comp.get_state then
                local state = comp.get_state()
                if state and state.active and state.items and #state.items > 0 then
                    local out = {}
                    for i, item in ipairs(state.items) do
                        table.insert(out, {
                            label = item.label or "",
                            description = item.description or "",
                            is_selected = (i == state.index)
                        })
                    end
                    return out
                end
            end
            return {}
        "#;
        if let Ok(tbl) = self.lua.load(code).eval::<mlua::Table>() {
            let mut items = Vec::new();
            for item_tbl in tbl.sequence_values::<mlua::Table>().flatten() {
                let label: String = item_tbl.get("label").unwrap_or_default();
                let description: String = item_tbl.get("description").unwrap_or_default();
                let is_selected: bool = item_tbl.get("is_selected").unwrap_or(false);
                items.push(CompletionItem {
                    label,
                    description,
                    is_selected,
                });
            }
            items
        } else {
            Vec::new()
        }
    }

    pub fn invoke_ui_callback(&self, callback_id: usize) {
        let code = r#"
            local id = ...
            local ui = package.loaded["ui"] or require("ui")
            if ui and ui.invoke_callback then
                ui.invoke_callback(id)
            end
        "#;
        if let Ok(func) = self.lua.load(code).into_function() {
            let _ = func.call::<()>(callback_id);
        }
    }

    pub fn get_completion_view(&self) -> Option<crate::ui::lua_view::LuaUiNode> {
        let code = r#"
            local comp = package.loaded["completion"]
            if comp and comp.get_view then
                return comp.get_view()
            end
            return nil
        "#;
        if let Ok(tbl) = self.lua.load(code).eval::<mlua::Table>() {
            crate::ui::lua_view::LuaUiNode::from_lua(&tbl)
        } else {
            None
        }
    }

    pub fn get_prompt_view(&self) -> Option<crate::ui::lua_view::LuaUiNode> {
        let code = r#"
            local modes = package.loaded["modes"]
            local ui = package.loaded["ui"] or require("ui")
            local prefix = nil
            local text = ""
            local is_modal = false
            if modes then
                local info = modes.get_mode_info and modes.get_mode_info()
                if info and info.prompt_prefix then
                    prefix = info.prompt_prefix
                end
                text = modes.get_prompt() or ""
                is_modal = modes.is_modal_input()
            end
            if not is_modal and not prefix and text == "" then
                return nil
            end
            local cur_mode = modes and modes.get_mode() or ""
            local show_comp = (cur_mode == "command")
            return ui.render_prompt({
                prefix = prefix or ":",
                text = text,
                show_completion = show_comp,
            })
        "#;
        if let Ok(tbl) = self.lua.load(code).eval::<mlua::Table>() {
            crate::ui::lua_view::LuaUiNode::from_lua(&tbl)
        } else {
            None
        }
    }

    pub fn get_tabs_view(
        &self,
        active_index: usize,
        tabs: &[crate::tabs::Tab],
        tick_count: usize,
        window_width: Option<f32>,
    ) -> Option<crate::ui::lua_view::LuaUiNode> {
        let tabs_tbl = self.lua.create_table().ok()?;
        let spinner_frame = (tick_count / 2) % 8;

        for (i, tab) in tabs.iter().enumerate() {
            let t = self.lua.create_table().ok()?;
            t.set("index", i + 1).ok()?;
            t.set("title", tab.title()).ok()?;
            t.set("uri", tab.uri()).ok()?;
            t.set("is_loading", tab.is_loading()).ok()?;

            if tab.is_loading() {
                let spinner_fav = crate::engine::icons::spinner_favicon(spinner_frame);
                let spinner_tbl = self.lua.create_table().ok()?;
                spinner_tbl.set("glyph", spinner_fav.glyph.as_str()).ok()?;
                if let Some(bytes) = &spinner_fav.bytes {
                    let lua_bytes = self.lua.create_string(bytes).ok()?;
                    spinner_tbl.set("data", lua_bytes).ok()?;
                }
                t.set("spinner", spinner_tbl).ok()?;
            }

            let fav_tbl = self.lua.create_table().ok()?;
            let fav = tab.favicon().cloned().unwrap_or_else(|| {
                if tab.uri().starts_with("about:help") {
                    crate::engine::icons::help_favicon()
                } else if tab.uri().starts_with("about:blank") {
                    crate::engine::icons::blank_favicon()
                } else if tab.uri().starts_with("about:") {
                    crate::engine::icons::neoweb_favicon()
                } else {
                    crate::engine::icons::default_globe_favicon()
                }
            });

            fav_tbl.set("glyph", fav.glyph.as_str()).ok()?;
            fav_tbl.set("is_custom", fav.is_custom).ok()?;
            if let Some(bytes) = &fav.bytes {
                let lua_bytes = self.lua.create_string(bytes).ok()?;
                fav_tbl.set("data", lua_bytes).ok()?;
            }
            if let Some(u) = &fav.uri {
                fav_tbl.set("uri", u.as_str()).ok()?;
            }
            t.set("favicon", fav_tbl).ok()?;

            tabs_tbl.set(i + 1, t).ok()?;
        }

        let data = self.lua.create_table().ok()?;
        data.set("active_index", active_index + 1).ok()?;
        data.set("tabs", tabs_tbl).ok()?;
        data.set("width", window_width.unwrap_or(1280.0)).ok()?;

        let code = r#"
            local data = ...
            local ui = package.loaded["ui"] or require("ui")
            if ui and ui.render_tabs then
                return ui.render_tabs(data)
            end
            return nil
        "#;
        let func = self.lua.load(code).into_function().ok()?;
        let res: mlua::Value = func.call(data).ok()?;
        if let mlua::Value::Table(tbl) = res {
            crate::ui::lua_view::LuaUiNode::from_lua(&tbl)
        } else {
            None
        }
    }

    pub fn get_status_view(
        &self,
        props: &crate::ui::status_bar::StatusBarProps<'_>,
    ) -> Option<crate::ui::lua_view::LuaUiNode> {
        let data = self.lua.create_table().ok()?;
        data.set("mode", props.mode).ok()?;
        data.set("uri", props.uri).ok()?;
        if let Some(h) = props.hover_uri {
            data.set("hover_uri", h).ok()?;
        }
        data.set("progress", props.progress).ok()?;
        data.set("key_buffer", props.key_buffer.as_str()).ok()?;
        data.set("tab_index", props.tab_index).ok()?;
        data.set("total_tabs", props.total_tabs).ok()?;
        data.set("scroll_pct", props.scroll_pct).ok()?;

        let code = r#"
            local data = ...
            local ui = package.loaded["ui"] or require("ui")
            if ui and ui.render_status then
                return ui.render_status(data)
            end
            return nil
        "#;
        let func = self.lua.load(code).into_function().ok()?;
        let res: mlua::Value = func.call(data).ok()?;
        if let mlua::Value::Table(tbl) = res {
            crate::ui::lua_view::LuaUiNode::from_lua(&tbl)
        } else {
            None
        }
    }

    pub fn get_notification_view(&self) -> Option<crate::ui::lua_view::LuaUiNode> {
        let code = r#"
            local notify = package.loaded["ui.notify"] or require("ui.notify")
            if notify and notify.get_view then
                return notify.get_view()
            end
            return nil
        "#;
        if let Ok(tbl) = self.lua.load(code).eval::<mlua::Table>() {
            crate::ui::lua_view::LuaUiNode::from_lua(&tbl)
        } else {
            None
        }
    }

    pub fn notify(&self, level: &str, message: &str) {
        let code = r#"
            local level, message = ...
            local notify = package.loaded["ui.notify"] or require("ui.notify")
            if notify and notify.notify then
                notify.notify(message, level)
            end
        "#;
        if let Ok(func) = self.lua.load(code).into_function() {
            let _ = func.call::<()>((level, message));
        }
    }

    pub fn clear_notification(&self) {
        let code = r#"
            local notify = package.loaded["ui.notify"] or require("ui.notify")
            if notify and notify.clear then
                notify.clear()
            end
        "#;
        let _ = self.lua.load(code).exec();
    }

    /// Query mandatory tab strip height from Lua without fallback.
    pub fn get_tab_strip_height(&self) -> f32 {
        let code = r#"
            local ui = package.loaded["ui"] or require("ui")
            return ui.get_tab_strip_height()
        "#;
        self.lua
            .load(code)
            .eval::<f32>()
            .expect("ui.tabs: get_height() must return tab strip height from Lua")
    }

    /// Query mandatory bottom bar height from Lua without fallback.
    pub fn get_bottom_bar_height(&self) -> f32 {
        let code = r#"
            local ui = package.loaded["ui"] or require("ui")
            return ui.get_bottom_bar_height()
        "#;
        self.lua
            .load(code)
            .eval::<f32>()
            .expect("ui.status: get_height() must return bottom bar height from Lua")
    }

    pub fn eval_lua(&self, code: &str) -> Result<String, String> {
        match self.lua.load(code).eval::<mlua::Value>() {
            Ok(val) => Ok(format!("{:?}", val)),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    pub description: String,
    pub is_selected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_runtime_and_bridge() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = LuaSender::new(tx);
        let storage = Arc::new(Mutex::new(crate::storage::Storage::in_memory().unwrap()));
        let temp_dir = std::env::temp_dir().join(format!("neoweb_lua_bm_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");
        let bookmarks = Arc::new(Mutex::new(crate::bookmarks::BookmarkStore::new(Some(
            bm_path,
        ))));
        let runtime =
            LuaRuntime::with_config_dir(sender, bookmarks.clone(), storage, Some(temp_dir.clone()))
                .unwrap();

        // Test evaluating Lua expressions
        let res = runtime.eval_lua("1 + 1").unwrap();
        assert_eq!(res, "Integer(2)");

        // Test window proxy calls emitting channel messages
        runtime
            .lua
            .load("w:navigate('https://servo.org')")
            .exec()
            .unwrap();
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Navigate(url) => assert_eq!(url, "https://servo.org"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        runtime.lua.load("w:notify('Welcome')").exec().unwrap();
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Notify { message, .. } => assert_eq!(message, "Welcome"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test bookmark glue
        runtime
            .lua
            .load("luakit.add_bookmark('https://example.com', 'Example', 'tag')")
            .exec()
            .unwrap();
        let bms: mlua::Table = runtime
            .lua
            .load("luakit.get_bookmarks('example')")
            .eval()
            .unwrap();
        assert_eq!(bms.len().unwrap(), 1);

        // Test Lua key router in normal mode
        runtime.load_init_script();
        assert!(runtime.handle_key("j"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Scroll(dx, dy) => {
                assert_eq!(dx, 0.0);
                assert_eq!(dy, 50.0);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test count prefix: 3j -> 150px
        assert!(runtime.handle_key("3"));
        assert_eq!(runtime.get_key_buffer(), "3");
        assert!(runtime.handle_key("j"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Scroll(dx, dy) => {
                assert_eq!(dx, 0.0);
                assert_eq!(dy, 150.0);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test multi-key sequence: gg -> ScrollPct(0.0)
        assert!(runtime.handle_key("g"));
        assert_eq!(runtime.get_key_buffer(), "g");
        assert!(runtime.handle_key("g"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::ScrollPct(pct) => {
                assert_eq!(pct, 0.0);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test "G" key in normal mode -> ScrollPct(100.0)
        assert!(runtime.handle_key("G"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::ScrollPct(pct) => {
                assert_eq!(pct, 100.0);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test count prefix with "G": 50G -> ScrollPct(50.0)
        assert!(runtime.handle_key("5"));
        assert_eq!(runtime.get_key_buffer(), "5");
        assert!(runtime.handle_key("0"));
        assert_eq!(runtime.get_key_buffer(), "50");
        assert!(runtime.handle_key("G"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::ScrollPct(pct) => {
                assert_eq!(pct, 50.0);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test <C-o> for history back
        assert!(runtime.handle_key("<C-o>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Back => {}
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test <C-i> for history forward
        assert!(runtime.handle_key("<C-i>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Forward => {}
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test count prefix with <C-o>: 2<C-o> sends 2 back messages
        assert!(runtime.handle_key("2"));
        assert_eq!(runtime.get_key_buffer(), "2");
        assert!(runtime.handle_key("<C-o>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Back => {}
            other => panic!("Unexpected msg: {:?}", other),
        }
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Back => {}
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test mode switch key: : -> SetMode("command")
        assert!(runtime.handle_key(":"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => {
                assert_eq!(m, "command");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert_eq!(runtime.get_prompt_text(), ":");

        // Test typing into prompt in command mode
        assert!(runtime.handle_key("b"));
        assert_eq!(runtime.get_prompt_text(), ":b");

        // Test backspace in command mode
        assert!(runtime.handle_key("<BS>"));
        assert_eq!(runtime.get_prompt_text(), ":");

        // Test backspacing past start exits to normal mode
        assert!(runtime.handle_key("<BS>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => {
                assert_eq!(m, "normal");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert_eq!(runtime.get_prompt_text(), "");

        // Test "o" key in normal mode -> sets prompt to ":open " and enters command mode
        assert!(runtime.handle_key("o"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => {
                assert_eq!(m, "command");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert_eq!(runtime.get_prompt_text(), ":open ");

        // Test typing digits in command mode (regression test for digit input buffering)
        for c in "192.168.1.1".chars() {
            assert!(runtime.handle_key(&c.to_string()));
        }
        assert_eq!(runtime.get_prompt_text(), ":open 192.168.1.1");

        // Test <C-w> deleting the last word cleanly without leaving any leading characters
        assert!(runtime.handle_key("<C-w>"));
        assert_eq!(runtime.get_prompt_text(), ":open ");

        // Re-type URL to verify execution
        for c in "https://ex".chars() {
            assert!(runtime.handle_key(&c.to_string()));
        }
        assert_eq!(runtime.get_prompt_text(), ":open https://ex");

        // Test <CR> in command mode -> executes command, returns to normal mode
        assert!(runtime.handle_key("<CR>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => {
                assert_eq!(m, "normal");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Navigate(url) => {
                assert_eq!(url, "https://ex");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert_eq!(runtime.get_prompt_text(), "");

        // Test "t" key in normal mode -> sets prompt to ":tabopen "
        assert!(runtime.handle_key("t"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => {
                assert_eq!(m, "command");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert_eq!(runtime.get_prompt_text(), ":tabopen ");

        // Test <Esc> cancels and returns to normal mode
        assert!(runtime.handle_key("<Esc>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => {
                assert_eq!(m, "normal");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert_eq!(runtime.get_prompt_text(), "");

        // Test :bookmark and :b commands
        runtime.sync_state("https://servo.org", "Servo Project", 1.0);
        assert!(runtime.run_command("bookmark", "").unwrap());
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Notify { level, message } => {
                assert_eq!(level, "info");
                assert_eq!(message, "Bookmarked: Servo Project");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        assert!(runtime.run_command("b", "other").unwrap());
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Notify { level, message } => {
                assert_eq!(level, "info");
                assert_eq!(message, "Bookmarked: Servo Project");
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        let saved = bookmarks.lock().unwrap().search("servo");
        assert_eq!(saved.len(), 2);

        // Test tab completion on ":" prompt
        assert!(runtime.handle_key(":"));
        assert_eq!(runtime.get_prompt_text(), ":");
        // Hit <Tab> on ":"
        assert!(runtime.handle_key("<Tab>"));
        // Prompt should be replaced with first command
        let comp_prompt = runtime.get_prompt_text();
        assert!(comp_prompt.starts_with(':'));
        assert!(comp_prompt.len() > 1);

        // Verify get_completion_view() returns a valid LuaUiNode (built by item_list in Lua)
        let node_opt = runtime.get_completion_view();
        assert!(node_opt.is_some(), "Expected completion_view to be Some");
        let node = node_opt.unwrap();
        match &node {
            crate::ui::lua_view::LuaUiNode::Container { child, .. } => {
                match &**child {
                    crate::ui::lua_view::LuaUiNode::Column { children, .. } => {
                        assert!(!children.is_empty());
                        let btn = children
                            .iter()
                            .find(|c| matches!(c, crate::ui::lua_view::LuaUiNode::Button { .. }));
                        assert!(
                            btn.is_some(),
                            "Expected item_list to contain button children"
                        );
                        if let Some(crate::ui::lua_view::LuaUiNode::Button {
                            callback_id, ..
                        }) = btn
                        {
                            assert!(callback_id.is_some(), "Expected button to have callback_id");
                            // Test invoking the Lua UI callback
                            runtime.invoke_ui_callback(callback_id.unwrap());
                        }
                    }
                    _ => panic!("Expected Column node inside Container for item_list view"),
                }
            }
            crate::ui::lua_view::LuaUiNode::Column { children, .. } => {
                assert!(!children.is_empty());
                let btn = children
                    .iter()
                    .find(|c| matches!(c, crate::ui::lua_view::LuaUiNode::Button { .. }));
                assert!(
                    btn.is_some(),
                    "Expected item_list to contain button children"
                );
                if let Some(crate::ui::lua_view::LuaUiNode::Button { callback_id, .. }) = btn {
                    assert!(callback_id.is_some(), "Expected button to have callback_id");
                    // Test invoking the Lua UI callback
                    runtime.invoke_ui_callback(callback_id.unwrap());
                }
            }
            _ => panic!(
                "Expected Container or Column node for item_list view, got {:?}",
                node
            ),
        }

        // Test <C-n> cycles forwards
        assert!(runtime.handle_key("<C-n>"));
        let next_prompt = runtime.get_prompt_text();
        assert!(next_prompt.starts_with(':'));

        // Test <C-p> cycles backwards
        assert!(runtime.handle_key("<C-p>"));
        let p_prompt = runtime.get_prompt_text();
        assert!(p_prompt.starts_with(':'));

        // Test <S-Tab> cycles backwards
        assert!(runtime.handle_key("<S-Tab>"));
        let prev_prompt = runtime.get_prompt_text();
        assert!(prev_prompt.starts_with(':'));

        // Test <Esc> dismisses command mode back to normal
        assert!(runtime.handle_key("<Esc>"));
        assert!(
            runtime.get_completion_view().is_none(),
            "Expected completion_view to be None after Esc"
        );
        while rx.try_recv().is_ok() {}

        // Test in-page search via Lua window bindings
        assert!(runtime.handle_key("/"));
        assert_eq!(runtime.get_prompt_text(), "/");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "search"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Type query 'r', 'u', 's', 't'
        assert!(runtime.handle_key("r"));
        assert!(runtime.handle_key("u"));
        assert!(runtime.handle_key("s"));
        assert!(runtime.handle_key("t"));
        assert_eq!(runtime.get_prompt_text(), "/rust");

        // Execute search with <CR>
        assert!(runtime.handle_key("<CR>"));
        assert_eq!(runtime.get_prompt_text(), "");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Search { query, opts } => {
                assert_eq!(query, Some("rust".to_string()));
                assert!(!opts.backward);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Test next/previous search bindings in normal mode: n and N
        assert!(runtime.handle_key("n"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Search { query, opts } => {
                assert_eq!(query, None);
                assert!(!opts.backward);
                assert!(opts.find_next);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        assert!(runtime.handle_key("N"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Search { query, opts } => {
                assert_eq!(query, None);
                assert!(opts.backward);
                assert!(opts.find_next);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lua_ui_components() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = LuaSender::new(tx);
        let storage = Arc::new(Mutex::new(crate::storage::Storage::in_memory().unwrap()));
        let temp_dir = std::env::temp_dir().join(format!("neoweb_lua_ui_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");
        let bookmarks = Arc::new(Mutex::new(crate::bookmarks::BookmarkStore::new(Some(
            bm_path,
        ))));
        let runtime =
            LuaRuntime::with_config_dir(sender, bookmarks, storage, Some(temp_dir.clone()))
                .unwrap();
        runtime.load_init_script();

        // 1. Test get_tabs_view
        let mut tab_mgr = crate::tabs::TabManager::new("about:neoweb");
        tab_mgr.new_tab("https://servo.org");
        tab_mgr.goto_tab(0);
        let tabs_node = runtime.get_tabs_view(tab_mgr.active_index(), tab_mgr.tabs(), 0, None);
        assert!(tabs_node.is_some(), "Expected get_tabs_view to return Some");
        let node = tabs_node.unwrap();
        match &node {
            crate::ui::lua_view::LuaUiNode::Container {
                style,
                height,
                child,
                ..
            } => {
                assert_eq!(style, "tab_strip");
                assert_eq!(height, &Some(iced::Length::Fixed(28.0)));
                match &**child {
                    crate::ui::lua_view::LuaUiNode::Row {
                        children,
                        width,
                        height,
                        ..
                    } => {
                        assert_eq!(width, &Some(iced::Length::Fill));
                        assert_eq!(height, &Some(iced::Length::Fixed(28.0)));
                        assert_eq!(children.len(), 2); // 2 tab pills (no '+' button)

                        // Verify tab pill takes all space and close button aligns right
                        match &children[0] {
                            crate::ui::lua_view::LuaUiNode::Container {
                                width: pill_w,
                                height: pill_h,
                                child: pill_child,
                                ..
                            } => {
                                assert_eq!(pill_w, &Some(iced::Length::Fill));
                                assert_eq!(pill_h, &Some(iced::Length::Fixed(28.0)));
                                match &**pill_child {
                                    crate::ui::lua_view::LuaUiNode::Row {
                                        children: inner,
                                        width: inner_w,
                                        ..
                                    } => {
                                        assert_eq!(inner_w, &Some(iced::Length::Fill));
                                        assert_eq!(inner.len(), 2);
                                        // select_btn takes all remaining space and contains favicon
                                        match &inner[0] {
                                            crate::ui::lua_view::LuaUiNode::Button {
                                                child: btn_child,
                                                width: btn_w,
                                                ..
                                            } => {
                                                assert_eq!(btn_w, &Some(iced::Length::Fill));
                                                match &**btn_child {
                                                    crate::ui::lua_view::LuaUiNode::Row {
                                                        children: btn_row,
                                                        ..
                                                    } => {
                                                        assert_eq!(btn_row.len(), 4); // [num_text, fav_widget, Space, title_text]
                                                        // Tab 0 is about:neoweb (not loading), renders static page favicon image
                                                        match &btn_row[1] {
                                                            crate::ui::lua_view::LuaUiNode::Image { width, height, .. } => {
                                                                assert_eq!(width, &Some(iced::Length::Fixed(16.0)));
                                                                assert_eq!(height, &Some(iced::Length::Fixed(16.0)));
                                                            }
                                                            _ => panic!("Expected favicon image widget"),
                                                        }
                                                    }
                                                    _ => panic!(
                                                        "Expected inner Row inside select button"
                                                    ),
                                                }
                                            }
                                            _ => panic!("Expected select button"),
                                        }
                                        // close_btn is pinned to the right with 16x16 size (matching favicon) and centered vertically
                                        match &inner[1] {
                                            crate::ui::lua_view::LuaUiNode::Button {
                                                width: btn_w,
                                                height: btn_h,
                                                child: btn_child,
                                                ..
                                            } => {
                                                assert_eq!(btn_w, &Some(iced::Length::Fixed(16.0)));
                                                assert_eq!(btn_h, &Some(iced::Length::Fixed(16.0)));
                                                match &**btn_child {
                                                    crate::ui::lua_view::LuaUiNode::Text {
                                                        content,
                                                        align,
                                                        width,
                                                        height,
                                                        ..
                                                    } => {
                                                        assert_eq!(content, "×");
                                                        assert_eq!(
                                                            align.as_deref(),
                                                            Some("center")
                                                        );
                                                        assert_eq!(
                                                            width,
                                                            &Some(iced::Length::Fixed(16.0))
                                                        );
                                                        assert_eq!(
                                                            height,
                                                            &Some(iced::Length::Fixed(16.0))
                                                        );
                                                    }
                                                    _ => panic!(
                                                        "Expected centered text child inside close button"
                                                    ),
                                                }
                                            }
                                            _ => panic!("Expected close button"),
                                        }
                                    }
                                    _ => panic!("Expected inner Row inside tab pill"),
                                }
                            }
                            _ => panic!("Expected Container for tab pill"),
                        }

                        // Tab 1 is https://servo.org (inactive tab), so it must not display close button (inner.len() == 1)
                        match &children[1] {
                            crate::ui::lua_view::LuaUiNode::Container {
                                child: pill_child, ..
                            } => match &**pill_child {
                                crate::ui::lua_view::LuaUiNode::Row {
                                    children: inner, ..
                                } => {
                                    assert_eq!(
                                        inner.len(),
                                        1,
                                        "Inactive tab must not display close button"
                                    );
                                    match &inner[0] {
                                        crate::ui::lua_view::LuaUiNode::Button {
                                            child: btn_child,
                                            ..
                                        } => match &**btn_child {
                                            crate::ui::lua_view::LuaUiNode::Row {
                                                children: btn_row,
                                                ..
                                            } => match &btn_row[1] {
                                                crate::ui::lua_view::LuaUiNode::Spinner {
                                                    width,
                                                    height,
                                                    circle_radius,
                                                } => {
                                                    assert_eq!(
                                                        width,
                                                        &Some(iced::Length::Fixed(16.0))
                                                    );
                                                    assert_eq!(
                                                        height,
                                                        &Some(iced::Length::Fixed(16.0))
                                                    );
                                                    assert_eq!(circle_radius, &Some(1.5));
                                                }
                                                _ => panic!(
                                                    "Expected Spinner widget for loading tab, got {:?}",
                                                    btn_row[1]
                                                ),
                                            },
                                            _ => panic!("Expected inner Row inside select button"),
                                        },
                                        _ => panic!("Expected select button"),
                                    }
                                }
                                _ => panic!("Expected inner Row inside tab pill"),
                            },
                            _ => panic!("Expected Container for tab pill"),
                        }
                        let mut callback_ids = Vec::new();
                        fn collect_callbacks(
                            n: &crate::ui::lua_view::LuaUiNode,
                            out: &mut Vec<usize>,
                        ) {
                            match n {
                                crate::ui::lua_view::LuaUiNode::Button {
                                    callback_id: Some(id),
                                    ..
                                } => {
                                    out.push(*id);
                                }
                                crate::ui::lua_view::LuaUiNode::Container { child, .. } => {
                                    collect_callbacks(child, out);
                                }
                                crate::ui::lua_view::LuaUiNode::Row { children, .. }
                                | crate::ui::lua_view::LuaUiNode::Column { children, .. } => {
                                    for c in children {
                                        collect_callbacks(c, out);
                                    }
                                }
                                _ => {}
                            }
                        }
                        for c in children {
                            collect_callbacks(c, &mut callback_ids);
                        }
                        assert!(!callback_ids.is_empty(), "Expected interactive tab buttons");

                        // Trigger the first callback (select tab 0)
                        runtime.invoke_ui_callback(callback_ids[0]);
                        match rx.try_recv().unwrap() {
                            LuaToRustMsg::GotoTab(idx) => assert_eq!(idx, 0),
                            other => panic!("Unexpected msg: {:?}", other),
                        }

                        // Trigger the close button callback
                        runtime.invoke_ui_callback(callback_ids[1]);
                        match rx.try_recv().unwrap() {
                            LuaToRustMsg::CloseTab(Some(idx)) => assert_eq!(idx, 0),
                            other => panic!("Unexpected msg: {:?}", other),
                        }
                    }
                    _ => panic!("Expected Row child inside tab_strip Container"),
                }
            }
            _ => panic!("Expected Container node for tabs view, got {:?}", node),
        }

        // 2. Test get_status_view
        let status_props = crate::ui::status_bar::StatusBarProps {
            mode: "normal",
            uri: "https://servo.org",
            hover_uri: Some("https://github.com/servo/servo"),
            progress: 0.85,
            key_buffer: "2j".to_string(),
            tab_index: 1,
            total_tabs: 2,
            scroll_pct: 42.0,
        };
        let status_node = runtime.get_status_view(&status_props);
        assert!(
            status_node.is_some(),
            "Expected get_status_view to return Some"
        );
        let node = status_node.unwrap();
        match &node {
            crate::ui::lua_view::LuaUiNode::Container { style, .. } => {
                assert_eq!(style, "status_bar");
            }
            _ => panic!("Expected Container node for status view, got {:?}", node),
        }

        // 3. Test get_notification_view
        assert!(runtime.get_notification_view().is_none());
        runtime.notify("warning", "Failed to connect to host");
        let notif_node = runtime.get_notification_view();
        assert!(
            notif_node.is_some(),
            "Expected get_notification_view to return Some"
        );
        let node = notif_node.unwrap();
        match &node {
            crate::ui::lua_view::LuaUiNode::Container { style, child, .. } => {
                assert_eq!(style, "dark");
                match &**child {
                    crate::ui::lua_view::LuaUiNode::Row { children, .. } => {
                        assert_eq!(children.len(), 2);
                        match &children[0] {
                            crate::ui::lua_view::LuaUiNode::Text { content, color, .. } => {
                                assert_eq!(content, "[WARNING] ");
                                assert_eq!(color, "yellow");
                            }
                            _ => panic!("Expected badge text"),
                        }
                        match &children[1] {
                            crate::ui::lua_view::LuaUiNode::Text { content, .. } => {
                                assert_eq!(content, "Failed to connect to host");
                            }
                            _ => panic!("Expected message text"),
                        }
                    }
                    _ => panic!("Expected Row child inside notification Container"),
                }
            }
            _ => panic!(
                "Expected Container node for notification view, got {:?}",
                node
            ),
        }
        runtime.clear_notification();
        assert!(runtime.get_notification_view().is_none());

        // 4. Test ui.image primitive
        let test_img_code = r#"
            local ui = require("ui")
            return ui.image({ data = "\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR", width = 16, height = 16 })
        "#;
        let tbl: mlua::Table = runtime.lua.load(test_img_code).eval().unwrap();
        let node = crate::ui::lua_view::LuaUiNode::from_lua(&tbl).unwrap();
        match node {
            crate::ui::lua_view::LuaUiNode::Image { width, height, .. } => {
                assert_eq!(width, Some(iced::Length::Fixed(16.0)));
                assert_eq!(height, Some(iced::Length::Fixed(16.0)));
            }
            _ => panic!("Expected Image node"),
        }

        // 5. Test ui.spinner primitive
        let test_spinner_code = r#"
            local ui = require("ui")
            return ui.spinner({ width = 16, height = 16, circle_radius = 1.5 })
        "#;
        let tbl: mlua::Table = runtime.lua.load(test_spinner_code).eval().unwrap();
        let node = crate::ui::lua_view::LuaUiNode::from_lua(&tbl).unwrap();
        match node {
            crate::ui::lua_view::LuaUiNode::Spinner {
                width,
                height,
                circle_radius,
            } => {
                assert_eq!(width, Some(iced::Length::Fixed(16.0)));
                assert_eq!(height, Some(iced::Length::Fixed(16.0)));
                assert_eq!(circle_radius, Some(1.5));
            }
            _ => panic!("Expected Spinner node"),
        }

        // 6. Test dynamic tab title truncation and strict 28px height
        let test_tabs_code = r#"
            local tabs = require("ui.tabs")
            return tabs.build({
                active_index = 1,
                width = 300,
                tabs = {
                    { index = 1, title = "A Very Long Page Title For Tab Truncation Testing — Ümläut & 🚀 Emojis", uri = "https://servo.org" },
                    { index = 2, title = "Second Tab", uri = "about:blank" },
                }
            })
        "#;
        let tabs_tbl: mlua::Table = runtime.lua.load(test_tabs_code).eval().unwrap();
        let narrow_tabs = crate::ui::lua_view::LuaUiNode::from_lua(&tabs_tbl).unwrap();
        match narrow_tabs {
            crate::ui::lua_view::LuaUiNode::Container { height, child, .. } => {
                assert_eq!(height, Some(iced::Length::Fixed(28.0)));
                match *child {
                    crate::ui::lua_view::LuaUiNode::Row { children, .. } => {
                        assert_eq!(children.len(), 2);
                        fn find_title(
                            node: &crate::ui::lua_view::LuaUiNode,
                        ) -> Option<(String, Option<String>)> {
                            match node {
                                crate::ui::lua_view::LuaUiNode::Text {
                                    content, wrapping, ..
                                } if content.contains("...") => {
                                    Some((content.clone(), wrapping.clone()))
                                }
                                crate::ui::lua_view::LuaUiNode::Container { child, .. } => {
                                    find_title(child)
                                }
                                crate::ui::lua_view::LuaUiNode::Button { child, .. } => {
                                    find_title(child)
                                }
                                crate::ui::lua_view::LuaUiNode::Row { children, .. }
                                | crate::ui::lua_view::LuaUiNode::Column { children, .. } => {
                                    for c in children {
                                        if let Some(res) = find_title(c) {
                                            return Some(res);
                                        }
                                    }
                                    None
                                }
                                _ => None,
                            }
                        }
                        let (truncated_title, wrapping) =
                            find_title(&children[0]).expect("expected truncated title");
                        assert!(
                            truncated_title.ends_with("..."),
                            "Title should end with ellipsis: {}",
                            truncated_title
                        );
                        assert!(
                            truncated_title.len()
                                < "A Very Long Page Title For Tab Truncation Testing — Ümläut & 🚀 Emojis"
                                    .len()
                        );
                        assert_eq!(wrapping.as_deref(), Some("none"));
                    }
                    _ => panic!("Expected Row child inside tab_strip"),
                }
            }
            _ => panic!("Expected Container for tab strip"),
        }

        // 7. Test get_prompt_view in normal, command, and search modes
        assert!(runtime.get_prompt_view().is_none());
        runtime.set_mode("command");
        let prompt_node = runtime.get_prompt_view();
        assert!(
            prompt_node.is_some(),
            "Expected get_prompt_view to return Some in command mode"
        );
        match prompt_node.unwrap() {
            crate::ui::lua_view::LuaUiNode::Container { style, .. } => {
                assert_eq!(style, "prompt_bar");
            }
            other => panic!("Expected Container with prompt_bar style, got {:?}", other),
        }

        runtime.set_mode("search");
        assert!(
            runtime.get_prompt_view().is_some(),
            "Expected get_prompt_view to return Some in search mode"
        );

        runtime.set_mode("normal");
        assert!(runtime.get_prompt_view().is_none());

        // 8. Test mandatory dynamic tab strip height and bottom bar height without fallback
        assert_eq!(runtime.get_tab_strip_height(), 28.0);
        assert_eq!(runtime.get_bottom_bar_height(), 28.0);

        runtime
            .lua
            .load("require('ui.tabs').set_height(35)")
            .exec()
            .unwrap();
        assert_eq!(runtime.get_tab_strip_height(), 35.0);

        runtime
            .lua
            .load("require('ui.tabs').set_height(28)")
            .exec()
            .unwrap();
        assert_eq!(runtime.get_tab_strip_height(), 28.0);

        runtime
            .lua
            .load("require('ui.status').set_height(32)")
            .exec()
            .unwrap();
        assert_eq!(runtime.get_bottom_bar_height(), 32.0);

        runtime
            .lua
            .load("require('ui.status').set_height(28)")
            .exec()
            .unwrap();
        assert_eq!(runtime.get_bottom_bar_height(), 28.0);

        tab_mgr.close_all();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lua_modal_state_machine() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = LuaSender::new(tx);
        let storage = Arc::new(Mutex::new(crate::storage::Storage::in_memory().unwrap()));
        let temp_dir = std::env::temp_dir().join(format!("neoweb_lua_fsm_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");
        let bookmarks = Arc::new(Mutex::new(crate::bookmarks::BookmarkStore::new(Some(
            bm_path,
        ))));
        let runtime =
            LuaRuntime::with_config_dir(sender, bookmarks, storage, Some(temp_dir.clone()))
                .unwrap();
        runtime.load_init_script();

        // 1. Initial state is normal mode
        assert_eq!(runtime.get_mode(), "normal");
        assert!(!runtime.is_passthrough_mode());
        assert!(!runtime.is_modal_input_mode());

        // 2. Register custom mode with lifecycle hooks and badge
        let custom_mode_code = r#"
            local modes = require("modes")
            _G.test_hook_trail = {}
            modes.new_mode("visual", {
                description = "Visual selection mode",
                badge = { text = "VISUAL", color = "yellow" },
                passthrough = false,
                modal_input = false,
                enter = function (w)
                    table.insert(_G.test_hook_trail, "enter_visual")
                end,
                leave = function (w)
                    table.insert(_G.test_hook_trail, "leave_visual")
                end,
            })
        "#;
        runtime.eval_lua(custom_mode_code).unwrap();

        // 3. Transition into custom mode
        runtime.set_mode("visual");
        assert_eq!(runtime.get_mode(), "visual");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "visual"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Verify enter hook ran
        let trail = runtime
            .eval_lua("table.concat(_G.test_hook_trail, ',')")
            .unwrap();
        assert!(trail.contains("enter_visual"));

        // 4. Transition into insert mode (passthrough)
        runtime.set_mode("insert");
        assert_eq!(runtime.get_mode(), "insert");
        assert!(runtime.is_passthrough_mode());
        assert!(!runtime.is_modal_input_mode());
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "insert"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Verify visual leave hook ran
        let trail = runtime
            .eval_lua("table.concat(_G.test_hook_trail, ',')")
            .unwrap();
        assert!(trail.contains("leave_visual"));

        // 5. Test modal input mode (command mode)
        runtime.set_mode("command");
        assert_eq!(runtime.get_mode(), "command");
        assert!(!runtime.is_passthrough_mode());
        assert!(runtime.is_modal_input_mode());
        assert_eq!(runtime.get_prompt_prefix(), Some(":".to_string()));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "command"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Verify digits 0-9 are buffered into prompt rather than swallowed by count prefix
        for d in "0123456789".chars() {
            assert!(runtime.handle_key(&d.to_string()));
        }
        assert_eq!(runtime.get_prompt_text(), ":0123456789");

        // 6. Test Esc returns to normal mode via state machine
        assert!(runtime.handle_key("<Esc>"));
        assert_eq!(runtime.get_mode(), "normal");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // 7. Test insert mode passthrough and <Esc> exit
        runtime.set_mode("insert");
        assert_eq!(runtime.get_mode(), "insert");
        assert!(runtime.is_passthrough_mode());
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "insert"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Regular character typing in insert mode is handled by Lua by explicitly sending to webview
        assert!(runtime.handle_key("a"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SendKey(k) => assert_eq!(k, "a"),
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert!(runtime.handle_key("<Space>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SendKey(k) => assert_eq!(k, "<Space>"),
            other => panic!("Unexpected msg: {:?}", other),
        }
        assert!(runtime.handle_key("<BS>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SendKey(k) => assert_eq!(k, "<BS>"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // <Esc> in insert mode is handled authoritatively by Lua, returning to normal mode
        assert!(runtime.handle_key("<Esc>"));
        assert_eq!(runtime.get_mode(), "normal");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_regression_digit_typing_in_command_and_search_modes() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = LuaSender::new(tx);
        let storage = Arc::new(Mutex::new(crate::storage::Storage::in_memory().unwrap()));
        let temp_dir =
            std::env::temp_dir().join(format!("neoweb_reg_digits_test_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");
        let bookmarks = Arc::new(Mutex::new(crate::bookmarks::BookmarkStore::new(Some(
            bm_path,
        ))));

        let runtime = LuaRuntime::new(sender, bookmarks, storage).unwrap();
        runtime.load_init_script();

        // Drain initial startup messages
        while rx.try_recv().is_ok() {}

        // --- 1. Command Mode Digit Typing ---
        // Enter command mode via ':'
        assert!(runtime.handle_key(":"));
        assert_eq!(runtime.get_mode(), "command");
        assert_eq!(runtime.get_prompt_text(), ":");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "command"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Type all digits 0-9
        for d in "0123456789".chars() {
            assert!(runtime.handle_key(&d.to_string()));
        }
        assert_eq!(runtime.get_prompt_text(), ":0123456789");

        // Clear prompt with <C-u>
        assert!(runtime.handle_key("<C-u>"));
        assert_eq!(runtime.get_prompt_text(), ":");

        // Type an IP address and port: open 192.168.1.1:8080
        for ch in "open 192.168.1.1:8080".chars() {
            let key = if ch == ' ' {
                "<Space>"
            } else {
                &ch.to_string()
            };
            assert!(runtime.handle_key(key));
        }
        assert_eq!(runtime.get_prompt_text(), ":open 192.168.1.1:8080");

        // Exit command mode via <Esc>
        assert!(runtime.handle_key("<Esc>"));
        assert_eq!(runtime.get_mode(), "normal");
        assert_eq!(runtime.get_prompt_text(), "");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // --- 2. Forward Search Mode Digit Typing ---
        // Enter search mode via '/'
        assert!(runtime.handle_key("/"));
        assert_eq!(runtime.get_mode(), "search");
        assert_eq!(runtime.get_prompt_text(), "/");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "search"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Type query containing digits: issue404 1234567890
        for ch in "issue404 1234567890".chars() {
            let key = if ch == ' ' {
                "<Space>"
            } else {
                &ch.to_string()
            };
            assert!(runtime.handle_key(key));
        }
        assert_eq!(runtime.get_prompt_text(), "/issue404 1234567890");

        // Clear search prompt with <C-u>
        assert!(runtime.handle_key("<C-u>"));
        assert_eq!(runtime.get_prompt_text(), "/");

        // Exit search mode via <Esc>
        assert!(runtime.handle_key("<Esc>"));
        assert_eq!(runtime.get_mode(), "normal");
        assert_eq!(runtime.get_prompt_text(), "");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::ClearSearch => {}
            other => panic!("Unexpected msg: {:?}", other),
        }
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // --- 3. Backward Search (via unified search mode) ---
        // Enter search mode via '?'
        assert!(runtime.handle_key("?"));
        assert_eq!(runtime.get_mode(), "search");
        assert_eq!(runtime.get_prompt_text(), "?");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "search"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Type query with digits: error500
        for ch in "error500".chars() {
            assert!(runtime.handle_key(&ch.to_string()));
        }
        assert_eq!(runtime.get_prompt_text(), "?error500");

        // Exit search mode via <Esc>
        assert!(runtime.handle_key("<Esc>"));
        assert_eq!(runtime.get_mode(), "normal");
        assert_eq!(runtime.get_prompt_text(), "");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::ClearSearch => {}
            other => panic!("Unexpected msg: {:?}", other),
        }
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Verify executing backward search passes backward=true and sets up n/N
        assert!(runtime.handle_key("?"));
        assert_eq!(runtime.get_mode(), "search");
        assert_eq!(runtime.get_prompt_text(), "?");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "search"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        for ch in "revmatch".chars() {
            assert!(runtime.handle_key(&ch.to_string()));
        }
        assert_eq!(runtime.get_prompt_text(), "?revmatch");

        assert!(runtime.handle_key("<CR>"));
        assert_eq!(runtime.get_mode(), "normal");
        assert_eq!(runtime.get_prompt_text(), "");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Search { query, opts } => {
                assert_eq!(query, Some("revmatch".to_string()));
                assert!(opts.backward);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // In reverse search, 'n' continues searching backward
        assert!(runtime.handle_key("n"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Search { query, opts } => {
                assert_eq!(query, None);
                assert!(opts.backward);
                assert!(opts.find_next);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // In reverse search, 'N' searches opposite direction (forward)
        assert!(runtime.handle_key("N"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::Search { query, opts } => {
                assert_eq!(query, None);
                assert!(!opts.backward);
                assert!(opts.find_next);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // --- 4. Normal Mode Count Buffering ---
        // In normal mode, typing '3' must buffer count and not type into prompt
        assert!(runtime.handle_key("3"));
        assert_eq!(runtime.get_mode(), "normal");
        assert_eq!(runtime.get_prompt_text(), "");
        assert_eq!(runtime.get_key_buffer(), "3");

        // Pressing 'j' consumes the count (scroll down 3 times) and clears the buffer
        assert!(runtime.handle_key("j"));
        assert_eq!(runtime.get_key_buffer(), "");
        assert_eq!(runtime.get_prompt_text(), "");

        match rx.try_recv().unwrap() {
            LuaToRustMsg::Scroll(0.0, dy) => assert_eq!(dy, 150.0),
            other => panic!("Unexpected msg: {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lua_window_signals_and_session() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = LuaSender::new(tx);
        let storage = Arc::new(Mutex::new(crate::storage::Storage::in_memory().unwrap()));
        let temp_dir =
            std::env::temp_dir().join(format!("neoweb_session_test_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");
        let bookmarks = Arc::new(Mutex::new(crate::bookmarks::BookmarkStore::new(Some(
            bm_path,
        ))));

        let runtime =
            LuaRuntime::with_config_dir(sender, bookmarks, storage, Some(temp_dir.clone()))
                .unwrap();
        runtime.load_init_script();

        // 1. Test serde_json bridge luakit.json.encode and luakit.json.decode
        let encode_test = r#"
            local json_str = luakit.json.encode({ active_index = 2, tabs = { { uri = "https://example.com", title = "Example" } } })
            local decoded = luakit.json.decode(json_str)
            return decoded.active_index == 2 and decoded.tabs[1].uri == "https://example.com"
        "#;
        let json_ok: bool = runtime.lua.load(encode_test).eval().unwrap();
        assert!(
            json_ok,
            "Expected luakit.json.encode/decode to preserve table structure"
        );

        // 2. Test w:add_signal and emit_signal for window close callback
        let signal_test = r#"
            _G.close_signal_fired = false
            w:add_signal("close", function (win)
                _G.close_signal_fired = true
            end)
        "#;
        runtime.lua.load(signal_test).exec().unwrap();
        runtime.emit_signal("close", "");
        let fired: bool = runtime
            .lua
            .load("return _G.close_signal_fired")
            .eval()
            .unwrap();
        assert!(
            fired,
            "Expected window close signal to be fired and received in Lua"
        );

        // 3. Test session save and restore using custom session name in temp_dir
        let custom_session_file = temp_dir.join("test_session.json");
        let custom_path_str = custom_session_file.to_str().unwrap();

        // Populate tab data in ui.tabs
        let set_tabs_code = format!(
            r#"
            local tabs_mod = require("ui.tabs")
            tabs_mod.build({{
                active_index = 2,
                tabs = {{
                    {{ index = 1, uri = "https://example.com/page1", title = "Page 1" }},
                    {{ index = 2, uri = "https://example.com/page2", title = "Page 2" }},
                }}
            }})

            local session = require("session")
            return session.save(w, "{}")
        "#,
            custom_path_str
        );

        let (save_ok, _): (bool, Option<String>) = runtime.lua.load(&set_tabs_code).eval().unwrap();
        assert!(save_ok, "Expected session.save to succeed");
        assert!(
            custom_session_file.exists(),
            "Expected session file to exist on disk"
        );

        // Verify session file content is valid JSON
        let content = std::fs::read_to_string(&custom_session_file).unwrap();
        let val: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(val["active_index"], 2);
        assert_eq!(val["tabs"].as_array().unwrap().len(), 2);

        // 4. Test session.restore (drain any previous messages first)
        while rx.try_recv().is_ok() {}

        let restore_code = format!(
            r#"
            local session = require("session")
            return session.restore(w, "{}")
        "#,
            custom_path_str
        );
        let restore_ok: bool = runtime.lua.load(&restore_code).eval().unwrap();
        assert!(restore_ok, "Expected session.restore to succeed");

        // Verify that restore triggered navigation messages on the channel
        let mut got_nav = false;
        let mut got_new_tab = false;
        let mut got_goto = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                LuaToRustMsg::Navigate(u) => {
                    assert_eq!(u, "https://example.com/page1");
                    got_nav = true;
                }
                LuaToRustMsg::NewTab(u) => {
                    assert_eq!(u, "https://example.com/page2");
                    got_new_tab = true;
                }
                LuaToRustMsg::GotoTab(idx) => {
                    assert_eq!(idx, 1); // 0-based for active_index 2
                    got_goto = true;
                }
                _ => {}
            }
        }
        assert!(got_nav, "Expected navigate message for tab 1");
        assert!(got_new_tab, "Expected new_tab message for tab 2");
        assert!(got_goto, "Expected goto_tab message for active index");

        // 5. Test :session command execution via run_command
        let cmd_res = runtime.run_command("session", "list");
        assert!(cmd_res.is_ok(), "Expected :session list to run");

        // 6. Test autocompletion for :session
        let comp_code = r#"
            local comp = require("completion")
            local items = comp.complete(":session ")
            local names = {}
            for _, item in ipairs(items) do
                table.insert(names, item.label)
            end
            return table.concat(names, ",")
        "#;
        let comp_labels: String = runtime.lua.load(comp_code).eval().unwrap();
        assert!(comp_labels.contains("save"));
        assert!(comp_labels.contains("restore"));
        assert!(comp_labels.contains("list"));
        assert!(comp_labels.contains("delete"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_unified_follow_mode_with_parameters() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = LuaSender::new(tx);
        let storage = Arc::new(Mutex::new(crate::storage::Storage::in_memory().unwrap()));
        let temp_dir =
            std::env::temp_dir().join(format!("neoweb_lua_follow_test_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");
        let bookmarks = Arc::new(Mutex::new(crate::bookmarks::BookmarkStore::new(Some(
            bm_path,
        ))));
        let runtime =
            LuaRuntime::with_config_dir(sender, bookmarks, storage, Some(temp_dir.clone()))
                .unwrap();
        runtime.load_init_script();

        assert_eq!(runtime.get_mode(), "normal");
        assert!(!runtime.is_follow_tab());
        assert_eq!(runtime.get_follow_target(), "current");

        // 1. Press 'f' -> enters follow mode targeting current tab
        let handled = runtime.handle_key("f");
        assert!(handled, "Key 'f' should be handled");
        assert_eq!(runtime.get_mode(), "follow");
        assert!(!runtime.is_follow_tab());
        assert_eq!(runtime.get_follow_target(), "current");

        // Verify SetMode("follow") message sent to host
        let mut got_set_mode_follow = false;
        while let Ok(msg) = rx.try_recv() {
            if let LuaToRustMsg::SetMode(m) = msg
                && m == "follow"
            {
                got_set_mode_follow = true;
            }
        }
        assert!(got_set_mode_follow, "Expected SetMode('follow') message");

        // Test hint buffering and query handling in Lua
        assert_eq!(runtime.get_follow_query(), "");
        let handled_a = runtime.handle_key("a");
        assert!(handled_a);
        assert_eq!(runtime.get_follow_query(), "a");
        let handled_b = runtime.handle_key("b");
        assert!(handled_b);
        assert_eq!(runtime.get_follow_query(), "ab");
        let handled_bs = runtime.handle_key("<BS>");
        assert!(handled_bs);
        assert_eq!(runtime.get_follow_query(), "a");
        runtime.pop_follow_query();
        assert_eq!(runtime.get_follow_query(), "");

        // Check status bar badge
        let props = crate::ui::status_bar::StatusBarProps {
            mode: "follow",
            uri: "about:neoweb",
            hover_uri: None,
            progress: 1.0,
            key_buffer: String::new(),
            tab_index: 0,
            total_tabs: 1,
            scroll_pct: 0.0,
        };
        let status_view = runtime.get_status_view(&props).unwrap();
        let view_str = format!("{:?}", status_view);
        assert!(
            view_str.contains("FOLLOW"),
            "Badge should contain FOLLOW: {}",
            view_str
        );
        assert!(
            !view_str.contains("FOLLOW-TAB"),
            "Badge should NOT be FOLLOW-TAB for current tab"
        );

        // 2. Press <Esc> -> returns to normal mode
        let esc_handled = runtime.handle_key("<Esc>");
        assert!(esc_handled, "<Esc> should be handled");
        assert_eq!(runtime.get_mode(), "normal");
        assert!(!runtime.is_follow_tab());

        // 3. Press 'F' -> enters follow mode targeting new tab
        let handled_f = runtime.handle_key("F");
        assert!(handled_f, "Key 'F' should be handled");
        assert_eq!(runtime.get_mode(), "follow");
        assert!(runtime.is_follow_tab());
        assert_eq!(runtime.get_follow_target(), "tab");

        // Check status bar badge for follow-tab
        let status_view_tab = runtime.get_status_view(&props).unwrap();
        let tab_view_str = format!("{:?}", status_view_tab);
        assert!(
            tab_view_str.contains("FOLLOW-TAB"),
            "Badge should display FOLLOW-TAB when targeting tab: {}",
            tab_view_str
        );

        // 4. Press <Esc> -> returns to normal mode
        let esc_handled2 = runtime.handle_key("<Esc>");
        assert!(esc_handled2);
        assert_eq!(runtime.get_mode(), "normal");
        assert!(!runtime.is_follow_tab());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_download_command_and_follow_hint() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = LuaSender::new(tx);
        let storage = Arc::new(Mutex::new(crate::storage::Storage::in_memory().unwrap()));
        let temp_dir =
            std::env::temp_dir().join(format!("neoweb_lua_dl_test_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");
        let bookmarks = Arc::new(Mutex::new(crate::bookmarks::BookmarkStore::new(Some(
            bm_path,
        ))));
        let runtime =
            LuaRuntime::with_config_dir(sender, bookmarks, storage, Some(temp_dir.clone()))
                .unwrap();
        runtime.load_init_script();

        // Drain any startup messages
        while rx.try_recv().is_ok() {}

        // 1. Test w:download Lua method
        runtime
            .lua
            .load(r#"_G.w:download("https://example.com/archive.zip", "my_archive.zip")"#)
            .exec()
            .unwrap();

        match rx.try_recv().unwrap() {
            LuaToRustMsg::Download { url, path } => {
                assert_eq!(url, "https://example.com/archive.zip");
                assert_eq!(path, Some("my_archive.zip".to_string()));
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // 2. Test :download command
        assert!(runtime.handle_key(":"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "command"),
            other => panic!("Unexpected msg: {:?}", other),
        }
        for c in "download https://example.com/release.tar.gz".chars() {
            assert!(runtime.handle_key(&c.to_string()));
        }
        assert!(runtime.handle_key("<CR>"));
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "normal"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        match rx.try_recv().unwrap() {
            LuaToRustMsg::Download { url, path } => {
                assert_eq!(url, "https://example.com/release.tar.gz");
                assert_eq!(path, None);
            }
            other => panic!("Unexpected msg: {:?}", other),
        }

        // 3. Test ;d follow hint download mode
        assert!(runtime.handle_key(";"));
        assert!(runtime.handle_key("d"));
        assert_eq!(runtime.get_mode(), "follow");
        assert_eq!(runtime.get_follow_target(), "download");
        match rx.try_recv().unwrap() {
            LuaToRustMsg::SetMode(m) => assert_eq!(m, "follow"),
            other => panic!("Unexpected msg: {:?}", other),
        }

        // Verify status bar badge
        let props = crate::ui::status_bar::StatusBarProps {
            mode: "follow",
            uri: "https://example.com",
            hover_uri: None,
            progress: 1.0,
            key_buffer: String::new(),
            tab_index: 0,
            total_tabs: 1,
            scroll_pct: 0.0,
        };
        let status_view = runtime.get_status_view(&props).unwrap();
        let view_str = format!("{:?}", status_view);
        assert!(
            view_str.contains("FOLLOW-DL"),
            "Badge should display FOLLOW-DL: {}",
            view_str
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
