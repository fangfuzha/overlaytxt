# WM_NCHITTEST 对 DComp 顶层窗口不可靠

**现象**：通过 `WM_NCHITTEST → HTTRANSPARENT` 实现鼠标穿透，但弹幕区域有时仍拦截鼠标事件。

**原因**：DWM 合成层拦截了 DComp 顶层窗口的 hit testing。`HTTRANSPARENT` 设计用于**子窗口**（"命中父窗口"），对顶层窗口 DWM 合成层会干扰命中测试的传递。调试验证：`WM_NCHITTEST` 确实被调用并返回了 `HTTRANSPARENT`，但系统反复查询（5次以上），说明穿透未真正生效。

**纠正**：经后续排查，此问题主要发生在 `IDCompositionTarget` 未正确保存导致 DComp 内容不显示的时期。**在 `IDCompositionTarget` 正确保存后**，`WM_NCHITTEST → HTTRANSPARENT` 配合 `WS_EX_NOREDIRECTIONBITMAP` 对顶层窗口也能可靠工作。最初认为"`WM_NCHITTEST` 对顶层窗口不可靠"的结论实际上是因为 DComp 内容不可见导致的误判。

**解决**：
- 方案 A（推荐）：`WS_EX_NOREDIRECTIONBITMAP` + `WM_NCHITTEST → HTTRANSPARENT`
- 方案 B：`WS_EX_LAYERED | WS_EX_TRANSPARENT`（不需要 `WM_NCHITTEST`）
- `WS_EX_NOREDIRECTIONBITMAP` 不实现鼠标穿透，必须配合 `WM_NCHITTEST`

**失败方案**：单独使用 `WS_EX_NOREDIRECTIONBITMAP`（不实现穿透）、`WS_EX_LAYERED | WS_EX_TRANSPARENT` 但不设 alpha（弹幕不可见）

**参考**：
- [Extended Window Styles (MSDN)](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)