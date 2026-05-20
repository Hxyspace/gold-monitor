# Gold Monitor

[![Release](https://img.shields.io/github/v/release/Hxyspace/gold-monitor)](https://github.com/Hxyspace/gold-monitor/releases)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](./LICENSE)
[![Build](https://github.com/Hxyspace/gold-monitor/actions/workflows/build.yml/badge.svg)](https://github.com/Hxyspace/gold-monitor/actions/workflows/build.yml)

Windows 黄金价格实时监控软件，同时展示美元指数和美债收益率以辅助判断金价走势。

## 为什么显示美元指数和国债收益率

根据历史数据的 Pearson 相关性分析，以下指标与金价存在显著关联：

| 指标 | 与金价相关系数 | 含义 |
|------|:----------:|------|
| **美元指数 (DXY)** | -0.396 | 美元走强 → 以美元计价的黄金承压；CCF 分析显示 DXY 领先金价约 1 天 |
| **美债 10Y 收益率** | -0.307 | 收益率上升 → 持有黄金的机会成本上升 → 金价承压 |

> 相关性数据来源：[gold_analysis_report.md](https://github.com/Hxyspace/gold-monitor)

在盯盘时同步观察这两个指标，可以更早感知金价的潜在方向变化。

## 功能

- **悬浮窗口** — 半透明圆角浮窗，显示金价、美元指数、美债收益率，可拖拽
- **鼠标穿透** — 悬浮窗可设为穿透模式，不遮挡操作
- **任务栏嵌入** — 在 Windows 任务栏内直接显示金价
- **系统托盘** — 右键菜单控制所有功能
- **便携运行** — 配置保存在 exe 同目录，无需安装

## 显示参数说明

| 标签 | 参数 | 单位 | 说明 |
|------|------|------|------|
| **XA** | XAU/USD | USD/盎司 | 国际现货黄金价格（伦敦金） |
| **AU** | AU9999 | CNY/克 | 上海黄金交易所 Au99.99 合约 |
| **DX** | DXY | — | 美元指数，衡量美元对一篮子货币的综合强弱 |
| **Y0** | US10Y | % | 美国 10 年期国债收益率，末尾 `+N` / `-N` 表示较前日变动的基点 |
| **PA** | PAXG | USD | PAX Gold 代币价格（链上黄金） |

## 数据源

| 品种 | 来源 | 刷新频率 |
|------|------|:--------:|
| XAU、AU9999、PAXG | 金投网 (jijinhao.com) | 10 秒 |
| DXY 美元指数 | 新浪财经 (finance.sina.com.cn) | 10 秒 |
| US10Y 美债收益率 | 东方财富 (eastmoney.com) | 每小时缓存 |

> 美债收益率每日仅更新一次，故采用 1 小时缓存，避免对数据源造成不必要压力。

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

## 悬浮窗颜色说明

| 颜色 | 数据 |
|------|------|
| 金色 `#FFD700` | XA / AU / PA 黄金价格 |
| 青色 `#66E6FF` | DX 美元指数 |
| 浅绿 `#80FF80` | Y0 美债收益率 |

## 技术栈

- Rust + `windows` crate (Win32 API)
- Direct2D + DirectWrite（悬浮窗抗锯齿渲染）
- DirectComposition + D3D11（任务栏嵌入渲染）
- reqwest（HTTP 请求）
- 单 exe，无运行时依赖

## License

[MIT License](./LICENSE)
