# OverlayTxt 需求文档

## 1. 项目概述

### 1.1 项目名称
OverlayTxt

### 1.2 项目定位
桌面顶层透明弹幕渲染库 —— 基于 Rust，在 Windows 桌面上创建透明覆盖层窗口，实现高性能弹幕（Danmaku）渲染。

### 1.3 目标用户
- 直播/视频平台弹幕可视化工具开发者
- 桌面弹幕应用开发者
- 需要在桌面顶层显示动态文本/图片的场景

---

## 2. 核心功能需求

### 2.1 透明覆盖层窗口
| 功能项 | 描述 |
|--------|------|
| 全屏透明窗口 | 覆盖整个屏幕，背景完全透明，不影响桌面操作 |
| 顶层显示 | 窗口始终位于所有普通窗口之上 |
| 鼠标穿透 | 鼠标事件穿透到下层桌面/应用程序 |
| 无任务栏图标 | 不在任务栏显示，不干扰用户 |
| DPI 缩放支持 | 正确处理高 DPI 显示器 |

### 2.2 弹幕渲染
| 功能项 | 描述 |
|--------|------|
| 滚动弹幕 | 从右向左水平滚动的文字弹幕 |
| 自定义字体大小 | 支持设置弹幕字体大小（像素） |
| 自定义颜色 | 支持 RGBA 四通道颜色设置 |
| 自定义速度 | 支持设置弹幕移动速度（像素/秒） |
| Track 轨道系统 | 自动分配轨道，避免弹幕重叠 |
| 文本缓存 | 文本渲染结果缓存为纹理，避免重复渲染 |

### 2.3 多显示器支持
| 功能项 | 描述 |
|--------|------|
| 指定单个显示器 | 可选择弹幕仅在某一指定显示器上显示 |
| 指定多个显示器 | 可选择弹幕同时在多个显示器上显示 |
| 全部显示器 | 默认在所有显示器上显示弹幕（向后兼容） |
| 显示器枚举 | 提供枚举可用显示器的 API，便于调用方选择 |
| 独立轨道系统 | 每个被选中的显示器拥有独立的 Track 轨道系统，互不干扰 |
| 跨显示器滚动策略 | 弹幕在单个显示器范围内从右向左滚动并消失，不在显示器之间穿越（避免跨屏断裂） |

### 2.4 富文本弹幕（扩展）
| 功能项 | 描述 |
|--------|------|
| 文字+图片混合排版 | 单条弹幕内文字和图片 inline 混合排版，基线对齐 |
| Emoji 支持 | 弹幕中嵌入系统 emoji 渲染 |
| 图片弹幕 | 支持图片作为弹幕内容 |
| URL 加载 | 支持从 URL 加载图片 |
| 图片缓存 | 已加载的图片缓存为纹理，避免重复解码 |

### 2.5 持续推送
| 功能项 | 描述 |
|--------|------|
| 启动后推送 | 窗口启动（`run()` / `start()`）后，调用方仍可持续发送弹幕 |
| 线程安全 | 可从任意线程安全地发送弹幕和修改配置 |
| 解耦设计 | 渲染循环与弹幕推送互相独立，不阻塞 |
| 富文本内容 | 支持推送包含文字、图片、emoji 混合内容的弹幕 |

---

## 3. 技术架构需求

### 3.1 技术栈
| 组件 | 技术选型（待定，见实现方案对比） |
|------|--------------------------------|
| 语言 | Rust (edition 2021) |
| 图形 API | 待定：wgpu 29 / Direct2D (windows-rs) |
| 窗口管理 | 待定：winit 0.30 / raw Win32 |
| 文本渲染 | 待定：cosmic-text 0.19 / DirectWrite (windows-rs) |
| Windows API | windows-sys 0.61 |

### 3.2 Windows 平台特定需求
| 需求项 | 实现方式 |
|--------|----------|
| 透明窗口 | DirectComposition 仅 (`DxgiFromVisual` / `CreateDxgiSurfaceRenderTarget`)，**不设 `WS_EX_LAYERED`**（与 DComp 冲突，导致白色矩形遮挡） |
| 鼠标穿透 | 窗口子类化拦截 `WM_NCHITTEST` 返回 `HTTRANSPARENT`，**不设 `WS_EX_TRANSPARENT`**（与 DComp 冲突） |
| 无任务栏 | `WS_EX_TOOLWINDOW` |
| 不获取焦点 | `WS_EX_NOACTIVATE` |
| 图形后端 | DX12 (Vulkan 不支持透明窗口合成) |
| Alpha 合成 | `PreMultiplied` alpha mode |

### 3.3 渲染管线
| 组件 | 描述 |
|------|------|
| Surface 配置 | DX12 + `DxgiFromVisual` (DirectComposition) |
| Alpha 模式 | `CompositeAlphaMode::PreMultiplied` |
| Blend 状态 | `PREMULTIPLIED_ALPHA_BLENDING` |
| Shader | WGSL，片元着色器做预乘 alpha |
| 帧率 | 基于 `request_redraw` 的持续渲染循环 |

---

## 4. API 设计需求

### 4.1 主入口 API（持续推送模式）

```rust
// 创建弹幕应用
let config = OverlayTxtConfig {
    track_height: 40.0,      // 轨道高度
    default_font_size: 28.0, // 默认字体大小
    default_speed: 150.0,    // 默认速度
    default_color: [255, 255, 255, 255], // RGBA
};
let app = OverlayTxt::new(config)?;

// 启动窗口和渲染循环（非阻塞）
app.start()?;

// 启动后任意线程持续推送弹幕
app.send_text("Hello World");
app.send_text_custom("自定义", 32.0, [255, 100, 100, 255], 200.0);

// 在其他线程持续发送
std::thread::spawn(move || {
    loop {
        app.send_text("实时弹幕");
        std::thread::sleep(Duration::from_millis(500));
    }
});

// 调用方可继续做自己的事
do_other_work();

// 可选：等待结束
app.wait()?;
```

