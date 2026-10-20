# Neoweb: Servo + Iced + Lua Browser Roadmap

> A lightweight, hackable, modal web browser inspired by [Luakit](https://github.com/luakit/luakit), built in Rust using the [Servo](https://servo.org/) browser engine and the [Iced](https://iced.rs/) GUI toolkit, driven and configured through Lua scripts.

---

## 1. Vision & Architecture Overview

Luakit is celebrated for its minimalism, strict adherence to vi-style modal browsing, and total hackability through Lua. Modern Luakit, however, is coupled to WebKitGTK and GTK C APIs.

**Neoweb** aims to reimagine Luakit in modern Rust:
- **Engine**: **Servo** for memory-safe, modular, multi-threaded web content rendering, decoupled behind a pluggable `WebEngine` trait (`ServoEngine` offscreen pipeline with full HTML5/CSS3/DOM layout, web fonts, images, and SpiderMonkey JavaScript execution).
- **UI Toolkit**: **Iced** for a clean, high-performance Elm-inspired GUI.
- **Display Server**: **Wayland Compositors Only** (Sway, Hyprland, GNOME Wayland, KDE Plasma Wayland, River). X11 is strictly out of scope.
- **Scripting & Core Logic**: **LuaJIT 2.1** (via `mlua` targeting vendored LuaJIT with FFI and async support enabled), ensuring high-performance JIT compilation and native parity with Luakit's micro-browser framework and Lua scripts.

```
┌────────────────────────────────────────────────────────────────────────┐
│                               Lua Layer                                │
│       config/rc.lua  ──►  lib/modes.lua  ──►  lib/binds.lua            │
│       lib/window.lua ──►  lib/webview.lua ──► lib/follow.lua           │
│       lib/lousy/     (signal, bind, uri, theme, settings)              │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ mlua FFI / Userdata Bridge
┌───────────────────────────────────▼────────────────────────────────────┐
│                       Rust Core Host & Dispatch                        │
│            Event Routing • Modal State Engine • Message Bus            │
├───────────────────────────────────┬────────────────────────────────────┤
│           Iced GUI Shell          │            Servo Engine            │
│  - Application Update & View Loop │  - Embedder & Web Content Pipeline │
│  - Viewport (WGPU Texture)        │  - HTML/CSS/DOM Rendering          │
│  - Tab Strip & Status Bar         │  - SpiderMonkey JS Execution       │
│  - Input Bar & Completions Overlay│  - Network Stack & Page Lifecycle  │
└───────────────────────────────────┴────────────────────────────────────┘
```

---

## 2. Minimum Viable Feature Set (MVP)

To be immediately useful as a daily driver while providing the authentic Luakit experience, the browser must implement the following core capabilities:

### 2.1. Modal State Machine (Vi Experience)
* **Neovim Key Notation Standard**: Strict standardization across Rust event formatting and Lua key bindings (`<CR>`, `<BS>`, `<Esc>`, `<Space>`, `<Tab>`, `<S-Tab>`, `<C-w>`, `<C-u>`, `<C-o>`, `<C-i>`, `<C-n>`, `<C-p>`).
* **Formal Lua Modal State Machine (`modes.lua`)**:
  - Dynamic mode registration with `modes.new_mode(name, spec)`.
  - Lifecycle hooks (`enter(w, ...)`, `leave(w)`, `activate(w)`), badge metadata, and prompt prefixes.
  - Zero shadow mode state in Rust; Lua is the authoritative state machine.
* **Modes**:
  - `normal`: Default navigation mode. Intercepts vim keybindings (`j`/`k`/`h`/`l`, `d`, `r`, `t`, `o`, `H`/`L`, `/`, `f`, `F`). History back `<C-o>`, forward `<C-i>`.
  - `command`: Triggered by `:`. Overlaps the bottom status bar for commands (`:open`, `:tabopen`, `:q`, etc.). Live suggestion cycling with `<Tab>`/`<S-Tab>` and `<C-n>`/`<C-p>`. Word deletion with `<C-w>`, prompt clearing with `<C-u>`.
  - `search`: Triggered by `/` (forward) and `?` (backward). In-page text search prompt overlapping the status bar; `n` and `N` navigate matches in query direction.
  - `insert`: Triggered automatically when clicking an editable element (`<input>`, `<textarea>`) or pressing `i`. Keystrokes pass directly to the browser engine; pressing `<Esc>` returns to `normal` mode.
  - `passthrough`: Triggered by `<C-z>`. Passes all input directly to the webview until `<Esc>` is pressed.
  - `follow`: Triggered by `f` (open in current tab) or `F` (open in background/new tab via mode parameter).
* **Key Sequences & Count Buffers**:
  - Multi-key bindings (e.g. `gg` to go to top, `gt`/`gT` for tab navigation, `ZZ` to quit).
  - Numeric count prefixes (e.g. `5j` to scroll 5 steps, `3d` to close 3 tabs, `2<C-o>` to navigate back 2 history entries).

### 2.2. Core Browser & Webview (ServoEngine Architecture)
* **Production Engine Architecture (`WebEngine` Trait)**:
  - `ServoEngine`: Production engine embedding Servo's offscreen WGPU pipeline with full HTML5/CSS3/DOM layout, web fonts/images, SpiderMonkey JavaScript execution, and W3C WebDriver automation API.
* **Navigation & Session History**:
  - **Chromium-Style `NavigationEntry`**: Linear history list (`Vec<NavigationEntry>`) with active `current_index`.
  - **Instant Title Display**: Traversal via `<C-o>` and `<C-i>` immediately updates the tab title from the cached entry for zero-latency, zero-flicker UI updates during provisional loading.
  - **Committed Title Update**: Document `<title>` commits to the active entry upon load completion and emits `EngineEvent::TitleChanged`.
  - **Synchronous In-Memory Pages**: `about:blank`, `about:help`, and `about:neoweb` resolve synchronously on the local thread to eliminate async task scheduling races.
  - Standard controls: Load URL, reload (with cache bypass `R`), back (`<C-o>` / `H`), forward (`<C-i>` / `L`), stop (`<C-c>`).
* **Scrolling & Zooming**: Vertical and horizontal scroll (`j`/`k`/`h`/`l`, `<C-d>`, `<C-u>`), document percentage jumps (`gg`, `G`, `%`), and zoom controls (`zi`, `zo`, `zz`).
* **JavaScript Evaluation**: Ability to evaluate arbitrary JS in the page context (`eval_js`).
* **Lifecycle Events & Signals**:
  - Page load state (`provisional`, `committed`, `finished`, `failed`).
  - Page title updates (`property::title`).
  - URI change updates (`property::uri`).
  - Link hover detection (`link-hover`, `link-unhover`) for status bar preview.
  - Editable element focus/blur signals (`form-active`, `root-active`).

### 2.3. User Interface Shell (Iced Framework & Lua AST)
* **Tab Strip**: Top bar showing rectangular tab pills with zero padding between them, tab titles expanding to occupy available space, right-aligned close buttons, and multi-format page favicons (supporting `.ico`, `.png`, `.svg` via `resvg`, `.webp`, and `data:` URIs, normalized to 16×16 PNGs with GPU texture handle caching to prevent flicker). Built-in pages (`about:neoweb`, `about:blank`, `about:help`) embed native raster PNG icons, eliminating missing-glyph tofu boxes on Wayland systems.
* **Webview Viewport**: Dedicated render area sampling Servo's offscreen WGPU texture.
* **Status Bar**:
  - Bottom bar rendered via Lua AST showing URI, SSL indicator, mode badge, key buffer, and scroll percentage.
* **Input & Notification Bar (Neovim Layout Standard)**:
  - When modal input is active (`command`, `search`), the input bar replaces/overlaps the status bar at the bottom, matching Neovim's `:ex` and `/search` prompt line.
  - Notifications display above the bottom bar only when active messages exist; no persistent startup banner.
* **Declarative Dynamic UI Overlay**:
  - Lua constructs UI widgets (tab strip, status bar, notifications, completion menus) as AST tables (`lua/lib/ui.lua`, `lua/lib/ui/tabs.lua`, `lua/lib/ui/status.lua`, `lua/lib/ui/notify.lua`, `lua/lib/ui/item_list.lua`).
  - Rust renders the AST on demand via `src/ui/lua_view.rs` with zero redundant Rust state and dispatches clicks back to Lua via `Message::LuaUiCallback`.
  - Zero Rust UI widget fallbacks.

### 2.4. Link Hinting & Following (`f` / `F`)
* Mouseless navigation without relying on a mouse cursor:
  - Injects a script into the page via `eval_js` that detects visible clickable targets (`<a>`, `<button>`, `<input>`, `[onclick]`).
  - Assigns unique short alphanumeric labels (e.g. `asdf` or `123`).
  - Renders hint boxes over elements.
  - Filters hints as the user types; executes a simulated click or returns the target URL upon match.

### 2.5. Smart Address Bar & Search Engines
* Navigation via `:open <query>` or `:tabopen <query>`:
  - Recognizes standard schemes (`https://`, `http://`, `file://`).
  - Keyword search engine dispatch: matches prefixes (e.g. `g rust servo` -> `https://google.com/search?q=rust+servo`).
  - Configurable default search engine (e.g. DuckDuckGo `%s`).

### 2.6. Clipboard & Selection Integration
* `y`: Yank current tab URL to the Wayland system clipboard.
* `p` / `P`: Open URL from Wayland system clipboard in current tab / new tab.

---

## 3. Lua Scripting Model & Adoption Strategy

One of Luakit's greatest strengths is that user configuration is code, not static JSON/YAML. Users can write arbitrary logic in `rc.lua` or `userconf.lua`.

### 3.1. Reusing Luakit's Pure-Lua Libraries
A significant portion of Luakit's code is pure Lua and is **completely decoupled from GTK**:
- `lib/lousy/bind.lua`: Robust key sequence matcher, trie lookup, and numeric count parsing.
- `lib/lousy/signal.lua`: Generic signal subscription and dispatch mechanism.
- `lib/lousy/uri.lua`: URL parsing, address normalization, and search query formatting.
- `lib/modes.lua`: State machine managing mode hooks (`enter`, `leave`, `changed`, `activate`).
- `lib/binds.lua`: Default vi bindings and command definitions (`:open`, `:tabopen`, `:close`, etc.).
- `lib/settings.lua`: Key-value settings store with domain rules and validation.

### 3.2. Lua-to-Rust Bridge Design (`mlua`)
Instead of reproducing GTK C primitives (`clib/widget.c`), expose a high-level, idiomatic Lua API backed by Rust `UserData`:

```lua
-- Example: Defining a custom keybinding in userconf.lua
local modes = require("modes")

modes.add_binds("normal", {
    { "yd", "Copy domain of current tab", function (w)
        local uri = w.view.uri
        local domain = uri:match("^%a+://([^/]+)")
        if domain then
            luakit.selection.clipboard = domain
            w:notify("Copied domain: " .. domain)
        end
    end },
})
```

### 3.3. Bridging Elm/Iced (Declarative) with Lua (Imperative Signals)
* **The Challenge**: Iced follows the Elm Architecture (`Model`, `Message`, `update`, `view`). Luakit scripts call imperative methods (`w:navigate(url)`, `w:scroll()`, `w:new_tab()`).
* **The Solution (Message Bus)**:
  1. Rust exposes `Window` and `Webview` `UserData` objects to Lua.
  2. Method calls on these objects push strongly-typed messages into an `mpsc::UnboundedSender<AppMessage>` channel.
  3. The Iced `update` loop processes these messages and modifies the application state.
  4. When state changes occur (title updated, progress updated, URL changed), Rust invokes registered Lua callbacks via `emit_signal`.

### 3.4. Core Architectural Concept: Lua-First Micro-Browser (Minimal Rust Primitives)
A foundational design principle for Neoweb to maintain minimalism and maximum hackability:
* **Rust is strictly an infrastructure host**: Rust provides the Wayland window, the offscreen Servo rendering surface, SQLite persistence, and raw Iced input event forwarding.
* **Lua is the source of truth for browser logic**: Vi modes, key sequences, command dispatch, and **command completion** are handled natively in Lua.
* **Zero Redundant Rust Logic**: Do not implement browser logic (such as command registries or auto-completion engines) in Rust when Lua can handle it natively.
* **Neovim Key Notation Standard**: Standardize strictly on Neovim key notation (`<CR>`, `<BS>`, `<Esc>`, `<Space>`, `<Tab>`, `<S-Tab>`, `<C-w>`, `<C-u>`, `<C-o>`, `<C-i>`, `<C-n>`, `<C-p>`) across Rust key parsing and Lua bindings. Avoid legacy GTK/Luakit key names (`<Return>`, `<Control-o>`).

### 3.5. Lua-First Declarative UI AST & Zero Redundant Rust UI State
* Dynamic UI elements (command completion menus, interactive suggestion lists, tab strip with page favicons, status bar, and notification bar) are authored declaratively in Lua using primitive AST components (`lua/lib/ui.lua`, `lua/lib/ui/tabs.lua`, `lua/lib/ui/status.lua`, `lua/lib/ui/notify.lua`, `lua/lib/ui/item_list.lua`), including `ui.text`, `ui.row`, `ui.column`, `ui.container`, `ui.button`, `ui.space`, and `ui.image`.
* **Single Source of Truth**: Lua manages dynamic layout, styles, badges, active tab highlighting, and status formatting. Rust maintains zero redundant UI copies or shadow models.
* **On-Demand Rendering**: During the Iced render pass (`NeowebApp::view(&self)`), Rust queries Lua dynamically via `ui.render_tabs`, `ui.render_status`, `ui.render_notification`, and `ui.render_completion`, mapping the AST into native Iced layout widgets (`src/ui/lua_view.rs`).
* **Bidirectional Callback Dispatch**: Interactive items (tab switches, tab closing, tab creation, list selections) declare Lua callback IDs; mouse clicks in Iced dispatch `Message::LuaUiCallback(id)` back to the Lua runtime without hardcoded Rust message types.

### 3.6. Chromium-Style Session History Architecture (`NavigationEntry`)
* **Session History Model**: Traversal history is managed as a linear list of `NavigationEntry { uri: String, title: String }` paired with an active `current_index: usize`.
* **Instant Title Feedback**: When navigating backwards (`<C-o>`) or forwards (`<C-i>`), the browser immediately updates the tab strip with the cached title from `NavigationEntry` for zero-latency, zero-flicker UI updates during provisional page loading.
* **Title Commit Lifecycle**: Once document parsing finishes, the true `<title>` is committed to the active `NavigationEntry` and broadcast via `EngineEvent::TitleChanged`.
* **Embedder vs. Servo Constellation Separation**: While Servo's internal Constellation handles WHATWG joint session history across frames, Neoweb maintains embedder-level navigation entries to guarantee immediate tab strip responsiveness.
* **Synchronous `about:` Page Resolution**: Internal pages (`about:blank`, `about:help`, `about:neoweb`) resolve synchronously on the local thread to prevent async task scheduling races.

### 3.7. Persistence & Storage Strategy
* **Bookmarks are strictly file-based (Chromium format)**: Never store bookmarks in SQLite. Bookmarks are stored in `~/.config/neoweb/Bookmarks` using Chromium's JSON tree format (`roots.bookmark_bar`, `roots.other`), ensuring human readability, zero database locking, dotfile tracking, and direct interoperability with Chromium/Brave/Vivaldi.
* **SQLite is strictly for browsing history**: SQLite (`neoweb.db`) is used exclusively for append-only browsing history records.

---

## 4. Phased Implementation Roadmap

### Phase 1: Engine & Shell Foundations
**Goal**: A single-window browser that renders a webpage via Servo in an Iced window and accepts basic commands from Lua.

- [x] **Iced Application Scaffold**: Set up `iced::application` with viewport layout, bottom status bar, and command entry.
- [x] **Pluggable Engine Trait & Mock Scraper**: Abstract `WebEngine` trait; `MockEngine` scraper implemented for rapid offline UI testing and zero-overhead CI.
- [x] **Servo Embedder Integration**:
  - [x] Initialize Servo embedder instance (`src/engine/servo.rs`).
  - [x] Implement offscreen rendering to a WGPU texture.
  - [x] Create a custom Iced widget/shader that displays the Servo texture (`src/ui/servo_shader.rs`).
  - [x] Forward window resize events to Servo viewport.
- [x] **Basic Input Forwarding**: Route mouse clicks, scrolling, and keyboard events from Iced into the active engine.
- [x] **Lua Runtime Setup**:
  - Embedded `mlua` targeting LuaJIT 2.1 with async and FFI enabled.
  - Exposed `luakit` global module: `luakit.navigate(url)`, `luakit.quit()`, `luakit.selection.clipboard`.
  - Bootstrapping of `rc.lua` and user configuration modules.

---

### Phase 2: Modal System & Tab Management
**Goal**: Multi-tab browsing with vi modal navigation (Normal, Command, Insert) driven by Lua.

- [x] **Iced Tab Strip**: Horizontal tab bar widget displaying tabs, titles, and active index.
- [x] **Multi-Tab State in Rust**: Manage multiple tab instances, tab switching (`gt`/`gT`), tab closing (`d`), and tab creation (`t`).
- [x] **Key Event Interceptor & Neovim Key Notation**:
  - Intercept raw keystrokes in Iced before widgets receive them.
  - Strict Neovim key notation (`<CR>`, `<BS>`, `<Esc>`, `<Space>`, `<Tab>`, `<S-Tab>`, `<C-w>`, `<C-u>`, `<C-o>`, `<C-i>`, `<C-n>`, `<C-p>`).
  - Forward keys to Lua's `lousy.bind` / `modes.lua` key router.
- [x] **Core Modes**:
  - `normal`: Vim navigation (`j`, `k`, `h`, `l`, `d`, `r`, `H`, `L`, `t`, `wH`, `wL`).
  - `command`: `:` opens input bar; executes `:open`, `:tabopen`, `:close`, `:quit`, `:help`.
  - `insert`: Auto-entered on form focus / `i`; forwards raw keystrokes to engine until `<Esc>`.
  - `passthrough`: `<C-z>` enters; bypasses browser bindings until `<Esc>`.
- [x] **Neovim History Navigation**: `<C-o>` (back) and `<C-i>` (forward) with numeric count prefix support (e.g. `2<C-o>`).
- [x] **Status Bar Elements**: Left URL/link display, right mode indicator, and key buffer display.

---

### Phase 3: Link Hinting, Search & Clipboard
**Goal**: Fully mouseless browsing workflow and search engine integration.

- [x] **Link Following (`f` / `F`)**:
  - Implement link detection and client bounding box extraction.
  - Render hint badges over elements.
  - Filter hints on keystrokes; open URL in current tab (`f`) or new background tab (`F`).
- [x] **Smart Search Engine Expansion**:
  - Support `window.search_engines` table in Lua with `%s` interpolation.
  - Fallback to default search engine on non-URL inputs.
- [x] **Wayland Native Clipboard Integration**:
  - Wayland clipboard integration via `arboard`.
  - Expose `luakit.selection.clipboard` to Lua.
  - Bind `y` (yank URL), `p` (open clipboard in current tab), `P` (open clipboard in new tab).
- [x] **In-Page Search (`/` and `?`)**:
  - Incremental text search mode.
  - `n` and `N` for next / previous search results.

---

### Phase 4: History, Bookmarks & Completions
**Goal**: Persistent browsing data and interactive command completion.

- [x] **Storage Backend**:
  - SQLite backend (`neoweb.db`) strictly for append-only browsing history.
  - Chromium-compatible JSON file-based bookmarks (`~/.config/neoweb/Bookmarks`).
  - Auto-record visited URLs with titles and timestamps.
- [x] **Chromium-Style `NavigationEntry` Linear Session History**:
  - Linear history list with active index.
  - Instant zero-latency cached title updates on `<C-o>` and `<C-i>`.
  - Document `<title>` committed on page load completion.
- [x] **Native Lua Declarative UI & Completion Pipeline**:
  - Declarative UI AST (`lua/lib/ui.lua`, `lua/lib/ui/tabs.lua`, `lua/lib/ui/status.lua`, `lua/lib/ui/notify.lua`, `lua/lib/ui/item_list.lua`) rendered via `src/ui/lua_view.rs`.
  - Tab strip, status bar, and notification bar rendered directly and exclusively from Lua.
  - Zero redundant Rust UI state.
  - Live suggestion cycling with `<Tab>` / `<S-Tab>` and `<C-n>` / `<C-p>`.
  - Prompt line editing with `<C-w>` (delete word) and `<C-u>` (clear line).
  - Autocomplete commands (`:open`, `:tabopen`, `:close`, etc.), history, bookmarks, and any custom commands registered via `modes.add_cmds` without Rust recompilation.
- [x] **Session Restore & Management**:
  - Save open tab URLs, titles, and active tab index to `session.json` on exit (`ZZ`, `:q`, `:quit`, `:wq`, or compositor `CloseRequested`).
  - Automatically restore previous tabs and active tab on launch if no CLI URL is passed.
  - Unified command interface `:session <save|restore|list|delete> [name]` with autocompletion and aliases `:w`, `:wq`, `:x`.
  - Window `close` callback lifecycle managed authoritatively in Lua.

---

### Phase 5: Plugin Ecosystem & Compatibility Layer
**Goal**: Seamless migration path for existing Luakit user configurations.

- [x] **Luakit Compatibility Modules**:
  - Provide drop-in compatible Lua modules for `window.lua`, `webview.lua`, `lousy.*`, `modes.lua`, `binds.lua`, `completion.lua`, `bookmarks.lua`, `history.lua`.
  - Support loading `$XDG_CONFIG_HOME/neoweb/userconf.lua` or `rc.lua`.
- [ ] **Full Servo WGPU Pipeline Activation**:
  - Activate `ServoEngine` as default rendering pipeline for full HTML5, CSS3, web fonts, images, and JS execution.
- [ ] **Adblocking**: Basic URL blocking / easy-list pattern matching via network interceptor.
- [ ] **Custom User Styles & Scripts**: Inject user CSS and GreaseMonkey-compatible scripts.
- [ ] **Theming**: Allow customizing colors, fonts, and widget layout through Lua tables.

---

## 5. Key Technical Challenges & Mitigations

| Challenge | Impact | Proposed Solution |
| :--- | :--- | :--- |
| **Servo WGPU Embedding** | Servo renders via WebRender / OpenGL / WGPU; Iced renders via WGPU. | Render Servo frames into an offscreen texture shared with Iced's WGPU render pipeline using custom Iced shader/widget primitives (`src/engine/servo.rs`). |
| **Key Event Precedence** | Web engines normally swallow keys, breaking vi navigation. | Intercept all window keyboard events in Iced's event subscription *before* the viewport widget. Only forward keys to engine when active mode is `insert` or `passthrough`. |
| **Key Notation Standardization** | Mismatched key strings between Rust and Lua lead to broken bindings. | Standardize strictly on Neovim key notation (`<CR>`, `<BS>`, `<Esc>`, `<Space>`, `<Tab>`, `<S-Tab>`, `<C-w>`, `<C-u>`, `<C-o>`, `<C-i>`, `<C-n>`, `<C-p>`). |
| **Declarative UI vs. Dynamic Extensions** | Complex Rust completion widgets duplicate Lua state and create sync bugs. | Lua builds declarative UI AST (`ui.lua`, `item_list.lua`); Rust renders it on demand via `lua_view.rs` with zero redundant Rust state. |
| **Session History & UI Responsiveness** | Asynchronous page loads delay tab title updates, causing UI flicker. | Adopt Chromium's `NavigationEntry` linear list: display cached title immediately upon `<C-o>` / `<C-i>`, commit true `<title>` when document parsing finishes. |
| **Declarative vs. Imperative State** | Iced is reactive/declarative; Lua scripts expect imperative mutations. | Treat Lua as the source of truth for browser actions. Lua calls push messages over an mpsc channel to update Iced state, while engine events push signals back to Lua. |
| **DOM Access for Hinting** | Luakit used WebKit Web Extension C IPC for deep DOM access. | Implement link hinting via asynchronous JavaScript evaluation (`eval_js`), returning compact JSON payloads of element coordinates and selectors to Rust. |
| **Wayland-Only Display Stack** | Modern Wayland requires strict protocol adherence (`xdg-shell`, layer shell, data control). | Target Wayland native protocols exclusively via Iced and Winit. X11, Xlib/XCB workarounds, and Xwayland fallbacks are strictly out of scope. |

---

## 6. Recommended Rust Crate Stack

- **GUI**: `iced` (with `wgpu` renderer and `tokio` support).
- **Browser Engine**: `servo` / `servo_embedder` (abstracted behind `WebEngine` trait).
- **Lua Scripting**: `mlua` (using vendored `luajit` feature flag with async and FFI enabled).
- **Memory Allocator**: `tikv-jemallocator` (avoids heap fragmentation under high-frequency DOM/SpiderMonkey allocations).
- **History Storage**: `rusqlite` for append-only browsing history (`neoweb.db`).
- **Bookmark Storage**: `serde_json` for file-based Chromium-format bookmarks (`~/.config/neoweb/Bookmarks`).
- **Clipboard**: `arboard` (Wayland native clipboard support).
- **Configuration & Paths**: `directories` (standard XDG paths for Linux/BSD).
- **Concurrency & Channels**: `tokio`, `crossbeam-channel`, `ipc-channel`.

---

## 7. Performance Architecture, Multiprocess Model & Engine Tuning

### 7.1. Separation of Concerns & High-Cohesion Boundaries
- **GUI Host Shell**: Responsible only for native Wayland windowing, platform event dispatching, menus, URL navigation chrome, and modal input handling via Iced.
- **Embedder Layer / Host State**: Glue layer implementing Servo's embedder interface (`components/servo`, `servo-embedder-traits`, `servo-constellation-traits`). Mediates communication between the GUI shell and engine without leaking internal DOM/JS types into the host shell.
- **Engine Core**: Servo's internal constellation, layout engine (Stylo/Taffy), pipeline rendering, and SpiderMonkey (`mozjs`) scripting runtime.
- **High-Cohesion Co-location**: Co-locate types, adapters, and handlers with shared lifecycles in the same module directory rather than scattering them across arbitrary utility folders.

### 7.2. Process Architecture & Concurrency Model
- **Multiprocess Architecture**:
  - By default, single-process execution causes the UI event loop, SpiderMonkey JavaScript execution, DOM tree mutations, and layout passes to compete for CPU time on the same thread pool.
  - Implement multiprocess separation using Servo’s IPC mechanism (`ipc-channel` and content process tokens). Keep the browser chrome/UI isolated from untrusted web content processes.
- **Thread Management**:
  - Configure layout threads (`layout_threads`, default 3) to match hardware concurrency without causing thread starvation.
  - Integrate an asynchronous runtime (`tokio`) cleanly with the window event loop; never block UI threads waiting on engine events or network responses.
  - Utilize background hang monitoring (`servo-background-hang-monitor`) to detect hung content pipelines.

### 7.3. GPU Acceleration & Rendering Pipeline
- **Hardware Acceleration by Default**:
  - Servo uses WebRender for GPU-accelerated rendering over OpenGL/EGL (managed via `surfman` and `glow`/`gleam`).
  - Ensure the host windowing layer provides a valid native display and window handle (`raw-window-handle`) to `surfman` to establish hardware-accelerated OpenGL / EGL contexts.
  - Keep offscreen rendering contexts and swapchains synchronized with window resizes and device pixel ratio changes.
  - Verify that the active GL renderer is a hardware GPU driver (e.g., checking `gl.get_string(gl::RENDERER)`) and maintain a clean fallback path for software rasterization (`LIBGL_ALWAYS_SOFTWARE=1`) when running in headless or VM environments.

### 7.4. Build Profiles & Performance Tuning
- **Compilation Profiles**:
  - Debug builds (`opt-level = 0`) are 10x–50x slower due to unoptimized SpiderMonkey bytecode interpretation, lack of function inlining, bounds checking, and unoptimized layout math. A debug binary can exceed 2GB.
  - For active development, configure a hybrid profile (`profile.medium` / `profile.dev`: `opt-level = 2`, `incremental = true`, `debug = "line-tables-only"`) to achieve usable interactive framerates with reasonable build times.
  - For production and benchmarking, always compile with `release` or `production` profiles (`opt-level = 3`, LTO, stripped debuginfo).
- **Allocator**:
  - Use `tikv-jemallocator` as the global allocator to avoid heap fragmentation and performance degradation in high-frequency DOM/script allocations.
- **JIT Configuration**:
  - Ensure `js_jit` is enabled for SpiderMonkey unless targeting restricted environments.

### 7.5. Web Platform Compatibility & JS Runtime Handling
- **Modern Web APIs & Experimental Features**:
  - Modern web apps (e.g., GitHub, SPA frameworks) make heavy use of newer standards (`IntersectionObserver`, `adoptedStyleSheets`, web components).
  - Enable experimental web platform features where needed (`--enable-experimental-web-platform-features` / appropriate engine preferences).
  - Provide graceful handling or polyfill injection for missing APIs to prevent script termination loops.
- **Exception & Log Dampening**:
  - Uncaught JavaScript errors in complex SPAs can flood the console. In development builds, ensure backtrace generation for JS errors is throttled or gated behind explicit log levels (`RUST_LOG`) so it does not degrade browser responsiveness.

### 7.6. Automated Testing: Servo Test Harness & W3C WebDriver Architecture
- Execute all test suites and UI layers directly against `ServoEngine`.
- Integrate W3C WebDriver API listener (`webdriver` + `servo::execute_webdriver_command`) for automated end-to-end regression testing, element inspection, and script execution.
