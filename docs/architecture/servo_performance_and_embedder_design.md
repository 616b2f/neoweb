# Neoweb: Servo Embedder Architecture, GPU Pipeline & Performance Design

## 1. Executive Summary

Neoweb is a modern, lightweight, modal web browser inspired by Luakit, implemented in Rust. It decouples browser operations into three distinct architectural tiers:
1. **Lua Scripting Runtime & State Machine** (Luakit parity via LuaJIT 2.1).
2. **GUI Host Shell** (Wayland-native windowing, modal input routing, and Elm-inspired declarative UI via Iced).
3. **Embedder Backend & Web Engine** (Servo 0.7.0 with SpiderMonkey `mozjs`, Stylo/Taffy layout, and WebRender/WGPU pipeline).

This design document specifies the production architecture for high-performance Servo embedding in Neoweb, detailing host windowing with `raw-window-handle`, embedder event loop mechanics, GPU acceleration via `surfman` and WebRender, multiprocess content isolation via `ipc-channel`, and tuned compilation profiles.

---

## 2. Architecture & Separation of Concerns

```
┌────────────────────────────────────────────────────────────────────────┐
│                        1. Scripting Layer (Lua)                        │
│   lua/init.lua • lib/modes.lua • lib/binds.lua • lib/ui/*.lua          │
│   (Zero redundant Rust state; declarative AST for tabs, prompt, status)│
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ mlua FFI / MPSC Channels
┌───────────────────────────────────▼────────────────────────────────────┐
│                    2. GUI Host Shell (Rust / Iced)                     │
│   - Wayland Windowing (winit / raw-window-handle)                      │
│   - Modal Input Precedence (Neovim key notation: <CR>, <BS>, <Esc>)    │
│   - Viewport Texture Renderer (iced::widget::shader / WGPU surface)    │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ WebEngine Trait / Embedder APIs
┌───────────────────────────────────▼────────────────────────────────────┐
│                 3. Embedder Glue Layer (src/engine/servo/)              │
│   - ServoHost & EventLoopWaker integration                             │
│   - surfman OpenGL / EGL context manager & WebRender bridge            │
│   - Chromium-style NavigationEntry linear session history              │
│   - WebDriver W3C Command Dispatcher                                   │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ IPC Channel / Constellation Msg
┌───────────────────────────────────▼────────────────────────────────────┐
│                     4. Servo Engine Core Pipeline                      │
│   - Constellation Coordinator                                          │
│   - Stylo / Taffy Layout Threads (layout_threads: 3)                   │
│   - SpiderMonkey JavaScript Runtime (mozjs / js_jit enabled)           │
│   - Isolated Content Processes (multiprocess IPC tokens)               │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Host Windowing & `raw-window-handle` Integration

Neoweb targets **Wayland compositors exclusively** (Sway, Hyprland, GNOME Wayland, KDE Plasma Wayland, River). X11 is out of scope.

### 3.1. Window Handle Extraction
Servo's rendering backend (`surfman`) requires raw display and window handles to create hardware-accelerated EGL contexts on Wayland. Neoweb implements the embedder window abstraction by querying `raw-window-handle` version 0.6 from Iced's window:

```rust
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
};
use std::ptr::NonNull;

pub struct WaylandHostWindow {
    display_ptr: NonNull<std::ffi::c_void>,
    surface_ptr: NonNull<std::ffi::c_void>,
    width: u32,
    height: u32,
    scale_factor: f64,
}

impl HasDisplayHandle for WaylandHostWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let handle = WaylandDisplayHandle::new(self.display_ptr);
        unsafe { Ok(DisplayHandle::borrow_raw(RawDisplayHandle::Wayland(handle))) }
    }
}

impl HasWindowHandle for WaylandHostWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = WaylandWindowHandle::new(self.surface_ptr);
        unsafe { Ok(WindowHandle::borrow_raw(RawWindowHandle::Wayland(handle))) }
    }
}
```

---

## 4. Embedder Event Loop & Constellation Abstraction

### 4.1. Non-Blocking Event Loop Synchronization
Single-process execution without proper wakers causes either CPU pegging or event loop starvation. The embedder must implement `servo::EventLoopWaker` to wake Iced's event loop whenever Servo's internal threads (constellation, script, layout, networking) produce messages:

```rust
use servo::{EventLoopWaker, Servo, ServoBuilder};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub enum EmbedderWakeEvent {
    ConstellationMessage,
    NewFrameReady,
    NeedsPaint,
}

pub struct IcedEventLoopWaker {
    sender: UnboundedSender<EmbedderWakeEvent>,
}

impl IcedEventLoopWaker {
    pub fn new(sender: UnboundedSender<EmbedderWakeEvent>) -> Self {
        Self { sender }
    }
}

