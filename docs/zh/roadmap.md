# 路线图（3–6 个月）

研究方向不变：**保留站点隔离**；让 bpb 成为 RDS **进程预算实验室**，用可复现数字对进程复用 / Memory Saver / 扩展 / 会话密度策略做 A/B。**不要**把关闭站点隔离、沙箱或强制单进程做成默认。Blink/Servo 的 Rust 重写与 Chromium fork **不在范围内**。

里程碑为规划占位；时间粗估；**不编造**基准数字。

## 当前（已交付）

- [x] 流水线：sample → classify → report
- [x] Bench 场景（`scripts/*.toml`）
- [x] Bench 中的**进程模型 profile A/B**（`[[profiles]]`、按 profile 输出目录、`compare.json` / `compare.md`）
- [x] 框架文档（架构 / 安全 / 本路线图），中英双语

## 第 1–2 个月

- [ ] 加固 Windows RDS 采样（会话 ID 覆盖、边界情况）
- [x] 扩展办公场景（标签密度、常见 SaaS 页面）——`office-dense-ab`（及可选 `office-dense-ab-quick`）；同站多标签以拉开 process-per-site——`office-samesite-ab`（及 `office-samesite-ab-quick`）；仍仅度量
- [x] 可选将 Memory Saver / discard 相关标志做成**具名 profile**（`office-memory-ab` / `office-memory-ab-quick`；可选 Preferences 预置；discard 会使非活动标签页重载——可能低估稳态内存；永不关闭隔离）
- [ ] CI：夹具 classify/report + `office-basic-ab` 冒烟（Linux 上 0 个 Chrome 进程可接受）

## 第 3–4 个月

- [ ] **多会话预算**（占位）：按 Windows 会话 ID 聚合采样；人均与整机合计
- [ ] 扩展策略 profile（空 vs 托管集合），仍在站点隔离下
- [ ] Compare 体验：`compare.md` 更丰富的角色差分；可选 CSV
- [ ] RDS 主机操作手册（启动 → 空闲 → 采样 → 对比）

## 第 5–6 个月

- [ ] **私有 WS / PSS 类归因**（占位）：在 OS 允许处改进共享 vs 私有估计
- [ ] 会话密度场景（实验机上 N 个合成会话——仅实验室）
- [ ] 文档化的「带风险标注」profile 目录（明确警告；不静默削弱）
- [ ] 若 compare/会话字段稳定，冻结 v0.2 schema 说明

## 明确延后

- 将关闭站点隔离 / `--no-sandbox` / `--single-process` 作为推荐默认
- Blink 或 Servo 的 Rust 重写
- 维护 Chromium fork
- 无主机实测却编造宣传数字

另见 [architecture.md](architecture.md) 与 [security.md](security.md)。
