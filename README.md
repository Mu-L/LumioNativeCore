# LumioNativeCore

跨项目复用的纯 Rust 内核源码库。项目社区与整体介绍见 [LumioGames](https://github.com/LumioGames)。

## 这个仓是什么

提供通用资源身份、有限调度、定时和空间计算。ABI schema、C 导出、绑定与动态库聚合由 LumioGameEngine 的 `engine/abi/native-abi.json` 和 SDK 插头拥有；本仓不恢复任何 ABI 镜像或独立 native-ffi。

## crate 一览

| crate | 当前实现范围 | 交付边界 | 模块文档 |
| --- | --- | --- | --- |
| `lumio-platform` | 可注入单调时钟、Deadline | Rust 原语；不进入权威状态 Hash | — |
| `lumio-kernel` | Handle、字节预算、资源登记、可续推进关闭 | Rust 实现；公开源码 API 可演进，不承诺二进制 Rust ABI | [`kernel`](crates/lumio-kernel/README.md)（[`error`](crates/lumio-kernel/src/error/README.md) · [`capability`](crates/lumio-kernel/src/capability/README.md) · [`handle`](crates/lumio-kernel/src/handle/README.md) · [`memory`](crates/lumio-kernel/src/memory/README.md) · [`context`](crates/lumio-kernel/src/context/README.md)） |
| `lumio-job` | 实际 Kernel 执行、有界队列/未回收任务、协作取消、结果与回收 | 实现与回归已加入；跨仓与长期负载另行验收 | [`job`](crates/lumio-job/README.md) |
| `lumio-timer` | 单调毫秒/逻辑刻度、排队与消费、有限补发预算 | 有 SDK 消费路径；本次修改后必须重编验证 | [`timer`](crates/lumio-timer/README.md) |
| `lumio-spatial` | AABB 更新/删除/批量交叠，真实 rstar 与独立参考后端 | 不声称完整 BVH、距离或连续碰撞已实现 | [`spatial`](crates/lumio-spatial/README.md) |
| `lumio-hfsm` | 无状态 HFSM 迁移计算器：数据定义状态图、宿主持 Snapshot、批量迁移计划 | 实施中 | [`hfsm`](crates/lumio-hfsm/README.md) |
| `lumio-codec` | 默认只有字节校验和限制值；压缩接缝需 prototype | 尚非可用 LZ4/Zstd 解码器 | [`codec`](crates/lumio-codec/README.md) |
| `lumio-diagnostics` | prototype 下的有界记录器 | 未集成通用 RecordPort | [`diagnostics`](crates/lumio-diagnostics/README.md) |
| `lumio-test-support` | 测试时钟、交错辅助 | dev-only | — |
| `xtask` | Cargo metadata 检查启动器 | 真实成员、可选/平台/build 依赖与产物类型 | — |

模块存在、编译通过、消费者通过、负载通过是四种不同结论。[修复记录与迁移说明](docs/reviews/2026-09-06-native-remediation.md)列出实现范围与验证边界。

## SDK 如何编入

LumioGameEngine 的 SDK 用 Cargo path dependency 编入本仓。稳定源码边界是公开 Rust API；开发期允许破坏式演进。修改后应在架构仓 `engine/native` 重建并测试 `lumio-engine-native`，实际验证 Native 装载与调用；旧产物存在或本仓单测不能替代这一步。

调用方必须保留 KernelContext 的 Arc 作为生命周期根；JobSystem 只持有 Weak Context，避免资源注册形成引用环。反复驱动 close，直到 Closed 或明确处理错误；Drop 不提供同步关闭完成保证。

## 职责

提供通用 Handle/Buffer、有限调度与合作式取消、错误类别、定时结果排序和 AABB 计算。资源上限在实际拥有者上执行；结果转移后字节租约随结果存活。Job/Completion 不无限保留已回收历史。

## 明确不负责什么

不拥有 World、ECS、GAS、Voxel、网络、账户、Session 或 Host 策略，不回调托管委托，不维护第二份 ABI。Bot 发言、心跳、重连时长仅留在显式 test-support 夹具中，不进入默认 Timer 构建。

## 收口门槛

Rust 版本由 `rust-toolchain.toml` 锁定；Python 3 用于仓库工具，Node 用于 Agent 规范检查。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features
cargo test --workspace
cargo test --workspace --all-features
cargo test -p lumio-spatial --no-default-features
cargo xtask check-dep-dag
cargo xtask assert-no-native-artifacts
python -m unittest discover -s tools -p 'test_*.py'
node .spec/tools/spec-lint.mjs
node --test .spec/tools/spec-lint.test.mjs
```

Linux、Windows、macOS CI 执行本仓测试。feature-gated 夹具与原型仍在 all-features 步骤运行，不能当作默认生产能力。供应商版本见 Cargo.toml/Cargo.lock，批准的直接依赖见 tools/check_repository.py。Miri、实际平台负载与 SDK/Host 联测需各自的运行证据。
