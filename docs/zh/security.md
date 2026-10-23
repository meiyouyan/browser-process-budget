# 安全：量什么 / 永不建议什么

bpb 是**度量实验室**，不是策略锤子。本页用白话划清边界。操作者顶层策略亦见根目录 [`SECURITY.md`](../../SECURITY.md)。

## 我们量什么

在**保持站点隔离开启**的前提下，度量并对比：

| 旋钮 / 信号 | 为何与 RDS 相关 |
| --- | --- |
| 进程数与角色构成 | Renderer 膨胀 vs Browser/GPU/Utility 基线 |
| 工作集（以及后续私有/PSS 类） | 跨会话内存压力 |
| 启动 profile（`[[profiles]]` 参数） | 进程复用密度（`--process-per-site`）、软 Renderer 上限（`--renderer-process-limit`）、未来的 Memory Saver / 扩展集合 |
| 会话 ID（Windows） | 共享主机上的按用户进程树（聚合规划中） |

Profile 的 marker 会记录 `args` 与 `security_notes`，保证一次运行可审计：启动了什么，以及隔离未被静默关闭。

## 可接受的度量权衡（不是「可盲目上线的默认」）

以下可作为**显式** A/B profile 出现，用于度量：

- `--process-per-site` — 合并同站点 Renderer；增加**站点内命运共享**；**不**关闭站点隔离。
- `--renderer-process-limit=N` — 软上限；负载下可能提高标签争用 / 命运共享。
- 未来：Memory Saver / discard 相关与托管扩展 profile —— 一律写明权衡。

采用任何密度旋钮前，操作者仍须在**自己的 RDS 镜像**上实测。

## 永不作为默认推荐

在共享多用户主机（RDS）上，**不要**把下列项当作推荐默认：

| 标志 / 做法 | 原因 |
| --- | --- |
| `--disable-site-isolation-trials` / 关闭站点隔离 | 跨站失陷的爆炸半径 |
| `--no-sandbox` | 去掉核心进程沙箱 |
| `--single-process` | 坍塌隔离边界 |
| 工具静默改策略 | 未经显式、带风险标注的操作者选择，bpb 不得削弱安全 |

若受控实验场景中出现此类标志，marker 会打出 **WARNING**，文档仍拒绝将其作为默认。

## 数据处理

- 面向公开的报告应**脱敏**主机名、用户名、内网 URL（`bpb report --redact`）。
- 仅用于你有权管理的系统。
- 默认姿势是只读采样。

## 相关文档

- [架构说明](architecture.md)
- [路线图](roadmap.md)
