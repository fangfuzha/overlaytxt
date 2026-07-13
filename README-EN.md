# OverlayTxt

**English** | [中文](README.md)

A transparent overlay danmaku (scrolling text) rendering library for Windows, powered by **Direct2D** + **DirectWrite** + **DirectComposition**.

## Features

- **Fully transparent** overlay — mouse events pass through to underlying windows
- **Multi-track scrolling** — automatic track allocation prevents horizontal overlap
- **Rich text styling** — configurable font size, color, and speed per danmaku
- **Inline images** — mix text and images in a single danmaku (e.g., PNG + text)
- **Color emoji** — built-in support via custom DWrite font fallback (Windows 10+)
- **Thread-safe API** — send danmaku from any thread
- **60 FPS** — timer-based message loop for smooth animation
- **Display area control** — limit danmaku to a portion of the screen (e.g., top 50%)
- **Optional background** — semi-transparent backdrop for improved readability

## Requirements

- Windows 10 or later
- Hardware GPU with D3D11 support

## Usage

```toml
[dependencies]
overlaytxt = "0.1"
```

```rust,no_run
use overlaytxt::{OverlayTxt, OverlayTxtConfig, InlineContent};

let config = OverlayTxtConfig::default();
let mut app = OverlayTxt::new(config).unwrap();
app.start().unwrap();

// Plain text
app.send_text("Hello World");

// Custom styling
app.send_text_custom("Big red text", Some(36.0), Some([255, 0, 0, 255]), Some(100.0));

// Mixed content (premultiplied RGBA data required; use straight_to_premul for PNG)
app.send_inline(vec![
    InlineContent::text("😀 ", 28.0, [255; 4]),
    InlineContent::rgba_image(32, 32, &[/* premultiplied RGBA */]),
    InlineContent::text(" image demo", 28.0, [255; 4]),
], None);

// Block until the window closes
app.wait().unwrap();
```

## Architecture

```
OverlayTxt (public API, thread-safe)
  └─ run_window() — render thread
        ├─ DcompRenderer (D3D11 → D2D → DWrite → DXGI swapchain → DComp)
        └─ DanmakuManager (track allocation + lifecycle)
```

## License

MIT