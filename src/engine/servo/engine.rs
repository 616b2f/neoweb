//! ServoEngine implementation of WebEngine trait embedding Servo constellation pipeline.

use super::delegate::{ServoTabDelegate, ServoTabState};
use super::hints::{HINT_CLEANUP_SCRIPT, HINT_EXTRACTION_SCRIPT, HINT_KEYS, HintJson};
use super::host::ServoHost;
use super::search::{
    SEARCH_CLEAR_SCRIPT, SearchMatchPayload, build_search_script, collect_builtin_page_matches,
    xml_escape_highlighted,
};
use crate::engine::{
    DialogKind, DialogRequest, DialogResponse, EngineEvent, Favicon, HintItem, LoadStatus,
    NavigationEntry, RenderFrame, SearchOptions, SearchResult, WebEngine, icons,
};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};
use scraper::{Html, Selector};
use servo::{
    CSSPixel, Code, DeviceIntPoint, DeviceIntRect, EmbedderControlId, InputEvent, JSValue, Key,
    KeyState, KeyboardEvent, Location, Modifiers, MouseButton, MouseButtonAction, MouseButtonEvent,
    MouseMoveEvent, NamedKey, Scroll, SimpleDialog, WebView, WebViewBuilder, WebViewPoint,
    WebViewVector, WheelDelta, WheelEvent, WheelMode,
};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

pub fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Debug, Clone)]
pub struct ServoWebPage {
    pub uri: String,
    pub title: String,
    pub favicon: Option<Favicon>,
    pub headings: Vec<String>,
    pub body_text: Vec<String>,
    pub links: Vec<(String, String)>,
    pub is_error: bool,
}

static FONT_DB: std::sync::LazyLock<Arc<resvg::usvg::fontdb::Database>> =
    std::sync::LazyLock::new(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        let family = db
            .faces()
            .next()
            .and_then(|f| f.families.first().map(|fam| fam.0.clone()));
        if let Some(f) = family {
            db.set_sans_serif_family(&f);
            db.set_serif_family(&f);
            db.set_monospace_family(&f);
        }
        Arc::new(db)
    });

/// Direct Servo Embedder Backend Pipeline
///
/// Implements offscreen rendering architecture defined in Neoweb ROADMAP.md:
/// - Manages Servo embedder instance and Constellation pipeline
/// - Renders web and about content into an offscreen texture surface (`RenderFrame`)
/// - Supports Chromium-style `NavigationEntry` linear session history with zero-latency cached title feedback
/// - Resolves synchronous `about:` pages (`about:blank`, `about:help`, `about:neoweb`) on the local thread
/// - Embeds real Servo offscreen pipeline for live web documents, rendering real document content
/// - Handles dynamic viewport resizing, vertical scrolling (`j`/`k`), and forwards resize events
/// - Bridges DOM events and link hinting via SpiderMonkey `eval_js`
pub struct ServoEngine {
    uri: String,
    title: String,
    favicon: Option<Favicon>,
    status: LoadStatus,
    progress: f32,
    scroll_y: f32,
    zoom: f32,
    history: Vec<NavigationEntry>,
    current_index: usize,
    width: u32,
    height: u32,
    frame: Option<RenderFrame>,
    events: Vec<EngineEvent>,
    hints: Vec<HintItem>,
    pending_hints: Arc<Mutex<Option<Vec<HintItem>>>>,
    active_search: Option<SearchResult>,
    last_search_query: Option<String>,
    last_search_opts: Option<SearchOptions>,
    active_dialog: Option<(u64, EmbedderControlId, SimpleDialog, DialogRequest)>,
    current_page: Option<ServoWebPage>,
    pending_page: Arc<Mutex<Option<ServoWebPage>>>,
    is_about_page: bool,
    webview: Option<WebView>,
    needs_paint: Arc<AtomicBool>,
    pub(crate) tab_state: Arc<Mutex<ServoTabState>>,
}

impl ServoEngine {
    pub fn new(initial_uri: &str) -> Self {
        info!(
            "Initializing ServoEmbedder backend pipeline for {}",
            initial_uri
        );
        let mut engine = Self {
            uri: String::new(),
            title: "Servo Webview".to_string(),
            favicon: None,
            status: LoadStatus::Finished,
            progress: 1.0,
            scroll_y: 0.0,
            zoom: 1.0,
            history: Vec::new(),
            current_index: 0,
            width: 1280,
            height: 784,
            frame: None,
            events: Vec::new(),
            hints: Vec::new(),
            pending_hints: Arc::new(Mutex::new(None)),
            active_search: None,
            last_search_query: None,
            last_search_opts: None,
            active_dialog: None,
            current_page: None,
            pending_page: Arc::new(Mutex::new(None)),
            is_about_page: true,
            webview: None,
            needs_paint: Arc::new(AtomicBool::new(false)),
            tab_state: Arc::new(Mutex::new(ServoTabState::default())),
        };

        engine.load_url(initial_uri);
        engine
    }

