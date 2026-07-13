# Vulkan 后端不支持透明覆盖层窗口

**现象**：使用 Vulkan 后端（AMD Radeon 专有驱动）时，`surface_caps.alpha_modes` 只返回 `Opaque`，透明窗口无法实现。

**原因**：AMD Radeon 专有驱动的 Vulkan 实现不支持 `VK_COMPOSITE_ALPHA_PRE_MULTIPLIED_BIT_KHR`，只能使用 `Opaque` 模式。使用 `Auto` 模式时驱动忽略 alpha 通道，输出不透明。

**解决**：对于透明覆盖层窗口，使用 DX12 或 D3D11 后端替代 Vulkan。Vulkan 只在支持 `VK_COMPOSITE_ALPHA_PRE_MULTIPLIED_BIT_KHR` 的驱动/NVIDIA 上可行。