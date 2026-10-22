# browser-process-budget (`bpb`)

在受限 Windows 主机（尤其是 **RDS / 远程桌面**）上，度量并归因 Chromium 系浏览器的进程与内存开销。

English: [README.md](README.md).

## 为什么做

站点隔离提升安全，但会放大渲染进程数量。在 RDS、瘦客户机、低性能办公终端上，进程数和工作集往往先拖垮体验。本项目把开销变成**可复现数据**和策略对照——不是另做浏览器，也默认不建议关闭隔离。

## 状态

**v0.1 — Linux 上 classify/report 可用；Windows 采样器在 Windows 目标上编译，供 RDS 主机使用。**

| 命令 | Linux（本机 / CI） | Windows RDS（主目标） |
| --- | --- | --- |
| `bpb classify` | 可用 | 可用 |
| `bpb report` | 可用（JSON + Markdown，可选脱敏） | 可用 |
| `bpb sample` | **仅冒烟**：通过 `/proc` 采集 google-chrome / chromium | 原生 Toolhelp + 工作集 + 会话 ID |
| `bpb bench` | 加载 `scripts/*.toml`，空闲等待后 sample→classify→report（URL 启动跳过） | 同上 |

Windows API 位于 `cfg(windows)`，Linux CI 不会跑到。生产数据请在 RDS 主机上执行 `bpb sample`。

## CLI

```bash
bpb sample --browser chrome --out sample.json
bpb classify --input sample.json --out classified.json
bpb report --input classified.json --out-dir ./out --redact
bpb bench --scenario office-basic --browser chrome --out-dir ./out
```

### 夹具冒烟（无需真实浏览器）

```bash
cargo build --release
./target/release/bpb classify --input examples/sample-fixture.json --out /tmp/bpb-classified.json
./target/release/bpb report --input /tmp/bpb-classified.json --out-dir /tmp/bpb-out --redact
```

### 可选：Linux 实机冒烟

```bash
bpb sample --browser chrome --out /tmp/sample.json
bpb classify --input /tmp/sample.json --out /tmp/classified.json
bpb report --input /tmp/classified.json --out-dir /tmp/bpb-live --redact
```

## 目录

```
src/           Rust CLI
docs/zh|en/    架构与指南（中英）
scripts/       基准场景
examples/      示例夹具
```

## 目标环境

- 主：Windows RDS + Chrome
- 次：Edge
- Linux 采样：仅冒烟 / 流水线测试
- 文档：中英双语

## 许可证

MIT