    fn ensure_webview(&mut self, initial_url: Option<url::Url>) -> Result<(), String> {
        if self.webview.is_none() {
            let delegate = Rc::new(ServoTabDelegate {
                state: self.tab_state.clone(),
                needs_paint: self.needs_paint.clone(),
            });
            let initial_url_clone = initial_url.clone();
            let psize = dpi::PhysicalSize::new(self.width.max(100), self.height.max(100));
            let webview_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ServoHost::with_or_init(self.width, self.height, |host| {
                    let mut builder =
                        WebViewBuilder::new(&host.servo, host.rendering_context.clone())
                            .delegate(delegate);
                    if let Some(url) = initial_url_clone {
                        builder = builder.url(url);
                    }
                    let wv = builder.build();
                    wv.resize(psize);
                    wv
                })
            }));
            match webview_res {
                Ok(Ok(wv)) => self.webview = Some(wv),
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    warn!(
                        "Servo webview creation failed or panicked (WebRender context unavailable on current thread)"
                    );
                    return Err("Failed to create webview".to_string());
                }
            }
        }
        Ok(())
    }

    /// Normalizes URL input (e.g. `github.com` -> `https://github.com`)
    pub fn normalize_url(input: &str) -> String {
        let trimmed = input.trim();
        if trimmed.starts_with("about:")
            || trimmed.starts_with("file:")
            || trimmed.starts_with("data:")
            || trimmed.contains("://")
        {
            trimmed.to_string()
        } else if trimmed == "localhost"
            || trimmed.starts_with("localhost:")
            || trimmed.starts_with("localhost/")
            || trimmed.starts_with("127.0.0.1")
            || trimmed.starts_with("[::1]")
        {
            format!("http://{}", trimmed)
        } else if trimmed.find('.').is_some() && !trimmed.contains(' ') {
            format!("https://{}", trimmed)
        } else {
            trimmed.to_string()
        }
    }

    /// Renders an SVG markup string into raw RGBA bytes at the specified dimensions
    pub fn render_svg_to_rgba(
        svg_bytes: &[u8],
        target_width: u32,
        target_height: u32,
    ) -> Option<Vec<u8>> {
        let opt = Options {
            fontdb: FONT_DB.clone(),
            ..Default::default()
        };
        let tree = Tree::from_data(svg_bytes, &opt).ok()?;
        let mut pixmap = Pixmap::new(target_width, target_height)?;

        let svg_size = tree.size();
        let sx = target_width as f32 / svg_size.width();
        let sy = target_height as f32 / svg_size.height();
        let transform = Transform::from_scale(sx, sy);

        resvg::render(&tree, transform, &mut pixmap.as_mut());
        Some(pixmap.data().to_vec())
    }

    /// Renders the current page content into the offscreen RGBA frame buffer
    fn update_offscreen_frame(&mut self) {
        let w = self.width.max(100);
        let h = self.height.max(100);
        let scroll_offset = self.scroll_y.max(0.0) as i32;

        let svg = match self.uri.as_str() {
            "about:blank" => format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
                    <rect width="{w}" height="{h}" fill="#1a1b26"/>
                    <text x="{}" y="{}" fill="#565f89" font-family="sans-serif" font-size="16" text-anchor="middle">about:blank</text>
                </svg>"##,
                w / 2,
                h / 2
            ),
            "about:help" => {
                let cs = self
                    .last_search_opts
                    .map(|o| o.case_sensitive)
                    .unwrap_or(false);
                let mut mc = 0;
                let mut h_esc = |text: &str| {
                    xml_escape_highlighted(text, self.active_search.as_ref(), cs, &mut mc)
                };

                format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
                    <rect width="{w}" height="{h}" fill="#1a1b26"/>
                    <g transform="translate(0, -{scroll_offset})">
                        <rect x="40" y="40" width="{}" height="{}" rx="8" fill="#24283b" stroke="#3b4261" stroke-width="2"/>
                        <text x="70" y="90" fill="#7aa2f7" font-family="sans-serif" font-size="28" font-weight="bold">{}</text>
                        <text x="70" y="130" fill="#c0caf5" font-family="sans-serif" font-size="16">{}</text>

                        <text x="70" y="180" fill="#9ece6a" font-family="sans-serif" font-size="18" font-weight="bold">{}</text>
                        <text x="70" y="210" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>
                        <text x="70" y="235" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>
                        <text x="70" y="260" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>
                        <text x="70" y="285" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>

                        <text x="70" y="335" fill="#e0af68" font-family="sans-serif" font-size="18" font-weight="bold">{}</text>
                        <text x="70" y="365" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>
                        <text x="70" y="390" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>
                        <text x="70" y="415" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>

                        <text x="70" y="465" fill="#bb9aaf7" font-family="sans-serif" font-size="18" font-weight="bold">{}</text>
                        <text x="70" y="495" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>
                        <text x="70" y="520" fill="#c0caf5" font-family="monospace" font-size="14">{}</text>
                    </g>
                </svg>"##,
                    w.saturating_sub(80),
                    h.saturating_sub(80),
                    h_esc("Neoweb Help & Keybindings"),
                    h_esc("Vi-Style Modal Web Browser powered by Servo & Iced"),
                    h_esc("Normal Mode Navigation"),
                    h_esc("j / k : Scroll down / up (supports counts like 5j)"),
                    h_esc("h / l : Scroll left / right"),
                    h_esc("<C-o> / <C-i> : Session history back / forward"),
                    h_esc("gg / G : Scroll to top / bottom"),
                    h_esc("Tabs & Links"),
                    h_esc("t / d : Open new tab / Close tab"),
                    h_esc("gt / gT : Next tab / Previous tab"),
                    h_esc("f / F : Follow hint (current tab / background tab)"),
                    h_esc("Commands & Search"),
                    h_esc(": : Enter command prompt (:open, :tabopen, :session save)"),
                    h_esc("/ : Search forward, ? : Search backward"),
                )
            }
            "about:neoweb" => format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
                    <rect width="{w}" height="{h}" fill="#1a1b26"/>
                    <g transform="translate(0, -{scroll_offset})">
                        <circle cx="{}" cy="120" r="48" fill="#24283b" stroke="#7aa2f7" stroke-width="3"/>
                        <text x="{}" y="132" fill="#7aa2f7" font-family="sans-serif" font-size="36" font-weight="bold" text-anchor="middle">N</text>
                        <text x="{}" y="200" fill="#7aa2f7" font-family="sans-serif" font-size="32" font-weight="bold" text-anchor="middle">Neoweb</text>
                        <text x="{}" y="235" fill="#c0caf5" font-family="sans-serif" font-size="16" text-anchor="middle">Servo + Iced + Lua Modal Web Browser</text>

                        <!-- Quick Navigation Cards -->
                        <rect x="{}" y="270" width="180" height="70" rx="8" fill="#24283b" stroke="#3b4261" stroke-width="1.5"/>
                        <text x="{}" y="300" fill="#9ece6a" font-family="sans-serif" font-size="14" font-weight="bold">about:help</text>
                        <text x="{}" y="322" fill="#565f89" font-family="sans-serif" font-size="12">Keybinding reference</text>

                        <rect x="{}" y="270" width="180" height="70" rx="8" fill="#24283b" stroke="#3b4261" stroke-width="1.5"/>
                        <text x="{}" y="300" fill="#7aa2f7" font-family="sans-serif" font-size="14" font-weight="bold">duckduckgo.com</text>
                        <text x="{}" y="322" fill="#565f89" font-family="sans-serif" font-size="12">Privacy search engine</text>

                        <rect x="{}" y="270" width="180" height="70" rx="8" fill="#24283b" stroke="#3b4261" stroke-width="1.5"/>
                        <text x="{}" y="300" fill="#e0af68" font-family="sans-serif" font-size="14" font-weight="bold">github.com</text>
                        <text x="{}" y="322" fill="#565f89" font-family="sans-serif" font-size="12">GitHub platform</text>
                    </g>
                </svg>"##,
                w / 2,
                w / 2,
                w / 2,
                w / 2,
                (w / 2).saturating_sub(290),
                (w / 2).saturating_sub(275),
                (w / 2).saturating_sub(275),
                (w / 2).saturating_sub(90),
                (w / 2).saturating_sub(75),
                (w / 2).saturating_sub(75),
                (w / 2) + 110,
                (w / 2) + 125,
                (w / 2) + 125,
            ),
            _ => {
                if let Some(page) = &self.current_page {
                    self.build_webpage_svg(page, w, h, scroll_offset)
                } else {
                    format!(
                        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
                            <rect width="{w}" height="{h}" fill="#1a1b26"/>
                            <rect x="40" y="40" width="{}" height="90" rx="8" fill="#24283b" stroke="#3b4261" stroke-width="1.5"/>
                            <text x="60" y="75" fill="#7aa2f7" font-family="sans-serif" font-size="20" font-weight="bold">Connecting to {}</text>
                            <text x="60" y="105" fill="#565f89" font-family="sans-serif" font-size="14">Fetching document and preparing Servo WGPU surface...</text>
                            <!-- Progress Bar -->
                            <rect x="40" y="145" width="{}" height="4" rx="2" fill="#24283b"/>
                            <rect x="40" y="145" width="{}" height="4" rx="2" fill="#7aa2f7"/>
                        </svg>"##,
                        w.saturating_sub(80),
                        xml_escape(&self.uri),
                        w.saturating_sub(80),
                        ((w.saturating_sub(80)) as f32 * self.progress.clamp(0.05, 1.0)) as u32
                    )
                }
            }
        };

        if let Some(rgba) = Self::render_svg_to_rgba(svg.as_bytes(), w, h) {
            self.frame = Some(RenderFrame {
                width: w,
                height: h,
                rgba_data: Arc::new(rgba),
            });
            self.events.push(EngineEvent::ContentUpdated);
        }
    }

    /// Formats live web content into an SVG document for offscreen rendering
    fn build_webpage_svg(&self, page: &ServoWebPage, w: u32, h: u32, scroll_offset: i32) -> String {
        let mut elements = String::new();
        let content_width = w.saturating_sub(80);
        let mut y = 50;

        let cs = self
            .last_search_opts
            .map(|o| o.case_sensitive)
            .unwrap_or(false);
        let mut mc = 0;

        // 1. Page Header Card
        let title_escaped =
            xml_escape_highlighted(&page.title, self.active_search.as_ref(), cs, &mut mc);
        let uri_escaped =
            xml_escape_highlighted(&page.uri, self.active_search.as_ref(), cs, &mut mc);
        elements.push_str(&format!(
            r##"<rect x="40" y="{y}" width="{content_width}" height="75" rx="8" fill="#24283b" stroke="#3b4261" stroke-width="1.5"/>
            <text x="60" y="{}" fill="#7aa2f7" font-family="sans-serif" font-size="20" font-weight="bold">{title_escaped}</text>
            <text x="60" y="{}" fill="#565f89" font-family="monospace" font-size="13">🔒 {uri_escaped}</text>"##,
            y + 35,
            y + 60
        ));
        y += 95;

        // 2. Headings
        for heading in page.headings.iter().take(5) {
            let h_escaped =
                xml_escape_highlighted(heading, self.active_search.as_ref(), cs, &mut mc);
            elements.push_str(&format!(
                r##"<text x="40" y="{y}" fill="#9ece6a" font-family="sans-serif" font-size="18" font-weight="bold">{h_escaped}</text>"##
            ));
            y += 28;
        }

        if !page.headings.is_empty() {
            y += 10;
        }

        // 3. Body paragraphs
        for para in page.body_text.iter().take(8) {
            let p_escaped = xml_escape_highlighted(para, self.active_search.as_ref(), cs, &mut mc);
            let truncated = if p_escaped.len() > 140 {
                format!("{}...", &p_escaped[..137])
            } else {
                p_escaped
            };
            elements.push_str(&format!(
                r##"<text x="40" y="{y}" fill="#c0caf5" font-family="sans-serif" font-size="14">{truncated}</text>"##
            ));
            y += 22;
        }

        if !page.body_text.is_empty() {
            y += 15;
        }

        // 4. Interactive Links & Follow Hint Badges
        if !page.links.is_empty() {
            elements.push_str(&format!(
                r##"<text x="40" y="{y}" fill="#bb9aaf7" font-family="sans-serif" font-size="16" font-weight="bold">Links &amp; Navigation Targets:</text>"##
            ));
            y += 25;

            for (idx, (label, href)) in page.links.iter().take(25).enumerate() {
                let hint_key = HINT_KEYS.get(idx).unwrap_or(&"?");
                let label_escaped =
                    xml_escape_highlighted(label, self.active_search.as_ref(), cs, &mut mc);
                let href_escaped =
                    xml_escape_highlighted(href, self.active_search.as_ref(), cs, &mut mc);

                let display_text = if label_escaped.is_empty() {
                    href_escaped
                } else {
                    format!("{label_escaped} → {href_escaped}")
                };
                let display_truncated = if display_text.len() > 100 {
                    format!("{}...", &display_text[..97])
                } else {
                    display_text
                };

                let card_h = 28;
                let card_w = content_width.min(750);
                elements.push_str(&format!(
                    r##"<rect x="40" y="{y}" width="{card_w}" height="{card_h}" rx="4" fill="#24283b" stroke="#3b4261" stroke-width="1"/>
                    <rect x="44" y="{}" width="28" height="20" rx="3" fill="#e0af68"/>
                    <text x="58" y="{}" fill="#1a1b26" font-family="monospace" font-size="12" font-weight="bold" text-anchor="middle">[{}]</text>
                    <text x="80" y="{}" fill="#7aa2f7" font-family="sans-serif" font-size="13">{display_truncated}</text>"##,
                    y + 4,
                    y + 18,
                    hint_key,
                    y + 19
                ));
                y += 34;
            }
        }

        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
                <rect width="{w}" height="{h}" fill="#1a1b26"/>
                <g transform="translate(0, -{scroll_offset})">
                    {elements}
                </g>
            </svg>"##
        )
    }

    /// Sets up link hint items for mouseless follow mode
    fn setup_hints_for_page(&mut self) {
        self.hints.clear();
        match self.uri.as_str() {
            "about:neoweb" => {
                let w = self.width.max(100);
                self.hints.push(HintItem {
                    key: "a".to_string(),
                    label: "about:help".to_string(),
                    uri: "about:help".to_string(),
                    x: (w / 2).saturating_sub(290) as f32,
                    y: 270.0,
                    width: 180.0,
                    height: 70.0,
                });
                self.hints.push(HintItem {
                    key: "s".to_string(),
                    label: "duckduckgo.com".to_string(),
                    uri: "https://duckduckgo.com".to_string(),
                    x: (w / 2).saturating_sub(90) as f32,
                    y: 270.0,
                    width: 180.0,
                    height: 70.0,
                });
                self.hints.push(HintItem {
                    key: "d".to_string(),
                    label: "github.com".to_string(),
                    uri: "https://github.com".to_string(),
                    x: ((w / 2) + 110) as f32,
                    y: 270.0,
                    width: 180.0,
                    height: 70.0,
                });
            }
            "about:help" => {
                self.hints.push(HintItem {
                    key: "a".to_string(),
                    label: "about:neoweb".to_string(),
                    uri: "about:neoweb".to_string(),
                    x: 70.0,
                    y: 180.0,
                    width: 200.0,
                    height: 25.0,
                });
            }
            _ => {
                if let Some(page) = &self.current_page {
                    let mut start_y = 50 + 95;
                    if !page.headings.is_empty() {
                        start_y += page.headings.len().min(5) as i32 * 28 + 10;
                    }
                    if !page.body_text.is_empty() {
                        start_y += page.body_text.len().min(8) as i32 * 22 + 15;
                    }
                    start_y += 25; // Links header

                    let content_width = self.width.saturating_sub(80).min(750);
                    for (idx, (label, href)) in page.links.iter().take(25).enumerate() {
                        if let Some(&key) = HINT_KEYS.get(idx) {
                            self.hints.push(HintItem {
                                key: key.to_string(),
                                label: label.clone(),
                                uri: href.clone(),
                                x: 40.0,
                                y: start_y as f32,
                                width: content_width as f32,
                                height: 28.0,
                            });
                        }
                        start_y += 34;
                    }
                }
            }
        }
    }

    /// Extracts favicon URL from the page DOM via SpiderMonkey evaluate_javascript
    /// and asynchronously downloads/normalizes it into a 16x16 PNG Favicon.
    fn extract_favicon_from_dom(&self) {
        if self.is_about_page || self.webview.is_none() {
            return;
        }
        let Some(wv) = &self.webview else { return };
        let tab_state_clone = self.tab_state.clone();
        let needs_paint_clone = self.needs_paint.clone();
        let page_url = self.uri.clone();
        let target_gen = self
            .tab_state
            .lock()
            .map(|st| st.favicon_generation)
            .unwrap_or(0);
        let current_favicon_uri = self.favicon.as_ref().and_then(|f| f.uri.clone());

        const FAVICON_QUERY_SCRIPT: &str = r#"(function() {
            var selectors = [
                'link[rel="icon"][type*="svg"]',
                'link[rel~="icon"][type*="svg"]',
                'link[rel="shortcut icon"]',
                'link[rel="icon"]',
                'link[rel~="icon"]',
                'link[rel~="apple-touch-icon"]',
                'link[rel="apple-touch-icon-precomposed"]'
            ];
            for (var i = 0; i < selectors.length; i++) {
                var el = document.querySelector(selectors[i]);
                if (el) {
                    var href = el.href || el.getAttribute('href');
                    if (href) return href;
                }
            }
            return "";
        })()"#;

        wv.evaluate_javascript(FAVICON_QUERY_SCRIPT, move |res| {
            let icon_raw = match res {
                Ok(JSValue::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
                _ => return,
            };

            // Avoid re-fetching if we already loaded this exact favicon URI
            if current_favicon_uri.as_deref() == Some(&icon_raw) {
                return;
            }

            // Resolve relative URLs if needed
            let abs_url = if icon_raw.starts_with("http://")
                || icon_raw.starts_with("https://")
                || icon_raw.starts_with("data:")
            {
                icon_raw
            } else if let Ok(base) = url::Url::parse(&page_url) {
                base.join(&icon_raw)
                    .map(|u| u.to_string())
                    .unwrap_or(icon_raw)
            } else {
                icon_raw
            };

            // 1. Handle data: URI
            if abs_url.starts_with("data:") {
                if let Some(png) = icons::parse_data_uri(&abs_url)
                    && let Ok(mut st) = tab_state_clone.lock()
                    && st.favicon_generation == target_gen
                {
                    st.pending_favicon = Some(Favicon {
                        glyph: "🌐".to_string(),
                        bytes: Some(png),
                        uri: Some(abs_url),
                        is_custom: true,
                    });
                    st.has_custom_favicon = true;
                    needs_paint_clone.store(true, Ordering::SeqCst);
                }
                return;
            }

            // 2. Handle HTTP/HTTPS URL
            if (abs_url.starts_with("http://") || abs_url.starts_with("https://"))
                && let Ok(handle) = tokio::runtime::Handle::try_current()
            {
                handle.spawn(async move {
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(5))
                        .build();
                    if let Ok(c) = client
                        && let Ok(resp) = c.get(&abs_url).send().await
                        && resp.status().is_success()
                        && let Ok(bytes) = resp.bytes().await
                        && let Some(png) = icons::process_favicon_bytes(&bytes)
                        && let Ok(mut st) = tab_state_clone.lock()
                        && st.favicon_generation == target_gen
                    {
                        st.pending_favicon = Some(Favicon {
                            glyph: "🌐".to_string(),
                            bytes: Some(png),
                            uri: Some(abs_url),
                            is_custom: true,
                        });
                        st.has_custom_favicon = true;
                        needs_paint_clone.store(true, Ordering::SeqCst);
                    }
                });
            }
        });
    }

    /// Asynchronously fetches and parses live web pages via HTTP stack
    fn fetch_web_page_async(&self, target_uri: String) {
        let pending = self.pending_page.clone();
        let task = async move {
            let client = match reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Servo/0.7.0 Neoweb/0.1.0")
                .timeout(std::time::Duration::from_secs(15))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to build reqwest client: {}", e);
                    return;
                }
            };

            match client.get(&target_uri).send().await {
                Ok(resp) => {
                    let final_url = resp.url().to_string();
                    match resp.text().await {
                        Ok(html_text) => {
                            let (title, headings, body_text, links, icon_urls) = {
                                let document = Html::parse_document(&html_text);

                                // 1. Extract title
                                let title_sel = Selector::parse("title").unwrap();
                                let title = document
                                    .select(&title_sel)
                                    .next()
                                    .map(|el| el.text().collect::<String>().trim().to_string())
                                    .filter(|t| !t.is_empty())
                                    .unwrap_or_else(|| final_url.clone());

                                // 2. Extract Headings
                                let mut headings = Vec::new();
                                if let Ok(h_sel) = Selector::parse("h1, h2, h3") {
                                    for el in document.select(&h_sel).take(8) {
                                        let text = el.text().collect::<String>().trim().to_string();
                                        if !text.is_empty() && text.len() > 2 {
                                            headings.push(text);
                                        }
                                    }
                                }

                                // 3. Extract Paragraphs
                                let mut body_text = Vec::new();
                                if let Ok(p_sel) = Selector::parse("p, li") {
                                    for el in document.select(&p_sel).take(12) {
                                        let text = el.text().collect::<String>().trim().to_string();
                                        if !text.is_empty() && text.len() > 10 {
                                            body_text.push(text);
                                        }
                                    }
                                }

                                // 4. Extract Links
                                let mut links = Vec::new();
                                if let Ok(a_sel) = Selector::parse("a[href]") {
                                    for el in document.select(&a_sel) {
                                        if let Some(href) = el.value().attr("href") {
                                            let text =
                                                el.text().collect::<String>().trim().to_string();
                                            let abs_url = if href.starts_with("http://")
                                                || href.starts_with("https://")
                                            {
                                                href.to_string()
                                            } else if let Ok(base) = url::Url::parse(&final_url) {
                                                base.join(href)
                                                    .map(|u| u.to_string())
                                                    .unwrap_or_else(|_| href.to_string())
                                            } else {
                                                href.to_string()
                                            };

                                            if !abs_url.starts_with("javascript:")
                                                && !abs_url.starts_with('#')
                                            {
                                                links.push((text, abs_url));
                                            }
                                        }
                                    }
                                }

                                // 5. Extract Favicon candidates
                                let mut icon_urls = Vec::new();
                                if let Ok(icon_sel) = Selector::parse(
                                    r#"link[rel="icon"][type*="svg"], link[rel~="icon"][type*="svg"], link[rel="shortcut icon"], link[rel="icon"], link[rel~="icon"], link[rel~="apple-touch-icon"]"#,
                                ) {
                                    for el in document.select(&icon_sel) {
                                        if let Some(href) = el.value().attr("href") {
                                            let abs_url = if href.starts_with("http://")
                                                || href.starts_with("https://")
                                                || href.starts_with("data:")
                                            {
                                                href.to_string()
                                            } else if let Ok(base) = url::Url::parse(&final_url) {
                                                base.join(href)
                                                    .map(|u| u.to_string())
                                                    .unwrap_or_else(|_| href.to_string())
                                            } else {
                                                href.to_string()
                                            };
                                            icon_urls.push(abs_url);
                                        }
                                    }
                                }

                                (title, headings, body_text, links, icon_urls)
                            };

                            let mut page_favicon = None;
                            for abs_url in icon_urls {
                                if abs_url.starts_with("data:") {
                                    if let Some(png) = icons::parse_data_uri(&abs_url) {
                                        page_favicon = Some(Favicon {
                                            glyph: "🌐".to_string(),
                                            bytes: Some(png),
                                            uri: Some(abs_url),
                                            is_custom: true,
                                        });
                                        break;
                                    }
                                } else if (abs_url.starts_with("http://")
                                    || abs_url.starts_with("https://"))
                                    && let Ok(resp) = client.get(&abs_url).send().await
                                    && resp.status().is_success()
                                    && let Ok(bytes) = resp.bytes().await
                                    && let Some(png) = icons::process_favicon_bytes(&bytes)
                                {
                                    page_favicon = Some(Favicon {
                                        glyph: "🌐".to_string(),
                                        bytes: Some(png),
                                        uri: Some(abs_url),
                                        is_custom: true,
                                    });
                                    break;
                                }
                            }

                            // If no <link> favicon succeeded, probe /favicon.ico
                            if page_favicon.is_none()
                                && let Ok(parsed_base) = url::Url::parse(&final_url)
                                && let Some(host) = parsed_base.host_str()
                            {
                                let port_part = parsed_base
                                    .port()
                                    .map(|p| format!(":{}", p))
                                    .unwrap_or_default();
                                let ico_url = format!(
                                    "{}://{}{}/favicon.ico",
                                    parsed_base.scheme(),
                                    host,
                                    port_part
                                );
                                if let Ok(resp) = client.get(&ico_url).send().await
                                    && resp.status().is_success()
                                    && let Ok(bytes) = resp.bytes().await
                                    && let Some(png) = icons::process_favicon_bytes(&bytes)
                                {
                                    page_favicon = Some(Favicon {
                                        glyph: "🌐".to_string(),
                                        bytes: Some(png),
                                        uri: Some(ico_url),
                                        is_custom: true,
                                    });
                                }
                            }

                            let page = ServoWebPage {
                                uri: final_url,
                                title,
                                favicon: page_favicon
                                    .or_else(|| Some(icons::default_globe_favicon())),
                                headings,
                                body_text,
                                links,
                                is_error: false,
                            };

                            let mut lock = pending.lock().unwrap();
                            *lock = Some(page);
                        }
                        Err(e) => {
                            let page = ServoWebPage {
                                uri: target_uri,
                                title: "Failed to read response".to_string(),
                                favicon: Some(icons::default_globe_favicon()),
                                headings: vec!["Network Response Error".to_string()],
                                body_text: vec![format!("Error reading document body: {}", e)],
                                links: vec![],
                                is_error: true,
                            };
                            let mut lock = pending.lock().unwrap();
                            *lock = Some(page);
                        }
                    }
                }
                Err(e) => {
                    let page = ServoWebPage {
                        uri: target_uri,
                        title: "Connection Failed".to_string(),
                        favicon: Some(icons::default_globe_favicon()),
                        headings: vec!["Unable to connect to website".to_string()],
                        body_text: vec![format!("Network error: {}", e)],
                        links: vec![(
                            "Return to Neoweb home".to_string(),
                            "about:neoweb".to_string(),
                        )],
                        is_error: true,
                    };
                    let mut lock = pending.lock().unwrap();
                    *lock = Some(page);
                }
            }
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(task);
        } else {
            std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    rt.block_on(task);
                }
            });
        }
    }
}

