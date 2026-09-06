# codec

默认构建只提供 CodecLimits 和 checksum_bytes。`prototype` feature 才暴露 CodecWorkspace/CodecResource 与解码供应商接缝。

当前没有实际 LZ4/Zstd 解码器。非空且满足输入限制的请求明确返回 CapabilityUnavailable，不伪造 frame 大小或声称已经识别截断帧。CodecLimits 的 expansion policy 有独立测试，不能代替真实 decoder 的解压炸弹防护测试。

all-features CI 仍运行原型测试。后续接入实际解码供应商时，必须在分配之前限制输出，并用真实压缩向量、损坏输入、膨胀限制与资源回收验证。领域 Schema 与 Serializer 不属于本模块。
