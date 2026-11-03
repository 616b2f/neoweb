use crate::engine::SearchOptions;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub enum LuaToRustMsg {
    Navigate(String),
    NewTab(String),
    CloseTab(Option<usize>),
    NextTab,
    PrevTab,
    GotoTab(usize),
    Reload(bool),
    Back,
    Forward,
    Scroll(f32, f32),
    ScrollPct(f32),
    Zoom(f32),
    ZoomReset,
    SetMode(String),
    Notify {
        level: String,
        message: String,
    },
    Quit,
    ExecuteJs(String),
    SetClipboard(String),
    Search {
        query: Option<String>,
        opts: SearchOptions,
    },
    ClearSearch,
    SendKey(String),
    FollowHint(String),
    Download {
        url: String,
        path: Option<String>,
    },
    DialogResponse {
        id: u64,
        response: crate::engine::DialogResponse,
    },
}

#[derive(Clone)]
pub struct LuaSender {
    pub tx: UnboundedSender<LuaToRustMsg>,
}

impl LuaSender {
    pub fn new(tx: UnboundedSender<LuaToRustMsg>) -> Self {
        Self { tx }
    }

    pub fn send(&self, msg: LuaToRustMsg) {
        let _ = self.tx.send(msg);
    }
}
