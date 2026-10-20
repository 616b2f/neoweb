//! W3C WebDriver BiDi (WebSocket + HTTP) server, client, automation bridge, and test harness.
//!
//! Provides:
//! - [`WebDriverServer`]: W3C WebDriver BiDi server (RFC 6455 WebSocket + HTTP discovery/upgrade) listening on a configurable TCP port (`--remote-debugging-port <PORT>`).
//! - [`WebDriverBiDiClient`]: Synchronous W3C WebDriver BiDi WebSocket client for integration tests.
//! - [`ServoWebDriverClient`]: Direct Constellation `WebDriverCommandMsg` bridge.
//! - [`WebDriverTestHarness`]: Headless test harness starting `NeowebApp` with `WebDriverServer` on a specific or OS-assigned port (`0`).

use crate::app::NeowebApp;
use crate::engine::servo::host::ServoHost;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use servo::WebDriverCommandMsg;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// Default TCP port for `--remote-debugging-port` matching Chromium and Firefox WebDriver BiDi (`9222`).
pub const DEFAULT_REMOTE_DEBUGGING_PORT: u16 = 9222;

/// Default TCP port for `--webdriver` matching Servo (`7000`).
pub const DEFAULT_WEBDRIVER_PORT: u16 = 7000;

/// W3C WebDriver element identifier key (`element-6066-11e4-a52e-4f735466cecf`).
pub const ELEMENT_KEY: &str = "element-6066-11e4-a52e-4f735466cecf";

/// RFC 6455 WebSocket handshake GUID.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub struct ServoWebDriverClient;

impl ServoWebDriverClient {
    /// Executes a WebDriver command message directly against the running Constellation.
    pub fn execute(command: WebDriverCommandMsg) -> Result<(), String> {
        ServoHost::with(|host| {
            info!("Executing WebDriver command via Constellation proxy");
            host.servo.execute_webdriver_command(command);
        })
        .ok_or_else(|| "ServoHost not initialized".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDriverErrorResponse {
    pub status_code: u16,
    pub error: String,
    pub message: String,
}

impl WebDriverErrorResponse {
    pub fn new(status_code: u16, error: &str, message: impl Into<String>) -> Self {
        Self {
            status_code,
            error: error.to_string(),
            message: message.into(),
        }
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::new(400, "invalid argument", msg)
    }

    pub fn invalid_session_id(msg: impl Into<String>) -> Self {
        Self::new(404, "invalid session id", msg)
    }

    pub fn no_such_element(msg: impl Into<String>) -> Self {
        Self::new(404, "no such element", msg)
    }

    pub fn no_such_node(msg: impl Into<String>) -> Self {
        Self::new(404, "no such node", msg)
    }

    pub fn no_such_window(msg: impl Into<String>) -> Self {
        Self::new(404, "no such window", msg)
    }

    pub fn no_such_frame(msg: impl Into<String>) -> Self {
        Self::new(404, "no such frame", msg)
    }

    pub fn unknown_command(msg: impl Into<String>) -> Self {
        Self::new(404, "unknown command", msg)
    }

    pub fn unknown_error(msg: impl Into<String>) -> Self {
        Self::new(500, "unknown error", msg)
    }
}

#[derive(Debug, Clone)]
pub enum WebDriverAppRequest {
    GetUrl,
    Navigate(String),
    GetTitle,
    Back,
    Forward,
    Refresh,
    GetSource,
    ExecuteScript {
        script: String,
        args: Vec<Value>,
    },
    GetWindowHandle,
    GetWindowHandles,
    NewWindow {
        window_type: String,
    },
    CloseWindow,
    SwitchToWindow(String),
    GetWindowRect,
    SetWindowRect {
        width: Option<u32>,
        height: Option<u32>,
    },
    TakeScreenshot,
    FindElement {
        using: String,
        value: String,
    },
    FindElements {
        using: String,
        value: String,
    },
    ElementClick(String),
    ElementText(String),
    SendKey(String),
    GetMode,
    GetBrowsingContextTree {
        root: Option<String>,
    },
    LocateNodes {
        context: Option<String>,
        locator_type: String,
        value: String,
    },
    PerformActions {
        actions: Vec<Value>,
    },
}

pub type WebDriverReplyTx = tokio::sync::oneshot::Sender<Result<Value, WebDriverErrorResponse>>;

#[derive(Clone)]
pub struct WebDriverBridgeMessage(
    pub Arc<Mutex<Option<(WebDriverAppRequest, WebDriverReplyTx)>>>,
);

impl std::fmt::Debug for WebDriverBridgeMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebDriverBridgeMessage").finish()
    }
}

type BridgeSender = tokio::sync::mpsc::UnboundedSender<WebDriverBridgeMessage>;

static WEBDRIVER_BRIDGE_TX: LazyLock<Arc<Mutex<Option<BridgeSender>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

/// Registers the channel sender used by `NeowebApp::subscription` to receive WebDriver BiDi requests.
pub fn register_webdriver_bridge(tx: BridgeSender) {
    if let Ok(mut guard) = WEBDRIVER_BRIDGE_TX.lock() {
        *guard = Some(tx);
    }
}

fn dispatch_to_global_bridge(req: WebDriverAppRequest) -> Result<Value, WebDriverErrorResponse> {
    let tx_opt = WEBDRIVER_BRIDGE_TX.lock().ok().and_then(|g| g.clone());
    let Some(tx) = tx_opt else {
        return Err(WebDriverErrorResponse::unknown_error(
            "NeowebApp WebDriver bridge is not connected",
        ));
    };

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let msg = WebDriverBridgeMessage(Arc::new(Mutex::new(Some((req, reply_tx)))));
    if tx.send(msg).is_err() {
        return Err(WebDriverErrorResponse::unknown_error(
            "Failed to send command to NeowebApp event loop",
        ));
    }

    reply_rx.blocking_recv().unwrap_or_else(|_| {
        Err(WebDriverErrorResponse::unknown_error(
            "NeowebApp dropped WebDriver reply channel",
        ))
    })
}

/// W3C WebDriver BiDi server (WebSocket + HTTP) listening on a local TCP port.
pub struct WebDriverServer {
    addr: SocketAddr,
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl WebDriverServer {
    /// Starts the WebDriver BiDi server on `127.0.0.1:<port>` dispatching to the global `NeowebApp` bridge.
    /// Pass `port = 0` to bind an OS-assigned ephemeral port.
    pub fn start(port: u16) -> std::io::Result<Self> {
        Self::start_with_dispatcher(port, Arc::new(dispatch_to_global_bridge))
    }

    /// Starts the WebDriver BiDi server on `127.0.0.1:<port>` with a custom request dispatcher.
    pub fn start_with_dispatcher<F>(port: u16, dispatcher: Arc<F>) -> std::io::Result<Self>
    where
        F: Fn(WebDriverAppRequest) -> Result<Value, WebDriverErrorResponse> + Send + Sync + 'static,
    {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();

        info!(
            "WebDriver BiDi listening on ws://{}/session (HTTP http://{})",
            addr, addr
        );

        let join_handle = std::thread::Builder::new()
            .name(format!("neoweb-webdriver-bidi-{}", addr.port()))
            .spawn(move || {
                let active_session: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
                let session_counter = Arc::new(AtomicUsize::new(1));

                while !stop_clone.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _peer)) => {
                            let _ = stream.set_nonblocking(false);
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                            let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                            let active_session_conn = active_session.clone();
                            let session_counter_conn = session_counter.clone();
                            let dispatcher_conn = dispatcher.clone();
                            let stop_conn = stop_clone.clone();
                            std::thread::spawn(move || {
                                handle_incoming_connection(
                                    stream,
                                    addr,
                                    &active_session_conn,
                                    &session_counter_conn,
                                    &stop_conn,
                                    dispatcher_conn.as_ref(),
                                );
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) => {
                            if !stop_clone.load(Ordering::SeqCst) {
                                warn!("WebDriver BiDi TCP accept error: {}", e);
                            }
                            break;
                        }
                    }
                }
            })?;

