//! Servo WebView delegate and per-tab navigation state.

use servo::{
    EmbedderControl, EmbedderControlId, LoadStatus, NavigationRequest, SimpleDialog, WebView,
    WebViewDelegate,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{error, warn};

#[derive(Default)]
pub struct ServoTabState {
    pub pending_url: Option<String>,
    pub pending_title: Option<String>,
    pub pending_status: Option<LoadStatus>,
    pub pending_favicon: Option<crate::engine::Favicon>,
    pub pending_form_active: Option<bool>,
    pub pending_downloads: Vec<(String, Option<String>)>,
    pub pending_dialog: Option<(u64, EmbedderControlId, SimpleDialog)>,
    pub hide_control_id: Option<EmbedderControlId>,
    pub crashed: bool,
}

pub struct ServoTabDelegate {
    pub state: Arc<Mutex<ServoTabState>>,
    pub needs_paint: Arc<AtomicBool>,
}

impl WebViewDelegate for ServoTabDelegate {
    fn notify_url_changed(&self, _webview: WebView, url: url::Url) {
        if let Ok(mut state) = self.state.lock() {
            state.pending_url = Some(url.to_string());
        }
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        if let Ok(mut state) = self.state.lock() {
            state.pending_title = title;
        }
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        if let Ok(mut state) = self.state.lock() {
            state.pending_status = Some(status);
        }
        if status == LoadStatus::Complete {
            self.needs_paint.store(true, Ordering::SeqCst);
        }
    }

    fn notify_favicon_changed(&self, webview: WebView) {
        if let Some(img_ref) = webview.favicon()
            && let Some(fav) = crate::engine::icons::favicon_from_servo_image(&img_ref)
        {
            if let Ok(mut state) = self.state.lock() {
                state.pending_favicon = Some(fav);
            }
            self.needs_paint.store(true, Ordering::SeqCst);
        }
    }

    fn notify_animating_changed(&self, _webview: WebView, animating: bool) {
        if animating {
            self.needs_paint.store(true, Ordering::SeqCst);
        }
    }

    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.needs_paint.store(true, Ordering::SeqCst);
    }

    fn notify_crashed(&self, _webview: WebView, reason: String, _backtrace: Option<String>) {
        error!("Servo content process crashed: {}", reason);
        if let Ok(mut state) = self.state.lock() {
            state.crashed = true;
        }
    }

    fn request_navigation(&self, _webview: WebView, navigation_request: NavigationRequest) {
        let scheme = navigation_request.url.scheme();
        if matches!(scheme, "http" | "https" | "about" | "data") {
            let url = navigation_request.url.clone();
            if crate::download::is_downloadable_url(&url) {
                let suggested = crate::download::suggest_filename_from_url(&url);
                tracing::info!(url = %url, suggested = %suggested, "Intercepted downloadable resource navigation");
                if let Ok(mut state) = self.state.lock() {
                    state
                        .pending_downloads
                        .push((url.to_string(), Some(suggested)));
                }
                navigation_request.deny();
                return;
            }
            navigation_request.allow();
        } else {
            warn!("Denying unsafe navigation scheme: {}", scheme);
            navigation_request.deny();
        }
    }

    fn show_embedder_control(&self, _webview: WebView, embedder_control: EmbedderControl) {
        let control_id = embedder_control.id();
        if let EmbedderControl::SimpleDialog(dialog) = embedder_control {
            static NEXT_DIALOG_ID: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(1);
            let id = NEXT_DIALOG_ID.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut state) = self.state.lock() {
                state.pending_dialog = Some((id, control_id, dialog));
            }
            self.needs_paint.store(true, Ordering::SeqCst);
        }
    }

    fn hide_embedder_control(&self, _webview: WebView, control_id: EmbedderControlId) {
        if let Ok(mut state) = self.state.lock() {
            state.hide_control_id = Some(control_id);
        }
        self.needs_paint.store(true, Ordering::SeqCst);
    }
}
