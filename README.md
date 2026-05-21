# ⏰ Cron-Ask-AI v0.1.0

> 定时快捷键执行工具 — 强制你高频使用 AI

## 核心目的

让 AI 对话框不定期自动弹出，迫使你大量问 AI、构建信息墙、形成高频使用习惯。

## 功能特性

### 三种触发模式

| 模式 | 说明 | 场景 |
|------|------|------|
| **时钟定时** ⏰ | 到指定时间点触发 | 每天 9:00、14:30 |
| **倒计时** ⏳ | 每隔固定时长触发 | 每 45 分钟 |
| **倒计时+随机偏移** 🎲 | 基础倒计时 ± 随机偏移 | 45分钟 ± 5~15分钟，赌博式随机感 |

### 快捷键模拟

- 支持修饰键：`Ctrl` / `Alt` / `Shift` / `Win`
- 支持字母、数字、功能键（F1-F12）、特殊键（Space、Enter、Tab 等）
- 默认配置：`Ctrl+Q`（通义千问打开对话框）
- RAII 守卫机制，即使 panic 也能释放按键

### 实时倒计时

- 每个任务卡片显示距下次触发的实时倒计时（`2分30秒后` → `2分29秒后` ...）
- 颜色随时间变化：🔵 蓝色（正常）→ 🟡 黄色（1分钟内）→ 🔴 红色（10秒内）

### 系统托盘

- 最小化到托盘，后台运行
- 关闭窗口自动最小化到托盘而非退出
- 托盘左键点击 / 右键菜单「显示窗口」恢复
- 右键菜单「退出」彻底退出

### 配置持久化

- TOML 格式配置文件，人类可读可编辑
- 加载时自动验证修复非法值
- 偏移量无上限限制，自由设置

## 技术栈

**纯 Rust，零 Web 依赖，无浏览器引擎**

