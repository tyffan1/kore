# Kore

A Firefox-inspired browser built in Rust with a multi-process architecture.

## Current status

Early development. `cargo run` opens a real window with a toolbar, tab bar, and
address bar. The navigation pipeline fetches URLs via HTTPS, parses HTML and
CSS, computes layout, and renders the result via wgpu.

## Architecture

Multi-process design inspired by Firefox:

- **Main process** — window management, event loop, UI chrome, session persistence
- **Renderer process** — sandboxed child process that receives `RenderFrame` IPC
  messages and paints via wgpu
- **Network process** — (future) isolated HTTP/HTTPS stack
- **GPU process** — (future) wgpu compositor in a dedicated process

Inter-process communication uses typed IPC over platform-native transports
(Named Pipes on Windows, Unix sockets on Linux/macOS) with serde + bincode
serialization.

## Completed modules

| Crate | Description | Tests |
|---|---|---|
| kore-html | HTML5 parser (tokenizer + tree builder) | 4 |
| kore-net | HTTP/HTTPS client with rustls | 4 |
| kore-css | CSS3 parser, specificity calculator, cascade | 8 |
| kore-ipc | Typed IPC with serde+bincode, async Sender/Receiver | 8 |
| kore-layout | Box model, flexbox, computed layout tree | 4 |
| kore-gpu | wgpu display list, rect pipeline, texture atlas stub | 9 |
| kore-sandbox | Process isolation, policy builder, job objects | 8 |
| kore-browser | Tab manager, session save/restore, renderer process | 19 |
| kore-ui | Toolbar, tabs, omnibox, theme system | 5 |
| kore-window | winit integration, input events, window handle | 22 |
| **Total** | | **91** |

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

## Roadmap

- Real page rendering (inline text, images, CSS backgrounds)
- JavaScript engine integration
- Browser extension system
- Installer and platform packaging

## License

MIT. See [LICENSE](LICENSE).
