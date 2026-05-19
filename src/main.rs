#![windows_subsystem = "windows"]

mod modules;

use modules::config::AppConfig;
use modules::scheduler::Scheduler;
use modules::ui::CronAskApp;

use eframe::egui;
use std::sync::{Arc, Mutex};

fn main() -> Result<(), eframe::Error> {
    // 无控制台模式下，将 panic 信息写入日志文件
    setup_panic_hook();

    // 初始化日志（写到文件）
    setup_logger();

    log::info!("Cron-Ask-AI v0.1.0 启动");

    // 加载配置
    let config = AppConfig::load();
    log::info!("已加载 {} 个任务", config.tasks.len());

    // 创建调度器
    let (mut scheduler, scheduler_rx) = Scheduler::new(config.tasks.clone());
    scheduler.start_all(&config.tasks);

    // 加载图标
    let icon_rgba = load_icon();

    // 托盘恢复窗口的共享标志
    let show_window_flag = Arc::new(Mutex::new(true));

    // 创建系统托盘
    let _tray_icon = create_tray_icon(&icon_rgba, show_window_flag.clone());

    // 配置 eframe
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([640.0, 560.0])
        .with_min_inner_size([480.0, 400.0]);

    // 设置窗口图标
    let viewport = if let Some(ref rgba) = icon_rgba {
        viewport.with_icon(egui::IconData {
            rgba: rgba.clone(),
            width: 64,
            height: 64,
        })
    } else {
        viewport
    };

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // 创建应用
    let app = CronAskApp::new(config, scheduler, scheduler_rx, show_window_flag);

    eframe::run_native(
        "Cron-Ask-AI v0.1.0",
        options,
        Box::new(move |cc| {
            // 暗色主题
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            cc.egui_ctx.set_style(style);

            // 加载中文字体，解决乱码问题
            let mut fonts = egui::FontDefinitions::default();
            if let Ok(font_data) = std::fs::read("C:/Windows/Fonts/msyh.ttc") {
                fonts.font_data.insert(
                    "msyh".into(),
                    egui::FontData::from_owned(font_data),
                );
                fonts.families.entry(egui::FontFamily::Proportional).or_default()
                    .push("msyh".into());
                fonts.families.entry(egui::FontFamily::Monospace).or_default()
                    .push("msyh".into());
                cc.egui_ctx.set_fonts(fonts);
                log::info!("已加载中文字体: msyh");
            } else {
                log::warn!("未找到中文字体 msyh.ttc，中文可能显示为乱码");
            }

            Ok(Box::new(app))
        }),
    )
}

/// 设置 panic hook，将 panic 信息写入日志文件（无控制台时可用）
fn setup_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let log_path = get_log_dir().join("panic.log");
        let msg = format!("{}", panic_info);
        let _ = std::fs::write(&log_path, &msg);
        // 也尝试写到 stderr（虽然 windows_subsystem 下不可见，但调试时有用）
        eprintln!("PANIC: {}", msg);
    }));
}

/// 获取日志目录（%APPDATA%/cron-ask-ai/logs）
fn get_log_dir() -> std::path::PathBuf {
    let dir = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("cron-ask-ai")
        .join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 初始化日志，同时输出到文件和 stderr
fn setup_logger() {
    let log_dir = get_log_dir();
    let log_file = log_dir.join("app.log");

    // 尝试创建文件日志
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .target(env_logger::Target::Pipe(Box::new(file)))
            .init();
    } else {
        // 文件创建失败，回退到 stderr
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .init();
    }
}

/// 加载图标 PNG 并转为 RGBA 字节，自动将白色/近白色像素设为透明
fn load_icon() -> Option<Vec<u8>> {
    let exe_dir = std::env::current_exe().ok()?;
    let icon_path = exe_dir.parent()?.join("assets/icon.png");
    let icon_path = if icon_path.exists() {
        icon_path
    } else {
        // 开发模式：从项目根目录查找
        let dev_path = std::path::PathBuf::from("assets/icon.png");
        if dev_path.exists() { dev_path } else { return None }
    };

    let img = match image::open(&icon_path) {
        Ok(img) => img,
        Err(e) => {
            log::error!("加载图标失败 {}: {}", icon_path.display(), e);
            return None;
        }
    };
    let resized = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
    let mut rgba_image = resized.to_rgba8();

    // 将白色/近白色背景像素设为透明（alpha=0）
    for pixel in rgba_image.pixels_mut() {
        let [r, g, b, _a] = pixel.0;
        // 近白色阈值：RGB 各通道 > 240 视为白色背景
        if r > 240 && g > 240 && b > 240 {
            *pixel = image::Rgba([r, g, b, 0]);
        }
    }

    Some(rgba_image.to_vec())
}

/// 创建系统托盘图标
fn create_tray_icon(icon_rgba: &Option<Vec<u8>>, show_flag: Arc<Mutex<bool>>) -> Option<tray_icon::TrayIcon> {
    use tray_icon::{TrayIconBuilder, menu::{Menu, MenuEvent, MenuItem}};

    let tray_icon = if let Some(rgba) = icon_rgba {
        match tray_icon::Icon::from_rgba(rgba.clone(), 64, 64) {
            Ok(icon) => TrayIconBuilder::new()
                .with_tooltip("Cron-Ask-AI - 定时快捷键执行工具")
                .with_icon(icon),
            Err(e) => {
                log::error!("创建托盘图标失败: {}", e);
                return None;
            }
        }
    } else {
        // 无图标时用纯色占位
        let pixels: Vec<u8> = vec![0u8, 191, 255, 255].repeat(64 * 64); // 蓝色
        let icon = tray_icon::Icon::from_rgba(pixels, 64, 64).ok()?;
        TrayIconBuilder::new()
            .with_tooltip("Cron-Ask-AI")
            .with_icon(icon)
    };

    // 右键菜单
    let menu = Menu::new();
    let show_item = MenuItem::new("显示窗口", true, None);
    let quit_item = MenuItem::new("退出", true, None);
    menu.append(&show_item).ok()?;
    menu.append(&quit_item).ok()?;

    // 提前提取 ID（MenuItem 不是 Send+Sync，但 MenuId 的内部字符串是）
    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    let tray = match tray_icon.with_menu(Box::new(menu)).build() {
        Ok(t) => t,
        Err(e) => {
            log::error!("构建托盘图标失败: {}", e);
            return None;
        }
    };

    // 处理菜单事件
    let flag_show = show_flag.clone();
    MenuEvent::set_event_handler(Some(move |event: tray_icon::menu::MenuEvent| {
        if event.id == show_id {
            if let Ok(mut v) = flag_show.lock() {
                *v = true;
            }
        } else if event.id == quit_id {
            std::process::exit(0);
        }
    }));

    // 处理托盘图标点击（左键）
    let flag_click = show_flag.clone();
    tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
        if let tray_icon::TrayIconEvent::Click {
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = event {
            if let Ok(mut v) = flag_click.lock() {
                *v = true;
            }
        }
    }));

    Some(tray)
}
