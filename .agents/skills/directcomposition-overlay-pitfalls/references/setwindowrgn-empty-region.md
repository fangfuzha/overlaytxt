# SetWindowRgn 空区域导致 DComp 内容不可见

**现象**：鼠标穿透成功，但 DComp 内容也不可见。

**原因**：DComp 通过 `CreateTargetForHwnd` 绑定到窗口，窗口的可见区域决定 DComp 视觉内容的显示区域。`SetWindowRgn` 设置空区域会让 DComp 内容也不可见。

**解决**：不要用 `SetWindowRgn` 实现鼠标穿透。使用 `WS_EX_NOREDIRECTIONBITMAP` + `WM_NCHITTEST → HTTRANSPARENT` 或 `WS_EX_LAYERED | WS_EX_TRANSPARENT` 替代。