impl EventLoopWaker for IcedEventLoopWaker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(Self {
            sender: self.sender.clone(),
        })
    }

    fn wake(&self) {
        let _ = self.sender.send(EmbedderWakeEvent::ConstellationMessage);
    }
}
```

### 4.2. Embedder Event Pumping
Servo's event loop is pumped non-blockingly inside the engine's `poll_events()` method, triggered reactively by waker events or UI frame ticks:

```rust
impl ServoHost {
    pub fn spin_event_loop(&self) {
        self.servo.spin_event_loop();
    }
}
```

---

## 5. GPU Acceleration & WebRender Presentation Pipeline

### 5.1. Context Creation via Surfman
To guarantee hardware rendering on Wayland compositors:
1. `surfman::Connection` connects to the Wayland display server (`wl_display`).
2. A hardware `Adapter` is selected (checking `gl.get_string(gl::RENDERER)` to ensure it is not `llvmpipe` unless software fallback is explicitly requested).
3. EGL context is created with generic offscreen surface support for zero-copy presentation into WGPU textures.

```rust
use servo::{DeviceIntPoint, DeviceIntRect, RenderingContext, SoftwareRenderingContext};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct GpuPipelineManager {
    is_hardware_accelerated: bool,
    gl_renderer: String,
    gl_version: String,
}

impl GpuPipelineManager {
    pub fn verify_hardware_driver(renderer: &str) -> bool {
        let is_software = renderer.contains("llvmpipe")
            || renderer.contains("software")
            || renderer.contains("softpipe");
        !is_software
    }
}
```

### 5.2. Offscreen Rendering & Texture Sharing
- Servo renders pages into an offscreen surfman/OpenGL surface managed by WebRender.
- When `WebViewDelegate::notify_new_frame_ready` signals a paint event, pixels are captured into a shared buffer (`RenderFrame`) and uploaded directly to an `iced::widget::image::Handle` or WGPU texture primitive, maintaining 60fps/120fps smooth scrolling.

---

## 6. Process Architecture: Single vs. Multiprocess Isolation

### 6.1. Single-Process vs. Multiprocess Mode
| Aspect | Single-Process Mode | Multiprocess Mode |
| :--- | :--- | :--- |
| **Execution** | UI, SpiderMonkey, DOM, Layout in same OS process | UI in host process, Web content in isolated worker process |
| **Stability** | A web page crash or JS hang can freeze the browser UI | Content process crash isolated; UI remains responsive |
| **Security** | Untrusted JS executes inside host process memory space | Content processes run inside strict sandboxes |
| **Memory Overhead** | Low (single allocator, shared heap) | Medium (per-process SpiderMonkey heap & IPC buffers) |
| **Target Use** | Fast local testing, unit tests, low-memory embedded devices | Production daily driving with untrusted web navigation |

### 6.2. Multiprocess Bootstrap via Content Tokens
Servo supports multiprocess isolation through content process tokens. When `Opts.multiprocess = true`:
1. The host process spawns a child worker binary (`neoweb --content-process <TOKEN>`).
2. The child connects back to the host via `ipc-channel` (`servo::run_content_process(token)`).
3. DOM parsing, SpiderMonkey execution, and Stylo styling run isolated in the child process.

```rust
pub fn handle_process_launch_args(args: &[String]) -> bool {
    if args.len() >= 3 && args[1] == "--content-process" {
        let token = args[2].clone();
        tracing::info!("Starting Servo isolated content process with token: {}", token);
        servo::run_content_process(token);
        return true;
    }
    false
}
```

---

## 7. Cargo Build Profiles & Runtime Allocator Tuning

### 7.1. Performance Profiles in `Cargo.toml`
Debug builds (`opt-level = 0`) are 10x–50x slower due to unoptimized SpiderMonkey bytecode interpretation and bounds checking. Neoweb implements a three-tier profile model:

```toml
# 1. Dev Profile: Fast compile time with basic optimizations for usable interactive development
[profile.dev]
opt-level = 1

# 2. Medium Profile: Hybrid balance for daily testing with responsive layout & JS framerates
[profile.medium]
inherits = "dev"
opt-level = 2
debug = "line-tables-only"
incremental = true

# 3. Release Profile: Production speed with thin LTO and stripped debug symbols
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
strip = true
```

### 7.2. Global Allocator: `tikv-jemallocator`
High-frequency DOM tree manipulations, microtask queues, and SpiderMonkey GC cycles lead to heap fragmentation with system `glibc` malloc. Neoweb standardizes on `tikv-jemallocator`:

```rust
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;
```

---

## 8. Automated Testing: Servo Headless & W3C WebDriver Architecture

Neoweb completely eliminates mock engines in favor of full end-to-end verification directly against `ServoEngine`:
1. **Unit & Integration Suite**: All 35+ test suites run directly against Servo offscreen pipeline (evaluating live JavaScript, DOM rendering, history entries, and modal commands).
2. **W3C WebDriver Architecture**:
   - Spawns background WebDriver HTTP server (`127.0.0.1:7000`) using the `webdriver` crate.
   - Forwards commands to `Servo::execute_webdriver_command(command)` for automated element inspection, click simulation, screenshot capture, and navigation assertions.