        Ok(Self {
            addr,
            stop_flag,
            join_handle: Some(join_handle),
        })
    }

    /// Returns the bound socket address (`127.0.0.1:<port>`).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the bound TCP port.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Returns the base HTTP URL (`http://127.0.0.1:<port>`).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Returns the W3C WebDriver BiDi WebSocket URL (`ws://127.0.0.1:<port>/session`).
    pub fn ws_url(&self) -> String {
        format!("ws://{}/session", self.addr)
    }

    /// Stops the WebDriver BiDi server cleanly and joins the listener thread.
    pub fn shutdown(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(50));
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WebDriverServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Synchronous W3C WebDriver BiDi WebSocket client for testing Neoweb over `ws://127.0.0.1:<port>/session`.
pub struct WebDriverBiDiClient {
    stream: TcpStream,
    next_id: AtomicU64,
    buffered_events: Vec<Value>,
}

impl WebDriverBiDiClient {
    /// Connects to the WebDriver BiDi server at `addr` and performs the RFC 6455 WebSocket upgrade handshake.
    pub fn connect(addr: SocketAddr) -> std::io::Result<Self> {
        Self::connect_path(addr, "/session")
    }

    /// Connects to a specific path (e.g. `/session` or `/session/<id>`) on `addr`.
    pub fn connect_path(addr: SocketAddr, path: &str) -> std::io::Result<Self> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        let sec_key = base64::engine::general_purpose::STANDARD.encode(b"neoweb-bidi-test");
        let handshake = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {sec_key}\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(handshake.as_bytes())?;
        stream.flush()?;

        // Read HTTP 101 response header
        let mut buf = Vec::with_capacity(512);
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            let n = stream.read(&mut byte)?;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Connection closed during WebSocket handshake",
                ));
            }
            buf.push(byte[0]);
            if buf.len() > 8192 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "WebSocket handshake headers too large",
                ));
            }
        }

        let resp_str = String::from_utf8_lossy(&buf);
        if !resp_str.starts_with("HTTP/1.1 101") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("WebSocket upgrade failed: {resp_str}"),
            ));
        }

        Ok(Self {
            stream,
            next_id: AtomicU64::new(1),
            buffered_events: Vec::new(),
        })
    }

    /// Sends a W3C WebDriver BiDi command (`{ "id", "method", "params" }`) and waits for the matching response.
    /// Any asynchronous BiDi event frames (`"type": "event"`) received before the command response are buffered
    /// and can be retrieved via [`Self::take_events`].
    pub fn send_command(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cmd = json!({
            "id": id,
            "method": method,
            "params": params
        });
        let payload = serde_json::to_string(&cmd).expect("Valid BiDi command JSON");
        write_ws_masked_text_frame(&mut self.stream, &payload)
            .expect("Failed to write WebSocket BiDi command frame");

        loop {
            let Some((opcode, data)) = read_ws_frame(&mut self.stream) else {
                panic!("WebSocket closed while awaiting BiDi response for {method}");
            };
            if opcode == 0x8 {
                panic!("WebSocket server sent Close frame while awaiting {method}");
            }
            if opcode == 0x9 {
                let _ = write_ws_frame(&mut self.stream, 0xA, &data, true);
                continue;
            }
            if opcode == 0x1
                && let Ok(msg) = serde_json::from_slice::<Value>(&data)
            {
                if msg.get("type").and_then(|v| v.as_str()) == Some("event") {
                    self.buffered_events.push(msg);
                    continue;
                }
                if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
                    return msg;
                }
            }
        }
    }

    /// Returns all BiDi event payloads (`{ "type": "event", "method": ..., "params": ... }`) received so far.
    pub fn take_events(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.buffered_events)
    }

    /// Sends a WebSocket Close frame.
    pub fn close(&mut self) {
        let _ = write_ws_frame(&mut self.stream, 0x8, &[], true);
    }
}

