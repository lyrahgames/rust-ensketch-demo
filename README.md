# Graphics research OBJ viewer proof of concept

This workspace renders indexed OBJ meshes with `wgpu` 30 on native windows and browser WebGPU. The graphics engine has no dependency on scripting runtimes or `winit`. Rust, Lua, Rhai, Python, and JavaScript use the same scene, camera, context, and renderer implementation.

## Workspace

| Path | Responsibility |
| --- | --- |
| `crates/engine` | OBJ parsing, CPU/GPU scenes, cameras, `ObjViewer`, input values, closure executors |
| `crates/script-lua`, `script-rhai` | Persistent embedded environments and userdata bindings |
| `crates/script-python` | Importable PyO3 module; Python is the host process |
| `crates/script-js` | WebAssembly bindings and generated ES module/types in `pkg/` |
| `apps/native` | One `winit` event loop, windows, input, REPL, optional Lua/Rhai runtimes |
| `apps/web`, `examples/web` | Hybrid Cargo/pnpm Vite host, demo link page, canvas events, animation frames |

`HostScene::load_obj(&bytes)` is the fundamental loader. Native adapters read files; browser bindings fetch URLs. It keeps indexed meshes, names, optional UVs, generated normals, and bounds. `HostScene::upload(&context)` creates a `DeviceScene`. A host scene caches only weak references to uploads by context identity: two viewers on one context reuse buffers, while unused device resources can be released. A device scene retains its creating context and is rejected by viewers on another context.

`Camera` is a cloneable value. Scripts own camera variables; `ObjViewer::render(&camera)` borrows one only for the frame. Projection uses right-handed, Y-up, zero-to-one depth and the viewer's current aspect ratio. The platform host attaches a surface after creating a window or canvas. The viewer stores no script callbacks.

Native callback registrations support mouse motion/buttons/wheel, keys, resize, frame updates, and close requests. Input events expose `kind`, pointer deltas, button state, key, and dimensions. The browser examples build equivalent plain JavaScript pointer and key objects from DOM events. A callback is replaced by registering another function and can be removed through the adapter API; callbacks run serially within their scripting environment.

## Native examples

Run from this directory. Each command opens windows and blocks until they close. Left drag orbits, middle/right drag pans, and the wheel zooms. Case 4 uses one device scene and camera at two viewport sizes.

```sh
cargo run -p native --example rust-case-1
cargo run -p native --example rust-case-2
cargo run -p native --example rust-case-3
cargo run -p native --example rust-case-4
```

Lua and Rhai are optional; the standalone native executable has no Python dependency.

```sh
cargo run -p native --features lua -- --lua examples/lua/case-1.lua
cargo run -p native --features lua -- --lua examples/lua/case-2.lua
cargo run -p native --features lua -- --lua examples/lua/case-3.lua
cargo run -p native --features lua -- --lua examples/lua/case-4.lua
```

```sh
cargo run -p native --features rhai -- --rhai examples/rhai/case-1.rhai
cargo run -p native --features rhai -- --rhai examples/rhai/case-2.rhai
cargo run -p native --features rhai -- --rhai examples/rhai/case-3.rhai
cargo run -p native --features rhai -- --rhai examples/rhai/case-4.rhai
```

The native REPL reads one source line at a time while a Lua or Rhai application is running. Use `:lua`, `:rhai`, or `:main` to select an available language or the main executor; `:quit` exits. The selected runtime keeps globals and callbacks between lines. For one window explicitly shared between both languages, run:

```sh
cargo run -p native --features lua,rhai -- --both examples/lua/case-1.lua examples/rhai/co-edit.rhai
```

The first Lua viewer is exposed to Rhai as `viewer`. The initial Rhai script changes its color; REPL commands in either language can then read or change that same object. Only the runtimes enabled for that application are constructed. Rhai uses `off_mouse_move()`, `off_frame()`, and corresponding `off_*` methods to remove callbacks; Lua passes `nil` to `on_*`. Python passes `None`.

The worker demo uses one window/viewer on the main thread and another on a dedicated executor thread. Its REPL defaults to worker Lua; use `:main` or `:worker` to select the executor, `:lua` or `:rhai` to select the language, and `:quit` to exit. For example, send `viewer:set_background_color(0.1,0.2,0.4,1)` to Lua or `viewer.set_background_color(0.1,0.2,0.4,1.0);` to Rhai. Each executor owns its own persistent Lua and Rhai environments, which share that executor's viewer. The worker also submits a closure that changes the main window's title and size on the main thread.

```sh
cargo run -p native --features lua,rhai --example worker
```

## Python extension

