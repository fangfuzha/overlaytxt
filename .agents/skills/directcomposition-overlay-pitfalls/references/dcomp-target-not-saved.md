# IDCompositionTarget 未保存导致 DComp 内容不显示

**现象**：DComp 初始化成功但屏幕完全看不到任何内容。

**原因**：`IDCompositionTarget` 是 DComp 与窗口的绑定句柄，作为局部变量时函数结束触发 Drop 导致绑定解除，此后 `Commit` 无效。

**解决**：必须将 `IDCompositionTarget` 保存在渲染器结构体字段中，与渲染器同生命周期。

> 注意与 `ID2D1DeviceContext`、`IDXGISwapChain1` 等对象的区别：这些对象 Drop 后只是内部状态释放，只要 DComp visual 还持有引用就不会影响显示。但 `IDCompositionTarget` 是**绑定层的生命周期控制器**，它的销毁意味着整层绑定消失。

**参考**：
- [CreateTargetForHwnd (MSDN)](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-createtargetforhwnd)