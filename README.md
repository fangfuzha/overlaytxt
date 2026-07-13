# OverlayTxt

[English](README-EN.md) | **中文**

基于 **Direct2D** + **DirectWrite** + **DirectComposition** 的 Windows 桌面透明弹幕渲染库。
硬件加速、鼠标穿透、多轨道滚动、图文混排、彩色 emoji。

## 特性

- **全透明覆盖层** — 鼠标事件穿透到下方窗口
- **多轨道滚动** — 自动轨道分配，避免水平重叠
- **图文混排** — 文字和图片在单条弹幕中交错排列
- **彩色 emoji** — 自定义 DWrite font fallback（Windows 10+）
- **线程安全 API** — 任意线程通过 `send_*` 方法推送弹幕
- **60 FPS** — `WM_TIMER` 驱动的平滑动画
- **显示区域控制** — 限制弹幕显示范围（如仅上半屏）
- **可选背景** — 半透明暗色背板提升可读性

## 环境要求

- Windows 10 或更高版本
- 支持 D3D11 的硬件 GPU

## 用法

```toml
[dependencies]
overlaytxt = "0.1"
```

```rust,no_run
use overlaytxt::{OverlayTxt, OverlayTxtConfig, InlineContent};

let config = OverlayTxtConfig::default();
let mut app = OverlayTxt::new(config).unwrap();
app.start().unwrap();

// 纯文字弹幕
app.send_text("Hello World");

// 自定义样式
app.send_text_custom("大号红字", Some(36.0), Some([255, 0, 0, 255]), Some(100.0));

// 图文混排（图片需预乘 RGBA 数据，可用 straight_to_premul 转换 PNG）
app.send_inline(vec![
    InlineContent::text("😀 ", 28.0, [255; 4]),
    InlineContent::rgba_image(32, 32, &[/* 预乘 RGBA */]),
    InlineContent::text(" 图片示例", 28.0, [255; 4]),
], None);

// 阻塞等待窗口关闭
app.wait().unwrap();
```

## 架构

```
OverlayTxt（公共 API，线程安全）
  └─ run_window() — 渲染线程
        ├─ DcompRenderer（D3D11 → D2D → DWrite → DXGI swapchain → DComp）
        └─ DanmakuManager（轨道分配 + 生命周期）
```

## 许可证

MIT