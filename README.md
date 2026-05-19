# Gold Monitor

[![Release](https://img.shields.io/github/v/release/Hxyspace/gold-monitor)](https://github.com/Hxyspace/gold-monitor/releases)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](./LICENSE)
[![Build](https://github.com/Hxyspace/gold-monitor/actions/workflows/build.yml/badge.svg)](https://github.com/Hxyspace/gold-monitor/actions/workflows/build.yml)

Windows 黄金价格实时监控软件。

## 功能

- **悬浮窗口** — 半透明圆角浮窗，显示三种金价，可拖拽
- **鼠标穿透** — 悬浮窗可设为穿透模式，不遮挡操作
- **任务栏嵌入** — 在 Windows 任务栏内直接显示金价
- **系统托盘** — 右键菜单控制所有功能
- **便携运行** — 配置保存在 exe 同目录，无需安装

## 数据源

| 品种 | 代码 | 说明 |
|------|------|------|
| XAU | JO_92233 | 国际金价 (USD/oz) |
| AU9999 | JO_71 | 上海金价 (CNY/g) |
| PAXG | JO_350022 | PAXG 代币价格 (USD) |

数据来自金投网，10 秒刷新。

## 安装

从 [GitHub Releases](https://github.com/Hxyspace/gold-monitor/releases) 下载 `gold-monitor.exe`，放到任意目录直接运行。

## 从源码构建

```bash
# 需要 Rust 1.75+ 和 MSVC 工具链
cargo build --release
```

产物在 `target/release/gold-monitor.exe`。

## 使用

- **左键托盘图标** — 切换悬浮窗显示/隐藏
- **右键托盘图标** — 菜单（显示窗口 / 鼠标穿透 / 任务栏显示 / 退出）
- **拖拽悬浮窗** — 移动位置（自动保存）

## 技术栈

- Rust + `windows` crate (Win32 API)
- Direct2D + DirectWrite（悬浮窗抗锯齿渲染）
- DirectComposition + D3D11（任务栏嵌入渲染）
- reqwest（HTTP 请求）
- 单 exe，无运行时依赖

## 📄 License

[MIT License](./LICENSE)