impl Drop for WebDriverBiDiClient {
    fn drop(&mut self) {
        self.close();
    }
}

/// Self-contained test harness that runs `NeowebApp` headlessly with `WebDriverServer` (BiDi + HTTP) listening on a port.
pub struct WebDriverTestHarness {
    server: WebDriverServer,
    app_stop: Arc<AtomicBool>,
    app_thread: Option<std::thread::JoinHandle<()>>,
    temp_dir: std::path::PathBuf,
}

impl WebDriverTestHarness {
    /// Starts a headless `NeowebApp` instance and a `WebDriverServer` listening on `127.0.0.1:<port>`.
    /// Pass `port = 0` to let the OS assign an available port automatically.
    pub fn start(port: u16) -> std::io::Result<Self> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<WebDriverBridgeMessage>();
        let temp_dir = std::env::temp_dir().join(format!(
            "neoweb_webdriver_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let temp_dir_clone = temp_dir.clone();

        let app_stop = Arc::new(AtomicBool::new(false));
        let app_stop_clone = app_stop.clone();

        let app_thread = std::thread::Builder::new()
            .name("neoweb-headless-app".to_string())
            .spawn(move || {
                let (mut app, _task) = NeowebApp::with_config_dir(Some(temp_dir_clone));
                while !app_stop_clone.load(Ordering::SeqCst) {
                    while let Ok(bridge_msg) = rx.try_recv() {
                        let pair_opt = bridge_msg.0.lock().ok().and_then(|mut g| g.take());
                        if let Some((req, reply_tx)) = pair_opt {
                            let res = app.handle_webdriver_request(req);
                            let _ = reply_tx.send(res);
                        }
                    }
                    let _ = app.update(crate::app::Message::Tick);
                    std::thread::sleep(Duration::from_millis(5));
                }
            })?;

        let server = WebDriverServer::start_with_dispatcher(
            port,
            Arc::new(move |req| {
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                let msg = WebDriverBridgeMessage(Arc::new(Mutex::new(Some((req, reply_tx)))));
                if tx.send(msg).is_err() {
                    return Err(WebDriverErrorResponse::unknown_error(
                        "Headless NeowebApp exited",
                    ));
                }
                reply_rx.blocking_recv().unwrap_or_else(|_| {
                    Err(WebDriverErrorResponse::unknown_error(
                        "Headless NeowebApp dropped reply channel",
                    ))
                })
            }),
        )?;

        Ok(Self {
            server,
            app_stop,
            app_thread: Some(app_thread),
            temp_dir,
        })
    }

    pub fn port(&self) -> u16 {
        self.server.port()
    }

    pub fn addr(&self) -> SocketAddr {
        self.server.addr()
    }

    pub fn url(&self) -> String {
        self.server.url()
    }

    pub fn endpoint_url(&self) -> String {
        self.server.url()
    }

    pub fn ws_url(&self) -> String {
        self.server.ws_url()
    }

    /// Opens a W3C WebDriver BiDi WebSocket client connection to this harness.
    pub fn connect_bidi(&self) -> std::io::Result<WebDriverBiDiClient> {
        WebDriverBiDiClient::connect(self.addr())
    }

    /// Sends a synchronous HTTP/1.1 request to the running WebDriver server and parses the JSON response.
    pub fn request(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut stream =
            TcpStream::connect(self.addr()).expect("Failed to connect to WebDriver server");
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

        let body_str = body
            .map(|v| serde_json::to_string(&v).unwrap_or_default())
            .unwrap_or_default();
        let req_str = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_str}",
            self.addr(),
            body_str.len()
        );
        stream
            .write_all(req_str.as_bytes())
            .expect("Failed to write HTTP request");
        let _ = stream.flush();

