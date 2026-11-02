//! Tests for ServoEngine, ServoHost, hints, delegates, and offscreen pipeline.

use super::delegate::{ServoTabDelegate, ServoTabState};
use super::engine::ServoEngine;
use super::host::shutdown_servo;
use super::webdriver::ServoWebDriverClient;
use crate::engine::{LoadStatus, WebEngine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[test]
fn test_servo_engine_initialization_about_pages() {
    let engine = ServoEngine::new("about:blank");
    assert_eq!(engine.uri(), "about:blank");
    assert_eq!(engine.title(), "Blank Page");
    assert_eq!(engine.status(), LoadStatus::Finished);
    assert!(engine.render_frame().is_some());

    let help_engine = ServoEngine::new("about:help");
    assert_eq!(help_engine.uri(), "about:help");
    assert_eq!(help_engine.title(), "Neoweb Help & Keybindings");
    assert!(help_engine.render_frame().is_some());

    let neoweb_engine = ServoEngine::new("about:neoweb");
    assert_eq!(neoweb_engine.uri(), "about:neoweb");
    assert_eq!(neoweb_engine.title(), "Neoweb - Home");
    assert!(neoweb_engine.render_frame().is_some());
    assert!(!neoweb_engine.generate_hints().is_empty());
}

#[test]
fn test_servo_engine_url_normalization() {
    assert_eq!(
        ServoEngine::normalize_url("github.com"),
        "https://github.com"
    );
    assert_eq!(
        ServoEngine::normalize_url("https://github.com"),
        "https://github.com"
    );
    assert_eq!(ServoEngine::normalize_url("about:help"), "about:help");
}

#[test]
fn test_servo_engine_history_navigation_instant_title() {
    let mut engine = ServoEngine::new("about:neoweb");
    assert_eq!(engine.title(), "Neoweb - Home");
    assert!(!engine.can_go_back());

    engine.load_url("about:help");
    assert_eq!(engine.title(), "Neoweb Help & Keybindings");
    assert!(engine.can_go_back());
    assert!(!engine.can_go_forward());

    // Go back: instant cached title display
    engine.go_back();
    assert_eq!(engine.uri(), "about:neoweb");
    assert_eq!(engine.title(), "Neoweb - Home");
    assert!(!engine.can_go_back());
    assert!(engine.can_go_forward());

    // Go forward
    engine.go_forward();
    assert_eq!(engine.uri(), "about:help");
    assert_eq!(engine.title(), "Neoweb Help & Keybindings");
}

#[test]
fn test_servo_engine_resize_and_render_frame() {
    let mut engine = ServoEngine::new("about:neoweb");
    assert_eq!(engine.viewport_size(), (1280, 784));

    let frame = engine.render_frame().expect("Frame must be rendered");
    assert_eq!(frame.width, 1280);
    assert_eq!(frame.height, 784);
    assert_eq!(frame.rgba_data.len(), 1280 * 784 * 4);

    // Resize viewport
    engine.resize(800, 600);
    assert_eq!(engine.viewport_size(), (800, 600));

    let new_frame = engine
        .render_frame()
        .expect("Resized frame must be rendered");
    assert_eq!(new_frame.width, 800);
    assert_eq!(new_frame.height, 600);
    assert_eq!(new_frame.rgba_data.len(), 800 * 600 * 4);
}

#[test]
fn test_servo_engine_hints_and_following() {
    let mut engine = ServoEngine::new("about:neoweb");
    let hints = engine.generate_hints();
    assert!(!hints.is_empty());

    let target_uri = engine.follow_hint("a").expect("Hint a should exist");
    assert_eq!(target_uri, "about:help");
}

#[test]
fn test_servo_engine_scrolling() {
    let mut engine = ServoEngine::new("about:help");
    assert_eq!(engine.scroll_y(), 0.0);

    engine.scroll_by(0.0, 100.0);
    assert_eq!(engine.scroll_y(), 100.0);

    engine.scroll_to_pct(50.0);
    assert_eq!(engine.scroll_y(), 500.0);
}

#[test]
fn test_servo_text_rasterization_produces_visible_pixels() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
        <text x="10" y="30" font-family="sans-serif" font-size="20" fill="#ffffff">GitHub</text>
    </svg>"##;
    let rgba =
        ServoEngine::render_svg_to_rgba(svg.as_bytes(), 100, 50).expect("SVG text must render");
    assert_eq!(rgba.len(), 100 * 50 * 4);
    let visible_pixels = rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
    assert!(
        visible_pixels > 0,
        "Text must produce visible non-zero pixels"
    );
}

