//! OverlayTxt - 桌面顶层透明弹幕渲染库
//!
//! 基于 **Direct2D** + **DirectWrite** + **DirectComposition** 在 Windows 桌面上
//! 创建透明覆盖层窗口，实现高性能弹幕渲染。支持鼠标穿透、多轨道并行、自定义
//! 弹幕样式。
//!
//! # 架构概述
//!
//! ```text
//! OverlayTxt (公共 API)  ← 线程安全，任意线程可调用 send_*
//!   └─ run_window()      ─→ 渲染线程：窗口创建 + 消息循环
//!         ├─ DcompRenderer  → D3D11 + D2D + DWrite + DXGI swapchain
//!         └─ DanmakuManager → 轨道分配 + 弹幕生命周期管理
//! ```
//!
//! # 快速开始
//!
//! ```no_run
//! use overlaytxt::{OverlayTxt, OverlayTxtConfig};
//!
//! let config = OverlayTxtConfig::default();
//! let mut app = OverlayTxt::new(config).unwrap();
//!
//! app.start().unwrap();
//! app.send_text("Hello World");
//! app.send_text_custom("自定义弹幕", Some(32.0), Some([255, 100, 100, 255]), Some(200.0));
//!
//! // 等待结束（放开此注释会阻塞直到窗口被关闭）
//! // app.wait().unwrap();
//! ```
//!
//! # 线程安全
//!
//! `OverlayTxt` 实现了 `Send + Sync`，可在任意线程安全地通过 `send_*`
//! 方法推送弹幕。但 [`wait`](OverlayTxt::wait) 和 [`stop`](OverlayTxt::stop)
//! 需要独占访问（`&mut self`），确保在调用这些方法时没有其他线程仍在推送。
//!
//! ```no_run
//! # use overlaytxt::{OverlayTxt, OverlayTxtConfig};
//! # let config = OverlayTxtConfig::default();
//! # let mut app = OverlayTxt::new(config).unwrap();
//! # app.start().unwrap();
//! let app = std::sync::Arc::new(std::sync::Mutex::new(app));
//! let app_clone = app.clone();
//! std::thread::spawn(move || {
//!     app_clone.lock().unwrap().send_text("来自其他线程的弹幕");
//! });
//! # app.lock().unwrap().wait().unwrap();
//! ```
//!
//! # 退出方式
//!
//! - 调用 [`stop`](OverlayTxt::stop) 或销毁对象（`Drop`）
//! - 向渲染线程发送 `Quit` 命令

mod danmaku;
mod overlay;
mod renderer;
mod window;

pub use danmaku::{InlineContent, straight_to_premul};
pub use overlay::*;
pub use window::get_virtual_screen_size;
