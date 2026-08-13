# Kore

A Firefox-inspired browser built in Rust with a multi-process architecture.

## Current status

**358/358 tests passing** — builds on Windows and macOS.

`cargo run` opens a real window with a toolbar, tab bar, and address bar.
The navigation pipeline fetches URLs via HTTPS, parses HTML and CSS,
computes layout, and renders the result via wgpu. Real pages load and
render (google.com tested). Cyrillic text displays correctly via HTML
entity decoding.

Recent additions:

- **HTML form submission** — GET (query-append navigation) and POST
  (urlencoded body), with control collection (`input`/`select`/`textarea`/`button`)
- **Media & embedding** — `<video>`/`<audio>` placeholders with
  proper default sizes, `<iframe>` rendering with nested layout,
  clipping, and `srcdoc`/`src` support
- **Enhanced Tracking Protection** — built-in tracker list
  (ads/analytics/social/fingerprinting), Standard/Strict levels, and
  third-party cookie blocking, with a shared block log
- **DOM mutations** — `appendChild`/`removeChild`/`insertBefore`/
  `replaceChild`/`remove()`, `textContent`/`innerText`/`nodeValue` setters
- **Real storage** — `localStorage` and `document.cookie` backed by
  shared storage that survives navigation, plus a live DevTools
  Storage Inspector
- **WebExtensions APIs** — real `webRequest`, `contextMenus`, and
  `notifications` registries with listener dispatch and filters

## Architecture

Multi-process design inspired by Firefox:

- **Main process** — window management, event loop, UI chrome, session persistence
- **Renderer process** — sandboxed child process that receives `RenderFrame` IPC
  messages and paints via wgpu
- **Network process** — isolated HTTP/HTTPS stack (`kore_network` bin):
  rustls client, cookie jar, HTTP cache, ETP cookie policy
- **GPU process** — wgpu compositor in a dedicated process (`kore_gpuprocess` bin)
- **Extension process** — sandboxed child process per WebExtension

Inter-process communication uses typed IPC over platform-native transports
(Named Pipes on Windows, Unix sockets on Linux/macOS) with serde + bincode
serialization.

## Completed modules

| Crate | Description | Tests |
|---|---|---|
| kore-html | HTML5 tokenizer, tree builder, entity decoding | 4 |
| kore-net | HTTP/HTTPS client (rustls), cookie jar, HTTP cache, tracking protection | 32 |
| kore-css | CSS3 parser, specificity, cascade, color parsing, transitions | 22 |
| kore-ipc | Typed IPC with serde+bincode, async Sender/Receiver, wire types | 16 |
| kore-layout | Box model, flexbox, computed layout tree, replaced-element sizing | 16 |
| kore-gpu | wgpu display list, rect pipeline, font rendering via fontdue | 9 |
| kore-sandbox | Process isolation, policy builder, cross-platform | 8 |
| kore-browser | Tab manager, session save/restore, history, renderer/network/GPU bridges | 26 |
| kore-ui | Toolbar, tabs, omnibox, theme system | 6 |
| kore-window | winit integration, input events, window handle | 28 |
| kore-pipeline | DOM→CSS→layout→display list pipeline, forms, iframes, scripts, ETP | 53 |
| kore-font | fontdue rasterizer, glyph cache, text shaping | 20 |
| kore-js | JS engine (Boa), DOM bindings, localStorage/cookies, script execution | 57 |
| kore-extensions | WebExtensions API (webRequest/contextMenus/notifications), manifest v2 | 27 |
| kore-devtools | Elements inspector, console capture, network log, storage inspector | 34 |
| **Total** | | **358** |

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
unsigned test executables in the local `target/` directory. All 358 tests pass
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

- Full tracker list (Disconnect-style) and per-site protection exceptions
- Cookie partitioning (State Partitioning, CHIPS)
- Improved CSS rendering (flexbox edge cases, positioned elements)
- JS execution inside iframes, `fetch()` in page scripts
- Installer (Windows .msi, macOS .dmg, Linux AppImage)

## License

MIT. See [LICENSE](LICENSE).