#[test]
fn test_software_rendering_context() {
    use servo::SoftwareRenderingContext;
    let ctx = SoftwareRenderingContext::new(dpi::PhysicalSize::new(200, 200));
    assert!(
        ctx.is_ok(),
        "SoftwareRenderingContext should initialize: {:?}",
        ctx.err()
    );
}

#[test]
fn test_servo_tab_delegate_crash_handling() {
    let state = Arc::new(Mutex::new(ServoTabState::default()));
    let needs_paint = Arc::new(AtomicBool::new(false));
    let _delegate = ServoTabDelegate {
        state: state.clone(),
        needs_paint: needs_paint.clone(),
    };

    assert!(!state.lock().unwrap().crashed);
    assert!(!needs_paint.load(Ordering::SeqCst));

    // Simulate crash notification from Servo content process
    {
        let mut s = state.lock().unwrap();
        s.crashed = true;
    }
    assert!(state.lock().unwrap().crashed);
}

#[test]
fn test_servo_engine_input_events() {
    let mut engine = ServoEngine::new("about:neoweb");
    engine.handle_mouse_move(100.0, 150.0);
    engine.handle_mouse_button(iced::mouse::Button::Left, true, 100.0, 150.0);
    engine.handle_mouse_button(iced::mouse::Button::Left, false, 100.0, 150.0);
    engine.handle_wheel(0.0, 50.0, 100.0, 150.0);
    engine.handle_key_str("a");
    engine.handle_key_str("<BS>");
    engine.activate();
    engine.deactivate();
}

#[test]
fn test_live_servo_load() {
    let mut engine = ServoEngine::new("about:blank");

    // 1. Verify live Servo offline HTML parsing and SpiderMonkey JavaScript execution
    engine.load_url("data:text/html,<title>Initial</title><script>document.title='JS Succeeded';</script><h1>Test</h1>");
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = engine.poll_events();
        if engine.title() == "JS Succeeded" {
            break;
        }
    }
    assert_eq!(engine.title(), "JS Succeeded");
    assert!(engine.render_frame().is_some());

    // 2. Verify secondary live URL navigation and frame rendering with a tall scrollable body
    engine.load_url("data:text/html,<html><head><title>Example Domain</title></head><body style='height:3000px;margin:0'><h1>Example</h1></body></html>");
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = engine.poll_events();
        if engine.status() == LoadStatus::Finished && engine.title() == "Example Domain" {
            break;
        }
    }
    assert_eq!(engine.title(), "Example Domain");
    assert_eq!(engine.status(), LoadStatus::Finished);
    assert!(engine.render_frame().is_some());

    // 3. Verify live SpiderMonkey eval_js on active WebView
    let js_title = engine
        .eval_js("document.title")
        .expect("eval_js should succeed");
    assert_eq!(js_title, "Example Domain");

    // 4. Verify j (+dy), k (-dy), G (100%), and gg (0%) scroll directions on live Servo WebView
    engine.scroll_by(0.0, 150.0);
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = engine.poll_events();
    }
    let y_after_j: f64 = engine
        .eval_js("window.scrollY")
        .unwrap()
        .parse()
        .unwrap_or(0.0);
    assert!(
        y_after_j > 0.0,
        "Scrolling down (j / +dy) must increase window.scrollY, got {y_after_j}"
    );

    engine.scroll_to_pct(100.0);
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = engine.poll_events();
    }
    let y_after_bottom: f64 = engine
        .eval_js("window.scrollY")
        .unwrap()
        .parse()
        .unwrap_or(0.0);
    assert!(
        y_after_bottom > y_after_j,
        "Scrolling to bottom (G / 100%) must jump past {y_after_j}, got {y_after_bottom}"
    );

    engine.scroll_by(0.0, -100.0);
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = engine.poll_events();
    }
    let y_after_k: f64 = engine
        .eval_js("window.scrollY")
        .unwrap()
        .parse()
        .unwrap_or(0.0);
    assert!(
        y_after_k < y_after_bottom,
        "Scrolling up (k / -dy) must decrease window.scrollY from {y_after_bottom}, got {y_after_k}"
    );

    engine.scroll_to_pct(0.0);
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = engine.poll_events();
    }
    let y_after_top: f64 = engine
        .eval_js("window.scrollY")
        .unwrap()
        .parse()
        .unwrap_or(1.0);
    assert_eq!(
        y_after_top, 0.0,
        "Scrolling to top (gg / 0%) must reset window.scrollY to 0"
    );

    // 5. Verify W3C WebDriver client bridge against active Constellation before shutdown
    drop(engine);
    let exec_res = ServoWebDriverClient::execute(servo::WebDriverCommandMsg::Shutdown);
    assert!(
        exec_res.is_ok(),
        "WebDriver command should be submitted to active constellation"
    );

    shutdown_servo();

    // 6. Verify WebDriver client returns clean error after ServoHost shutdown
    let post_shutdown_res = ServoWebDriverClient::execute(servo::WebDriverCommandMsg::Shutdown);
    assert!(post_shutdown_res.is_err());
}

