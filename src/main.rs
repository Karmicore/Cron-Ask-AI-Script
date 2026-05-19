mod modules;

use modules::config::AppConfig;
use modules::scheduler::Scheduler;
use modules::ui::CronAskApp;

use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Cron-Ask-AI v0.1.0 启动");

    // 加载配置
    let config = AppConfig::load();
    log::info!("已加载 {} 个任务", config.tasks.len());

    // 创建调度器
    let (mut scheduler, scheduler_rx) = Scheduler::new(config.tasks.clone());
    scheduler.start_all();

    // 配置 eframe
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 520.0])
            .with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };

    // 创建应用（调度器 + 消息接收端一起传入）
    let app = CronAskApp::new(config, scheduler, scheduler_rx);

    eframe::run_native(
        "Cron-Ask-AI v0.1.0",
        options,
        Box::new(move |cc| {
            // 暗色主题
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            cc.egui_ctx.set_style(style);

            Ok(Box::new(app))
        }),
    )
}
