# Kore

A Firefox-inspired browser built in Rust with a multi-process architecture.

## Current status

**205/205 tests passing** — builds on Windows and macOS.

`cargo run` opens a real window with a toolbar, tab bar, and address bar.
The navigation pipeline fetches URLs via HTTPS, parses HTML and CSS,
computes layout, and renders the result via wgpu. Real pages load and
render (google.com tested). Cyrillic text displays correctly via HTML
entity decoding.

## Architecture

Multi-process design inspired by Firefox:

- **Main process** — window management, event loop, UI chrome, session persistence
- **Renderer process** — sandboxed child process that receives `RenderFrame` IPC
  messages and paints via wgpu
- **Network process** — (future) isolated HTTP/HTTPS stack
- **GPU process** — (future) wgpu compositor in a dedicated process
- **Extension process** — sandboxed child process per WebExtension

Inter-process communication uses typed IPC over platform-native transports
(Named Pipes on Windows, Unix sockets on Linux/macOS) with serde + bincode
serialization.

## Completed modules

| Crate | Description | Tests |
|---|---|---|
| kore-html | HTML5 tokenizer, tree builder, entity decoding | 4 |
| kore-net | HTTP/HTTPS client with rustls, redirect following | 4 |
| kore-css | CSS3 parser, specificity, cascade, color parsing | 16 |
| kore-ipc | Typed IPC with serde+bincode, async Sender/Receiver | 8 |
| kore-layout | Box model, flexbox, computed layout tree | 4 |
| kore-gpu | wgpu display list, rect pipeline, font rendering via fontdue | 9 |
| kore-sandbox | Process isolation, policy builder, cross-platform | 8 |
| kore-browser | Tab manager, session save/restore, history, renderer process | 24 |
| kore-ui | Toolbar, tabs, omnibox, theme system | 5 |
| kore-window | winit integration, input events, window handle | 28 |
| kore-pipeline | DOM→CSS→layout→display list render pipeline | 15 |
| kore-font | fontdue rasterizer, glyph cache, text shaping | 20 |
| kore-js | QuickJS engine, DOM bindings, script execution | 14 |
| kore-extensions | WebExtensions API, manifest v2 parsing, sandboxed processes | 17 |
| kore-devtools | Elements inspector, console capture, network log, storage stubs | 33 |
| **Total** | | **205** |

## Prerequisites

- Rust 1.78+ (edition 2021)
- Windows, Linux, or macOS
- GPU with Vulkan/Metal/DX12 support (for wgpu)

## Build and run

```sh
cargo build --workspace
cargo test --workspace
cargo run
```

## Windows development

**AppLocker / Smart App Control**

If `cargo test` fails with `os error 4551`, Windows security policy is blocking
unsigned test executables in the local `target/` directory. All 205 tests pass
on macOS; this is a Windows-environment-only issue.

Solutions (pick one):

- **Recommended** — uncomment the `target-dir` line in `.cargo/config.toml`
  to redirect build output to `C:\Users\Public\kore-target` (the Public folder
  is typically not restricted by AppLocker).
- Run `cargo test` from a terminal launched **as Administrator**.
- Temporarily disable **Smart App Control** in Windows Security settings.
- Move the project to `C:\Users\Public\` or another unrestricted path.
- Add the project's `target\` directory to **AppLocker exclusions** (via
  Local Security Policy or group policy).

## Roadmap

- Improved CSS rendering (flexbox edge cases, positioned elements)
- More complete JS DOM API (element manipulation, events)
- Privacy features (tracking protection, Enhanced Tracking Protection)
- Web compatibility (form submission, media elements, iframes)
- Installer (Windows .msi, macOS .dmg, Linux AppImage)

## License

MIT. See [LICENSE](LICENSE).
