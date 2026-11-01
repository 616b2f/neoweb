use crate::bookmarks::BookmarkStore;
use crate::keymap::format_iced_key;
use crate::lua::LuaRuntime;
use crate::lua::bridge::{LuaSender, LuaToRustMsg};
use crate::storage::Storage;
use crate::tabs::TabManager;
use crate::ui::lua_view::render_lua_node;
use crate::ui::status_bar::StatusBarProps;
use crate::ui::viewport::view_viewport;

use iced::keyboard::Key;
use iced::widget::{Space, column, container};
use iced::{Element, Event, Length, Subscription, Task, Theme, event, time};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::error;

#[derive(Debug, Clone)]
pub enum Message {
    EventOccurred(Event),
    Tick,
    ServoWake,
    WebDriverRequest(crate::engine::servo::WebDriverBridgeMessage),
    NavigateUrl(String),
    ClearNotification,
    LuaUiCallback(usize),
    ViewportBoundsChanged(iced::Rectangle),
    Download(crate::download::DownloadEvent),
}

pub struct NeowebApp {
    pub tabs: TabManager,
    pub storage: Arc<Mutex<Storage>>,
    pub bookmarks: Arc<Mutex<BookmarkStore>>,
    pub lua_runtime: LuaRuntime,
    pub lua_rx: tokio::sync::mpsc::UnboundedReceiver<LuaToRustMsg>,
    pub hover_uri: Option<String>,
    pub tick_count: usize,
    pub cursor_position: iced::Point,
    pub window_width: f32,
    pub viewport_bounds: iced::Rectangle,
}

impl NeowebApp {
    pub fn new() -> (Self, Task<Message>) {
        Self::with_options(None, None)
    }

    pub fn new_with_url(initial_url: Option<&str>) -> (Self, Task<Message>) {
        Self::with_options(None, initial_url)
    }

    pub fn with_config_dir(config_dir: Option<std::path::PathBuf>) -> (Self, Task<Message>) {
        Self::with_options(config_dir, None)
    }

    pub fn with_options(
        config_dir: Option<std::path::PathBuf>,
        initial_url: Option<&str>,
    ) -> (Self, Task<Message>) {
        let (lua_tx, lua_rx) = tokio::sync::mpsc::unbounded_channel();
        let lua_sender = LuaSender::new(lua_tx);

        let storage_path = config_dir.as_ref().map(|d| d.join("neoweb.db"));
        let storage = Arc::new(Mutex::new(
            Storage::new(storage_path).expect("Failed to initialize SQLite storage"),
        ));
        let bm_path = config_dir.as_ref().map(|d| d.join("Bookmarks"));
        let bookmarks = Arc::new(Mutex::new(BookmarkStore::new(bm_path)));

        let lua_runtime = match LuaRuntime::with_config_dir(
            lua_sender,
            bookmarks.clone(),
            storage.clone(),
            config_dir,
        ) {
            Ok(rt) => rt,
            Err(e) => {
                error!("Failed to initialize Lua runtime: {}", e);
                panic!("Lua runtime error: {}", e);
            }
        };

        let start_url = initial_url.unwrap_or("about:neoweb");
        let tabs = TabManager::new(start_url);

        lua_runtime.load_init_script();

        let tab_strip_h = lua_runtime.get_tab_strip_height();
        let bottom_bar_h = lua_runtime.get_bottom_bar_height();
        let initial_viewport_h = (840.0 - tab_strip_h - bottom_bar_h).max(100.0);
        let viewport_bounds = iced::Rectangle::new(
            iced::Point::new(0.0, tab_strip_h),
            iced::Size::new(1280.0, initial_viewport_h),
        );

        let mut app = Self {
            tabs,
            storage,
            bookmarks,
            lua_runtime,
            lua_rx,
            hover_uri: None,
            tick_count: 0,
            cursor_position: iced::Point::ORIGIN,
            window_width: 1280.0,
            viewport_bounds,
        };

        let init_task = app.drain_lua_messages();

        (app, init_task)
    }

    pub fn title(&self) -> String {
        self.tabs
            .tabs()
            .get(self.tabs.active_index())
            .map(|t| format!("Neoweb - {}", t.title()))
            .unwrap_or_else(|| "Neoweb".to_string())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.tick_count = self.tick_count.wrapping_add(1);
                self.poll_active_engine()
            }

            Message::ServoWake => {
                self.poll_active_engine()
            }

