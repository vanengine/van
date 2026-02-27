<h1 align="center">Van</h1>

<p align="center">
  <strong>使用 Vue SFC 语法的语言无关模板引擎</strong><br>
  服务端 HTML 渲染 · 信号响应式客户端交互 · WASM 驱动
</p>

<p align="center">
  <a href="../../../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://github.com/vanengine/van/releases"><img src="https://img.shields.io/github/v/release/vanengine/van?include_prereleases" alt="Release" /></a>
  <a href="https://crates.io/crates/van-compiler"><img src="https://img.shields.io/crates/v/van-compiler" alt="Crates.io" /></a>
  <img src="https://img.shields.io/badge/platforms-linux%20%7C%20macOS%20%7C%20windows-lightgrey" alt="Platforms" />
</p>

<p align="center">
  <a href="#特性">特性</a> ·
  <a href="#安装">安装</a> ·
  <a href="#使用">使用</a> ·
  <a href="#示例">示例</a> ·
  <a href="#架构">架构</a> ·
  <a href="#从源码构建">构建</a>
</p>

<p align="center">
  🌐 <a href="../../../README.md">English</a> · <a href="README.md">简体中文</a>
</p>

---

## 特性

- **Vue SFC 语法** — 使用熟悉的 `<template>`、`<script setup>`、`<style scoped>` 编写模板
- **无 Node.js 依赖** — 核心由 Rust 编写，编译为 WASM 供后端集成
- **信号响应式** — 轻量客户端交互，直接 DOM 更新（~4KB 运行时，无虚拟 DOM）
- **框架无关** — WASM 编译器通过 JSON stdin/stdout 协议与任何后端集成
- **跨平台** — 预构建 WASM + 原生二进制（Linux x64/ARM64、macOS x64/ARM64、Windows x64）

## 安装

**一键安装**（Linux / macOS）：

```bash
curl -fsSL https://raw.githubusercontent.com/vanengine/van/main/install.sh | sh
```

**手动下载**：从 [GitHub Releases](https://github.com/vanengine/van/releases) 下载最新的 `van-cli-*` 二进制文件，放入 `PATH` 目录。

## 使用

```bash
van init my-project        # 创建新的 Van 项目
van dev                    # 启动开发服务器（热重载）
van generate               # 静态站点生成
```

### 框架集成

Van 通过 WASM 二进制将 `.van` 文件编译为 HTML，可集成到任何后端：

- **Spring Boot** — [van-spring-boot-starter](https://github.com/vanengine/van-spring-boot-starter)

底层 JSON 协议详见下方 [WASM 集成](#从源码构建)。

## 示例

```vue
<template>
  <h1>{{ title }}</h1>
  <button @click="count++">Clicked {{ count }} times</button>
</template>

<script setup>
let count = 0
</script>

<style scoped>
h1 { color: steelblue; }
</style>
```

服务端的 `{{ title }}` 由宿主框架插值；`count` 成为响应式信号，在客户端自动更新 DOM。

## 架构

```
.van 文件 → [van-parser] → VanBlock
                              ├── [van-compiler] → 服务端 HTML（含 {{ expr }}）
                              └── [van-signal-gen] → 信号响应式 JS（直接 DOM 操作）
```

**核心引擎**（`crates/`）

| Crate | 用途 |
|---|---|
| `van-parser` | 手写递归下降解析器，解析 `.van` 文件 |
| `van-compiler` | 编排服务端 HTML + 客户端 JS 编译 |
| `van-compiler-wasi` | WASM 入口（JSON stdin/stdout 协议） |
| `van-signal-gen` | `<script setup>` → 信号响应式直接 DOM JS |

**CLI 工具链**（`crates/van-cli/`）

| Crate | 用途 |
|---|---|
| `van-cli` | CLI 二进制（`van init`、`van dev`、`van generate`） |
| `van-context` | 项目上下文与配置 |
| `van-dev` | 开发服务器（热重载） |
| `van-init` | 项目脚手架 |

<a id="从源码构建"></a>
<details>
<summary><strong>从源码构建</strong></summary>

前置条件：[Rust 工具链](https://rustup.rs/)（1.70+）

```bash
# 构建所有 crate
cargo build --release

# 构建 CLI 二进制
cargo build --release -p van-cli

# 构建 WASM 二进制（用于框架集成）
cargo build --target wasm32-wasip1 -p van-compiler-wasi --release

# 运行测试
cargo test
```

</details>

<details>
<summary><strong>WASM 集成</strong></summary>

WASM 编译器通过 stdin 接收 JSON，返回编译后的 HTML：

```jsonc
// 输入
{ "entry_path": "pages/index.van", "files": { ... }, "mock_data_json": "..." }

// 输出
{ "ok": true, "html": "<h1>{{ title }}</h1>..." }
```

两种执行模式：

- **单次执行**（默认）— 读取 stdin，编译一次，写入响应
- **守护进程**（`--daemon`）— JSON Lines 协议，保持运行直到 stdin EOF

宿主框架执行第二轮处理，将 `{{ expr }}` 替换为服务端模型数据。

</details>

## 相关项目

- [**van-spring-boot-starter**](https://github.com/vanengine/van-spring-boot-starter) — Spring Boot 集成

## 许可证

[MIT](../../../LICENSE)
