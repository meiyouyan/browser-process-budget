# browser-process-budget (`bpb`)

在受限 Windows 主机（尤其是 **RDS / 远程桌面**）上，度量并归因 Chromium 系浏览器的进程与内存开销。

English: [README.md](README.md).

## 为什么做

站点隔离提升安全，但会放大渲染进程数量。在 RDS、瘦客户机、低性能办公终端上，进程数和工作集往往先拖垮体验。本项目是 **RDS 进程预算实验室**：保留站点隔离，对进程复用 / 密度策略做 A/B，把开销变成**可复现数据**。不是另做浏览器，也**默认不建议**关闭隔离、沙箱或强制单进程。

框架说明（问题、流水线、模块、原则、组件地图）：

- 中文: [docs/zh/architecture.md](docs/zh/architecture.md) · [安全](docs/zh/security.md) · [路线图](docs/zh/roadmap.md)
- English: [docs/en/architecture.md](docs/en/architecture.md) · [security](docs/en/security.md) · [roadmap](docs/en/roadmap.md)

## 状态

**v0.1 — Linux 上 classify/report 可用；Windows 采样器供 RDS；bench 支持进程模型 profile A/B。**

| 命令 | Linux（本机 / CI） | Windows RDS（主目标） |
| --- | --- | --- |
| `bpb classify` | 可用 | 可用 |
| `bpb report` | 可用（JSON + Markdown，可选脱敏） | 可用 |
| `bpb sample` | **仅冒烟**：通过 `/proc` 采集 google-chrome / chromium | 原生 Toolhelp + 工作集 + 会话 ID |
| `bpb bench` | 场景 TOML → 空闲 → sample→classify→report；可选 `[[profiles]]` A/B + `compare.*`；尽力启动浏览器 | 同上（经常见 Chrome/Edge 路径启动） |

Windows API 位于 `cfg(windows)`，Linux CI 不会跑到。生产数据请在 RDS 主机上执行 `bpb sample` / `bpb bench`。文档不编造基准数字。

**当前关键组件：** bench 中的 profile A/B — 见 [架构 §6](docs/zh/architecture.md#6-profile-ab-设计bench)。

## CLI

```bash
bpb sample --browser chrome --out sample.json
bpb classify --input sample.json --out classified.json
bpb report --input classified.json --out-dir ./out --redact
bpb bench --scenario office-basic --browser chrome --out-dir ./out
```

### 进程模型 A/B（profiles）

```bash
# 三个 profile → out/baseline、out/process-per-site、out/renderer-limit-4，以及 out/compare.json|md
cargo build
./target/debug/bpb bench --scenario office-basic-ab --browser chrome --out-dir ./out-ab

# 多 profile 场景中只跑其中一个
./target/debug/bpb bench --scenario office-basic-ab --browser chrome --out-dir ./out-ab --profile baseline
```

`scripts/office-basic.toml` 仍无 profiles（单次运行，产物在 `out_dir` 根目录）。`scripts/office-basic-ab.toml` 声明 baseline / `--process-per-site` / `--renderer-process-limit=4`，空闲时间较短便于 CI。若要在 RDS 上拉开进程模型差异（仍保留站点隔离），请用 **`office-dense-ab`**（6–8 个多源 URL，`idle_seconds = 20`）；可选 **`office-dense-ab-quick`**（4 个 URL，空闲 3 秒）做测试机快速冒烟。多源标签在各 profile 下渲染进程数往往接近；要拉开 **`--process-per-site` 同站合并**效果，请用 **`office-samesite-ab`**（同一 eTLD+1 上 6–8 个维基百科页面，空闲 20 秒）或 **`office-samesite-ab-quick`**（4 个同站 URL，空闲 3 秒）。多 profile 时每组使用独立 `--user-data-dir`，并在组间停止匹配的浏览器进程、按该目录过滤采样，避免进程叠采。默认 profile **均不**关闭站点隔离。

```bash
# 更密的多源 A/B（面向 RDS 的空闲时间）
./target/debug/bpb bench --scenario office-dense-ab --browser chrome --out-dir ./out-dense-ab

# 更快的冒烟孪生场景
./target/debug/bpb bench --scenario office-dense-ab-quick --browser chrome --out-dir ./out-dense-ab-quick

# 同站多标签 A/B（站点隔离下的 process-per-site 合并）
./target/debug/bpb bench --scenario office-samesite-ab --browser chrome --out-dir ./out-samesite-ab

# 更快的同站冒烟孪生场景
./target/debug/bpb bench --scenario office-samesite-ab-quick --browser chrome --out-dir ./out-samesite-ab-quick
```

### 夹具冒烟（无需真实浏览器）

```bash
cargo build --release
./target/release/bpb classify --input examples/sample-fixture.json --out /tmp/bpb-classified.json
./target/release/bpb report --input /tmp/bpb-classified.json --out-dir /tmp/bpb-out --redact
# 或: ./scripts/smoke-fixture.sh
```

### 可选：Linux 实机冒烟

```bash
bpb sample --browser chrome --out /tmp/sample.json
bpb classify --input /tmp/sample.json --out /tmp/classified.json
bpb report --input /tmp/classified.json --out-dir /tmp/bpb-live --redact
```

## 目录

```
src/           Rust CLI（sample / classify / report / bench+profiles / model）
docs/zh|en/    架构、安全、路线图（中英）
scripts/       基准场景（office-basic、office-basic-ab、office-dense-ab、office-samesite-ab 等）
examples/      示例夹具
```

## 目标环境

- 主：Windows RDS + Chrome
- 次：Edge
- Linux 采样：仅冒烟 / 流水线测试
- 文档：中英双语

## 许可证

MIT