        let mut resp_bytes = Vec::new();
        stream
            .read_to_end(&mut resp_bytes)
            .expect("Failed to read HTTP response");
        let resp_str = String::from_utf8_lossy(&resp_bytes);
        let (headers, json_body) = resp_str.split_once("\r\n\r\n").unwrap_or((&resp_str, "{}"));
        let status_code = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .unwrap_or(500);
        let parsed = serde_json::from_str(json_body).unwrap_or(Value::Null);
        (status_code, parsed)
    }

    pub fn get(&self, path: &str) -> Value {
        self.request("GET", path, None).1
    }

    pub fn post(&self, path: &str, body: Value) -> Value {
        self.request("POST", path, Some(body)).1
    }

    pub fn delete(&self, path: &str) -> Value {
        self.request("DELETE", path, None).1
    }

    pub fn shutdown(&mut self) {
        self.server.shutdown();
        self.app_stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.app_thread.take() {
            let _ = handle.join();
        }
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

impl Drop for WebDriverTestHarness {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum ParsedInitialRequest {
    WebSocketUpgrade {
        sec_websocket_key: String,
    },
    Http {
        method: String,
        path: String,
        body: String,
    },
}

fn handle_incoming_connection<F>(
    mut stream: TcpStream,
    server_addr: SocketAddr,
    active_session: &Arc<Mutex<Option<String>>>,
    session_counter: &Arc<AtomicUsize>,
    stop_flag: &Arc<AtomicBool>,
    dispatcher: &F,
) where
    F: Fn(WebDriverAppRequest) -> Result<Value, WebDriverErrorResponse>,
{
    let Some(req) = read_initial_request(&mut stream) else {
        return;
    };

    match req {
        ParsedInitialRequest::WebSocketUpgrade { sec_websocket_key } => {
            let accept_key = compute_websocket_accept(&sec_websocket_key);
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                 Upgrade: websocket\r\n\
                 Connection: Upgrade\r\n\
                 Sec-WebSocket-Accept: {accept_key}\r\n\r\n"
            );
            if stream.write_all(response.as_bytes()).is_err() || stream.flush().is_err() {
                return;
            }
            handle_bidi_websocket_session(
                stream,
                server_addr,
                active_session,
                session_counter,
                stop_flag,
                dispatcher,
            );
        }
        ParsedInitialRequest::Http { method, path, body } => {
            if method == "GET" && (path == "/json/version" || path == "/json") {
                let version_json = json!({
                    "Browser": format!("Neoweb/{}", env!("CARGO_PKG_VERSION")),
                    "Protocol-Version": "1.3",
                    "User-Agent": "Neoweb (Servo; Linux Wayland)",
                    "webSocketDebuggerUrl": format!("ws://{}/session", server_addr)
                });
                write_http_json_response(&mut stream, 200, &version_json);
                return;
            }

            let (status_code, response_json) = match route_webdriver_request(
                &method,
                &path,
                &body,
                server_addr,
                active_session,
                session_counter,
                dispatcher,
            ) {
                Ok(val) => (200, json!({ "value": val })),
                Err(err) => (
                    err.status_code,
                    json!({
                        "value": {
                            "error": err.error,
                            "message": err.message,
                            "stacktrace": ""
                        }
                    }),
                ),
            };
            write_http_json_response(&mut stream, status_code, &response_json);
        }
    }
}

fn write_http_json_response(stream: &mut TcpStream, status_code: u16, response_json: &Value) {
    let payload = serde_json::to_string(response_json).unwrap_or_else(|_| "{}".to_string());
    let status_text = match status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };

    let http_response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n{}",
        status_code,
        status_text,
        payload.len(),
        payload
    );
    let _ = stream.write_all(http_response.as_bytes());
    let _ = stream.flush();
}

fn read_initial_request(stream: &mut TcpStream) -> Option<ParsedInitialRequest> {
    let mut buf = Vec::with_capacity(4096);
    let mut temp = [0u8; 1024];
    let mut header_end = None;

    while header_end.is_none() {
        let n = stream.read(&mut temp).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&temp[..n]);
        header_end = buf.windows(4).position(|w| w == b"\r\n\r\n");
        if buf.len() > 65536 {
            return None;
        }
    }

    let header_idx = header_end?;
    let header_str = std::str::from_utf8(&buf[..header_idx]).ok()?;
    let mut lines = header_str.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_uppercase();
    let raw_path = parts.next()?;
    let path = raw_path.split('?').next().unwrap_or(raw_path).to_string();

    let mut content_length = 0usize;
    let mut is_ws_upgrade = false;
    let mut sec_ws_key = None;

    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            if key.eq_ignore_ascii_case("content-length") {
                content_length = val.parse::<usize>().unwrap_or(0);
            } else if key.eq_ignore_ascii_case("upgrade") && val.eq_ignore_ascii_case("websocket") {
                is_ws_upgrade = true;
            } else if key.eq_ignore_ascii_case("sec-websocket-key") {
                sec_ws_key = Some(val.to_string());
            }
        }
    }

    if is_ws_upgrade && let Some(sec_websocket_key) = sec_ws_key {
        return Some(ParsedInitialRequest::WebSocketUpgrade { sec_websocket_key });
    }

    let body_start = header_idx + 4;
    while buf.len() < body_start + content_length {
        let n = stream.read(&mut temp).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&temp[..n]);
    }

    let body_end = (body_start + content_length).min(buf.len());
    let body = String::from_utf8_lossy(&buf[body_start..body_end]).to_string();
    Some(ParsedInitialRequest::Http { method, path, body })
}