#[test]
fn test_servo_engine_in_page_search() {
    use crate::engine::SearchOptions;

    let mut engine = ServoEngine::new("about:help");
    assert!(engine.active_search().is_none());

    let res = engine
        .search(Some("scroll"), SearchOptions::default())
        .expect("Expected search result");
    assert_eq!(res.query, "scroll");
    assert_eq!(res.current_match, 1);
    assert!(
        res.total_matches >= 3,
        "Expected at least 3 matches for 'scroll' on about:help"
    );
    assert!(engine.active_search().is_some());

    // Step forward (n)
    let next_res = engine
        .search(
            None,
            SearchOptions {
                find_next: true,
                ..Default::default()
            },
        )
        .expect("Expected next result");
    assert_eq!(next_res.query, "scroll");
    assert_eq!(next_res.current_match, 2);

    // Step backward (N)
    let prev_res = engine
        .search(
            None,
            SearchOptions {
                find_next: true,
                backward: true,
                ..Default::default()
            },
        )
        .expect("Expected prev result");
    assert_eq!(prev_res.query, "scroll");
    assert_eq!(prev_res.current_match, 1);

    // Case sensitive search
    let cs_res = engine
        .search(
            Some("SCROLL"),
            SearchOptions {
                case_sensitive: true,
                ..Default::default()
            },
        )
        .expect("Expected search result");
    assert_eq!(cs_res.total_matches, 0);

    engine.clear_search();
    assert!(engine.active_search().is_none());
}

#[test]
fn test_servo_engine_follow_mode_lifecycle() {
    use super::hints::HINT_EXTRACTION_SCRIPT;

    // Ensure DOM hint overlay badges display lowercase keys matching input keys
    assert!(!HINT_EXTRACTION_SCRIPT.contains("toUpperCase()"));
    assert!(HINT_EXTRACTION_SCRIPT.contains("badge.innerText = key;"));

    let mut engine = ServoEngine::new("about:neoweb");
    engine.activate_follow_mode();
    let hints = engine.generate_hints();
    assert_eq!(hints.len(), 3);
    assert_eq!(hints[0].key, "a");
    assert_eq!(hints[0].uri, "about:help");
    assert_eq!(hints[1].key, "s");
    assert_eq!(hints[1].uri, "https://duckduckgo.com");
    assert_eq!(hints[2].key, "d");
    assert_eq!(hints[2].uri, "https://github.com");

    let target = engine.follow_hint("d");
    assert_eq!(target, Some("https://github.com".to_string()));

    engine.clear_hints();
    assert!(engine.generate_hints().is_empty());
}