impl WebEngine for ServoEngine {
    fn uri(&self) -> &str {
        &self.uri
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn favicon(&self) -> Option<&Favicon> {
        self.favicon.as_ref()
    }

    fn status(&self) -> LoadStatus {
        self.status
    }

    fn progress(&self) -> f32 {
        self.progress
    }

    fn load_url(&mut self, url: &str) {
        let normalized = Self::normalize_url(url);
        self.uri = normalized.clone();
        self.status = LoadStatus::Provisional;
        self.progress = 0.2;
        self.scroll_y = 0.0;
        self.current_page = None;
        self.active_search = None;
        self.events.push(EngineEvent::UriChanged(self.uri.clone()));
        self.events
            .push(EngineEvent::StatusChanged(LoadStatus::Provisional));
        self.events.push(EngineEvent::ProgressChanged(0.2));

        // 1. Synchronous internal page resolution (Rule 6)
        if normalized.starts_with("about:") {
            self.is_about_page = true;
            if let Some(wv) = &self.webview {
                wv.hide();
                wv.set_focused(false);
            }
            match normalized.as_str() {
                "about:blank" => {
                    self.title = "Blank Page".to_string();
                    self.favicon = Some(icons::blank_favicon());
                }
                "about:help" => {
                    self.title = "Neoweb Help & Keybindings".to_string();
                    self.favicon = Some(icons::help_favicon());
                }
                "about:neoweb" => {
                    self.title = "Neoweb - Home".to_string();
                    self.favicon = Some(icons::neoweb_favicon());
                }
                _ => {
                    self.title = format!("About: {}", normalized);
                    self.favicon = Some(icons::default_globe_favicon());
                }
            }

            self.status = LoadStatus::Finished;
            self.progress = 1.0;

            let entry = NavigationEntry {
                uri: self.uri.clone(),
                title: self.title.clone(),
                favicon: self.favicon.clone(),
            };
            if self.history.is_empty() {
                self.history.push(entry);
                self.current_index = 0;
            } else if self.current_index + 1 < self.history.len() {
                self.history.truncate(self.current_index + 1);
                self.history.push(entry);
                self.current_index = self.history.len() - 1;
            } else {
                self.history.push(entry);
                self.current_index = self.history.len() - 1;
            }

            self.events
                .push(EngineEvent::TitleChanged(self.title.clone()));
            if let Some(fav) = &self.favicon {
                self.events.push(EngineEvent::FaviconChanged(fav.clone()));
            }
            self.events
                .push(EngineEvent::StatusChanged(LoadStatus::Finished));
            self.events.push(EngineEvent::ProgressChanged(1.0));
            self.setup_hints_for_page();
            self.update_offscreen_frame();
            return;
        }

        // 2. Web URL load pipeline
        self.is_about_page = false;
        let hostname = url::Url::parse(&normalized)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| normalized.clone());
        self.title = format!("Loading {}...", hostname);
        self.favicon = Some(icons::default_globe_favicon());

        let current_gen = if let Ok(mut st) = self.tab_state.lock() {
            st.pending_favicon = None;
            st.has_custom_favicon = false;
            st.favicon_generation += 1;
            st.favicon_generation
        } else {
            0
        };

        let entry = NavigationEntry {
            uri: self.uri.clone(),
            title: self.title.clone(),
            favicon: self.favicon.clone(),
        };
        if self.history.is_empty() {
            self.history.push(entry);
            self.current_index = 0;
        } else if self.current_index + 1 < self.history.len() {
            self.history.truncate(self.current_index + 1);
            self.history.push(entry);
            self.current_index = self.history.len() - 1;
        } else {
            self.history.push(entry);
            self.current_index = self.history.len() - 1;
        }

        self.events
            .push(EngineEvent::TitleChanged(self.title.clone()));
        self.setup_hints_for_page();
        self.update_offscreen_frame();

        // Load via real Servo pipeline
        let mut servo_loaded = false;
        if let Ok(parsed_url) = url::Url::parse(&normalized) {
            if matches!(parsed_url.scheme(), "http" | "https")
                && let Some(host) = parsed_url.host_str()
            {
                let port_part = parsed_url
                    .port()
                    .map(|p| format!(":{}", p))
                    .unwrap_or_default();
                let favicon_url = format!(
                    "{}://{}{}/favicon.ico",
                    parsed_url.scheme(),
                    host,
                    port_part
                );
                let tab_state_clone = self.tab_state.clone();
                let needs_paint_clone = self.needs_paint.clone();
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let client = reqwest::Client::builder()
                            .timeout(std::time::Duration::from_secs(5))
                            .build();
                        if let Ok(c) = client
                            && let Ok(resp) = c.get(&favicon_url).send().await
                            && resp.status().is_success()
                            && let Ok(bytes) = resp.bytes().await
                            && let Some(png) = icons::process_favicon_bytes(&bytes)
                            && let Ok(mut st) = tab_state_clone.lock()
                            && st.favicon_generation == current_gen
                            && !st.has_custom_favicon
                            && st.pending_favicon.is_none()
                        {
                            st.pending_favicon = Some(Favicon {
                                glyph: "🌐".to_string(),
                                bytes: Some(png),
                                uri: Some(favicon_url),
                                is_custom: true,
                            });
                            needs_paint_clone.store(true, Ordering::SeqCst);
                        }
                    });
                }
            }

            let is_new = self.webview.is_none();
            if self.ensure_webview(Some(parsed_url.clone())).is_ok()
                && let Some(wv) = &self.webview
            {
                wv.show();
                wv.set_focused(true);
                if !is_new {
                    wv.load(parsed_url);
                }
                servo_loaded = true;
            }
        }

        if !servo_loaded {
            self.fetch_web_page_async(normalized);
        }
    }

    fn reload(&mut self, _bypass_cache: bool) {
        if !self.is_about_page
            && let Some(wv) = &self.webview
        {
            wv.reload();
        } else {
            let uri = self.uri.clone();
            self.load_url(&uri);
        }
    }

    fn can_go_back(&self) -> bool {
        self.current_index > 0
    }

    fn can_go_forward(&self) -> bool {
        self.current_index + 1 < self.history.len()
    }

    fn go_back(&mut self) {
        if self.can_go_back() {
            self.current_index -= 1;
            let entry = self.history[self.current_index].clone();
            // Immediate cached title display for zero-latency feedback (Rule 6)
            self.title = entry.title;
            self.uri = entry.uri.clone();
            self.favicon = entry.favicon;
            self.scroll_y = 0.0;
            self.current_page = None;

            self.events
                .push(EngineEvent::TitleChanged(self.title.clone()));
            self.events.push(EngineEvent::UriChanged(self.uri.clone()));
            if let Some(fav) = &self.favicon {
                self.events.push(EngineEvent::FaviconChanged(fav.clone()));
            }

            if self.uri.starts_with("about:") {
                self.is_about_page = true;
                if let Some(wv) = &self.webview {
                    wv.hide();
                    wv.set_focused(false);
                }
                self.setup_hints_for_page();
                self.update_offscreen_frame();
            } else if let Some(wv) = &self.webview {
                self.is_about_page = false;
                wv.show();
                wv.set_focused(true);
                wv.go_back(1);
            } else {
                self.fetch_web_page_async(self.uri.clone());
            }
        }
    }

    fn go_forward(&mut self) {
        if self.can_go_forward() {
            self.current_index += 1;
            let entry = self.history[self.current_index].clone();
            // Immediate cached title display for zero-latency feedback (Rule 6)
            self.title = entry.title;
            self.uri = entry.uri.clone();
            self.favicon = entry.favicon;
            self.scroll_y = 0.0;
            self.current_page = None;

            self.events
                .push(EngineEvent::TitleChanged(self.title.clone()));
            self.events.push(EngineEvent::UriChanged(self.uri.clone()));
            if let Some(fav) = &self.favicon {
                self.events.push(EngineEvent::FaviconChanged(fav.clone()));
            }

            if self.uri.starts_with("about:") {
                self.is_about_page = true;
                if let Some(wv) = &self.webview {
                    wv.hide();
                    wv.set_focused(false);
                }
                self.setup_hints_for_page();
                self.update_offscreen_frame();
            } else if let Some(wv) = &self.webview {
                self.is_about_page = false;
                wv.show();
                wv.set_focused(true);
                wv.go_forward(1);
            } else {
                self.fetch_web_page_async(self.uri.clone());
            }
        }
    }

    fn stop(&mut self) {
        self.status = LoadStatus::Finished;
        self.events
            .push(EngineEvent::StatusChanged(LoadStatus::Finished));
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll_y = (self.scroll_y + dy).max(0.0);
        if let Some(wv) = &self.webview {
            let pt = WebViewPoint::from(euclid::Point2D::<f32, CSSPixel>::new(
                (self.width / 2) as f32,
                (self.height / 2) as f32,
            ));
            let delta = WebViewVector::from(euclid::Vector2D::<f32, CSSPixel>::new(dx, dy));
            wv.notify_scroll_event(Scroll::Delta(delta), pt);
            self.needs_paint.store(true, Ordering::SeqCst);
        } else {
            self.update_offscreen_frame();
            self.events.push(EngineEvent::ContentUpdated);
        }
    }

    fn scroll_to_pct(&mut self, pct: f32) {
        if let Some(wv) = &self.webview {
            let pt = WebViewPoint::from(euclid::Point2D::<f32, CSSPixel>::new(
                (self.width / 2) as f32,
                (self.height / 2) as f32,
            ));
            if pct <= 0.0 {
                self.scroll_y = 0.0;
                wv.notify_scroll_event(Scroll::Start, pt);
                self.needs_paint.store(true, Ordering::SeqCst);
                return;
            } else if pct >= 100.0 {
                self.scroll_y = 1000.0;
                wv.notify_scroll_event(Scroll::End, pt);
                self.needs_paint.store(true, Ordering::SeqCst);
                return;
            }
        }
        let target_y = pct * 10.0;
        let diff = target_y - self.scroll_y;
        self.scroll_by(0.0, diff);
    }

    fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    fn scroll_pct(&self) -> f32 {
        (self.scroll_y / 1000.0).clamp(0.0, 1.0) * 100.0
    }

    fn zoom_by(&mut self, delta: f32) {
        self.zoom = (self.zoom + delta).clamp(0.2, 5.0);
        if let Some(wv) = &self.webview {
            wv.set_page_zoom(self.zoom);
        } else {
            self.update_offscreen_frame();
        }
    }

    fn zoom_reset(&mut self) {
        self.zoom = 1.0;
        if let Some(wv) = &self.webview {
            wv.set_page_zoom(1.0);
        } else {
            self.update_offscreen_frame();
        }
    }

    fn zoom_level(&self) -> f32 {
        self.zoom
    }

    fn eval_js(&mut self, script: &str) -> Result<String, String> {
        info!("Servo SpiderMonkey eval_js: {}", script);
        if !self.is_about_page
            && let Some(wv) = &self.webview
        {
            let result_slot: Arc<Mutex<Option<Result<String, String>>>> =
                Arc::new(Mutex::new(None));
            let slot_clone = result_slot.clone();
            wv.evaluate_javascript(script, move |res| {
                let mapped = match res {
                    Ok(JSValue::String(s)) => Ok(s),
                    Ok(JSValue::Number(n)) => Ok(n.to_string()),
                    Ok(JSValue::Boolean(b)) => Ok(b.to_string()),
                    Ok(JSValue::Null) | Ok(JSValue::Undefined) => Ok("null".to_string()),
                    Ok(val) => Ok(format!("{val:?}")),
                    Err(e) => Err(format!("{e:?}")),
                };
                if let Ok(mut lock) = slot_clone.lock() {
                    *lock = Some(mapped);
                }
            });
            for _ in 0..50 {
                let _ = ServoHost::with(|host| {
                    host.servo.spin_event_loop();
                });
                if let Ok(mut lock) = result_slot.lock()
                    && let Some(res) = lock.take()
                {
                    return res;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        if script.contains("document.title") {
            Ok(self.title.clone())
        } else if script.contains("document.location.href")
            || script.contains("window.location.href")
        {
            Ok(self.uri.clone())
        } else {
            Ok("{}".to_string())
        }
    }

    fn eval_js_async(
        &mut self,
        script: &str,
        callback: Box<dyn FnOnce(Result<String, String>) + 'static>,
    ) {
        if let Some(wv) = &self.webview {
            wv.evaluate_javascript(script, move |res| {
                let mapped = match res {
                    Ok(JSValue::String(s)) => Ok(s),
                    Ok(val) => Ok(format!("{val:?}")),
                    Err(e) => Err(format!("{e:?}")),
                };
                callback(mapped);
            });
        } else {
            let res = self.eval_js(script);
            callback(res);
        }
    }

    fn generate_hints(&self) -> Vec<HintItem> {
        self.hints.clone()
    }

    fn follow_hint(&mut self, key: &str) -> Option<String> {
        let matching = self
            .hints
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(key))
            .cloned();

        if let Some(wv) = &self.webview {
            let key_escaped = key
                .to_ascii_lowercase()
                .replace('\\', "\\\\")
                .replace('\'', "\\'");
            let script = format!(
                r#"
                (function() {{
                    var key = '{key_escaped}';
                    var el = document.querySelector('[data-neoweb-hint="' + key + '"]');
                    var old = document.getElementById('__neoweb_hints_overlay');
                    if (old) old.remove();
                    var tagged = document.querySelectorAll('[data-neoweb-hint]');
                    for (var i = 0; i < tagged.length; i++) {{
                        tagged[i].removeAttribute('data-neoweb-hint');
                    }}
                    if (el) {{
                        var tag = el.tagName.toLowerCase();
                        if (tag === 'input' || tag === 'textarea' || tag === 'select') {{
                            el.focus();
                            if (el.select) el.select();
                        }} else {{
                            el.click();
                        }}
                        return el.href || el.getAttribute('href') || '';
                    }}
                    return '';
                }})()
                "#
            );
            let needs_paint = self.needs_paint.clone();
            wv.evaluate_javascript(script, move |_| {
                needs_paint.store(true, Ordering::SeqCst);
            });
        }

        matching.map(|h| h.uri)
    }

    fn activate_follow_mode(&mut self) {
        if self.is_about_page || self.current_page.is_some() {
            self.setup_hints_for_page();
            self.update_offscreen_frame();
        } else if let Some(wv) = &self.webview {
            let pending_hints = self.pending_hints.clone();
            let needs_paint = self.needs_paint.clone();
            wv.evaluate_javascript(HINT_EXTRACTION_SCRIPT, move |res| {
                let json_str_opt = match res {
                    Ok(JSValue::String(s)) => Some(s),
                    _ => None,
                };
                if let Some(json_str) = json_str_opt
                    && let Ok(items) = serde_json::from_str::<Vec<HintJson>>(&json_str)
                {
                    let hints: Vec<HintItem> = items
                        .into_iter()
                        .map(|item| HintItem {
                            key: item.key,
                            label: item.label,
                            uri: item.uri,
                            x: item.x,
                            y: item.y,
                            width: item.width,
                            height: item.height,
                        })
                        .collect();
                    if let Ok(mut lock) = pending_hints.lock() {
                        *lock = Some(hints);
                    }
                    needs_paint.store(true, Ordering::SeqCst);
                }
            });
        }
    }

    fn clear_hints(&mut self) {
        self.hints.clear();
        if let Ok(mut lock) = self.pending_hints.lock() {
            *lock = None;
        }
        if let Some(wv) = &self.webview {
            let needs_paint = self.needs_paint.clone();
            wv.evaluate_javascript(HINT_CLEANUP_SCRIPT, move |_| {
                needs_paint.store(true, Ordering::SeqCst);
            });
        }
        if self.is_about_page {
            self.update_offscreen_frame();
        }
    }

    fn search(&mut self, query_opt: Option<&str>, mut opts: SearchOptions) -> Option<SearchResult> {
        let q = match query_opt {
            Some("") => {
                self.clear_search();
                return None;
            }
            Some(q_str) => {
                self.last_search_query = Some(q_str.to_string());
                self.last_search_opts = Some(opts);
                q_str
            }
            None => {
                if let Some(last_q) = &self.last_search_query {
                    opts.find_next = true;
                    last_q.as_str()
                } else {
                    return None;
                }
            }
        };

        if self.is_about_page || self.current_page.is_some() {
            let matches = collect_builtin_page_matches(
                &self.uri,
                self.current_page.as_ref(),
                q,
                opts.case_sensitive,
            );
            let total = matches.len();
            if total == 0 {
                let res = SearchResult {
                    query: q.to_string(),
                    current_match: 0,
                    total_matches: 0,
                };
                self.active_search = Some(res.clone());
                self.update_offscreen_frame();
                return Some(res);
            }

            let next_match = match &self.active_search {
                Some(prev) if opts.find_next && prev.query == q && prev.total_matches == total => {
                    let prev_idx = prev.current_match.saturating_sub(1);
                    if opts.backward {
                        if opts.wrap {
                            (prev_idx + total - 1) % total + 1
                        } else {
                            prev_idx.saturating_sub(1) + 1
                        }
                    } else if opts.wrap {
                        (prev_idx + 1) % total + 1
                    } else {
                        (prev_idx + 1).min(total - 1) + 1
                    }
                }
                _ => {
                    if opts.backward {
                        total
                    } else {
                        1
                    }
                }
            };

            let res = SearchResult {
                query: q.to_string(),
                current_match: next_match,
                total_matches: total,
            };
            self.active_search = Some(res.clone());

            // Scroll match into view
            if let Some(&y_pos) = matches.get(next_match.saturating_sub(1)) {
                let view_h = self.height as f32;
                if y_pos < self.scroll_y + 40.0 || y_pos > self.scroll_y + view_h - 40.0 {
                    self.scroll_y = (y_pos - view_h / 2.0).max(0.0);
                }
            }

            self.update_offscreen_frame();
            return Some(res);
        }

        // Live Servo WebView DOM Search
        if let Some(wv) = &self.webview {
            let script = build_search_script(q, opts);
            let result_slot: Arc<Mutex<Option<Option<SearchResult>>>> = Arc::new(Mutex::new(None));
            let slot_clone = result_slot.clone();
            let query_str = q.to_string();
            let needs_paint = self.needs_paint.clone();

            wv.evaluate_javascript(script, move |res| {
                let parsed = match res {
                    Ok(JSValue::String(json_str)) => {
                        serde_json::from_str::<SearchMatchPayload>(&json_str).ok()
                    }
                    _ => None,
                };
                let search_res = parsed.map(|p| SearchResult {
                    query: query_str,
                    current_match: p.current_match,
                    total_matches: p.total_matches,
                });
                if let Ok(mut lock) = slot_clone.lock() {
                    *lock = Some(search_res);
                }
                needs_paint.store(true, Ordering::SeqCst);
            });

            // Pump event loop to receive JS evaluation result
            for _ in 0..50 {
                let _ = ServoHost::with(|host| {
                    host.servo.spin_event_loop();
                });
                if let Ok(mut lock) = result_slot.lock()
                    && let Some(res_opt) = lock.take()
                {
                    self.active_search = res_opt.clone();
                    return res_opt;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        None
    }

    fn clear_search(&mut self) {
        self.active_search = None;
        if let Some(wv) = &self.webview {
            let needs_paint = self.needs_paint.clone();
            wv.evaluate_javascript(SEARCH_CLEAR_SCRIPT, move |_| {
                needs_paint.store(true, Ordering::SeqCst);
            });
        }
        if self.is_about_page || self.current_page.is_some() {
            self.update_offscreen_frame();
        }
    }

    fn active_search(&self) -> Option<&SearchResult> {
        self.active_search.as_ref()
    }

    fn active_dialog(&self) -> Option<&DialogRequest> {
        self.active_dialog.as_ref().map(|(_, _, _, req)| req)
    }

    fn handle_dialog_response(&mut self, id: u64, response: DialogResponse) {
        if let Some((active_id, _, _, _)) = &self.active_dialog
            && *active_id == id
        {
            let (_, _, dialog, _) = self.active_dialog.take().unwrap();
            match response {
                DialogResponse::Cancel => {
                    dialog.dismiss();
                }
                DialogResponse::Ok(val) => match dialog {
                    SimpleDialog::Alert(alert) => alert.confirm(),
                    SimpleDialog::Confirm(confirm) => confirm.confirm(),
                    SimpleDialog::Prompt(mut prompt) => {
                        if let Some(text) = val {
                            prompt.set_current_value(&text);
                        }
                        prompt.confirm();
                    }
                },
            }
            if self.webview.is_some() {
                let _ = ServoHost::with(|host| {
                    host.servo.spin_event_loop();
                });
            }
            self.needs_paint.store(true, Ordering::SeqCst);
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.width != width || self.height != height {
            self.width = width.max(100);
            self.height = height.max(100);
            if self.is_about_page {
                self.setup_hints_for_page();
                self.update_offscreen_frame();
            }
            if let Some(wv) = &self.webview {
                let psize = dpi::PhysicalSize::new(self.width, self.height);
                wv.resize(psize);
                self.needs_paint.store(true, Ordering::SeqCst);
            }
        }
    }

    fn viewport_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn render_frame(&self) -> Option<RenderFrame> {
        self.frame.clone()
    }

    fn activate(&mut self) {
        if let Some(wv) = &self.webview {
            wv.show();
            wv.set_focused(true);
        }
    }

    fn deactivate(&mut self) {
        if let Some(wv) = &self.webview {
            wv.hide();
            wv.set_focused(false);
        }
    }

    fn handle_mouse_move(&mut self, x: f32, y: f32) {
        if let Some(wv) = &self.webview {
            let pt = WebViewPoint::from(euclid::Point2D::<f32, CSSPixel>::new(x, y));
            let ev = InputEvent::MouseMove(MouseMoveEvent::new(pt));
            wv.notify_input_event(ev);
        }
    }

    fn handle_mouse_button(&mut self, button: iced::mouse::Button, pressed: bool, x: f32, y: f32) {
        if let Some(wv) = &self.webview {
            let pt = WebViewPoint::from(euclid::Point2D::<f32, CSSPixel>::new(x, y));
            let action = if pressed {
                MouseButtonAction::Down
            } else {
                MouseButtonAction::Up
            };
            let btn = match button {
                iced::mouse::Button::Left => MouseButton::Primary,
                iced::mouse::Button::Right => MouseButton::Secondary,
                iced::mouse::Button::Middle => MouseButton::Auxiliary,
                iced::mouse::Button::Back => MouseButton::Back,
                iced::mouse::Button::Forward => MouseButton::Forward,
                _ => MouseButton::Primary,
            };
            let ev = InputEvent::MouseButton(MouseButtonEvent::new(action, btn, pt));
            wv.notify_input_event(ev);
            let _ = ServoHost::with(|host| {
                host.servo.spin_event_loop();
            });

            if !pressed && button == iced::mouse::Button::Left {
                let state_clone = self.tab_state.clone();
                let check_script = r#"
                    (function() {
                        var el = document.activeElement;
                        if (!el) return "false";
                        var tag = el.tagName ? el.tagName.toLowerCase() : "";
                        if (tag === "input") {
                            var typ = (el.type || "text").toLowerCase();
                            return (typ !== "button" && typ !== "submit" && typ !== "reset" && typ !== "checkbox" && typ !== "radio" && typ !== "image") ? "true" : "false";
                        }
                        if (tag === "textarea" || tag === "select") return "true";
                        if (el.isContentEditable) return "true";
                        return "false";
                    })()
                "#;
                wv.evaluate_javascript(check_script, move |res| {
                    let is_active = match res {
                        Ok(JSValue::String(s)) => s == "true",
                        Ok(JSValue::Boolean(b)) => b,
                        _ => false,
                    };
                    if let Ok(mut st) = state_clone.lock() {
                        st.pending_form_active = Some(is_active);
                    }
                });
            }
        }
    }

    fn handle_wheel(&mut self, dx: f32, dy: f32, x: f32, y: f32) {
        if let Some(wv) = &self.webview {
            let pt = WebViewPoint::from(euclid::Point2D::<f32, CSSPixel>::new(x, y));
            let delta = WheelDelta {
                x: dx as f64,
                y: dy as f64,
                z: 0.0,
                mode: WheelMode::DeltaPixel,
            };
            let ev = InputEvent::Wheel(WheelEvent::new(delta, pt));
            wv.notify_input_event(ev);
        }
    }

    fn handle_key_str(&mut self, key_str: &str) {
        if let Some(wv) = &self.webview {
            let stroke = crate::keymap::KeyStroke::from_neovim_string(key_str);
            let (servo_key, servo_code) = match stroke.as_ref().map(|s| &s.key) {
                Some(crate::keymap::KeyKind::Named(named)) => match named {
                    crate::keymap::NamedKeyKind::Enter => {
                        (Key::Named(NamedKey::Enter), Code::Enter)
                    }
                    crate::keymap::NamedKeyKind::Backspace => {
                        (Key::Named(NamedKey::Backspace), Code::Backspace)
                    }
                    crate::keymap::NamedKeyKind::Tab => (Key::Named(NamedKey::Tab), Code::Tab),
                    crate::keymap::NamedKeyKind::Space => {
                        (Key::Character(" ".to_string()), Code::Space)
                    }
                    crate::keymap::NamedKeyKind::Escape => {
                        (Key::Named(NamedKey::Escape), Code::Escape)
                    }
                    crate::keymap::NamedKeyKind::Delete => {
                        (Key::Named(NamedKey::Delete), Code::Delete)
                    }
                    crate::keymap::NamedKeyKind::Insert => {
                        (Key::Named(NamedKey::Insert), Code::Insert)
                    }
                    crate::keymap::NamedKeyKind::Home => (Key::Named(NamedKey::Home), Code::Home),
                    crate::keymap::NamedKeyKind::End => (Key::Named(NamedKey::End), Code::End),
                    crate::keymap::NamedKeyKind::PageUp => {
                        (Key::Named(NamedKey::PageUp), Code::PageUp)
                    }
                    crate::keymap::NamedKeyKind::PageDown => {
                        (Key::Named(NamedKey::PageDown), Code::PageDown)
                    }
                    crate::keymap::NamedKeyKind::ArrowUp => {
                        (Key::Named(NamedKey::ArrowUp), Code::ArrowUp)
                    }
                    crate::keymap::NamedKeyKind::ArrowDown => {
                        (Key::Named(NamedKey::ArrowDown), Code::ArrowDown)
                    }
                    crate::keymap::NamedKeyKind::ArrowLeft => {
                        (Key::Named(NamedKey::ArrowLeft), Code::ArrowLeft)
                    }
                    crate::keymap::NamedKeyKind::ArrowRight => {
                        (Key::Named(NamedKey::ArrowRight), Code::ArrowRight)
                    }
                },
                Some(crate::keymap::KeyKind::Character(c)) => {
                    let code = if let Some(ch) = c.chars().next() {
                        match ch.to_ascii_uppercase() {
                            'A' => Code::KeyA,
                            'B' => Code::KeyB,
                            'C' => Code::KeyC,
                            'D' => Code::KeyD,
                            'E' => Code::KeyE,
                            'F' => Code::KeyF,
                            'G' => Code::KeyG,
                            'H' => Code::KeyH,
                            'I' => Code::KeyI,
                            'J' => Code::KeyJ,
                            'K' => Code::KeyK,
                            'L' => Code::KeyL,
                            'M' => Code::KeyM,
                            'N' => Code::KeyN,
                            'O' => Code::KeyO,
                            'P' => Code::KeyP,
                            'Q' => Code::KeyQ,
                            'R' => Code::KeyR,
                            'S' => Code::KeyS,
                            'T' => Code::KeyT,
                            'U' => Code::KeyU,
                            'V' => Code::KeyV,
                            'W' => Code::KeyW,
                            'X' => Code::KeyX,
                            'Y' => Code::KeyY,
                            'Z' => Code::KeyZ,
                            '0' => Code::Digit0,
                            '1' => Code::Digit1,
                            '2' => Code::Digit2,
                            '3' => Code::Digit3,
                            '4' => Code::Digit4,
                            '5' => Code::Digit5,
                            '6' => Code::Digit6,
                            '7' => Code::Digit7,
                            '8' => Code::Digit8,
                            '9' => Code::Digit9,
                            ' ' => Code::Space,
                            _ => Code::Unidentified,
                        }
                    } else {
                        Code::Unidentified
                    };
                    (Key::Character(c.clone()), code)
                }
                None => (Key::Named(NamedKey::Unidentified), Code::Unidentified),
            };

            let mut mods = Modifiers::empty();
            if let Some(s) = &stroke {
                if s.ctrl {
                    mods |= Modifiers::CONTROL;
                }
                if s.alt {
                    mods |= Modifiers::ALT;
                }
                if s.shift {
                    mods |= Modifiers::SHIFT;
                }
            }

            let ev_down = InputEvent::Keyboard(KeyboardEvent::new_without_event(
                KeyState::Down,
                servo_key.clone(),
                servo_code,
                Location::Standard,
                mods,
                false,
                false,
            ));
            wv.notify_input_event(ev_down);

            let ev_up = InputEvent::Keyboard(KeyboardEvent::new_without_event(
                KeyState::Up,
                servo_key,
                servo_code,
                Location::Standard,
                mods,
                false,
                false,
            ));
            wv.notify_input_event(ev_up);

            let _ = ServoHost::with(|host| {
                host.servo.spin_event_loop();
            });
            self.needs_paint.store(true, Ordering::SeqCst);
        }
    }

    fn poll_events(&mut self) -> Vec<EngineEvent> {
        if self.webview.is_some() {
            let _ = ServoHost::with(|host| {
                host.servo.spin_event_loop();
            });
        }

        if let Ok(mut lock) = self.pending_hints.lock()
            && let Some(hints) = lock.take()
        {
            self.hints = hints;
            self.events.push(EngineEvent::ContentUpdated);
        }

        let mut should_extract_favicon = false;
        // Process tab delegate events
        if let Ok(mut state) = self.tab_state.lock() {
            if let Some(fav) = state.pending_favicon.take() {
                self.favicon = Some(fav.clone());
                if let Some(entry) = self.history.get_mut(self.current_index) {
                    entry.favicon = Some(fav.clone());
                }
                self.events.push(EngineEvent::FaviconChanged(fav));
            }

            if let Some(active) = state.pending_form_active.take() {
                self.events.push(EngineEvent::FormActive(active));
            }

            for (url, suggested_filename) in state.pending_downloads.drain(..) {
                self.events.push(EngineEvent::DownloadRequested {
                    url,
                    suggested_filename,
                });
            }

            if let Some((id, control_id, dialog)) = state.pending_dialog.take() {
                let kind = match &dialog {
                    SimpleDialog::Alert(a) => DialogKind::Alert {
                        message: a.message().to_string(),
                    },
                    SimpleDialog::Confirm(c) => DialogKind::Confirm {
                        message: c.message().to_string(),
                    },
                    SimpleDialog::Prompt(p) => DialogKind::Prompt {
                        message: p.message().to_string(),
                        default_value: p.current_value().to_string(),
                    },
                };
                let req = DialogRequest { id, kind };
                self.active_dialog = Some((id, control_id, dialog, req.clone()));
                self.events.push(EngineEvent::Dialog(req));
            }

            if let Some(control_id) = state.hide_control_id.take()
                && let Some((id, active_cid, _, _)) = &self.active_dialog
                && *active_cid == control_id
            {
                let dismissed_id = *id;
                self.active_dialog = None;
                self.events.push(EngineEvent::DialogDismissed(dismissed_id));
            }

            if state.crashed {
                state.crashed = false;
                self.status = LoadStatus::Failed;
                self.events
                    .push(EngineEvent::StatusChanged(LoadStatus::Failed));
            }

            if let Some(url) = state.pending_url.take() {
                let is_spurious_blank = url == "about:blank" && !self.is_about_page;
                if !is_spurious_blank {
                    self.uri = url.clone();
                    if let Some(entry) = self.history.get_mut(self.current_index) {
                        entry.uri = url.clone();
                    }
                    self.events.push(EngineEvent::UriChanged(url));
                }
            }

            if let Some(title) = state.pending_title.take() {
                let is_spurious_blank_title = title == "about:blank" && !self.is_about_page;
                if !is_spurious_blank_title {
                    self.title = title.clone();
                    if let Some(entry) = self.history.get_mut(self.current_index) {
                        entry.title = title.clone();
                    }
                    self.events.push(EngineEvent::TitleChanged(title));
                }
            }

            if let Some(status) = state.pending_status.take() {
                let mapped = match status {
                    servo::LoadStatus::Started => LoadStatus::Provisional,
                    servo::LoadStatus::HeadParsed => LoadStatus::Committed,
                    servo::LoadStatus::Complete => LoadStatus::Finished,
                };
                self.status = mapped;
                self.events.push(EngineEvent::StatusChanged(mapped));
                let p = match mapped {
                    LoadStatus::Provisional => 0.5,
                    LoadStatus::Committed => 0.8,
                    LoadStatus::Finished => 1.0,
                    LoadStatus::Failed => 0.0,
                };
                self.progress = p;
                self.events.push(EngineEvent::ProgressChanged(p));

                if matches!(mapped, LoadStatus::Committed | LoadStatus::Finished) {
                    should_extract_favicon = true;
                }
            }
        }

        if should_extract_favicon {
            self.extract_favicon_from_dom();
        }

        // Paint frame if requested by Servo or if completed real web page needs initial frame
        let should_paint = self.needs_paint.swap(false, Ordering::SeqCst)
            || (self.frame.is_none()
                && self.status == LoadStatus::Finished
                && !self.is_about_page
                && self.current_page.is_none());
        if should_paint && let Some(wv) = &self.webview {
            let _ = ServoHost::with(|host| {
                wv.paint();
                let ctx_size = host.rendering_context.size();
                let target_w = self.width.min(ctx_size.width);
                let target_h = self.height.min(ctx_size.height);
                let rect = DeviceIntRect::new(
                    DeviceIntPoint::new(0, 0),
                    DeviceIntPoint::new(target_w as i32, target_h as i32),
                );
                if let Some(img) = host.rendering_context.read_to_image(rect) {
                    self.frame = Some(RenderFrame {
                        width: target_w,
                        height: target_h,
                        rgba_data: Arc::new(img.into_raw()),
                    });
                    self.events.push(EngineEvent::ContentUpdated);
                }
            });
        }

        // Drain pending web page from background fallback fetcher
        let page_opt = self
            .pending_page
            .try_lock()
            .ok()
            .and_then(|mut lock| lock.take());

        if let Some(page) = page_opt {
            self.title = page.title.clone();
            self.favicon = page.favicon.clone();
            self.status = LoadStatus::Finished;
            self.progress = 1.0;
            if self.current_index < self.history.len() {
                self.history[self.current_index].title = self.title.clone();
                self.history[self.current_index].favicon = self.favicon.clone();
            }

            self.events
                .push(EngineEvent::TitleChanged(self.title.clone()));
            if let Some(fav) = &self.favicon {
                self.events.push(EngineEvent::FaviconChanged(fav.clone()));
            }
            self.events
                .push(EngineEvent::StatusChanged(LoadStatus::Finished));
            self.events.push(EngineEvent::ProgressChanged(1.0));

            self.current_page = Some(page);
            self.setup_hints_for_page();
            self.update_offscreen_frame();
        }

        std::mem::take(&mut self.events)
    }
}