fn handle_bidi_websocket_session<F>(
    mut stream: TcpStream,
    server_addr: SocketAddr,
    active_session: &Arc<Mutex<Option<String>>>,
    session_counter: &Arc<AtomicUsize>,
    stop_flag: &Arc<AtomicBool>,
    dispatcher: &F,
) where
    F: Fn(WebDriverAppRequest) -> Result<Value, WebDriverErrorResponse>,
{
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let mut subscriptions: HashSet<String> = HashSet::new();
    let nav_counter = AtomicUsize::new(1);

    while !stop_flag.load(Ordering::SeqCst) {
        let Some((opcode, payload)) = read_ws_frame(&mut stream) else {
            continue;
        };

        match opcode {
            0x8 => {
                // Close frame
                let _ = write_ws_frame(&mut stream, 0x8, &[], false);
                break;
            }
            0x9 => {
                // Ping -> Pong
                let _ = write_ws_frame(&mut stream, 0xA, &payload, false);
            }
            0x1 => {
                // Text JSON-RPC BiDi command
                let Ok(cmd) = serde_json::from_slice::<Value>(&payload) else {
                    continue;
                };
                let id = cmd.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let method = cmd.get("method").and_then(|v| v.as_str()).unwrap_or("");
                let params = cmd.get("params").cloned().unwrap_or_else(|| json!({}));

                let mut events_to_emit: Vec<Value> = Vec::new();
                let response = route_bidi_command(
                    id,
                    method,
                    &params,
                    server_addr,
                    active_session,
                    session_counter,
                    &nav_counter,
                    &mut subscriptions,
                    &mut events_to_emit,
                    dispatcher,
                );

                // Emit any subscribed BiDi events first (e.g. navigationStarted, domContentLoaded, load)
                for ev in events_to_emit {
                    if let Ok(ev_str) = serde_json::to_string(&ev) {
                        let _ = write_ws_text_frame(&mut stream, &ev_str);
                    }
                }

                if let Ok(resp_str) = serde_json::to_string(&response)
                    && write_ws_text_frame(&mut stream, &resp_str).is_err()
                {
                    break;
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn route_bidi_command<F>(
    id: u64,
    method: &str,
    params: &Value,
    server_addr: SocketAddr,
    active_session: &Arc<Mutex<Option<String>>>,
    session_counter: &Arc<AtomicUsize>,
    nav_counter: &AtomicUsize,
    subscriptions: &mut HashSet<String>,
    events_to_emit: &mut Vec<Value>,
    dispatcher: &F,
) -> Value
where
    F: Fn(WebDriverAppRequest) -> Result<Value, WebDriverErrorResponse>,
{
    let result: Result<Value, WebDriverErrorResponse> = (|| match method {
        "session.status" => Ok(json!({
            "ready": true,
            "message": "Neoweb W3C WebDriver BiDi server ready"
        })),
        "session.new" => {
            let session_id = format!(
                "neoweb-bidi-session-{}",
                session_counter.fetch_add(1, Ordering::SeqCst)
            );
            if let Ok(mut guard) = active_session.lock() {
                *guard = Some(session_id.clone());
            }
            Ok(json!({
                "sessionId": session_id,
                "capabilities": {
                    "browserName": "neoweb",
                    "browserVersion": env!("CARGO_PKG_VERSION"),
                    "platformName": "linux",
                    "acceptInsecureCerts": false,
                    "setWindowRect": true,
                    "webSocketUrl": format!("ws://{}/session/{}", server_addr, session_id)
                }
            }))
        }
        "session.end" | "browser.close" => {
            if let Ok(mut guard) = active_session.lock() {
                *guard = None;
            }
            subscriptions.clear();
            Ok(json!({}))
        }
        "session.subscribe" => {
            let events = params
                .get("events")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'events' array")
                })?;
            for ev in events {
                if let Some(name) = ev.as_str() {
                    subscriptions.insert(name.to_string());
                }
            }
            Ok(json!({
                "subscription": format!("sub-{}", session_counter.fetch_add(1, Ordering::SeqCst))
            }))
        }
        "session.unsubscribe" => {
            if let Some(events) = params.get("events").and_then(|v| v.as_array()) {
                for ev in events {
                    if let Some(name) = ev.as_str() {
                        subscriptions.remove(name);
                    }
                }
            } else {
                subscriptions.clear();
            }
            Ok(json!({}))
        }
        "browser.getClientWindows" => {
            let rect = dispatcher(WebDriverAppRequest::GetWindowRect)?;
            Ok(json!({
                "clientWindows": [{
                    "clientWindow": "window-1",
                    "active": true,
                    "state": "normal",
                    "x": rect.get("x").cloned().unwrap_or(json!(0)),
                    "y": rect.get("y").cloned().unwrap_or(json!(0)),
                    "width": rect.get("width").cloned().unwrap_or(json!(1280)),
                    "height": rect.get("height").cloned().unwrap_or(json!(784))
                }]
            }))
        }
        "browsingContext.getTree" => {
            let root = params
                .get("root")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            dispatcher(WebDriverAppRequest::GetBrowsingContextTree { root })
        }
        "browsingContext.create" => {
            let window_type = params
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("tab")
                .to_string();
            let res = dispatcher(WebDriverAppRequest::NewWindow { window_type })?;
            let context = res
                .get("handle")
                .and_then(|v| v.as_str())
                .unwrap_or("tab-0")
                .to_string();
            if is_event_subscribed(subscriptions, "browsingContext.contextCreated") {
                events_to_emit.push(json!({
                    "type": "event",
                    "method": "browsingContext.contextCreated",
                    "params": {
                        "context": context,
                        "url": "about:blank",
                        "userContext": "default",
                        "children": null,
                        "parent": null
                    }
                }));
            }
            Ok(json!({ "context": context }))
        }
        "browsingContext.close" => {
            if let Some(ctx) = params.get("context").and_then(|v| v.as_str()) {
                let _ = dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()));
            }
            let _ = dispatcher(WebDriverAppRequest::CloseWindow)?;
            Ok(json!({}))
        }
        "browsingContext.activate" => {
            let ctx = params
                .get("context")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'context' parameter")
                })?;
            dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()))?;
            Ok(json!({}))
        }
        "browsingContext.navigate" => {
            let context = if let Some(ctx) = params.get("context").and_then(|v| v.as_str()) {
                dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()))?;
                ctx.to_string()
            } else {
                dispatcher(WebDriverAppRequest::GetWindowHandle)?
                    .as_str()
                    .unwrap_or("tab-0")
                    .to_string()
            };
            let url = params
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'url' parameter")
                })?;
            let nav_id = format!("nav-{}", nav_counter.fetch_add(1, Ordering::SeqCst));
            let timestamp = current_timestamp_ms();

            if is_event_subscribed(subscriptions, "browsingContext.navigationStarted") {
                events_to_emit.push(json!({
                    "type": "event",
                    "method": "browsingContext.navigationStarted",
                    "params": {
                        "context": context,
                        "navigation": nav_id,
                        "timestamp": timestamp,
                        "url": url
                    }
                }));
            }

            dispatcher(WebDriverAppRequest::Navigate(url.to_string()))?;
            let final_url = dispatcher(WebDriverAppRequest::GetUrl)?
                .as_str()
                .unwrap_or(url)
                .to_string();

            if is_event_subscribed(subscriptions, "browsingContext.domContentLoaded") {
                events_to_emit.push(json!({
                    "type": "event",
                    "method": "browsingContext.domContentLoaded",
                    "params": {
                        "context": context,
                        "navigation": nav_id,
                        "timestamp": current_timestamp_ms(),
                        "url": final_url
                    }
                }));
            }
            if is_event_subscribed(subscriptions, "browsingContext.load") {
                events_to_emit.push(json!({
                    "type": "event",
                    "method": "browsingContext.load",
                    "params": {
                        "context": context,
                        "navigation": nav_id,
                        "timestamp": current_timestamp_ms(),
                        "url": final_url
                    }
                }));
            }

            Ok(json!({
                "navigation": nav_id,
                "url": final_url
            }))
        }
        "browsingContext.reload" => {
            if let Some(ctx) = params.get("context").and_then(|v| v.as_str()) {
                dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()))?;
            }
            dispatcher(WebDriverAppRequest::Refresh)?;
            let url = dispatcher(WebDriverAppRequest::GetUrl)?
                .as_str()
                .unwrap_or("about:blank")
                .to_string();
            let nav_id = format!("nav-{}", nav_counter.fetch_add(1, Ordering::SeqCst));
            Ok(json!({
                "navigation": nav_id,
                "url": url
            }))
        }
        "browsingContext.traverseHistory" => {
            if let Some(ctx) = params.get("context").and_then(|v| v.as_str()) {
                dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()))?;
            }
            let delta = params.get("delta").and_then(|v| v.as_i64()).ok_or_else(|| {
                WebDriverErrorResponse::invalid_argument("Missing 'delta' parameter")
            })?;
            if delta < 0 {
                for _ in 0..delta.unsigned_abs() {
                    dispatcher(WebDriverAppRequest::Back)?;
                }
            } else if delta > 0 {
                for _ in 0..delta {
                    dispatcher(WebDriverAppRequest::Forward)?;
                }
            }
            if is_event_subscribed(subscriptions, "browsingContext.historyUpdated") {
                let url = dispatcher(WebDriverAppRequest::GetUrl)?;
                let context = dispatcher(WebDriverAppRequest::GetWindowHandle)?;
                events_to_emit.push(json!({
                    "type": "event",
                    "method": "browsingContext.historyUpdated",
                    "params": {
                        "context": context,
                        "url": url
                    }
                }));
            }
            Ok(json!({}))
        }
        "browsingContext.setViewport" => {
            if let Some(ctx) = params.get("context").and_then(|v| v.as_str()) {
                dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()))?;
            }
            let viewport = params.get("viewport").unwrap_or(params);
            let width = viewport
                .get("width")
                .and_then(|v| v.as_u64())
                .map(|w| w as u32);
            let height = viewport
                .get("height")
                .and_then(|v| v.as_u64())
                .map(|h| h as u32);
            dispatcher(WebDriverAppRequest::SetWindowRect { width, height })?;
            Ok(json!({}))
        }
        "browsingContext.captureScreenshot" => {
            if let Some(ctx) = params.get("context").and_then(|v| v.as_str()) {
                dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()))?;
            }
            let b64 = dispatcher(WebDriverAppRequest::TakeScreenshot)?;
            Ok(json!({ "data": b64 }))
        }
        "browsingContext.locateNodes" => {
            let context = params
                .get("context")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let locator = params.get("locator").ok_or_else(|| {
                WebDriverErrorResponse::invalid_argument("Missing 'locator' object")
            })?;
            let locator_type = locator
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'locator.type'")
                })?
                .to_string();
            let value = locator
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'locator.value'")
                })?
                .to_string();
            dispatcher(WebDriverAppRequest::LocateNodes {
                context,
                locator_type,
                value,
            })
        }
        "script.evaluate" | "script.callFunction" => {
            let target_ctx = params
                .get("target")
                .and_then(|t| t.get("context").or_else(|| t.get("realm")))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(ref ctx) = target_ctx {
                dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.clone()))?;
            }
            let realm = match target_ctx {
                Some(c) => c,
                None => dispatcher(WebDriverAppRequest::GetWindowHandle)?
                    .as_str()
                    .unwrap_or("tab-0")
                    .to_string(),
            };
            let script = if method == "script.evaluate" {
                params
                    .get("expression")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        WebDriverErrorResponse::invalid_argument("Missing 'expression' parameter")
                    })?
                    .to_string()
            } else {
                let decl = params
                    .get("functionDeclaration")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        WebDriverErrorResponse::invalid_argument(
                            "Missing 'functionDeclaration' parameter",
                        )
                    })?;
                format!("({decl})()")
            };
            let raw_val = dispatcher(WebDriverAppRequest::ExecuteScript {
                script,
                args: Vec::new(),
            })?;
            let remote_val = to_bidi_remote_value(&raw_val);
            Ok(json!({
                "type": "success",
                "realm": realm,
                "result": remote_val
            }))
        }
        "script.getRealms" => {
            let handle = dispatcher(WebDriverAppRequest::GetWindowHandle)?
                .as_str()
                .unwrap_or("tab-0")
                .to_string();
            let url = dispatcher(WebDriverAppRequest::GetUrl)?
                .as_str()
                .unwrap_or("about:blank")
                .to_string();
            Ok(json!({
                "realms": [{
                    "realm": handle,
                    "origin": url,
                    "type": "window",
                    "context": handle
                }]
            }))
        }
        "input.performActions" => {
            if let Some(ctx) = params.get("context").and_then(|v| v.as_str()) {
                dispatcher(WebDriverAppRequest::SwitchToWindow(ctx.to_string()))?;
            }
            let actions = params
                .get("actions")
                .and_then(|v| v.as_array())
                .cloned()
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'actions' array")
                })?;
            dispatcher(WebDriverAppRequest::PerformActions { actions })
        }
        "input.releaseActions" => Ok(json!({})),
        "neoweb.getMode" => {
            let mode = dispatcher(WebDriverAppRequest::GetMode)?;
            Ok(json!({ "mode": mode }))
        }
        "neoweb.sendKey" => {
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| WebDriverErrorResponse::invalid_argument("Missing 'key' field"))?;
            dispatcher(WebDriverAppRequest::SendKey(key.to_string()))
        }
        "neoweb.followHint" => {
            let key = params
                .get("key")
                .or_else(|| params.get("sharedId"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| WebDriverErrorResponse::invalid_argument("Missing 'key' field"))?;
            dispatcher(WebDriverAppRequest::ElementClick(key.to_string()))?;
            let url = dispatcher(WebDriverAppRequest::GetUrl)?;
            let title = dispatcher(WebDriverAppRequest::GetTitle)?;
            Ok(json!({ "url": url, "title": title }))
        }
        _ => Err(WebDriverErrorResponse::unknown_command(format!(
            "Unknown WebDriver BiDi method: {method}"
        ))),
    })();

    match result {
        Ok(res) => json!({
            "type": "success",
            "id": id,
            "result": res
        }),
        Err(err) => json!({
            "type": "error",
            "id": id,
            "error": err.error,
            "message": err.message
        }),
    }
}

