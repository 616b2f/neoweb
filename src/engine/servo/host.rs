//! Global Servo constellation host singleton, event loop waker, and preferences.

use raw_window_handle::{DisplayHandle, WindowHandle};
use servo::{
    EventLoopWaker, Opts, Preferences, RenderingContext, Servo, ServoBuilder,
    SoftwareRenderingContext, WindowRenderingContext,
};
use std::rc::Rc;
use std::sync::Arc;
use tracing::{info, warn};

pub struct ServoEventLoopWaker {
    sender: Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<()>>>>,
}

impl ServoEventLoopWaker {
    pub fn new(sender: Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<()>>>>) -> Self {
        Self { sender }
    }
}

impl EventLoopWaker for ServoEventLoopWaker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(Self {
            sender: self.sender.clone(),
        })
    }

    fn wake(&self) {
        if let Ok(guard) = self.sender.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.send(());
        }
    }
}

static SERVO_WAKER_TX: std::sync::LazyLock<Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<()>>>>> =
    std::sync::LazyLock::new(|| Arc::new(std::sync::Mutex::new(None)));

pub fn register_servo_waker(tx: tokio::sync::mpsc::UnboundedSender<()>) {
    if let Ok(mut guard) = SERVO_WAKER_TX.lock() {
        *guard = Some(tx);
    }
}

pub struct ServoHost {
    pub servo: Servo,
    pub rendering_context: Rc<dyn RenderingContext>,
    pub is_hardware_accelerated: bool,
    pub owner_thread: std::thread::ThreadId,
}

// ServoHost is protected by a Mutex and thread-id check, ensuring single-threaded exclusive access to its internal Rcs.
unsafe impl Send for ServoHost {}
unsafe impl Sync for ServoHost {}

static SERVO_HOST: std::sync::Mutex<Option<ServoHost>> = std::sync::Mutex::new(None);

/// Returns the optimal number of layout & content worker threads based on hardware concurrency.
pub fn optimal_thread_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8))
        .unwrap_or(3)
}

/// Constructs Servo engine preferences with modern web API capabilities and tuned thread pools (Section 7.2 & 7.5).
pub fn build_servo_preferences() -> Preferences {
    let workers = optimal_thread_workers();
    Preferences {
        // Modern Web APIs & Standards (Section 7.5)
        dom_intersection_observer_enabled: true,
        dom_resize_observer_enabled: true,
        dom_adoptedstylesheet_enabled: true,
        dom_async_clipboard_enabled: true,
        dom_crypto_subtle_enabled: true,
        dom_fontface_enabled: true,
        dom_parallel_css_parsing_enabled: true,
        dom_visual_viewport_enabled: true,
        dom_web_animations_enabled: true,
        dom_servoparser_async_html_tokenizer_enabled: true,
        dom_offscreen_canvas_enabled: true,
        dom_allow_scripts_to_close_windows: true,

        // Concurrency & Thread Pool Management (Section 7.2)
        thread_pool_workers_max: workers as u64,
        thread_pool_webrender_workers_max: (workers / 2).max(1) as u64,
        thread_pool_async_runtime_workers_max: workers as u64,

        ..Default::default()
    }
}

static SERVO_MULTIPROCESS_MODE: std::sync::atomic::AtomicI8 = std::sync::atomic::AtomicI8::new(0);

