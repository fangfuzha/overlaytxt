# SetLayeredWindowAttributes 必须设置 alpha=255

**现象**：使用 `WS_EX_LAYERED` 创建窗口后，DComp 内容完全不可见。

**原因**：MSDN 明确指出分层窗口的渲染行为：
> "The window is a layered window. A layered window is not rendered until you call SetLayeredWindowAttributes or UpdateLayeredWindow."

因此分层窗口默认 alpha=0（完全不可见），DComp 内容渲染在分层窗口上，但窗口本身 alpha=0 导致整体透明。

**解决**：创建窗口后必须调用：
```rust
SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
```
- 设置 alpha=255 让 DComp 内容可见
- per-pixel alpha 仍由 DComp premultiplied alpha 处理，透明区域保持透明

**参考**：
- [SetLayeredWindowAttributes (MSDN)](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes)