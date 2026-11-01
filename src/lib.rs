pub mod app;
pub mod bookmarks;
pub mod download;
pub mod engine;
pub mod keymap;
pub mod lua;
pub mod storage;
pub mod tabs;
pub mod ui;

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

pub fn log_file_path() -> PathBuf {
    let dirs = directories::ProjectDirs::from("org", "neoweb", "neoweb");
    let state_dir = dirs
        .as_ref()
        .and_then(|d| d.state_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| {
            dirs.as_ref()
                .map(|d| d.data_local_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        });
    state_dir.join("neoweb.log")
}

pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,neoweb=debug"));

    let stderr_layer = fmt::layer().with_writer(std::io::stderr).with_ansi(true);

    let log_path = log_file_path();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let file_layer = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => Some(
            fmt::layer()
                .with_writer(Mutex::new(file))
                .with_ansi(false),
        ),
        Err(e) => {
            eprintln!("Warning: failed to open log file at {log_path:?}: {e}");
            None
        }
    };

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(file_layer)
        .try_init();

    info!("Neoweb logging initialized. Log file: {:?}", log_path);
}

/// Command-line and environment configuration for starting Neoweb.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliArgs {
    /// Content process token when spawned as a Servo multiprocess worker (`--content-process <TOKEN>`).
    pub content_process_token: Option<String>,
    /// Optional TCP port on `127.0.0.1` for the W3C WebDriver BiDi server
    /// (`--remote-debugging-port [PORT]`, `--webdriver [PORT]`, `--port <PORT>`, `-p <PORT>`).
    pub remote_debugging_port: Option<u16>,
    /// Optional initial URL to open on startup.
    pub initial_url: Option<String>,
    /// Whether single-process mode is requested (`--single-process` or `NEOWEB_SINGLE_PROCESS=1`).
    /// By default, multiprocess content isolation is enabled.
    pub single_process: bool,
}

impl CliArgs {
    /// Returns the configured WebDriver BiDi / remote debugging TCP port, if enabled.
    pub fn webdriver_port(&self) -> Option<u16> {
        self.remote_debugging_port
    }

    /// Parse CLI arguments from `std::env::args()` and fallback environment variables
    /// (`NEOWEB_REMOTE_DEBUGGING_PORT`, `NEOWEB_WEBDRIVER_PORT`, `NEOWEB_SINGLE_PROCESS`).
    pub fn from_env() -> Self {
        let env_port = std::env::var("NEOWEB_REMOTE_DEBUGGING_PORT")
            .or_else(|_| std::env::var("NEOWEB_WEBDRIVER_PORT"))
            .ok()
            .and_then(|v| v.trim().parse::<u16>().ok());
        Self::parse_from_with_env(std::env::args().skip(1), env_port)
    }

    /// Parse an iterator of CLI arguments (excluding `argv[0]`).
    pub fn parse_from<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::parse_from_with_env(args, None)
    }

    /// Parse an iterator of CLI arguments with an optional fallback environment port value.
    pub fn parse_from_with_env<I, S>(args: I, env_port: Option<u16>) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let env_single = std::env::var("NEOWEB_SINGLE_PROCESS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let mut parsed = Self {
            content_process_token: None,
            remote_debugging_port: env_port,
            initial_url: None,
            single_process: env_single,
        };

        let items: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        let mut idx = 0;
        while idx < items.len() {
            let arg = &items[idx];
            if arg == "--content-process" {
                if let Some(token) = items.get(idx + 1) {
                    parsed.content_process_token = Some(token.clone());
                    idx += 2;
                    continue;
                }
            } else if let Some(token) = arg.strip_prefix("--content-process=") {
                parsed.content_process_token = Some(token.to_string());
                idx += 1;
                continue;
            } else if arg == "--remote-debugging-port" {
                // Chromium / Firefox WebDriver BiDi flag (defaults to 9222 when bare)
                if let Some(next) = items.get(idx + 1)
                    && let Ok(port) = next.parse::<u16>()
                {
                    parsed.remote_debugging_port = Some(port);
                    idx += 2;
                    continue;
                }
                parsed.remote_debugging_port = Some(engine::servo::DEFAULT_REMOTE_DEBUGGING_PORT);
                idx += 1;
                continue;
            } else if let Some(val) = arg.strip_prefix("--remote-debugging-port=") {
                if let Ok(port) = val.parse::<u16>() {
                    parsed.remote_debugging_port = Some(port);
                } else {
                    parsed.remote_debugging_port =
                        Some(engine::servo::DEFAULT_REMOTE_DEBUGGING_PORT);
                }
                idx += 1;
                continue;
            } else if arg == "--webdriver" || arg == "--webdriver-port" {
                // Servo flag (defaults to 7000 when bare)
                if let Some(next) = items.get(idx + 1)
                    && let Ok(port) = next.parse::<u16>()
                {
                    parsed.remote_debugging_port = Some(port);
                    idx += 2;
                    continue;
                }
                parsed.remote_debugging_port = Some(engine::servo::DEFAULT_WEBDRIVER_PORT);
                idx += 1;
                continue;
            } else if let Some(val) = arg
                .strip_prefix("--webdriver=")
                .or_else(|| arg.strip_prefix("--webdriver-port="))
            {
                if let Ok(port) = val.parse::<u16>() {
                    parsed.remote_debugging_port = Some(port);
                } else {
                    parsed.remote_debugging_port = Some(engine::servo::DEFAULT_WEBDRIVER_PORT);
                }
                idx += 1;
                continue;
            } else if arg == "--port" || arg == "-p" {
                // GeckoDriver / ChromeDriver / SafariDriver flag
                if let Some(next) = items.get(idx + 1)
                    && let Ok(port) = next.parse::<u16>()
                {
                    parsed.remote_debugging_port = Some(port);
                    idx += 2;
                    continue;
                }
            } else if let Some(val) = arg.strip_prefix("--port=") {
                if let Ok(port) = val.parse::<u16>() {
                    parsed.remote_debugging_port = Some(port);
                }
                idx += 1;
                continue;
            } else if arg == "--single-process" {
                parsed.single_process = true;
                idx += 1;
                continue;
            } else if !arg.starts_with('-') && parsed.initial_url.is_none() {
                parsed.initial_url = Some(arg.clone());
            }
            idx += 1;
        }

        parsed
    }
}

pub fn run() -> iced::Result {
    run_with_args(CliArgs::from_env())
}

pub fn run_with_args(args: CliArgs) -> iced::Result {
    init_logging();

    // Multiprocess content isolation is enabled by default unless --single-process is requested
    engine::servo::set_process_mode(!args.single_process);

    let _webdriver_server = match args.remote_debugging_port {
        Some(port) => match engine::servo::WebDriverServer::start(port) {
            Ok(server) => {
                info!(
                    "WebDriver BiDi listening on {} (HTTP {})",
                    server.ws_url(),
                    server.url()
                );
                eprintln!("WebDriver BiDi listening on {}", server.ws_url());
                Some(server)
            }
            Err(e) => {
                tracing::error!(
                    "Failed to start W3C WebDriver BiDi server on 127.0.0.1:{}: {}",
                    port,
                    e
                );
                None
            }
        },
        None => None,
    };

    let initial_url = args.initial_url.clone();
    iced::application(
        move || app::NeowebApp::new_with_url(initial_url.as_deref()),
        app::NeowebApp::update,
        app::NeowebApp::view,
    )
    .title(app::NeowebApp::title)
    .subscription(app::NeowebApp::subscription)
    .theme(app::NeowebApp::theme)
    .window(iced::window::Settings {
        size: iced::Size::new(1280.0, 840.0),
        ..Default::default()
    })
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_file_path_resolution() {
        let path = log_file_path();
        assert!(path.ends_with("neoweb/neoweb.log"));
    }

    #[test]
    fn test_cli_args_remote_debugging_and_webdriver_port_variants() {
        // Chromium / Firefox WebDriver BiDi standard: `--remote-debugging-port=<PORT>`
        let args = CliArgs::parse_from(["--remote-debugging-port=9222", "about:help"]);
        assert_eq!(args.remote_debugging_port, Some(9222));
        assert_eq!(args.webdriver_port(), Some(9222));
        assert_eq!(args.initial_url.as_deref(), Some("about:help"));

        // Chromium / Firefox WebDriver BiDi space-separated: `--remote-debugging-port <PORT>`
        let args = CliArgs::parse_from(["--remote-debugging-port", "9333"]);
        assert_eq!(args.remote_debugging_port, Some(9333));

        // Ephemeral port 0: `--remote-debugging-port=0`
        let args = CliArgs::parse_from(["--remote-debugging-port=0"]);
        assert_eq!(args.remote_debugging_port, Some(0));

        // Bare `--remote-debugging-port` defaults to 9222
        let args = CliArgs::parse_from(["--remote-debugging-port", "about:neoweb"]);
        assert_eq!(
            args.remote_debugging_port,
            Some(engine::servo::DEFAULT_REMOTE_DEBUGGING_PORT)
        );
        assert_eq!(args.initial_url.as_deref(), Some("about:neoweb"));

        // GeckoDriver / ChromeDriver / SafariDriver standard: `--port <PORT>` and `-p <PORT>`
        let args = CliArgs::parse_from(["--port", "4444"]);
        assert_eq!(args.remote_debugging_port, Some(4444));
        let args = CliArgs::parse_from(["-p", "9515"]);
        assert_eq!(args.remote_debugging_port, Some(9515));

        // Servo standard: `--webdriver <PORT>` and `--webdriver=<PORT>`
        let args = CliArgs::parse_from(["--webdriver", "7444", "about:help"]);
        assert_eq!(args.remote_debugging_port, Some(7444));
        assert_eq!(args.initial_url.as_deref(), Some("about:help"));

        let args = CliArgs::parse_from(["--webdriver=8123"]);
        assert_eq!(args.remote_debugging_port, Some(8123));

        // Bare `--webdriver` flag defaults to 7000
        let args = CliArgs::parse_from(["--webdriver", "about:neoweb"]);
        assert_eq!(
            args.remote_debugging_port,
            Some(engine::servo::DEFAULT_WEBDRIVER_PORT)
        );
        assert_eq!(args.initial_url.as_deref(), Some("about:neoweb"));

        // Environment fallback (`NEOWEB_REMOTE_DEBUGGING_PORT` / `NEOWEB_WEBDRIVER_PORT`)
        let args = CliArgs::parse_from_with_env(["about:blank"], Some(9229));
        assert_eq!(args.remote_debugging_port, Some(9229));
        assert_eq!(args.initial_url.as_deref(), Some("about:blank"));

        // CLI flag overrides environment fallback
        let args =
            CliArgs::parse_from_with_env(["--remote-debugging-port", "9444"], Some(9229));
        assert_eq!(args.remote_debugging_port, Some(9444));

        // Content process token parsing
        let args = CliArgs::parse_from(["--content-process", "ipc-token-123"]);
        assert_eq!(
            args.content_process_token.as_deref(),
            Some("ipc-token-123")
        );

        // Process isolation flags
        // Process isolation: default is multiprocess (single_process = false)
        let args_default = CliArgs::parse_from(["about:help"]);
        assert!(!args_default.single_process);
        assert_eq!(args_default.initial_url.as_deref(), Some("about:help"));

        // Explicit --single-process flag
        let args_single = CliArgs::parse_from(["--single-process", "about:help"]);
        assert!(args_single.single_process);
        assert_eq!(args_single.initial_url.as_deref(), Some("about:help"));
    }
}
