# Neoweb Project Guidelines

The global rules from `~/.gemini/config/AGENTS.md` apply to this repository (commit messages, root-cause debugging, clean compiler warnings, and the high-cohesion co-location pattern).

The following rules apply specifically to the **Neoweb** project:

## 1. Engine & GUI Decoupling (ServoEngine Architecture)
- Keep the `WebEngine` trait decoupled from Iced UI widgets (`src/ui/`). The engine implementation (`ServoEngine`) communicates strictly through the trait and channel events to facilitate clean offscreen rendering and event dispatch.
- **Engine Role**:
  - `ServoEngine`: Production browser engine embedding Servo's offscreen WGPU pipeline with complete HTML5/CSS3/DOM layout, web fonts/images, and SpiderMonkey JavaScript execution. All test suites and UI layers execute directly against ServoEngine.

## 2. Luakit Compatibility & Scripting Standards
- Preserve Luakit API parity for user scripts (`luakit.*`, window `w`, `view.*`, `lousy` modules).
- All Lua modules must remain fully compatible with LuaJIT 2.1 (Lua 5.1 language specification and LuaJIT FFI).

## 3. Modal Input Precedence & Neovim Key Notation
- Ensure window keyboard events in Iced are intercepted by the modal router (`src/keymap.rs`) before reaching widgets.
- Direct keystrokes must only be passed through to the browser engine when in `insert` or `passthrough` mode.
- Standardize strictly on **Neovim key notation** (`<CR>`, `<BS>`, `<Esc>`, `<Space>`, `<Tab>`, `<S-Tab>`, `<C-w>`, `<C-u>`, `<C-o>`, `<C-i>`, `<C-n>`, `<C-p>`) across Rust key formatting and Lua bindings. Do not introduce legacy or Luakit-specific key names (`<Return>`, `<Control-o>`).
- History navigation standard: `<C-o>` (back) and `<C-i>` (forward) with numeric count prefix support.
- Suggestion cycling standard: `<Tab>` / `<C-n>` for next item, `<S-Tab>` / `<C-p>` for previous item.
- Command-line editing standard: `<C-w>` deletes the previous word cleanly, `<C-u>` clears the prompt line.

## 4. Lua-First UI Primitives & Zero Redundant Rust UI State
- UI elements (tab strip, status bar, notification bar, completion menus, interactive popups, suggestion lists) must be constructed in Lua using declarative Iced primitives (`lua/lib/ui.lua`, `lua/lib/ui/tabs.lua`, `lua/lib/ui/status.lua`, `lua/lib/ui/notify.lua`, `lua/lib/ui/item_list.lua`).
- Do not store or register redundant UI state or fallback UI widgets in Rust structs. Lua is the single source of truth for UI elements, completion, and list states.
- Rust dynamically queries Lua on demand in `NeowebApp::view(&self)` and renders the AST via `src/ui/lua_view.rs` with click dispatch to `Message::LuaUiCallback(id)`.
- UI Layout: Notifications render above the bottom status bar only when active (no default startup banner). When modal input (`:`, `/`, `?`) is active, the prompt input bar overlaps/replaces the status bar, exactly matching Neovim's bottom command line layout.
- **Tab Strip Specifications**:
  - Strict Tab Strip Height: Tab bar and tab pills must adhere strictly to 28px height (`height = 28`, `max_height = 28`, `overflow = "clip"`, no vertical wrapping) to prevent vertical jitter or layout jumping.
  - Dynamic Title Truncation: Computed dynamically in Lua based on available width (`tab_tile_width - non_title_width` accounting for 16px favicon, 16px close button, padding, and spacing). Title slicing must be multi-byte UTF-8 safe (`utf8_len`, `utf8_truncate`) and append `"..."` when clipped.
  - Close Button (`×`):
    - Visible strictly on the active/focused tab; inactive tabs hide the close button (matching Chrome behavior) to reclaim horizontal title space.
    - Sized strictly 16×16px (matching favicon dimensions) with rounded hover styling (`radius: 3.0`) so hover highlights fit comfortably within the 28px tab pill without overflowing.
    - Centered horizontally and vertically inside the button container: inner text uses `align = "center"` (`align_x(Center)` and `align_y(Center)`), and the button wraps its inner child in `container(inner).align_x(Center).align_y(Center)`. Intermediate `Row` containers that force horizontal positioning to `x = 0` must not be used.
