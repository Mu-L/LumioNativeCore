# Spatial 后端与预算

默认后端是真实 `rstar = 0.12.2`，通过 RStarIndexAdapter 隔离第三方类型；不再委托 GridReferenceIndex。关闭 `rstar-backend` feature 时，默认 SpatialContext 使用独立暴力参考实现。也可以通过 `with_backend` 显式注入后端。

选用已发布版本而非追逐 latest。上游 API 与许可来源：https://docs.rs/crate/rstar/0.12.2 与 https://docs.rs/rstar/0.12.2/rstar/struct.RTree.html 。依赖锁文件固定传递依赖；这不是已完成漏洞扫描或全平台性能验收的声明。

默认索引最多 65,536 个对象；两个具体后端都提供 with_capacity。批量查询默认最多 4,096 个 query、262,144 个 hit，先计算容量再生成有界暂存，失败不覆盖调用方 out。所有公开插入与查询路径均重新验证 AABB，不依赖调用方必须使用构造函数。查询结果按 ObjectId 排序；这只承诺结果顺序，不承诺第三方树内部布局跨平台相同。

SpatialResource 用 RwLock 保护实际索引，quiesce 观察查询借用结束，destroy 真正取走并释放索引。普通构造的 SpatialContext 属于调用者；使用 KernelContext 时应将 SpatialResource 注册为资源。

本次仅实现 AABB 插入/更新/删除、单个/批量交叠查询，不把完整 BVH、连续碰撞、邻域、距离核的规划记作已交付。性能报告须包括构建、更新、查询、排序、输出以及目标数据分布，不以局部算法名称代替吞吐证据。