#[test]
fn test_webdriver_http_server_and_harness_end_to_end() {
    use super::webdriver::{ELEMENT_KEY, WebDriverTestHarness};
    use serde_json::json;

    // 1. Start Neoweb with WebDriver listening on an OS-assigned port (0)
    let mut harness = WebDriverTestHarness::start(0).expect("WebDriverTestHarness should start");
    assert!(harness.port() > 0);
    assert!(harness.endpoint_url().starts_with("http://127.0.0.1:"));

    // 2. Verify GET /status
    let status_resp = harness.get("/status");
    assert_eq!(status_resp["value"]["ready"], json!(true));

    // 3. Create session (POST /session)
    let session_resp = harness.post("/session", json!({ "capabilities": {} }));
    let session_id = session_resp["value"]["sessionId"]
        .as_str()
        .expect("sessionId must be string")
        .to_string();
    assert!(!session_id.is_empty());

    // 4. Verify initial URL & title (GET /session/{id}/url, GET /session/{id}/title)
    let url_resp = harness.get(&format!("/session/{session_id}/url"));
    assert_eq!(url_resp["value"], json!("about:neoweb"));

    let title_resp = harness.get(&format!("/session/{session_id}/title"));
    assert_eq!(title_resp["value"], json!("Neoweb - Home"));

    // 5. Find element & click hint link on about:neoweb (POST /session/{id}/element, POST .../click)
    let elem_resp = harness.post(
        &format!("/session/{session_id}/element"),
        json!({ "using": "css selector", "value": "a" }),
    );
    let element_id = elem_resp["value"][ELEMENT_KEY]
        .as_str()
        .expect("Element ID must be returned")
        .to_string();

    let click_resp = harness.post(
        &format!("/session/{session_id}/element/{element_id}/click"),
        json!({}),
    );
    assert!(click_resp["value"].is_null());

    // Clicking first link ("a" -> about:help) navigates to about:help
    let url_after_click = harness.get(&format!("/session/{session_id}/url"));
    assert_eq!(url_after_click["value"], json!("about:help"));

    // 6. Test history back & forward (POST /back, POST /forward)
    harness.post(&format!("/session/{session_id}/back"), json!({}));
    let url_back = harness.get(&format!("/session/{session_id}/url"));
    assert_eq!(url_back["value"], json!("about:neoweb"));

    harness.post(&format!("/session/{session_id}/forward"), json!({}));
    let url_fwd = harness.get(&format!("/session/{session_id}/url"));
    assert_eq!(url_fwd["value"], json!("about:help"));

    // 7. Test synchronous script execution (POST /execute/sync)
    let exec_resp = harness.post(
        &format!("/session/{session_id}/execute/sync"),
        json!({ "script": "return document.title;", "args": [] }),
    );
    assert_eq!(exec_resp["value"], json!("Neoweb Help & Keybindings"));

    // 8. Test window/tab handles & new tab creation (GET /window/handles, POST /window/new)
    let handles_resp = harness.get(&format!("/session/{session_id}/window/handles"));
    assert_eq!(handles_resp["value"].as_array().unwrap().len(), 1);

    let new_win_resp = harness.post(
        &format!("/session/{session_id}/window/new"),
        json!({ "type": "tab" }),
    );
    assert!(new_win_resp["value"]["handle"].as_str().is_some());

    // 9. Test PNG screenshot capture (GET /screenshot)
    let screenshot_resp = harness.get(&format!("/session/{session_id}/screenshot"));
    let b64_png = screenshot_resp["value"]
        .as_str()
        .expect("Base64 PNG string expected");
    assert!(!b64_png.is_empty());

    // 10. Test Neoweb modal key dispatch extension (/neoweb/mode, /neoweb/key)
    let mode_resp = harness.get(&format!("/session/{session_id}/neoweb/mode"));
    assert_eq!(mode_resp["value"], json!("normal"));

    let key_resp = harness.post(
        &format!("/session/{session_id}/neoweb/key"),
        json!({ "key": "i" }),
    );
    assert_eq!(key_resp["value"]["mode"], json!("insert"));

    let esc_resp = harness.post(
        &format!("/session/{session_id}/neoweb/key"),
        json!({ "key": "<Esc>" }),
    );
    assert_eq!(esc_resp["value"]["mode"], json!("normal"));

    // 11. Delete session & shut down first harness
    let del_resp = harness.delete(&format!("/session/{session_id}"));
    assert!(del_resp["value"].is_null());
    harness.shutdown();

    // 12. Verify starting WebDriverTestHarness on an explicit specific port
    let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let specific_port = probe.local_addr().unwrap().port();
    drop(probe);

    let mut specific_harness =
        WebDriverTestHarness::start(specific_port).expect("Should bind specific port");
    assert_eq!(specific_harness.port(), specific_port);
    let specific_status = specific_harness.get("/status");
    assert_eq!(specific_status["value"]["ready"], json!(true));
    specific_harness.shutdown();
}

