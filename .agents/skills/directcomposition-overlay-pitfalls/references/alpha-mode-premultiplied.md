# DirectComposition Alpha 模式必须为 PreMultiplied

**现象**：`CreateSwapChainForComposition` 创建 swapchain 成功，但 `surface.configure` 或 Present 时报 `DXGI_ERROR_INVALID_CALL`（0x887A0001）。

**原因**：DComp 只支持 `DXGI_ALPHA_MODE_PREMULTIPLIED` 和 `DXGI_ALPHA_MODE_OPAQUE`。使用 `DXGI_ALPHA_MODE_POST_MULTIPLIED` 会导致 swapchain 与 DComp 合成引擎不兼容。

**解决**：始终使用 `DXGI_ALPHA_MODE_PREMULTIPLIED`，确保 swapchain、D2D bitmap、Clear 颜色三者 alpha 模式一致。D2D 渲染时使用 `BlendState::PREMULTIPLIED_ALPHA_BLENDING`，shader RGB 值预乘 alpha。