            Message::EventOccurred(event) => {
                match event {
                    Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key,
                        modifiers,
                        text,
                        ..
                    }) => {
                        return self.handle_key_event(key, modifiers, text.as_deref());
                    }
                    Event::Window(iced::window::Event::Resized(size)) => {
                        self.window_width = size.width;
                    }
                    Event::Window(iced::window::Event::CloseRequested) => {
                        // 1. Synchronously trigger Lua window "close" signal (triggers session saving & callbacks)
                        self.lua_runtime.emit_signal("close", "");
                        // 2. Tear down WebViews and tabs cleanly
                        self.tabs.close_all();
                        // 3. Gracefully shutdown Servo constellation and background thread pools
                        crate::engine::servo::shutdown_servo();
                        // 4. Terminate Iced runtime loop
                        return iced::exit();
                    }
                    Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                        self.cursor_position = position;
                        let webview_x = position.x - self.viewport_bounds.x;
                        let webview_y = position.y - self.viewport_bounds.y;
                        self.tabs
                            .active_engine_mut()
                            .handle_mouse_move(webview_x, webview_y);
                    }
                    Event::Mouse(iced::mouse::Event::ButtonPressed(button))
                        if self.viewport_bounds.contains(self.cursor_position) =>
                    {
                        let webview_x = self.cursor_position.x - self.viewport_bounds.x;
                        let webview_y = self.cursor_position.y - self.viewport_bounds.y;
                        self.tabs
                            .active_engine_mut()
                            .handle_mouse_button(button, true, webview_x, webview_y);
                    }
                    Event::Mouse(iced::mouse::Event::ButtonReleased(button)) => {
                        let webview_x = self.cursor_position.x - self.viewport_bounds.x;
                        let webview_y = self.cursor_position.y - self.viewport_bounds.y;
                        self.tabs
                            .active_engine_mut()
                            .handle_mouse_button(button, false, webview_x, webview_y);
                    }
                    Event::Mouse(iced::mouse::Event::WheelScrolled { delta })
                        if self.viewport_bounds.contains(self.cursor_position) =>
                    {
                        let (dx, dy) = match delta {
                            iced::mouse::ScrollDelta::Lines { x, y } => (x * 40.0, y * 40.0),
                            iced::mouse::ScrollDelta::Pixels { x, y } => (x, y),
                        };
                        let webview_x = self.cursor_position.x - self.viewport_bounds.x;
                        let webview_y = self.cursor_position.y - self.viewport_bounds.y;
                        self.tabs
                            .active_engine_mut()
                            .handle_wheel(dx, dy, webview_x, webview_y);
                    }
                    _ => {}
                }
                Task::none()
            }

            Message::ViewportBoundsChanged(bounds) => {
                self.viewport_bounds = bounds;
                let w = bounds.width.round().max(100.0) as u32;
                let h = bounds.height.round().max(100.0) as u32;
                let (cur_w, cur_h) = self.tabs.active_engine().viewport_size();
                if cur_w != w || cur_h != h {
                    self.tabs.resize_all(w, h);
                }
                Task::none()
            }

            Message::NavigateUrl(url) => {
                self.tabs.active_engine_mut().load_url(&url);
                Task::none()
            }

            Message::ClearNotification => {
                self.lua_runtime.clear_notification();
                Task::none()
            }

            Message::LuaUiCallback(cb_id) => {
                self.lua_runtime.invoke_ui_callback(cb_id);
                self.drain_lua_messages()
            }

            Message::WebDriverRequest(bridge_msg) => {
                let pair_opt = bridge_msg.0.lock().ok().and_then(|mut g| g.take());
                if let Some((req, reply_tx)) = pair_opt {
                    let res = self.handle_webdriver_request(req);
                    let _ = reply_tx.send(res);
                }
                Task::none()
            }

            Message::Download(ev) => {
                match ev {
                    crate::download::DownloadEvent::Started { filename, .. } => {
                        self.lua_runtime
                            .notify("info", &format!("Download started: {filename}"));
                        self.lua_runtime.emit_signal("download::start", &filename);
                    }
                    crate::download::DownloadEvent::Progress { .. } => {}
                    crate::download::DownloadEvent::Finished { filename, size, .. } => {
                        let size_str = crate::download::format_bytes(size);
                        self.lua_runtime.notify(
                            "info",
                            &format!("Download complete: {filename} ({size_str})"),
                        );
                        self.lua_runtime.emit_signal("download::finish", &filename);
                    }
                    crate::download::DownloadEvent::Failed { filename, error, .. } => {
                        self.lua_runtime
                            .notify("error", &format!("Download failed: {filename}: {error}"));
                        self.lua_runtime.emit_signal("download::error", &filename);
                    }
                }
                Task::none()
            }
        }
    }

    fn drain_lua_messages(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        while let Ok(msg) = self.lua_rx.try_recv() {
            tasks.push(self.handle_lua_msg(msg));
        }
        Task::batch(tasks)
    }

    fn poll_active_engine(&mut self) -> Task<Message> {
        let lua_task = self.drain_lua_messages();
        if self.tabs.is_empty() {
            return lua_task;
        }

        let engine = self.tabs.active_engine_mut();
        let events = engine.poll_events();
        let uri = engine.uri();
        let title = engine.title();
        let progress = engine.progress();

        for ev in events {
            match ev {
                crate::engine::EngineEvent::TitleChanged(t) => {
                    self.storage.lock().unwrap().record_history(uri, &t).ok();
                    self.lua_runtime.emit_signal("property::title", &t);
                }
                crate::engine::EngineEvent::UriChanged(u) => {
                    self.lua_runtime.emit_signal("property::uri", &u);
                }
                crate::engine::EngineEvent::HoverUri(h) => {
                    self.hover_uri = h;
                }
                crate::engine::EngineEvent::StatusChanged(_) => {}
                crate::engine::EngineEvent::ProgressChanged(_) => {}
                crate::engine::EngineEvent::FormActive(active) => {
                    let cur_mode = self.lua_runtime.get_mode();
                    if active && cur_mode == "normal" {
                        self.lua_runtime.set_mode("insert");
                    } else if !active && cur_mode == "insert" {
                        self.lua_runtime.set_mode("normal");
                    }
                }
                crate::engine::EngineEvent::ContentUpdated => {}
                crate::engine::EngineEvent::FaviconChanged(fav) => {
                    self.lua_runtime.emit_signal("property::favicon", &fav.glyph);
                }
                crate::engine::EngineEvent::DownloadRequested {
                    url,
                    suggested_filename,
                } => {
                    crate::download::start_download(url, suggested_filename.map(Into::into));
                }
            }
        }

        self.lua_runtime.sync_state(uri, title, progress);
        lua_task
    }

    fn handle_key_event(
        &mut self,
        key: Key,
        modifiers: iced::keyboard::Modifiers,
        text: Option<&str>,
    ) -> Task<Message> {
        if let Some(key_str) = format_iced_key(&key, &modifiers, text) {
            let _handled = self.lua_runtime.handle_key(&key_str);
            return self.drain_lua_messages();
        }
        Task::none()
    }

    fn handle_lua_msg(&mut self, msg: LuaToRustMsg) -> Task<Message> {
        match msg {
            LuaToRustMsg::Navigate(url) => {
                self.tabs.active_engine_mut().load_url(&url);
                Task::none()
            }
            LuaToRustMsg::NewTab(url) => {
                self.tabs.new_tab(&url);
                Task::none()
            }
            LuaToRustMsg::CloseTab(idx) => {
                if let Some(i) = idx {
                    self.tabs.close_tab(i);
                } else {
                    self.tabs.close_active_tab();
                }
                Task::none()
            }
            LuaToRustMsg::NextTab => {
                self.tabs.next_tab();
                Task::none()
            }
            LuaToRustMsg::PrevTab => {
                self.tabs.prev_tab();
                Task::none()
            }
            LuaToRustMsg::GotoTab(idx) => {
                self.tabs.goto_tab(idx);
                Task::none()
            }
            LuaToRustMsg::Reload(bypass) => {
                self.tabs.active_engine_mut().reload(bypass);
                Task::none()
            }
            LuaToRustMsg::Back => {
                self.tabs.active_engine_mut().go_back();
                Task::none()
            }
            LuaToRustMsg::Forward => {
                self.tabs.active_engine_mut().go_forward();
                Task::none()
            }
            LuaToRustMsg::Scroll(dx, dy) => {
                self.tabs.active_engine_mut().scroll_by(dx, dy);
                iced::widget::operation::scroll_by(
                    iced::widget::Id::new(crate::ui::viewport::VIEWPORT_SCROLL_ID),
                    iced::widget::scrollable::AbsoluteOffset { x: dx, y: dy },
                )
            }
            LuaToRustMsg::ScrollPct(pct) => {
                self.tabs.active_engine_mut().scroll_to_pct(pct);
                let rel_y = (pct / 100.0).clamp(0.0, 1.0);
                if rel_y >= 0.999 {
                    iced::widget::operation::snap_to_end(iced::widget::Id::new(
                        crate::ui::viewport::VIEWPORT_SCROLL_ID,
                    ))
                } else {
                    iced::widget::operation::snap_to(
                        iced::widget::Id::new(crate::ui::viewport::VIEWPORT_SCROLL_ID),
                        iced::widget::scrollable::RelativeOffset {
                            x: None,
                            y: Some(rel_y),
                        },
                    )
                }
            }
            LuaToRustMsg::Zoom(delta) => {
                self.tabs.active_engine_mut().zoom_by(delta);
                Task::none()
            }
            LuaToRustMsg::ZoomReset => {
                self.tabs.active_engine_mut().zoom_reset();
                Task::none()
            }
            LuaToRustMsg::SetMode(m) => {
                if m == "follow" {
                    self.tabs.active_engine_mut().activate_follow_mode();
                } else {
                    self.tabs.active_engine_mut().clear_hints();
                }
                Task::none()
            }
            LuaToRustMsg::Notify { level, message } => {
                self.lua_runtime.notify(&level, &message);
                Task::none()
            }
            LuaToRustMsg::Quit => {
                // Lua has already executed its shutdown and session saving hooks.
                // Now clean up Rust engine resources and terminate the event loop.
                self.tabs.close_all();
                crate::engine::servo::shutdown_servo();
                iced::exit()
            }
            LuaToRustMsg::ExecuteJs(script) => {
                let _ = self.tabs.active_engine_mut().eval_js(&script);
                Task::none()
            }
            LuaToRustMsg::SetClipboard(val) => {
                if let Ok(mut clip) = arboard::Clipboard::new() {
                    let _ = clip.set_text(val);
                }
                Task::none()
            }
            LuaToRustMsg::Search { query, backward } => {
                let result = self.tabs.active_engine_mut().search(&query, backward);
                if let Some(res) = result {
                    if res.total_matches == 0 {
                        self.lua_runtime.notify(
                            "warning",
                            &format!("Pattern not found: {}", res.query),
                        );
                    } else {
                        self.lua_runtime.notify(
                            "info",
                            &format!(
                                "[{}/{}] Match: {}",
                                res.current_match, res.total_matches, res.query
                            ),
                        );
                        let scroll_y = self.tabs.active_engine().scroll_y();
                        return iced::widget::operation::snap_to(
                            iced::widget::Id::new(crate::ui::viewport::VIEWPORT_SCROLL_ID),
                            iced::widget::scrollable::RelativeOffset {
                                x: None,
                                y: Some((scroll_y / 1000.0).clamp(0.0, 1.0)),
                            },
                        );
                    }
                }
                Task::none()
            }
            LuaToRustMsg::SearchNext { backward } => {
                let result = self.tabs.active_engine_mut().search_next(backward);
                if let Some(res) = result {
                    if res.total_matches == 0 {
                        self.lua_runtime.notify(
                            "warning",
                            &format!("Pattern not found: {}", res.query),
                        );
                    } else {
                        self.lua_runtime.notify(
                            "info",
                            &format!(
                                "[{}/{}] Match: {}",
                                res.current_match, res.total_matches, res.query
                            ),
                        );
                        let scroll_y = self.tabs.active_engine().scroll_y();
                        return iced::widget::operation::snap_to(
                            iced::widget::Id::new(crate::ui::viewport::VIEWPORT_SCROLL_ID),
                            iced::widget::scrollable::RelativeOffset {
                                x: None,
                                y: Some((scroll_y / 1000.0).clamp(0.0, 1.0)),
                            },
                        );
                    }
                }
                Task::none()
            }
            LuaToRustMsg::ClearSearch => {
                self.tabs.active_engine_mut().clear_search();
                Task::none()
            }
            LuaToRustMsg::SendKey(key_str) => {
                self.tabs.active_engine_mut().handle_key_str(&key_str);
                self.poll_active_engine()
            }
            LuaToRustMsg::FollowHint(query) => {
                if query.is_empty() {
                    return Task::none();
                }
                if let Some(target_url) =
                    self.tabs.active_engine_mut().follow_hint(&query)
                {
                    let follow_target = self.lua_runtime.get_follow_target();
                    self.lua_runtime.set_mode("normal");
                    self.tabs.active_engine_mut().clear_hints();

                    if !target_url.is_empty() && !target_url.starts_with("javascript:") {
                        if follow_target == "tab" {
                            self.tabs.new_tab(&target_url);
                        } else if follow_target == "download" {
                            crate::download::start_download(target_url, None);
                        } else {
                            self.tabs.active_engine_mut().load_url(&target_url);
                        }
                    }
                } else {
                    let hints = self.tabs.active_engine_mut().generate_hints();
                    if !hints.is_empty()
                        && !hints
                            .iter()
                            .any(|h| h.key.to_lowercase().starts_with(&query.to_lowercase()))
                    {
                        self.lua_runtime.pop_follow_query();
                    }
                }
                Task::none()
            }
            LuaToRustMsg::Download { url, path } => {
                crate::download::start_download(url, path.map(Into::into));
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.tabs.is_empty() {
            return container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        let active_idx = self.tabs.active_index();
        let total_tabs = self.tabs.len();
        let active_tab = self.tabs.active_tab();
        let engine = active_tab.engine.as_ref();

        // 1. Tab strip widget (Lua-driven)
        let tab_strip = self
            .lua_runtime
            .get_tabs_view(active_idx, self.tabs.tabs(), self.tick_count, Some(self.window_width))
            .map(|node| render_lua_node(&node, Message::LuaUiCallback))
            .unwrap_or_else(|| Space::new().into());

        // 2. Viewport
        let cur_mode = self.lua_runtime.get_mode();
        let is_follow_mode = cur_mode == "follow";
        let follow_query = if is_follow_mode {
            self.lua_runtime.get_follow_query()
        } else {
            String::new()
        };

        let hints = engine.generate_hints();
        let viewport = view_viewport(
            engine,
            is_follow_mode,
            hints,
            &follow_query,
            Some(Message::ViewportBoundsChanged),
        );

        // 3. Status bar (Lua-driven)
        let key_disp = self.lua_runtime.get_key_buffer();
        let status_props = StatusBarProps {
            mode: &cur_mode,
            uri: engine.uri(),
            hover_uri: self.hover_uri.as_deref(),
            progress: engine.progress(),
            key_buffer: key_disp,
            tab_index: active_idx,
            total_tabs,
            scroll_pct: engine.scroll_pct(),
        };
        let status_bar = self
            .lua_runtime
            .get_status_view(&status_props)
            .map(|node| render_lua_node(&node, Message::LuaUiCallback))
            .unwrap_or_else(|| Space::new().into());

        // 4. Command prompt bar (overlaps status bar when active, like Neovim)
        let bottom_bar: Element<'_, Message> =
            if let Some(prompt_node) = self.lua_runtime.get_prompt_view() {
                render_lua_node(&prompt_node, Message::LuaUiCallback)
            } else {
                status_bar
            };

        // 5. Notifications: positioned above status/bottom bar, shown only when active
        let notification_view = self.lua_runtime.get_notification_view();

        let mut layout = column![tab_strip, viewport];

        if let Some(notif_node) = notification_view {
            layout = layout.push(render_lua_node(&notif_node, Message::LuaUiCallback));
        }

        layout = layout.push(bottom_bar);

        container(layout.width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            event::listen().map(Message::EventOccurred),
            time::every(Duration::from_millis(50)).map(|_| Message::Tick),
            Subscription::run(servo_waker_stream),
            Subscription::run(webdriver_request_stream),
            Subscription::run(crate::download::download_event_stream),
        ])
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn handle_webdriver_request(
        &mut self,
        req: crate::engine::servo::WebDriverAppRequest,
    ) -> Result<serde_json::Value, crate::engine::servo::WebDriverErrorResponse> {
        use crate::engine::servo::{WebDriverAppRequest, WebDriverErrorResponse};
        use base64::Engine;
        use serde_json::{Value, json};

        match req {
            WebDriverAppRequest::GetUrl => {
                let _ = self.poll_active_engine();
                Ok(json!(self.tabs.active_engine().uri()))
            }
            WebDriverAppRequest::Navigate(url) => {
                self.tabs.active_engine_mut().load_url(&url);
                for _ in 0..40 {
                    let _ = self.poll_active_engine();
                    if self.tabs.active_engine().status() == crate::engine::LoadStatus::Finished {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Ok(Value::Null)
            }
            WebDriverAppRequest::GetTitle => {
                let _ = self.poll_active_engine();
                Ok(json!(self.tabs.active_engine().title()))
            }
            WebDriverAppRequest::Back => {
                self.tabs.active_engine_mut().go_back();
                let _ = self.poll_active_engine();
                Ok(Value::Null)
            }
            WebDriverAppRequest::Forward => {
                self.tabs.active_engine_mut().go_forward();
                let _ = self.poll_active_engine();
                Ok(Value::Null)
            }
            WebDriverAppRequest::Refresh => {
                self.tabs.active_engine_mut().reload(false);
                let _ = self.poll_active_engine();
                Ok(Value::Null)
            }
            WebDriverAppRequest::GetSource => {
                let _ = self.poll_active_engine();
                let uri = self.tabs.active_engine().uri().to_string();
                let title = self.tabs.active_engine().title().to_string();
                if uri.starts_with("about:") {
                    self.tabs.active_engine_mut().activate_follow_mode();
                    let hints = self.tabs.active_engine().generate_hints();
                    let mut links_html = String::new();
                    for h in hints {
                        links_html.push_str(&format!(
                            "<a href=\"{}\" data-hint=\"{}\">{}</a>",
                            h.uri, h.key, h.label
                        ));
                    }
                    Ok(json!(format!(
                        "<!DOCTYPE html><html><head><title>{}</title></head><body><h1>{}</h1>{}</body></html>",
                        title, title, links_html
                    )))
                } else {
                    let src = self
                        .tabs
                        .active_engine_mut()
                        .eval_js("document.documentElement.outerHTML")
                        .unwrap_or_else(|_| {
                            format!("<html><head><title>{}</title></head><body></body></html>", title)
                        });
                    Ok(json!(src))
                }
            }
            WebDriverAppRequest::ExecuteScript { script, args: _ } => {
                let _ = self.poll_active_engine();
                let trimmed = script.trim();
                let expr = trimmed
                    .strip_prefix("return ")
                    .unwrap_or(trimmed)
                    .trim_end_matches(';');
                let res = self
                    .tabs
                    .active_engine_mut()
                    .eval_js(expr)
                    .map_err(WebDriverErrorResponse::unknown_error)?;
                if let Ok(parsed) = serde_json::from_str::<Value>(&res) {
                    Ok(parsed)
                } else {
                    Ok(json!(res))
                }
            }
            WebDriverAppRequest::GetWindowHandle => {
                Ok(json!(format!("tab-{}", self.tabs.active_index())))
            }
            WebDriverAppRequest::GetWindowHandles => {
                let handles: Vec<String> =
                    (0..self.tabs.len()).map(|i| format!("tab-{}", i)).collect();
                Ok(json!(handles))
            }
            WebDriverAppRequest::NewWindow { window_type } => {
                self.tabs.new_tab("about:blank");
                let _ = self.poll_active_engine();
                let handle = format!("tab-{}", self.tabs.active_index());
                Ok(json!({
                    "handle": handle,
                    "type": window_type
                }))
            }
            WebDriverAppRequest::CloseWindow => {
                self.tabs.close_active_tab();
                let _ = self.poll_active_engine();
                let handles: Vec<String> =
                    (0..self.tabs.len()).map(|i| format!("tab-{}", i)).collect();
                Ok(json!(handles))
            }
            WebDriverAppRequest::SwitchToWindow(handle) => {
                let idx_str = handle.strip_prefix("tab-").unwrap_or(&handle);
                let idx = idx_str.parse::<usize>().map_err(|_| {
                    WebDriverErrorResponse::no_such_window(format!(
                        "Invalid window handle: {}",
                        handle
                    ))
                })?;
                if idx >= self.tabs.len() {
                    return Err(WebDriverErrorResponse::no_such_window(format!(
                        "Window handle out of range: {}",
                        handle
                    )));
                }
                self.tabs.goto_tab(idx);
                let _ = self.poll_active_engine();
                Ok(Value::Null)
            }
            WebDriverAppRequest::GetWindowRect => {
                let (w, h) = self.tabs.active_engine().viewport_size();
                Ok(json!({
                    "x": 0,
                    "y": 0,
                    "width": w,
                    "height": h
                }))
            }
            WebDriverAppRequest::SetWindowRect { width, height } => {
                let (cur_w, cur_h) = self.tabs.active_engine().viewport_size();
                let new_w = width.unwrap_or(cur_w).max(100);
                let new_h = height.unwrap_or(cur_h).max(100);
                self.tabs.resize_all(new_w, new_h);
                let _ = self.poll_active_engine();
                Ok(json!({
                    "x": 0,
                    "y": 0,
                    "width": new_w,
                    "height": new_h
                }))
            }
            WebDriverAppRequest::TakeScreenshot => {
                let _ = self.poll_active_engine();
                let frame = self.tabs.active_engine().render_frame().ok_or_else(|| {
                    WebDriverErrorResponse::unknown_error("No rendered frame available")
                })?;
                let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                    frame.width,
                    frame.height,
                    (*frame.rgba_data).clone(),
                )
                .ok_or_else(|| {
                    WebDriverErrorResponse::unknown_error("Invalid RGBA frame dimensions")
                })?;
                let mut png_bytes = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut png_bytes);
                img.write_to(&mut cursor, image::ImageFormat::Png)
                    .map_err(|e| WebDriverErrorResponse::unknown_error(e.to_string()))?;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                Ok(json!(encoded))
            }
            WebDriverAppRequest::FindElement { using, value } => {
                let elements = self.find_webdriver_elements(&using, &value)?;
                elements.into_iter().next().ok_or_else(|| {
                    WebDriverErrorResponse::no_such_element(format!(
                        "Unable to locate element: {}={}",
                        using, value
                    ))
                })
            }
            WebDriverAppRequest::FindElements { using, value } => {
                let elements = self.find_webdriver_elements(&using, &value)?;
                Ok(json!(elements))
            }
            WebDriverAppRequest::ElementClick(element_id) => {
                let key = element_id.strip_prefix("hint-").unwrap_or(&element_id);
                self.tabs.active_engine_mut().activate_follow_mode();
                let _ = self.poll_active_engine();
                if let Some(target_url) = self.tabs.active_engine_mut().follow_hint(key) {
                    self.tabs.active_engine_mut().clear_hints();
                    if !target_url.is_empty() && !target_url.starts_with("javascript:") {
                        self.tabs.active_engine_mut().load_url(&target_url);
                        let _ = self.poll_active_engine();
                    }
                    Ok(Value::Null)
                } else {
                    Err(WebDriverErrorResponse::no_such_element(format!(
                        "Element '{}' not found",
                        element_id
                    )))
                }
            }
            WebDriverAppRequest::ElementText(element_id) => {
                let key = element_id.strip_prefix("hint-").unwrap_or(&element_id);
                self.tabs.active_engine_mut().activate_follow_mode();
                let _ = self.poll_active_engine();
                let hints = self.tabs.active_engine().generate_hints();
                if let Some(item) = hints.into_iter().find(|h| h.key == key) {
                    Ok(json!(item.label))
                } else {
                    Err(WebDriverErrorResponse::no_such_element(format!(
                        "Element '{}' not found",
                        element_id
                    )))
                }
            }
            WebDriverAppRequest::SendKey(key) => {
                let _ = self.lua_runtime.handle_key(&key);
                let _ = self.drain_lua_messages();
                let _ = self.poll_active_engine();
                Ok(json!({
                    "mode": self.lua_runtime.get_mode(),
                    "uri": self.tabs.active_engine().uri(),
                    "title": self.tabs.active_engine().title(),
                    "tabs": self.tabs.len()
                }))
            }
            WebDriverAppRequest::GetMode => Ok(json!(self.lua_runtime.get_mode())),
            WebDriverAppRequest::GetBrowsingContextTree { root } => {
                let _ = self.poll_active_engine();
                let mut contexts = Vec::new();
                for (idx, tab) in self.tabs.tabs().iter().enumerate() {
                    let context_id = format!("tab-{}", idx);
                    if root.as_ref().is_none_or(|r| r == &context_id) {
                        contexts.push(json!({
                            "context": context_id,
                            "url": tab.uri(),
                            "title": tab.title(),
                            "userContext": "default",
                            "children": [],
                            "parent": null,
                            "clientWindow": "window-1",
                            "originalOpener": null
                        }));
                    }
                }
                Ok(json!({ "contexts": contexts }))
            }
            WebDriverAppRequest::LocateNodes {
                context,
                locator_type,
                value,
            } => {
                if let Some(ctx) = context {
                    let idx_str = ctx.strip_prefix("tab-").unwrap_or(&ctx);
                    let idx = idx_str.parse::<usize>().map_err(|_| {
                        WebDriverErrorResponse::no_such_frame(format!(
                            "Invalid browsing context: {}",
                            ctx
                        ))
                    })?;
                    if idx >= self.tabs.len() {
                        return Err(WebDriverErrorResponse::no_such_frame(format!(
                            "Browsing context out of range: {}",
                            ctx
                        )));
                    }
                    self.tabs.goto_tab(idx);
                }
                self.tabs.active_engine_mut().activate_follow_mode();
                let _ = self.poll_active_engine();
                let hints = self.tabs.active_engine().generate_hints();
                let nodes: Vec<Value> = hints
                    .into_iter()
                    .filter(|h| match locator_type.as_str() {
                        "css" | "css selector" => {
                            value == "a"
                                || value == "*"
                                || h.label.contains(&value)
                                || h.uri.contains(&value)
                        }
                        "innerText" | "partial link text" | "accessibility" => {
                            h.label.contains(&value) || h.uri.contains(&value)
                        }
                        "link text" => h.label == value || h.uri == value,
                        _ => true,
                    })
                    .map(|h| {
                        json!({
                            "type": "node",
                            "sharedId": format!("hint-{}", h.key),
                            "value": {
                                "nodeType": 1,
                                "localName": "a",
                                "namespaceURI": "http://www.w3.org/1999/xhtml",
                                "childNodeCount": 1,
                                "attributes": {
                                    "href": h.uri,
                                    "data-hint": h.key,
                                    "aria-label": h.label
                                }
                            }
                        })
                    })
                    .collect();
                Ok(json!({ "nodes": nodes }))
            }
            WebDriverAppRequest::PerformActions { actions } => {
                for seq in actions {
                    let seq_type = seq.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let Some(steps) = seq.get("actions").and_then(|v| v.as_array()) else {
                        continue;
                    };
                    match seq_type {
                        "key" => {
                            for step in steps {
                                if step.get("type").and_then(|v| v.as_str()) == Some("keyDown")
                                    && let Some(raw_val) =
                                        step.get("value").and_then(|v| v.as_str())
                                {
                                    let mapped_key = match raw_val {
                                        "\u{E00C}" => "<Esc>",
                                        "\u{E007}" | "\u{E006}" => "<CR>",
                                        "\u{E003}" => "<BS>",
                                        "\u{E004}" => "<Tab>",
                                        "\u{E00D}" => "<Space>",
                                        other => other,
                                    };
                                    let _ = self.lua_runtime.handle_key(mapped_key);
                                    let _ = self.drain_lua_messages();
                                }
                            }
                        }
                        "pointer" => {
                            let mut cur_x = self.cursor_position.x;
                            let mut cur_y = self.cursor_position.y;
                            let mut shared_hint: Option<String> = None;
                            for step in steps {
                                match step.get("type").and_then(|v| v.as_str()) {
                                    Some("pointerMove") => {
                                        if let Some(x) = step.get("x").and_then(|v| v.as_f64()) {
                                            cur_x = x as f32;
                                        }
                                        if let Some(y) = step.get("y").and_then(|v| v.as_f64()) {
                                            cur_y = y as f32;
                                        }
                                        if let Some(id) = step
                                            .get("origin")
                                            .and_then(|o| o.get("element"))
                                            .and_then(|e| e.get("sharedId"))
                                            .and_then(|s| s.as_str())
                                        {
                                            shared_hint = Some(id.to_string());
                                        }
                                        self.tabs
                                            .active_engine_mut()
                                            .handle_mouse_move(cur_x, cur_y);
                                    }
                                    Some("pointerDown") => {
                                        if let Some(ref hint_id) = shared_hint {
                                            let key =
                                                hint_id.strip_prefix("hint-").unwrap_or(hint_id);
                                            self.tabs.active_engine_mut().activate_follow_mode();
                                            if let Some(target_url) =
                                                self.tabs.active_engine_mut().follow_hint(key)
                                            {
                                                self.tabs.active_engine_mut().clear_hints();
                                                if !target_url.is_empty()
                                                    && !target_url.starts_with("javascript:")
                                                {
                                                    self.tabs
                                                        .active_engine_mut()
                                                        .load_url(&target_url);
                                                }
                                            }
                                        } else {
                                            self.tabs.active_engine_mut().handle_mouse_button(
                                                iced::mouse::Button::Left,
                                                true,
                                                cur_x,
                                                cur_y,
                                            );
                                        }
                                    }
                                    Some("pointerUp") => {
                                        self.tabs.active_engine_mut().handle_mouse_button(
                                            iced::mouse::Button::Left,
                                            false,
                                            cur_x,
                                            cur_y,
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "wheel" => {
                            for step in steps {
                                if step.get("type").and_then(|v| v.as_str()) == Some("scroll") {
                                    let dx = step
                                        .get("deltaX")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0)
                                        as f32;
                                    let dy = step
                                        .get("deltaY")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0)
                                        as f32;
                                    self.tabs.active_engine_mut().scroll_by(dx, dy);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let _ = self.poll_active_engine();
                Ok(json!({}))
            }
        }
    }

    fn find_webdriver_elements(
        &mut self,
        using: &str,
        value: &str,
    ) -> Result<Vec<serde_json::Value>, crate::engine::servo::WebDriverErrorResponse> {
        use crate::engine::servo::{ELEMENT_KEY, WebDriverErrorResponse};
        use serde_json::json;

        self.tabs.active_engine_mut().activate_follow_mode();
        let _ = self.poll_active_engine();
        let hints = self.tabs.active_engine().generate_hints();

        let matched: Vec<serde_json::Value> = match using {
            "css selector" | "tag name" => hints
                .into_iter()
                .filter(|h| {
                    value == "a"
                        || value == "*"
                        || h.label.contains(value)
                        || h.uri.contains(value)
                })
                .map(|h| json!({ ELEMENT_KEY: format!("hint-{}", h.key) }))
                .collect(),
            "link text" => hints
                .into_iter()
                .filter(|h| h.label == value || h.uri == value)
                .map(|h| json!({ ELEMENT_KEY: format!("hint-{}", h.key) }))
                .collect(),
            "partial link text" => hints
                .into_iter()
                .filter(|h| h.label.contains(value) || h.uri.contains(value))
                .map(|h| json!({ ELEMENT_KEY: format!("hint-{}", h.key) }))
                .collect(),
            _ => {
                return Err(WebDriverErrorResponse::invalid_argument(format!(
                    "Unsupported locator strategy: {}",
                    using
                )));
            }
        };

        Ok(matched)
    }
}

impl Drop for NeowebApp {
    fn drop(&mut self) {
        self.tabs.close_all();
        crate::engine::servo::shutdown_servo();
    }
}

fn servo_waker_stream() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::SinkExt;
    iced::stream::channel(
        100,
        |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            crate::engine::servo::register_servo_waker(tx);
            while let Some(()) = rx.recv().await {
                let _ = output.send(Message::ServoWake).await;
            }
        },
    )
}

fn webdriver_request_stream() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::SinkExt;
    iced::stream::channel(
        100,
        |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            crate::engine::servo::register_webdriver_bridge(tx);
            while let Some(req) = rx.recv().await {
                let _ = output.send(Message::WebDriverRequest(req)).await;
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_operations() {
        // Verify snap_to_end and relative snap operations create valid iced Tasks
        let _task_end: Task<Message> =
            iced::widget::operation::snap_to_end(iced::widget::Id::new("test"));
        let _task_rel: Task<Message> = iced::widget::operation::snap_to(
            iced::widget::Id::new("test"),
            iced::widget::scrollable::RelativeOffset {
                x: None,
                y: Some(1.0),
            },
        );
    }

    #[test]
    fn test_view_layout_neovim_overlapping_prompt_and_notifications() {
        let temp_dir = std::env::temp_dir().join(format!("neoweb_test_app_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let (app, _task) = NeowebApp::with_config_dir(Some(temp_dir.clone()));
        // 1. By default, notification is None
        assert!(app.lua_runtime.get_notification_view().is_none());

        // 2. In normal mode with empty prompt, view renders cleanly
        let _ = app.view();

        // 3. Set a notification and verify view renders with notification above bottom bar
        app.lua_runtime.notify("info", "Bookmarked: Neoweb");
        assert!(app.lua_runtime.get_notification_view().is_some());
        let _ = app.view();

        // 4. In command mode, prompt overlaps status bar
        app.lua_runtime.set_mode("command");
        let _ = app.view();

        // 5. In search mode, prompt overlaps status bar
        app.lua_runtime.set_mode("search");
        let _ = app.view();

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_app_graceful_shutdown_and_session_save() {
        let temp_dir = std::env::temp_dir().join(format!("neoweb_test_shutdown_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let (mut app, _task) = NeowebApp::with_config_dir(Some(temp_dir.clone()));

        assert_eq!(app.tabs.len(), 1);
        assert!(!app.tabs.is_empty());

        // Process CloseRequested event
        let _close_task = app.update(Message::EventOccurred(Event::Window(
            iced::window::Event::CloseRequested,
        )));

        // Tabs should be cleanly closed and emptied
        assert!(app.tabs.is_empty());
        assert_eq!(app.title(), "Neoweb");

        // View should render safely without panic even when tabs are empty
        let _ = app.view();

        // Session file should have been written by Lua "close" signal
        let session_file = temp_dir.join("session.json");
        assert!(session_file.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_viewport_bounds_and_mouse_event_dispatch() {
        let temp_dir = std::env::temp_dir()
            .join(format!("neoweb_test_mouse_bounds_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let (mut app, _task) = NeowebApp::with_config_dir(Some(temp_dir.clone()));

        // 1. Initial viewport bounds has y == tab_strip_height
        assert_eq!(app.viewport_bounds.x, 0.0);
        assert_eq!(app.viewport_bounds.y, 28.0);
        assert_eq!(app.viewport_bounds.width, 1280.0);
        assert_eq!(app.viewport_bounds.height, 784.0);

        // 2. ViewportBoundsChanged updates bounds and viewport size
        let custom_bounds = iced::Rectangle::new(
            iced::Point::new(10.0, 35.0),
            iced::Size::new(1000.0, 600.0),
        );
        let _ = app.update(Message::ViewportBoundsChanged(custom_bounds));
        assert_eq!(app.viewport_bounds, custom_bounds);
        let (vw, vh) = app.tabs.active_engine().viewport_size();
        assert_eq!(vw, 1000);
        assert_eq!(vh, 600);

        // 3. CursorMoved updates cursor_position
        let _ = app.update(Message::EventOccurred(Event::Mouse(
            iced::mouse::Event::CursorMoved {
                position: iced::Point::new(110.0, 135.0),
            },
        )));
        assert_eq!(app.cursor_position, iced::Point::new(110.0, 135.0));

        // 4. ButtonPressed inside viewport_bounds is processed
        let _ = app.update(Message::EventOccurred(Event::Mouse(
            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
        )));

        // 5. ButtonPressed outside viewport_bounds (in tab strip, y = 15.0) is rejected
        let _ = app.update(Message::EventOccurred(Event::Mouse(
            iced::mouse::Event::CursorMoved {
                position: iced::Point::new(100.0, 15.0),
            },
        )));
        assert_eq!(app.cursor_position, iced::Point::new(100.0, 15.0));
        assert!(!app.viewport_bounds.contains(app.cursor_position));

        // ButtonPressed when cursor is at tab strip does not panic and is ignored
        let _ = app.update(Message::EventOccurred(Event::Mouse(
            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
        )));

        // 6. WheelScrolled outside viewport_bounds is ignored
        let _ = app.update(Message::EventOccurred(Event::Mouse(
            iced::mouse::Event::WheelScrolled {
                delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
            },
        )));

        // 7. ButtonReleased is always delivered
        let _ = app.update(Message::EventOccurred(Event::Mouse(
            iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
        )));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