fn is_event_subscribed(subscriptions: &HashSet<String>, event_name: &str) -> bool {
    if subscriptions.contains(event_name) {
        return true;
    }
    if let Some((module, _)) = event_name.split_once('.') {
        return subscriptions.contains(module);
    }
    false
}

fn to_bidi_remote_value(val: &Value) -> Value {
    match val {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(b) => json!({ "type": "boolean", "value": b }),
        Value::Number(n) => json!({ "type": "number", "value": n }),
        Value::String(s) => json!({ "type": "string", "value": s }),
        Value::Array(arr) => json!({
            "type": "array",
            "value": arr.iter().map(to_bidi_remote_value).collect::<Vec<_>>()
        }),
        Value::Object(obj) => json!({
            "type": "object",
            "value": obj.iter().map(|(k, v)| json!([k, to_bidi_remote_value(v)])).collect::<Vec<_>>()
        }),
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn route_webdriver_request<F>(
    method: &str,
    path: &str,
    body: &str,
    server_addr: SocketAddr,
    active_session: &Arc<Mutex<Option<String>>>,
    session_counter: &Arc<AtomicUsize>,
    dispatcher: &F,
) -> Result<Value, WebDriverErrorResponse>
where
    F: Fn(WebDriverAppRequest) -> Result<Value, WebDriverErrorResponse>,
{
    if method == "GET" && path == "/status" {
        let ready = active_session
            .lock()
            .map(|g| g.is_none())
            .unwrap_or(true);
        return Ok(json!({
            "ready": ready,
            "message": "Neoweb W3C WebDriver BiDi server ready"
        }));
    }

    if method == "POST" && path == "/session" {
        let mut guard = active_session
            .lock()
            .map_err(|_| WebDriverErrorResponse::unknown_error("Session lock poisoned"))?;
        let id = format!(
            "neoweb-session-{}",
            session_counter.fetch_add(1, Ordering::SeqCst)
        );
        *guard = Some(id.clone());
        return Ok(json!({
            "sessionId": id,
            "capabilities": {
                "browserName": "neoweb",
                "browserVersion": env!("CARGO_PKG_VERSION"),
                "platformName": "linux",
                "acceptInsecureCerts": false,
                "pageLoadStrategy": "normal",
                "setWindowRect": true,
                "webSocketUrl": format!("ws://{}/session/{}", server_addr, id)
            }
        }));
    }

    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segments.len() < 2 || segments[0] != "session" {
        return Err(WebDriverErrorResponse::unknown_command(format!(
            "Unrecognized WebDriver endpoint: {} {}",
            method, path
        )));
    }

    let req_session_id = segments[1];
    {
        let mut guard = active_session
            .lock()
            .map_err(|_| WebDriverErrorResponse::unknown_error("Session lock poisoned"))?;
        match guard.as_deref() {
            Some(active_id) if active_id == req_session_id => {
                if method == "DELETE" && segments.len() == 2 {
                    *guard = None;
                    return Ok(Value::Null);
                }
            }
            _ => {
                return Err(WebDriverErrorResponse::invalid_session_id(format!(
                    "Invalid or expired session ID: {}",
                    req_session_id
                )));
            }
        }
    }

    let sub_route = &segments[2..];
    match (method, sub_route) {
        ("GET", ["url"]) => dispatcher(WebDriverAppRequest::GetUrl),
        ("POST", ["url"]) => {
            let parsed: Value = serde_json::from_str(body)
                .map_err(|e| WebDriverErrorResponse::invalid_argument(e.to_string()))?;
            let url = parsed
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| WebDriverErrorResponse::invalid_argument("Missing 'url' field"))?;
            dispatcher(WebDriverAppRequest::Navigate(url.to_string()))
        }
        ("GET", ["title"]) => dispatcher(WebDriverAppRequest::GetTitle),
        ("POST", ["back"]) => dispatcher(WebDriverAppRequest::Back),
        ("POST", ["forward"]) => dispatcher(WebDriverAppRequest::Forward),
        ("POST", ["refresh"]) => dispatcher(WebDriverAppRequest::Refresh),
        ("GET", ["source"]) => dispatcher(WebDriverAppRequest::GetSource),
        ("POST", ["execute", "sync"]) => {
            let parsed: Value = serde_json::from_str(body)
                .map_err(|e| WebDriverErrorResponse::invalid_argument(e.to_string()))?;
            let script = parsed
                .get("script")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'script' field")
                })?
                .to_string();
            let args = parsed
                .get("args")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            dispatcher(WebDriverAppRequest::ExecuteScript { script, args })
        }
        ("GET", ["window"]) => dispatcher(WebDriverAppRequest::GetWindowHandle),
        ("GET", ["window", "handles"]) => dispatcher(WebDriverAppRequest::GetWindowHandles),
        ("POST", ["window", "new"]) => {
            let parsed: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
            let window_type = parsed
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("tab")
                .to_string();
            dispatcher(WebDriverAppRequest::NewWindow { window_type })
        }
        ("DELETE", ["window"]) => dispatcher(WebDriverAppRequest::CloseWindow),
        ("POST", ["window"]) => {
            let parsed: Value = serde_json::from_str(body)
                .map_err(|e| WebDriverErrorResponse::invalid_argument(e.to_string()))?;
            let handle = parsed
                .get("handle")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'handle' field")
                })?;
            dispatcher(WebDriverAppRequest::SwitchToWindow(handle.to_string()))
        }
        ("GET", ["window", "rect"]) => dispatcher(WebDriverAppRequest::GetWindowRect),
        ("POST", ["window", "rect"]) => {
            let parsed: Value = serde_json::from_str(body)
                .map_err(|e| WebDriverErrorResponse::invalid_argument(e.to_string()))?;
            let width = parsed
                .get("width")
                .and_then(|v| v.as_u64())
                .map(|w| w as u32);
            let height = parsed
                .get("height")
                .and_then(|v| v.as_u64())
                .map(|h| h as u32);
            dispatcher(WebDriverAppRequest::SetWindowRect { width, height })
        }
        ("GET", ["screenshot"]) => dispatcher(WebDriverAppRequest::TakeScreenshot),
        ("POST", ["element"]) => {
            let parsed: Value = serde_json::from_str(body)
                .map_err(|e| WebDriverErrorResponse::invalid_argument(e.to_string()))?;
            let using = parsed
                .get("using")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'using' field")
                })?
                .to_string();
            let value = parsed
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'value' field")
                })?
                .to_string();
            dispatcher(WebDriverAppRequest::FindElement { using, value })
        }
        ("POST", ["elements"]) => {
            let parsed: Value = serde_json::from_str(body)
                .map_err(|e| WebDriverErrorResponse::invalid_argument(e.to_string()))?;
            let using = parsed
                .get("using")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'using' field")
                })?
                .to_string();
            let value = parsed
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    WebDriverErrorResponse::invalid_argument("Missing 'value' field")
                })?
                .to_string();
            dispatcher(WebDriverAppRequest::FindElements { using, value })
        }
        ("POST", ["element", element_id, "click"]) => {
            dispatcher(WebDriverAppRequest::ElementClick((*element_id).to_string()))
        }
        ("GET", ["element", element_id, "text"]) => {
            dispatcher(WebDriverAppRequest::ElementText((*element_id).to_string()))
        }
        ("POST", ["neoweb", "key"]) => {
            let parsed: Value = serde_json::from_str(body)
                .map_err(|e| WebDriverErrorResponse::invalid_argument(e.to_string()))?;
            let key = parsed
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| WebDriverErrorResponse::invalid_argument("Missing 'key' field"))?;
            dispatcher(WebDriverAppRequest::SendKey(key.to_string()))
        }
        ("GET", ["neoweb", "mode"]) => dispatcher(WebDriverAppRequest::GetMode),
        _ => Err(WebDriverErrorResponse::unknown_command(format!(
            "Unsupported WebDriver command: {} {}",
            method, path
        ))),
    }
}

