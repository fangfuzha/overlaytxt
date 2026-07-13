# D2D bitmap alphaMode 必须与 swapchain 一致（PREMULTIPLIED）

**现象**：弹幕内容可见，但窗口背景不透明（全黑或全白），看不到桌面/下方窗口。

**原因**：`D2D1_ALPHA_MODE_IGNORE` 强制拒绝 alpha 通道，所有像素 alpha 被置为 1.0（完全不透明）。即使 swapchain 创建为 `DXGI_ALPHA_MODE_PREMULTIPLIED`，D2D bitmap 用 `IGNORE` 就会覆盖 swapchain 的 alpha 模式，导致透明区域变成不透明。

**解决**：D2D bitmap 的 `alphaMode` 必须与 swapchain 一致。swapchain 用 `DXGI_ALPHA_MODE_PREMULTIPLIED`，D2D bitmap 就必须用 `D2D1_ALPHA_MODE_PREMULTIPLIED`：

```rust
let bmp_props = D2D1_BITMAP_PROPERTIES1 {
    pixelFormat: D2D1_PIXEL_FORMAT {
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
    },
    dpiX: 96.0,
    dpiY: 96.0,
    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    colorContext: core::mem::ManuallyDrop::new(None),
};
```