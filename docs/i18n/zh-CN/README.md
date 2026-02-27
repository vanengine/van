<h1 align="center">Van</h1>

<p align="center">
  <strong>使用 Vue 语法的语言无关模板引擎</strong><br>
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
  <a href="#示例">示例</a>
</p>

<p align="center">
  🌐 <a href="../../../README.md">English</a> · <a href="README.md">简体中文</a>
</p>

---

## 特性

- **Vue 语法** — 使用熟悉的 `<template>`、`<script setup>`、`<style scoped>` 编写模板
- **信号响应式** — 轻量客户端交互，直接 DOM 更新（~4KB 运行时）

## 安装

**一键安装**（Linux / macOS）：

```bash
curl -fsSL https://raw.githubusercontent.com/vanengine/van/main/install.sh | sh
```

**手动下载**：从 [GitHub Releases](https://github.com/vanengine/van/releases) 下载最新的 `van-*` 二进制文件，放入 `PATH` 目录。

## 使用

```bash
van init my-project        # 创建新的 Van 项目
van dev                    # 启动开发服务器（热重载）
van generate               # 静态站点生成
```

### 框架集成

Van 通过 WASM 二进制将 `.van` 文件编译为 HTML，可集成到多个后端：

- **Spring Boot** — [van-spring-boot-starter](https://github.com/van-java/van-spring-boot-starter)

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

## 相关项目

- [**van-spring-boot-starter**](https://github.com/van-java/van-spring-boot-starter) — Spring Boot 集成

## 许可证

[MIT](../../../LICENSE)
