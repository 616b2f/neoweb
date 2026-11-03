pub mod icons;
pub mod servo;

use std::fmt::Debug;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderFrame {
    pub width: u32,
    pub height: u32,
    pub rgba_data: Arc<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStatus {
    Provisional,
    Committed,
    Finished,
    Failed,
}

#[derive(Debug, Clone)]
pub struct HintItem {
    pub key: String,
    pub label: String,
    pub uri: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favicon {
    pub glyph: String,
    pub bytes: Option<Vec<u8>>,
    pub uri: Option<String>,
    pub is_custom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationEntry {
    pub uri: String,
    pub title: String,
    pub favicon: Option<Favicon>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    pub backward: bool,
    pub find_next: bool,
    pub wrap: bool,
    pub case_sensitive: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            backward: false,
            find_next: false,
            wrap: true,
            case_sensitive: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchResult {
    pub query: String,
    pub current_match: usize, // 1-indexed (0 if no matches)
    pub total_matches: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogKind {
    Alert {
        message: String,
    },
    Confirm {
        message: String,
    },
    Prompt {
        message: String,
        default_value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogRequest {
    pub id: u64,
    pub kind: DialogKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResponse {
    Ok(Option<String>),
    Cancel,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    TitleChanged(String),
    UriChanged(String),
    FaviconChanged(Favicon),
    StatusChanged(LoadStatus),
    ProgressChanged(f32),
    HoverUri(Option<String>),
    FormActive(bool),
    ContentUpdated,
    DownloadRequested {
        url: String,
        suggested_filename: Option<String>,
    },
    Dialog(DialogRequest),
    DialogDismissed(u64),
}

pub trait WebEngine {
    fn uri(&self) -> &str;
    fn title(&self) -> &str;
    fn favicon(&self) -> Option<&Favicon> {
        None
    }
    fn status(&self) -> LoadStatus;
    fn progress(&self) -> f32;

    fn load_url(&mut self, url: &str);
    fn reload(&mut self, bypass_cache: bool);
    fn go_back(&mut self);
    fn go_forward(&mut self);
    fn can_go_back(&self) -> bool {
        false
    }
    fn can_go_forward(&self) -> bool {
        false
    }
    fn stop(&mut self);

    fn scroll_by(&mut self, dx: f32, dy: f32);
    fn scroll_to_pct(&mut self, pct: f32);
    fn scroll_y(&self) -> f32;
    fn scroll_pct(&self) -> f32;

    fn zoom_by(&mut self, delta: f32);
    fn zoom_reset(&mut self);
    fn zoom_level(&self) -> f32;

    fn eval_js(&mut self, script: &str) -> Result<String, String>;
    fn generate_hints(&self) -> Vec<HintItem>;
    fn follow_hint(&mut self, key: &str) -> Option<String>;
    fn activate_follow_mode(&mut self) {}
    fn clear_hints(&mut self) {}

    fn search(&mut self, _query: Option<&str>, _opts: SearchOptions) -> Option<SearchResult> {
        None
    }
    fn clear_search(&mut self) {}
    fn active_search(&self) -> Option<&SearchResult> {
        None
    }

    fn active_dialog(&self) -> Option<&DialogRequest> {
        None
    }
    fn handle_dialog_response(&mut self, _id: u64, _response: DialogResponse) {}

    fn resize(&mut self, _width: u32, _height: u32) {}
    fn viewport_size(&self) -> (u32, u32) {
        (1024, 768)
    }
    fn render_frame(&self) -> Option<RenderFrame> {
        None
    }

    fn activate(&mut self) {}
    fn deactivate(&mut self) {}

    fn handle_mouse_move(&mut self, _x: f32, _y: f32) {}
    fn handle_mouse_button(
        &mut self,
        _button: iced::mouse::Button,
        _pressed: bool,
        _x: f32,
        _y: f32,
    ) {
    }
    fn handle_wheel(&mut self, _dx: f32, _dy: f32, _x: f32, _y: f32) {}
    fn handle_key_str(&mut self, _key_str: &str) {}

    fn eval_js_async(
        &mut self,
        script: &str,
        callback: Box<dyn FnOnce(Result<String, String>) + 'static>,
    ) {
        let res = self.eval_js(script);
        callback(res);
    }

    fn poll_events(&mut self) -> Vec<EngineEvent>;
}