- **Completion & Item List UX / Layout Standards**:
  - Strictly Left-Aligned Columns: Button inner containers in `lua_view.rs` must default to `Alignment::Start` horizontally. Never use blanket horizontal centering on button contents (only specialized buttons like `close_btn` center content).
  - Zero Marker Noise: Do not use prefix markers (such as `"▶ "` or leading spaces) for selected items. Visual selection is indicated cleanly through row background color (`item_selected` style with `BG_ACTIVE`) and text color changes (yellow title, primary description) to eliminate font-glyph width misalignment across monospace and variable-pitch fonts.
  - Fixed Column Widths: The command/label column must have a fixed width (160px) with `wrapping = "none"` and `width = "fill"` on the row, guaranteeing clean tabular alignment across all rows.
- **Favicons & Loading Feedback**:
  - Favicon resolution supports multi-format icons (`.ico`, `.png`, `.svg` via `resvg`, `.webp`, `data:` URIs) with GPU texture handle caching to prevent flicker.
  - Loading State: Tab favicons render an animated vector spinner (`iced_aw::Spinner`) during provisional and committed page loading until load finishes.
  - Built-in pages (`about:neoweb`, `about:blank`, `about:help`) embed native raster PNG icons, eliminating missing-glyph tofu boxes on Wayland compositors lacking fallback fonts.
- **Session Management & Tab Restoration**:
  - Pure Lua session management (`lua/lib/session.lua`) supporting `:session save|restore|list|delete [name]`, `:w [name]`, `:wq [name]`, and `:x`.
  - Automatic session restoration on browser startup when no CLI URL is provided.
  - JSON-serialized tab state (`active_index`, list of URIs and cached titles) stored in `~/.config/neoweb/session.json` or named session profiles.

## 5. Formal Lua Modal State Machine (`modes.lua`)
- All browser modes (`normal`, `command`, `search`, `insert`, `passthrough`, `follow`) and custom user modes are registered dynamically in Lua using `modes.new_mode(name, spec)`.
- Mode specifications encapsulate lifecycle hooks (`enter(w, ...)`, `leave(w)`, `activate(w)`), UI badge metadata, prompt prefixes, and input policies (`passthrough`, `modal_input`).
- Rust maintains zero shadow mode state: mode transitions, key buffering, and prompt state are managed authoritatively by Lua. Rust queries Lua on demand (`runtime.get_mode()`, `runtime.is_passthrough_mode()`, `runtime.is_modal_input_mode()`, `runtime.get_prompt_prefix()`).

## 6. Chromium-Style Session History Architecture (`NavigationEntry`)
- `WebEngine` implementations must maintain session history as a linear list of `NavigationEntry` objects (`Vec<NavigationEntry>`) with an active `current_index: usize`.
- On back/forward navigation (`<C-o>` / `<C-i>`), the engine must immediately display the cached title from `NavigationEntry` for zero-latency, zero-flicker UI feedback during provisional load.
- When page content commits and parsing completes, the true document `<title>` must be committed to the active `NavigationEntry` and signaled via `EngineEvent::TitleChanged`.
- Built-in `about:` pages (`about:blank`, `about:help`, `about:neoweb`) must resolve synchronously on the local thread to eliminate async task scheduling races.

## 7. Wayland-Only Windowing & Display Target
- Neoweb exclusively targets Wayland compositors (such as Sway, Hyprland, GNOME Wayland, KDE Wayland, River).
- X11 is strictly out of scope. Do not add legacy Xlib/XCB fallbacks, X11-specific workarounds, or Xwayland dependencies. Windowing, clipboard, and rendering pipelines must leverage native Wayland protocols (`xdg-shell`, Wayland data-control clipboard, fractional scaling).

## 8. Storage & Persistence: Chromium File-Based Bookmarks
- Bookmarks must never be stored in SQLite databases. Neoweb strictly uses a file-based bookmark model compatible with Chromium and its derivatives (`~/.config/neoweb/Bookmarks` in JSON tree format).
- SQLite is reserved exclusively for append-only browsing history (`neoweb.db`).