#[test]
fn test_webdriver_bidi_websocket_end_to_end() {
    use super::webdriver::WebDriverTestHarness;
    use serde_json::json;

    // 1. Bind a specific port and start WebDriver BiDi harness
    let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let mut harness = WebDriverTestHarness::start(port).expect("BiDi harness should start");
    assert_eq!(harness.port(), port);
    assert_eq!(harness.ws_url(), format!("ws://127.0.0.1:{port}/session"));

    // 2. Verify Chromium/Firefox `/json/version` discovery endpoint exposes `webSocketDebuggerUrl`
    let version_info = harness.get("/json/version");
    assert_eq!(
        version_info["webSocketDebuggerUrl"],
        json!(format!("ws://127.0.0.1:{port}/session"))
    );

    // 3. Connect W3C WebDriver BiDi client over RFC 6455 WebSocket
    let mut bidi = harness
        .connect_bidi()
        .expect("WebSocket BiDi handshake should succeed");

    // 4. Verify `session.status` & `session.new`
    let status = bidi.send_command("session.status", json!({}));
    assert_eq!(status["type"], json!("success"));
    assert_eq!(status["result"]["ready"], json!(true));

    let session_new = bidi.send_command("session.new", json!({ "capabilities": {} }));
    assert_eq!(session_new["type"], json!("success"));
    assert!(
        session_new["result"]["capabilities"]["webSocketUrl"]
            .as_str()
            .unwrap()
            .starts_with(&format!("ws://127.0.0.1:{port}/session/"))
    );

    // 5. Subscribe to BiDi browsingContext lifecycle events (`session.subscribe`)
    let sub_resp = bidi.send_command(
        "session.subscribe",
        json!({
            "events": [
                "browsingContext.navigationStarted",
                "browsingContext.domContentLoaded",
                "browsingContext.load",
                "browsingContext.contextCreated",
                "browsingContext.historyUpdated"
            ]
        }),
    );
    assert_eq!(sub_resp["type"], json!("success"));
    assert!(sub_resp["result"]["subscription"].as_str().is_some());

    // 6. Query browsingContext tree (`browsingContext.getTree`)
    let tree = bidi.send_command("browsingContext.getTree", json!({}));
    assert_eq!(tree["type"], json!("success"));
    let contexts = tree["result"]["contexts"].as_array().unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0]["context"], json!("tab-0"));
    assert_eq!(contexts[0]["url"], json!("about:neoweb"));

    // 7. Locate DOM/hint nodes (`browsingContext.locateNodes`)
    let located = bidi.send_command(
        "browsingContext.locateNodes",
        json!({
            "context": "tab-0",
            "locator": { "type": "css", "value": "a" }
        }),
    );
    assert_eq!(located["type"], json!("success"));
    let nodes = located["result"]["nodes"].as_array().unwrap();
    assert!(!nodes.is_empty());
    assert_eq!(nodes[0]["sharedId"], json!("hint-a"));
    assert_eq!(nodes[0]["value"]["attributes"]["href"], json!("about:help"));

    // 8. Navigate via `browsingContext.navigate` and verify pushed BiDi events
    let nav_resp = bidi.send_command(
        "browsingContext.navigate",
        json!({
            "context": "tab-0",
            "url": "about:help",
            "wait": "complete"
        }),
    );
    assert_eq!(nav_resp["type"], json!("success"));
    assert_eq!(nav_resp["result"]["url"], json!("about:help"));

    let nav_events = bidi.take_events();
    let event_methods: Vec<&str> = nav_events
        .iter()
        .filter_map(|e| e.get("method").and_then(|m| m.as_str()))
        .collect();
    assert!(event_methods.contains(&"browsingContext.navigationStarted"));
    assert!(event_methods.contains(&"browsingContext.domContentLoaded"));
    assert!(event_methods.contains(&"browsingContext.load"));

    // 9. Evaluate JS via `script.evaluate` and verify W3C BiDi RemoteValue format
    let eval_resp = bidi.send_command(
        "script.evaluate",
        json!({
            "expression": "document.title",
            "target": { "context": "tab-0" },
            "awaitPromise": false
        }),
    );
    assert_eq!(eval_resp["type"], json!("success"));
    assert_eq!(
        eval_resp["result"]["result"],
        json!({ "type": "string", "value": "Neoweb Help & Keybindings" })
    );

    // 10. Traverse history back (`delta: -1`) and forward (`delta: 1`)
    let back_resp = bidi.send_command(
        "browsingContext.traverseHistory",
        json!({ "context": "tab-0", "delta": -1 }),
    );
    assert_eq!(back_resp["type"], json!("success"));
    let hist_events = bidi.take_events();
    assert!(
        hist_events
            .iter()
            .any(|e| e["method"] == json!("browsingContext.historyUpdated")
                && e["params"]["url"] == json!("about:neoweb"))
    );

    let fwd_resp = bidi.send_command(
        "browsingContext.traverseHistory",
        json!({ "context": "tab-0", "delta": 1 }),
    );
    assert_eq!(fwd_resp["type"], json!("success"));

    // 11. Test `input.performActions` with W3C BiDi key actions & Unicode PUA Escape (`\u{E00C}`)
    let key_down_i = bidi.send_command(
        "input.performActions",
        json!({
            "context": "tab-0",
            "actions": [{
                "type": "key",
                "id": "keyboard",
                "actions": [
                    { "type": "keyDown", "value": "i" },
                    { "type": "keyUp", "value": "i" }
                ]
            }]
        }),
    );
    assert_eq!(key_down_i["type"], json!("success"));
    let mode_insert = bidi.send_command("neoweb.getMode", json!({}));
    assert_eq!(mode_insert["result"]["mode"], json!("insert"));

    let key_esc = bidi.send_command(
        "input.performActions",
        json!({
            "context": "tab-0",
            "actions": [{
                "type": "key",
                "id": "keyboard",
                "actions": [
                    { "type": "keyDown", "value": "\u{E00C}" },
                    { "type": "keyUp", "value": "\u{E00C}" }
                ]
            }]
        }),
    );
    assert_eq!(key_esc["type"], json!("success"));
    let mode_normal = bidi.send_command("neoweb.getMode", json!({}));
    assert_eq!(mode_normal["result"]["mode"], json!("normal"));

    // 12. Test `browsingContext.captureScreenshot` & `browsingContext.create`
    let shot = bidi.send_command(
        "browsingContext.captureScreenshot",
        json!({ "context": "tab-0" }),
    );
    assert_eq!(shot["type"], json!("success"));
    assert!(!shot["result"]["data"].as_str().unwrap().is_empty());

    let created = bidi.send_command("browsingContext.create", json!({ "type": "tab" }));
    assert_eq!(created["type"], json!("success"));
    assert_eq!(created["result"]["context"], json!("tab-1"));
    let create_events = bidi.take_events();
    assert!(
        create_events
            .iter()
            .any(|e| e["method"] == json!("browsingContext.contextCreated"))
    );

    // 13. End BiDi session and shut down harness
    let end_resp = bidi.send_command("session.end", json!({}));
    assert_eq!(end_resp["type"], json!("success"));
    drop(bidi);
    harness.shutdown();
}

