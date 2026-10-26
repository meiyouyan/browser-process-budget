# 架构说明

`browser-process-budget`（**bpb**）定位为 **RDS 进程预算实验室**：在**保留站点隔离（Site Isolation）**的前提下，对 Chromium 的进程复用 / Memory Saver / 扩展 / 会话密度等策略做 **A/B**，产出**可复现数字**。

本项目**不是**浏览器分支，**不**把关闭站点隔离、`--no-sandbox`、`--single-process` 当作默认建议。Blink/Servo 的 Rust 重写与 Chromium fork **延后**。

## 1. 问题

在 Windows **RDS**（及类似多会话主机）上，Chromium 进程模型会放大成本：

| 因子 | 为何在 RDS 上痛 |
| --- | --- |
| **进程数** | 站点隔离 → 大量 Renderer；每个会话再挂一棵浏览器进程树 |
| **工作集（WS）** | 每进程基线在用户间累加 |
| **会话** | N 个并发用户 ≈ N ×（Browser + GPU + Utility + Renderer），除非策略审慎复用/合并 |

运维需要的是在**保持站点隔离**下、对密度与命运共享（fate-sharing）做**可度量**权衡，而不是“关掉隔离”的口口相传。

## 2. 流水线

```
sample  →  classify  →  report
                ↑
         bench（+ profiles / scenarios）
```

1. **sample** — 枚举 Chrome/Edge 进程；采集 PID、PPID、会话 ID（Windows）、工作集、命令行。只读。
2. **classify** — 按命令行启发式归类为 Browser / Renderer / GPU / Utility / Extension / Other。
3. **report** — 输出 `report.json` + `report.md`（可选主机名/用户名脱敏）。
4. **bench** — 加载 `scripts/` 下场景 TOML，可选空闲等待，可选尽力启动浏览器，再 sample→classify→report。若有 `[[profiles]]`，则**按 profile** 写入 `out_dir/<profile_id>/`，并生成 `compare.json` / `compare.md`。

## 3. 模块

| 模块 | 职责 | 状态 |
| --- | --- | --- |
| **sample** | 操作系统进程快照（`src/sample.rs`） | 已实现（Windows 主路径；Linux `/proc` 冒烟） |
| **classify** | 角色标注（`src/classify.rs`） | 已实现 |
| **report** | JSON + Markdown（`src/report.rs`） | 已实现 |
| **bench** | 场景运行器 + **profile A/B**（`src/bench.rs`） | 已实现（本版本关键组件） |
| **scenarios** | `scripts/` 下 TOML 场景 | 已实现（`office-basic`、`office-basic-ab`、`office-dense-ab` 等） |
| **profiles（A/B）** | 场景 TOML 中声明的启动参数集 | **当前已实现** |
| **session aggregation** | 跨会话 RDS 汇总 | 规划中（见路线图） |
| **private-WS attribution** | 更强的私有/PSS 类归因 | 规划中（见路线图） |

共享 serde 类型在 `src/model.rs`。CLI 在 `src/main.rs`（`bpb` 二进制）。

## 4. 原则

1. **默认只读** — 采样与报告不改 OS/浏览器策略；除非操作者显式进入有文档、带风险标注的启动 profile。
2. **不静默削弱站点隔离** — 默认 profile 保留隔离；文档与 marker 明确反对将 `--disable-site-isolation-trials`、`--no-sandbox`、`--single-process` 作为默认。
3. **先度量** — 在推荐密度旋钮（`--process-per-site`、`--renderer-process-limit`、Memory Saver、扩展策略）之前，先用 A/B 数字说话。
4. **文档不编造基准数字** — 只发布在目标主机上真实跑出的结果。

## 5. 组件地图

| 关注点 | 路径 |
| --- | --- |
| CLI | `src/main.rs` |
| 数据模型 | `src/model.rs` |
| 采样（Win/Linux） | `src/sample.rs` |
| 分类 + 单测 | `src/classify.rs` |
| 报告 + 脱敏 | `src/report.rs` |
| Bench + **profile A/B** + compare | `src/bench.rs` |
| 场景 | `scripts/*.toml` |
| 夹具冒烟 | `examples/sample-fixture.json`、`scripts/smoke-fixture.sh` |
| 框架文档 | `docs/zh/*`、`docs/en/*` |
| 顶层安全策略 | `SECURITY.md` |

**当前已实现的关键组件：** bench 中的进程模型 **profile A/B** — 场景声明 `[[profiles]]`（`id` / `description` / `args`）；每个 profile 对应 `out_dir/<id>/`（marker、sample、classified、report）；多 profile 运行后写出 `out_dir/compare.json` 与 `compare.md`。

## 6. Profile A/B 设计（bench）

场景片段示例：

```toml
id = "office-basic"
browser = "chrome"
urls = ["https://example.com"]
idle_seconds = 5

[[profiles]]
id = "baseline"
description = "Default Site Isolation"
args = []

[[profiles]]
id = "process-per-site"
description = "Same-site consolidation via --process-per-site"
args = ["--process-per-site"]

[[profiles]]
id = "renderer-limit-4"
description = "Soft renderer cap"
args = ["--renderer-process-limit=4"]
```

行为：

- **无 `[[profiles]]`** → 单次运行；产物落在 `out_dir/` 根目录（向后兼容）。
- **有 profiles** → 对每个 profile：使用独立 `--user-data-dir`（`out_dir/<id>/user-data`），组前/组后停止命令行匹配该目录的浏览器进程；写 `scenario-marker.json`（含 `profile_id`、`args`、`launch_args`、`user_data_dir`、`security_notes`），空闲，再 sample（按 user-data-dir 过滤）→classify→report 到 `out_dir/<profile_id>/`；最后汇总 `compare.json` / `compare.md`（`process_count`、`total_ws_bytes`、按角色聚合）。
- **启动** — 若 `urls` 或 `args` 非空，尽力启动 Chrome/Edge（Windows 常见路径；Linux 的 `google-chrome` / `chromium`）。启动失败只打警告，**仍采样**已有进程（不使整次 bench 失败）。
- **CLI** — `bpb bench --scenario … --browser … --out-dir …`；多 profile 时可用 `--profile <id>` 只跑其中一个。

Marker/文档中的安全立场：`--process-per-site` 与 renderer 上限是在站点隔离**约束内**权衡命运共享/争用的度量旋钮，不是关闭隔离的许可。

## 7. 后续组件（仅占位）

见 [roadmap.md](roadmap.md)。近期占位（此处未实现）：多会话预算汇总、PSS 类私有归因。更丰富办公场景：已提供 `office-dense-ab`（多源标签）；后续可再扩展 SaaS 集合。

## 相关文档

- [安全：量什么 / 永不建议什么](security.md)
- [路线图](roadmap.md)