/// Computes `Sec-WebSocket-Accept` (`Base64(SHA-1(key + WS_GUID))`) per RFC 6455 Section 4.2.2.
fn compute_websocket_accept(sec_websocket_key: &str) -> String {
    let mut input = Vec::with_capacity(sec_websocket_key.len() + WS_GUID.len());
    input.extend_from_slice(sec_websocket_key.trim().as_bytes());
    input.extend_from_slice(WS_GUID.as_bytes());
    let digest = sha1_digest(&input);
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// Pure-Rust FIPS 180-4 SHA-1 implementation for RFC 6455 WebSocket handshake.
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

/// Reads a single RFC 6455 WebSocket frame, returning `(opcode, payload)`.
fn read_ws_frame(stream: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).ok()?;
    let opcode = header[0] & 0x0F;
    let masked = (header[1] & 0x80) != 0;
    let mut payload_len = (header[1] & 0x7F) as usize;

    if payload_len == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext).ok()?;
        payload_len = u16::from_be_bytes(ext) as usize;
    } else if payload_len == 127 {
        let mut ext = [0u8; 8];
        stream.read_exact(&mut ext).ok()?;
        payload_len = u64::from_be_bytes(ext) as usize;
    }

    let mut mask_key = [0u8; 4];
    if masked {
        stream.read_exact(&mut mask_key).ok()?;
    }

    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).ok()?;
    if masked {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask_key[i % 4];
        }
    }

    Some((opcode, payload))
}

fn write_ws_text_frame(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    write_ws_frame(stream, 0x1, text.as_bytes(), false)
}

fn write_ws_masked_text_frame(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    write_ws_frame(stream, 0x1, text.as_bytes(), true)
}

fn write_ws_frame(
    stream: &mut TcpStream,
    opcode: u8,
    payload: &[u8],
    masked: bool,
) -> std::io::Result<()> {
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | (opcode & 0x0F)); // FIN = 1
    let mask_bit = if masked { 0x80 } else { 0x00 };
    if payload.len() < 126 {
        frame.push(mask_bit | (payload.len() as u8));
    } else if payload.len() <= u16::MAX as usize {
        frame.push(mask_bit | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        frame.push(mask_bit | 127);
        frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }

    if masked {
        let mask_key: [u8; 4] = [0x37, 0xFA, 0x21, 0x3D];
        frame.extend_from_slice(&mask_key);
        for (i, &b) in payload.iter().enumerate() {
            frame.push(b ^ mask_key[i % 4]);
        }
    } else {
        frame.extend_from_slice(payload);
    }

    stream.write_all(&frame)?;
    stream.flush()
}