#[test]
fn test_servo_host_tuning_and_options() {
    use super::host::{build_servo_opts, build_servo_preferences, optimal_thread_workers};

    // 1. Thread concurrency bounds
    let workers = optimal_thread_workers();
    assert!((2..=8).contains(&workers));

    // 2. Preferences modern web APIs and thread pools
    let prefs = build_servo_preferences();
    assert!(prefs.dom_intersection_observer_enabled);
    assert!(prefs.dom_resize_observer_enabled);
    assert!(prefs.dom_adoptedstylesheet_enabled);
    assert!(prefs.dom_async_clipboard_enabled);
    assert!(prefs.dom_crypto_subtle_enabled);
    assert!(prefs.dom_fontface_enabled);
    assert!(prefs.dom_parallel_css_parsing_enabled);
    assert!(prefs.dom_visual_viewport_enabled);
    assert!(prefs.dom_web_animations_enabled);
    assert!(prefs.dom_servoparser_async_html_tokenizer_enabled);
    assert!(prefs.dom_offscreen_canvas_enabled);
    assert_eq!(prefs.thread_pool_workers_max, workers as u64);
    assert_eq!(
        prefs.thread_pool_webrender_workers_max,
        (workers / 2).max(1) as u64
    );
    assert_eq!(prefs.thread_pool_async_runtime_workers_max, workers as u64);

    // 3. Opts background hang monitoring & process isolation toggle
    let opts = build_servo_opts();
    assert!(opts.background_hang_monitor);
    assert!(
        !opts.multiprocess,
        "Tests should run in single-process mode by default"
    );

    // Explicitly enabling multiprocess mode (production default when single-process is not passed)
    super::host::set_process_mode(true);
    let opts_multi = build_servo_opts();
    assert!(opts_multi.multiprocess);

    // Resetting to single-process mode (as requested via --single-process)
    super::host::set_process_mode(false);
    let opts_single = build_servo_opts();
    assert!(!opts_single.multiprocess);
}