| 类别 | 选型 | 版本 | 说明 |
|------|------|------|------|
| GUI 框架 | [egui](https://github.com/emilk/egui) + [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) | 0.29 | 即时模式渲染，无需 DOM/HTML/CSS，天然适合工具类应用 |
| 系统托盘 | [tray-icon](https://github.com/nicebyte/tray-icon) | 0.19 | 纯 Rust 托盘图标，支持右键菜单和点击事件 |
| 键盘模拟 | [rdev](https://github.com/Narsil/rdev) | 0.5 | 原生键盘事件模拟，跨平台按键监听与注入 |
| 异步调度 | [tokio](https://github.com/tokio-rs/tokio) | 1 | 多线程异步运行时，驱动定时任务的 sleep/wake 循环 |
| 时间处理 | [chrono](https://github.com/chronotope/chrono) | 0.4 | 日期时间计算，时钟定时模式的核心依赖 |
| 随机偏移 | [rand](https://github.com/rust-random/rand) | 0.8 | 随机偏移量生成，赌博式触发间隔的来源 |
| 配置序列化 | [serde](https://github.com/serde-rs/serde) + [toml](https://github.com/toml-rs/toml) | 1 / 0.8 | TOML 格式的配置持久化，serde derive 零样板代码 |
| 图标加载 | [image](https://github.com/image-rs/image) | 0.25 | PNG 解码 + 缩放 + 白色透明化处理 |
| 日志 | [log](https://github.com/rust-lang/log) + [env_logger](https://github.com/rust-lang/env_logger) | 0.4 / 0.11 | 结构化日志输出到文件（无控制台窗口模式下） |

## 项目结构

```
src/
├── main.rs              # 程序入口：初始化、panic hook、日志、图标加载、托盘创建、eframe 启动
└── modules/
    ├── mod.rs            # 模块声明
    ├── config.rs        # 配置管理：数据结构、TOML 读写、验证修复、跨平台路径
    ├── keyboard.rs       # 键盘模拟：按键映射、RAII 守卫、异步执行、全局串行锁
    ├── scheduler.rs      # 调度引擎：tokio 异步任务、定时触发、消息传递
    └── ui.rs             # 界面渲染：egui 面板、任务列表、编辑表单、倒计时显示
```

### 模块详解

#### `main.rs` — 程序入口

- 设置 `#![windows_subsystem = "windows"]` 隐藏控制台窗口
- 注册自定义 panic hook，将崩溃信息写入 `%APPDATA%/cron-ask-ai/logs/panic.log`
- 初始化文件日志（`app.log`），无控制台也能排查问题
- 加载 PNG 图标，自动将白色/近白色像素透明化（兜底处理 AI 生成的图标白边）
- 创建系统托盘图标（左键恢复窗口、右键菜单）
- 配置 eframe 窗口（暗色主题、中文字体加载）

#### `config.rs` — 配置管理

- **`TriggerMode`** — 三种触发模式枚举：`Clock`（时钟定时）、`Countdown`（倒计时）、`CountdownRandom`（倒计时+随机偏移）
- **`Hotkey`** — 快捷键组合（修饰键 + 主键）
- **`Task`** — 单个定时任务（ID + 名称 + 启用状态 + 触发模式 + 快捷键）
- **`AppConfig`** — 应用配置（任务列表 + 开机自启 + 关闭最小化）
- 配置路径：`%APPDATA%/cron-ask-ai/config.toml`（Windows）
- 加载时自动验证修复：小时 ≤ 23、分钟 ≤ 59、倒计时 ≥ 1秒、偏移最小≤最大自动交换
- 短 UUID 生成：纳秒时间戳 + 原子计数器

#### `keyboard.rs` — 键盘模拟

- 将配置中的修饰键 / 主键映射到 `rdev::Key`
- `KeyGuard` RAII 守卫：按下按键后即使 panic 也能保证释放
- `execute_hotkey_async`：后台线程执行快捷键，不阻塞 UI
- 全局 `Mutex` 串行化：确保同一时刻只有一个快捷键在执行，避免并发冲突
- 按键顺序：修饰键按下 → 50ms 延迟 → 主键按下/释放 → 修饰键逆序释放

#### `scheduler.rs` — 调度引擎

- `Scheduler` 持有 `tokio::runtime::Runtime`（多线程模式，2 个 worker）
- 每个任务 spawn 一个 tokio 异步任务，循环执行：计算延迟 → 通知下次触发时间 → sleep → 触发
- `compute_next_delay`：根据触发模式计算下次延迟和精确触发时间
  - `Clock`：今天/明天指定时间点
  - `Countdown`：固定间隔（最少 1 秒）
  - `CountdownRandom`：基础 ± 随机偏移（方向随机、偏移量随机），最终间隔至少 1 秒
- 通过 `std::sync::mpsc` 发送消息给 UI：
  - `NextTrigger` — 下次触发时间（用于倒计时显示）
  - `Triggered` — 任务触发（驱动键盘模拟）
- 支持 `reload_tasks`（停止所有 → 重新启动）、`abort` 真正取消 tokio task

#### `ui.rs` — 界面渲染

- `CronAskApp` 实现 `eframe::App` trait
- 每帧 `poll_scheduler_messages` 接收调度器消息
- 500ms 自动重绘（有活跃任务时），驱动倒计时实时更新
- 任务列表卡片：名称、启用开关、触发模式、快捷键、实时倒计时
- 倒计时颜色：蓝色（>60s）→ 黄色（≤60s）→ 红色（≤10s）
- 任务编辑面板：触发模式切换、时长设置、快捷键选择、表单验证
- 挂起操作队列 `PendingAction`：同一帧多个操作（保存/删除/切换/测试）不冲突
- 托盘恢复窗口：通过 `Arc<Mutex<bool>>` 标志与托盘事件通信

## 构建与运行

```bash
# 开发模式（带控制台窗口，可看日志）
cargo run

# 发布构建（体积小、性能高、无控制台窗口）
cargo build --release
```

发布产物在 `dist/` 目录，包含：
- `cron-ask-ai.exe` — 主程序
- `assets/icon.png` — 托盘图标（需与 exe 同级目录）

## 文件位置

| 文件 | 路径（Windows） |
|------|------|
| 配置文件 | `%APPDATA%/cron-ask-ai/config.toml` |
| 运行日志 | `%APPDATA%/cron-ask-ai/logs/app.log` |
| 崩溃日志 | `%APPDATA%/cron-ask-ai/logs/panic.log` |

## 默认配置

首个任务预设：
- 名称：打开通义千问
- 模式：倒计时+随机偏移
- 基础时长：45 分钟
- 随机偏移：±5~15 分钟
- 快捷键：`Ctrl+Q`

## License

MIT
