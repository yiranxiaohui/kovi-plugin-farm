# kovi-plugin-farm

一个基于 [Kovi](https://github.com/Threkork/kovi) 框架的 QQ 机器人插件，用于自动化管理 QQ 农场。

## 功能

- **登录农场** — 生成 QQ 农场登录二维码，扫码后自动启动农场脚本
- **农场状态** — 查看当前农场脚本的运行状态和最近输出日志
- **多用户支持** — 每个用户独立运行各自的农场脚本，互不干扰

## 前置要求

- [Rust](https://www.rust-lang.org/) (Edition 2024)
- [Node.js](https://nodejs.org/) (需要 `node` 和 `npm` 在系统 PATH 中)
- 一个基于 Kovi 框架的 QQ 机器人实例

## 安装

在你的 Kovi 机器人项目中添加依赖：

```toml
[dependencies]
kovi-plugin-farm = "0.1"
```

或通过 crates.io 安装：

```bash
cargo add kovi-plugin-farm
```

## 使用方法

### 指令列表

| 指令 | 说明 |
|------|------|
| `登录农场` | 获取登录二维码链接，扫码完成登录后自动启动农场脚本 |
| `农场状态` | 查看当前脚本运行状态及最近 10 条输出日志 |

### 工作流程

1. 向机器人发送 `登录农场`
2. 机器人返回一个二维码链接，使用 QQ 扫码登录
3. 登录成功后，插件自动下载 [qq-farm-bot](https://github.com/ryunnet/qq-farm-bot) 脚本并启动
4. 发送 `农场状态` 可随时查看脚本运行情况

## 技术细节

- 使用 `DashMap` 实现并发安全的多用户状态管理
- 基于 `tokio` 异步运行时，每个用户的脚本在独立子进程中运行
- 自动从 GitHub 下载 qq-farm-bot 并执行 `npm install` 安装依赖
- 支持 Windows 和 Linux/macOS 跨平台运行

## 许可证

本项目采用 MIT 或 Apache-2.0 双许可证，你可以任选其一。
