# DXGI 翻转交换链要求 BufferCount >= 2

**现象**：`BufferCount: 1` 时编译通过，运行时 `Present` 报 `DXGI_ERROR_INVALID_CALL`（0x887A0001）。

**原因**：`DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL` 翻转模型要求至少 2 个缓冲区（前缓冲 + 后缓冲）。单缓冲区无法实现翻转操作。

**解决**：`BufferCount: 2` 是 DComp 场景下唯一可行的配置。单缓冲会引入撕裂，且 DComp 强制要求翻转模型。