Use a normal Python virtual environment. Maturin compiles the extension; no Python interpreter is embedded by the Rust executable, and there is no IPC service.

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install maturin
maturin develop --manifest-path crates/script-python/Cargo.toml --features lua,rhai
python examples/python/case1.py
python examples/python/case2.py
python examples/python/case3.py
python examples/python/case4.py
```

The four scripts import `graphics` and create normal Python variables for contexts, scenes, viewers, and cameras. `gfx.show()` creates missing OS windows, releases the GIL while the `winit` loop waits, reacquires it for callbacks, and returns after all windows close. Viewers can be constructed before `show()`. `viewer.render(camera)` works during frame callbacks; `viewer.close()` closes deterministically. The optional `lua,rhai` wheel features add `viewer.eval_lua(source, camera=None)` and `viewer.eval_rhai(source, camera=None)` so Python can explicitly share the same Rust viewer with both persistent runtimes. Without these features, the core Python API still works.

For a Python-owned outer loop on native desktops, call `gfx.process_events(timeout_ms=0)` repeatedly on the OS main thread; it returns `False` once all viewers close. The default timeout is nonblocking, so the caller must throttle its own loop. Rendering and callbacks still happen synchronously inside each pump call. This winit API is not available on Web/iOS and may have platform-specific event-loop restrictions.

Python window and cross-language checks:

```sh
python tests/python_window_smoke.py
python tests/python_manual_pump.py
python tests/python_pump_then_show.py
python tests/python_cross_language.py
```

## Browser examples

`apps/web` is both the existing Cargo application and the pnpm/Vite package. Install its frontend dependencies once with `pnpm --dir apps/web install`. A small root `package.json` forwards common commands into that app, so from the workspace root you can start the dev server with:

```sh
pnpm dev
```

Vite opens the existing `apps/web/index.html` link page, which points to the four source pages in `examples/web`. From the workspace root, `pnpm build` creates the static production bundle in `dist/`, and `pnpm wasm:build` rebuilds only the importable module in `pkg/`. The root manifest is only a command router; it has no frontend dependencies or pnpm workspace configuration.

The first WebAssembly build may download the Rust `wasm32-unknown-unknown` target and the matching `wasm-bindgen` CLI through `wasm-pack`. Rust, Node.js 22.12 or newer, and pnpm 12.15 or newer are required. The `wasm-pack` install script is explicitly approved in `apps/web/pnpm-workspace.yaml` because pnpm otherwise blocks dependency install scripts by default.

JavaScript owns browser event listeners, ordinary camera/viewer variables, and `requestAnimationFrame`. The WASM module exposes context, scene, device scene, camera, and canvas viewer classes. `HostScene.loadObjUrl` fetches the OBJ directly from static hosting. Call `.close()` on a viewer and generated `.free()` methods on WASM objects for deterministic cleanup; garbage collection is a fallback. The example host stops its frame loop, unregisters listeners, and frees all WASM objects on `pagehide`.

## Tasks, ownership, and lifecycle

`Executor<State>` runs type-erased `FnOnce(&mut State)` jobs in FIFO start order. `spawn` has no result; `call` returns a future-like one-shot `Task<R>` and also works for `R=()`; `call_async` queues its start in FIFO order, then runs an independently owned future on a native helper thread or the browser's logical main executor. Async operations cannot hold `&mut State` across an await: submit later mutations through a cloned `Submitter<State>`. `spawn_local` and `with_state` run non-`Send` closures immediately on the owning thread. A main executor is drained at valid host lifecycle points; native worker executors construct state and keep it on their assigned thread. Waiting for a task from within its own executor would deadlock and must be avoided. The provided hosts wait only from REPL input threads. Shutdown rejects new work and joins worker threads; queued work before the shutdown sentinel completes, and abandoned task receivers report cancellation.

Rust `Arc` ownership keeps contexts and shared GPU scenes alive while viewers need them. Script wrappers own the same Rust objects through their normal variables. Hosts retain active windows; Python's pre-show list holds only weak viewer references. Native callbacks are held in the scripting adapter or host. No generated engine IDs, global graphics registry, or synchronized camera internals are used. A viewer is fixed to its executor for its lifetime. Window creation, title, and size changes occur in `winit` callbacks. Surface loss requests recreation by the host; outdated surfaces are reconfigured. A lost device requires making a new context and re-uploading scenes.

## Verification and limits

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo build -p native --no-default-features
cargo check -p script-js --target wasm32-unknown-unknown
```

The native viewer requires a usable GPU adapter and display server. On Linux, `WGPU_BACKEND=vulkan` or `WGPU_BACKEND=gl` can help diagnose adapter selection; Python's `GpuContext(adapter="name fragment")` and the Lua/Rhai constructors can select a listed native adapter. This repository was smoke-tested with Linux X11 and llvmpipe. Worker presentation depends on platform/backend threading support; platforms that require rendering on the main thread should keep viewers on the default main executor. Browser WebGPU requires a compatible browser/device; static HTTP on an arbitrary remote origin is insufficient. Headless Chromium in this environment failed to initialize Vulkan, so browser GPU presentation was not verified here, although the WASM build, generated module, and CPU OBJ/camera path were verified. Native live scripting currently accepts one source line per REPL command and creates windows before `show()` starts.

For a manual rendering check, launch each case, confirm both models shade with depth testing, drag/scroll in each viewer, resize windows, and verify case 4's wide viewport keeps correct proportions while either window's input changes both views. Close one of two windows and confirm the other remains usable.