## 9. Architectural Integrity & Separation of Concerns
- Maintain strict boundary separation across layers:
  - **GUI Host Shell**: Responsible only for native Wayland windowing, event dispatching, menus, URL navigation chrome, and modal input handling via Iced.
  - **Embedder Layer / Host State**: Glue layer implementing Servo's embedder interface (`components/servo`, `servo-embedder-traits`, `servo-constellation-traits`). Mediates communication between GUI shell and engine without leaking internal DOM/JS types into the host shell.
  - **Engine Core**: Servo's internal constellation, layout engine (Stylo/Taffy), pipeline rendering, and SpiderMonkey (`mozjs`) scripting runtime.
  - **High-Cohesion & Co-location**: Co-locate types, adapters, and handlers with shared lifecycles in the same module directory rather than scattering them across arbitrary utility folders.

## 10. Process Architecture & Concurrency Model
- **Multiprocess Architecture**:
  - By default, single-process execution causes the UI event loop, SpiderMonkey JavaScript execution, DOM tree mutations, and layout passes to compete for CPU time on the same thread pool.
  - Implement multiprocess separation using Servo's IPC mechanism (`ipc-channel` and content process tokens). Keep the browser chrome/UI strictly isolated from untrusted web content processes.
- **Thread Management**:
  - Configure layout threads (`layout_threads`, default 3) to match hardware concurrency without causing thread starvation.
  - Integrate an asynchronous runtime (`tokio`) cleanly with the window event loop; never block UI threads waiting on engine events or network responses.
  - Utilize background hang monitoring (`servo-background-hang-monitor`) to detect hung content pipelines.

## 11. GPU Acceleration & WebRender Pipeline
- **Hardware Acceleration by Default**:
  - Servo uses WebRender for GPU-accelerated rendering over OpenGL/EGL (managed via `surfman` and `glow`/`gleam`).
  - Ensure the host windowing layer provides a valid native display and window handle (`raw-window-handle`) to `surfman` to establish hardware-accelerated OpenGL / EGL contexts.
  - Keep offscreen rendering contexts and swapchains synchronized with window resizes and device pixel ratio changes.
  - Verify that the active GL renderer is a hardware GPU driver (e.g., checking `gl.get_string(gl::RENDERER)`) and maintain a clean fallback path for software rasterization (`LIBGL_ALWAYS_SOFTWARE=1`) when running in headless or VM environments.

## 12. Build Profiles & Performance Tuning
- **Compilation Profiles**:
  - Debug builds (`opt-level = 0`) are 10x–50x slower due to unoptimized SpiderMonkey bytecode interpretation, lack of function inlining, bounds checking, and unoptimized layout math.
  - For active development, configure a hybrid profile (such as `profile.medium` or `profile.dev-fast`: `opt-level = 2`, `incremental = true`, `debug = "line-tables-only"`) to achieve interactive framerates with fast build iteration.
  - For production and benchmarking, always compile with `release` or `production` profiles (`opt-level = 3`, LTO, stripped debuginfo).
- **Allocator**:
  - Use `tikv-jemallocator` as the global allocator to avoid heap fragmentation and performance degradation in high-frequency DOM/script allocations.
- **JIT Configuration**:
  - Ensure `js_jit` is enabled for SpiderMonkey unless targeting restricted environments.

## 13. Web Platform Compatibility & JS Runtime Handling
- **Modern Web APIs & Experimental Features**:
  - Modern web apps (GitHub, SPA frameworks) require newer standards (`IntersectionObserver`, `adoptedStyleSheets`, web components). Enable experimental web platform features where needed via engine preferences.
  - Provide graceful handling or polyfill injection for missing APIs to prevent script termination loops.
- **Exception & Log Dampening**:
  - Uncaught JavaScript errors in complex SPAs must not flood the console. In development builds, ensure backtrace generation for JS errors is throttled or gated behind explicit log levels (`RUST_LOG`) so it does not degrade browser responsiveness.

## 14. Toolchain, Linters & Clean Compiler Standards
- **Rust Toolchain Pinning**:
  - Servo components frequently use `#![feature(rustc_private)]`. Ensure the project adheres to the pinned toolchain in `rust-toolchain.toml` with required components (`rustc-dev`, `llvm-tools`).
- **Clean Builds**:
  - Keep code clean with zero compiler warnings or unhandled linter errors.
  - Always treat root causes (race conditions, event loop deadlocks, initialization order) rather than adding symptom workarounds or suppressing lints.

