# WS_EX_NOREDIRECTIONBITMAP 与 WS_EX_LAYERED 冲突

**现象**：窗口显示白色矩形遮挡，DComp 内容不可见。

**原因**：`WS_EX_NOREDIRECTIONBITMAP` 与 `WS_EX_LAYERED` 语义冲突：
- `WS_EX_NOREDIRECTIONBITMAP`：要求窗口无重定向位图（MSDN："The window does not render to a redirection surface. This is for windows that do not have visible content or that use mechanisms other than surfaces to provide their visual."）
- `WS_EX_LAYERED`：要求窗口有重定向位图以实现分层混合
- 两者语义冲突 → 白色矩形

**解决**：使用 `WS_EX_LAYERED | WS_EX_TRANSPARENT`，不要加 `WS_EX_NOREDIRECTIONBITMAP`。`WS_EX_LAYERED` 和 DComp 可以共存（TrafficMonitor 验证）。

**相关注意事项**：
- `WS_EX_NOREDIRECTIONBITMAP` **不实现鼠标穿透**，MSDN 明确它仅控制渲染表面，与命中测试无关
- 单独使用 `WS_EX_NOREDIRECTIONBITMAP | WS_EX_TRANSPARENT` 也无法实现穿透，因为 `WS_EX_TRANSPARENT` 需与 `WS_EX_LAYERED` 组合才生效

**参考**：
- [Extended Window Styles (MSDN)](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- [TrafficMonitor SetMousePenetrate 实现](https://github.com/zhongyang219/TrafficMonitor)