### 4.2 多显示器 API（示例设计）
```rust
// 枚举可用显示器
let monitors = app.list_monitors(); // 返回 Vec<MonitorInfo>

// 方案 A：配置时指定目标显示器
let config = OverlayTxtConfig {
    // ... 其他配置
    target_monitors: MonitorTarget::All,              // 全部显示器（默认）
    // 或
    target_monitors: MonitorTarget::Single(0),        // 仅主显示器
    // 或
    target_monitors: MonitorTarget::Multiple(vec![0, 2]), // 同时在第 0、2 号显示器显示
};

// 方案 B：运行时动态切换
app.set_target_monitors(MonitorTarget::Multiple(vec![0, 1]));
```

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `target_monitors` | `MonitorTarget` | `All` | 弹幕目标显示器选择 |

`MonitorTarget` 枚举：
- `All` — 所有显示器
- `Single(usize)` — 指定单个显示器（索引）
- `Multiple(Vec<usize>)` — 指定多个显示器

### 4.3 配置项
| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `track_height` | `f32` | 40.0 | 弹幕轨道高度 |
| `default_font_size` | `f32` | 28.0 | 默认字体大小 |
| `default_speed` | `f32` | 150.0 | 默认移动速度 |
| `default_color` | `[u8; 4]` | `[255, 255, 255, 255]` | RGBA 颜色 |
| `target_monitors` | `MonitorTarget` | `All` | 弹幕目标显示器选择 |

### 4.4 持续推送线程安全契约

持续推送 API 必须满足以下线程安全要求：

| 契约 | 说明 |
|------|------|
| `Send + Sync` | `OverlayTxt` 类型必须实现 `Send + Sync`，可在任意线程间转移和共享引用 |
| 内部同步 | 内部使用 Windows 消息队列 (`PostMessage`) 或锁机制，调用方无需额外同步 |
| 调用方无阻塞 | `send_*` 方法不阻塞调用方，弹幕内容通过内部通道/消息传递到渲染线程 |
| 跨线程安全 | 任意数量线程可同时调用 `send_*` 而不会导致数据竞争 |

实现对比：

| 机制 | 优点 | 缺点 | 适用场景 |
|------|------|------|---------|
| **PostMessage** | 天然线程安全，零拷贝（传指针），不阻塞 | 大块数据需 GMEM 分配 | 通用 |
| **mpsc channel** | 纯 Rust，类型安全 | 事件循环需额外 poll | wgpu 方案 |
| **Arc<Mutex<Vec>>** | 实现简单 | 锁竞争，渲染线程需锁定 | 简单场景 |

### 4.5 富文本弹幕 API（示例设计）

```rust
// 富文本弹幕：文字、图片、emoji inline 混合排版
app.send_rich(RichDanmaku {
    elements: vec![
        RichElement::Image { data: img_bytes, width: 24.0, height: 24.0 },
        RichElement::Text { content: "主播好厉害".into(), font_size: 28.0, color: [255; 4] },
        RichElement::Emoji { codepoint: 0x1F600, font_size: 28.0 },
        RichElement::Text { content: " 666".into(), font_size: 28.0, color: [255; 4] },
    ],
    speed: 150.0,
});
```

```rust
/// 富文本弹幕的 inline 元素
enum RichElement {
    /// 纯文本片段
    Text { content: String, font_size: f32, color: [u8; 4] },
    /// 图片
    Image { data: Vec<u8>, width: f32, height: f32 },
    /// Emoji
    Emoji { codepoint: u32, font_size: f32 },
}

/// 富文本弹幕内容
struct RichDanmaku {
    elements: Vec<RichElement>,
    speed: f32,
    // track_height, font_size 等继承自默认配置
}
```

---

## 5. 性能需求

| 需求项 | 目标 |
|--------|------|
| 帧率 | 60 FPS 稳定 |
| 弹幕数量 | 支持同时渲染 100+ 条弹幕 |
| 内存 | 文本纹理缓存，及时清理已消失弹幕的纹理 |
| CPU 占用 | 渲染逻辑尽量在 GPU 上完成 |

---

## 6. 已知问题与待解决

| 问题 | 描述 | 优先级 |
|------|------|--------|
| DPI 缩放 | DirectComposition visual 大小在高 DPI 下可能不正确 | 高 |
| 窗口大小 | 当前窗口创建逻辑可能需要改进 | 中 |

---

## 7. 未来扩展方向

| 方向 | 描述 |
|------|------|
| 更多弹幕类型 | 顶部固定弹幕、底部固定弹幕、逆向弹幕等 |
| 弹幕过滤 | 按关键词、颜色、用户过滤 |
| 跨平台支持 | macOS/Linux 支持（需不同的透明窗口方案） |
| 插件系统 | 允许自定义弹幕行为和渲染效果 |

---

## 8. 文档与示例

| 内容 | 描述 |
|------|------|
| API 文档 | rustdoc 生成的标准文档 |
| 示例程序 | `examples/basic.rs` 基础使用示例 |
| 需求文档 | 本文档 `docs/requirements.md` |

---

*文档版本: v0.2.0*
*创建时间: 2026-07-05*
*最近更新: 2026-07-05 (新增持续推送、富文本弹幕需求；修正 Windows 平台实现细节)*