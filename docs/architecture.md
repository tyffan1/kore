# Kore Architecture

Kore follows a Firefox-inspired browser split:

- Main process: browser UI, tabs, session state, extension host.
- Renderer processes: sandboxed document parsing, style, layout, and script execution.
- Network process: HTTP(S), DNS, cookies, cache, CORS, and mixed-content policy.
- GPU process: retained display lists and wgpu-based compositing.
- Extension processes: isolated WebExtensions runtime.

The first implementation milestone builds the foundational parser and networking crates. Later
milestones will connect them through typed IPC and process sandboxing.
