# lumio-kernel

> `lumio-kernel`：error / capability / handle / memory / kernel-context 五个核心模块的编译承载，生命周期根。

**RepositoryDeliveryPhase**：Foundation  
**ImplementationPriority**：I0  

本 crate 拥有 NativeCore 基础原语与生命周期根（[ADR 0002](../../.spec/decisions/0002-kernel-context-lifecycle-root.md)）：定义 `ContextResource` port，`lumio-job` / `lumio-spatial` / `lumio-codec` / `lumio-hfsm` 实现该 port 并注册进 Context。

## 承载的 5 个子模块

为了避免循环依赖并提供统一底层抽象，以下 5 个核心模块聚合在 `lumio-kernel` 内：

| 子模块 | 说明 | 对应源码与契约 |
| --- | --- | --- |
| **`error`** | 统一 NativeCore 内部错误类别、错误码承载和诊断载荷边界 | [`src/error/README.md`](src/error/README.md) |
| **`capability`** | 声明并校验平台、Feature 与资源上限，键为不透明数值 | [`src/capability/README.md`](src/capability/README.md) |
| **`handle`** | 带 `Index + Generation + Context` 校验的不透明 Handle 生命周期 | [`src/handle/README.md`](src/handle/README.md) |
| **`memory`** | 管理调用方 Buffer、Allocator 边界、受限内存池和 Native 资源统计 | [`src/memory/README.md`](src/memory/README.md) |
| **`kernel-context`** | NativeCore 的生命周期根，统一拥有域内跨调用资源并裁决关闭时序 | [`src/context/README.md`](src/context/README.md) |

## 相关设计规范

- [FFI Buffer 所有权契约](../../docs/specs/ffi-buffer-ownership.md)
- [KernelContext 生命周期契约](../../docs/specs/kernel-context-lifecycle.md)
- [仓库边界与架构契约](../../.spec/knowledge/standards/repository-architecture.md)
- [根 README](../../README.md)