/// Explicitly configure multiprocess mode (true = multiprocess, false = single-process).
pub fn set_process_mode(multiprocess: bool) {
    SERVO_MULTIPROCESS_MODE.store(
        if multiprocess { 2 } else { 1 },
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// Constructs Servo startup options with hang monitoring and multiprocess support (Section 7.2).
pub fn build_servo_opts() -> Opts {
    let mode = SERVO_MULTIPROCESS_MODE.load(std::sync::atomic::Ordering::Relaxed);
    let multiprocess = match mode {
        1 => false, // explicitly forced single-process (via --single-process or set_process_mode(false))
        2 => true,  // explicitly forced multiprocess
        _ => {
            // Default: multiprocess is enabled, unless single-process is requested via NEOWEB_SINGLE_PROCESS=1
            // or when running unit tests (where test binaries lack a content process runner in main).
            let single_process = cfg!(test)
                || std::env::var("NEOWEB_SINGLE_PROCESS")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);

            !single_process
        }
    };

    Opts {
        multiprocess,
        background_hang_monitor: true,
        sandbox: false,
        hard_fail: false,
        ..Default::default()
    }
}

impl ServoHost {
    pub fn with<R>(f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        let mut guard = SERVO_HOST.lock().ok()?;
        let host = guard.as_mut()?;
        if host.owner_thread != std::thread::current().id() {
            return None;
        }
        Some(f(host))
    }

    pub fn is_initialized() -> bool {
        SERVO_HOST.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn is_hardware_accelerated() -> bool {
        SERVO_HOST
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|h| h.is_hardware_accelerated))
            .unwrap_or(false)
    }

    pub fn with_or_init<R>(
        width: u32,
        height: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Result<R, String> {
        let mut guard = SERVO_HOST
            .lock()
            .map_err(|e| format!("ServoHost lock error: {e}"))?;
        if guard.is_none() {
            let opts = build_servo_opts();
            let size = dpi::PhysicalSize::new(width.max(100), height.max(100));

            // Default fallback to SoftwareRenderingContext when raw window handles are not provided
            let rendering_context = SoftwareRenderingContext::new(size)
                .map_err(|e| format!("Failed to create SoftwareRenderingContext: {e:?}"))?;

            let preferences = build_servo_preferences();
            let waker = Box::new(ServoEventLoopWaker::new(SERVO_WAKER_TX.clone()));

            let servo = ServoBuilder::default()
                .opts(opts)
                .preferences(preferences)
                .event_loop_waker(waker)
                .build();

            let host = Self {
                servo,
                rendering_context: Rc::new(rendering_context),
                is_hardware_accelerated: false,
                owner_thread: std::thread::current().id(),
            };

            *guard = Some(host);
        }
        let host = guard.as_mut().unwrap();
        Ok(f(host))
    }

    /// Initializes ServoHost with native Wayland display and window handles for hardware-accelerated OpenGL/EGL
    /// with graceful fallback to SoftwareRenderingContext (Rule 11).
    pub fn init_with_handles<R>(
        display_handle: DisplayHandle,
        window_handle: WindowHandle,
        width: u32,
        height: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Result<R, String> {
        let mut guard = SERVO_HOST
            .lock()
            .map_err(|e| format!("ServoHost lock error: {e}"))?;
        if guard.is_none() {
            let opts = build_servo_opts();
            let size = dpi::PhysicalSize::new(width.max(100), height.max(100));

            let (rendering_context, is_hardware): (Rc<dyn RenderingContext>, bool) = {
                if std::env::var("LIBGL_ALWAYS_SOFTWARE").map(|v| v == "1").unwrap_or(false) {
                    info!("LIBGL_ALWAYS_SOFTWARE=1 set; bypassing hardware acceleration");
                    let sw_ctx = SoftwareRenderingContext::new(size)
                        .map_err(|e| format!("Failed to create SoftwareRenderingContext: {e:?}"))?;
                    (Rc::new(sw_ctx), false)
                } else {
                    match WindowRenderingContext::new(display_handle, window_handle, size) {
                        Ok(hw_ctx) => {
                            info!("Hardware-accelerated WindowRenderingContext established via surfman");
                            (Rc::new(hw_ctx), true)
                        }
                        Err(e) => {
                            warn!("Failed to initialize WindowRenderingContext ({e:?}); falling back to SoftwareRenderingContext");
                            let sw_ctx = SoftwareRenderingContext::new(size)
                                .map_err(|err| format!("Failed to create fallback SoftwareRenderingContext: {err:?}"))?;
                            (Rc::new(sw_ctx), false)
                        }
                    }
                }
            };

            let preferences = build_servo_preferences();
            let waker = Box::new(ServoEventLoopWaker::new(SERVO_WAKER_TX.clone()));

            let servo = ServoBuilder::default()
                .opts(opts)
                .preferences(preferences)
                .event_loop_waker(waker)
                .build();

            let host = Self {
                servo,
                rendering_context,
                is_hardware_accelerated: is_hardware,
                owner_thread: std::thread::current().id(),
            };

            *guard = Some(host);
        }
        let host = guard.as_mut().unwrap();
        Ok(f(host))
    }

    pub fn shutdown() {
        if let Ok(mut guard) = SERVO_HOST.lock()
            && let Some(host) = guard.take()
        {
            drop(host);
        }
        if let Ok(mut waker_guard) = SERVO_WAKER_TX.lock() {
            *waker_guard = None;
        }
    }
}

pub fn shutdown_servo() {
    ServoHost::shutdown();
}
