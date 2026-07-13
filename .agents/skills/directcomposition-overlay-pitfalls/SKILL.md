---
name: "directcomposition-overlay-pitfalls"
description: "Documents DirectComposition transparent overlay pitfalls (mouse passthrough, alpha, DComp binding, swapchain). Invoke when DComp overlay window has rendering/passthrough issues or user reports blank/opaque screen."
---

# DirectComposition 透明覆盖层窗口踩坑记录

## 坑位索引

Each reference file under `references/` documents one specific pitfall with 现象、原因、解决.

| # | 坑位 | 适用场景 |
|---|------|----------|
| 1 | [IDCompositionTarget 未保存](references/dcomp-target-not-saved.md) | DComp 初始化成功但屏幕完全空白 |
| 2 | [Alpha 模式必须为 PreMultiplied](references/alpha-mode-premultiplied.md) | `DXGI_ERROR_INVALID_CALL` 或 alpha 通道不生效 |
| 3 | [WS_EX_NOREDIRECTIONBITMAP 与 WS_EX_LAYERED 冲突](references/window-style-conflict.md) | 窗口显示白色矩形遮挡 |
| 4 | [SetLayeredWindowAttributes 必须 alpha=255](references/layered-window-alpha.md) | 分层窗口下 DComp 内容完全不可见 |
| 5 | [D2D bitmap alphaMode 必须与 swapchain 一致](references/bitmap-from-surface-alpha.md) | 弹幕可见但背景不透明（全黑/全白） |
| 6 | [DXGI 翻转交换链要求 BufferCount >= 2](references/buffer-count-flip.md) | Present 报 `DXGI_ERROR_INVALID_CALL` |
| 7 | [背景蒙版分割线](references/background-mask-divider.md) | 弹幕区有半透明蒙版和可见分割线 |
| 8 | [图片弹幕垂直位置偏高](references/image-vertical-align.md) | 图文混排中图片高于文字 |
| 9 | [WM_NCHITTEST 对 DComp 顶层窗口不可靠](references/wmnchittest-unreliable.md) | 鼠标穿透间歇性失效 |
| 10 | [Vulkan 后端不支持透明覆盖层](references/vulkan-overlay-limitation.md) | AMD Radeon 上透明窗口输出不透明 |
| 11 | [SetWindowRgn 空区域导致 DComp 内容不可见](references/setwindowrgn-empty-region.md) | 鼠标穿透但 DComp 内容也不可见 |
| 12 | [不能对 front buffer 创建 TARGET bitmap](references/front-buffer-target-bitmap.md) | `CreateBitmapFromDxgiSurface` 对 buffer 1 返回 `E_INVALIDARG` |

## 使用方式

When the user reports any of the symptoms above, check the corresponding reference file first. Each reference file contains the root cause and the fix. If the issue is not covered by existing references, investigate and add a new reference file + update this index.