use eframe::egui;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::config::*;
use super::keyboard;
use super::scheduler::{Scheduler, SchedulerMessage};

/// 状态消息的自动消失时间（秒）
const STATUS_MESSAGE_TTL_SECS: f64 = 5.0;

/// 主应用状态
pub struct CronAskApp {
    pub config: AppConfig,
    pub next_triggers: HashMap<String, chrono::DateTime<chrono::Local>>,
    pub last_trigger_info: Option<String>,
    pub editing_task_idx: Option<usize>,
    pub new_task: Option<Task>,
    pub status_message: Option<String>,
    pub status_message_set_at: Option<Instant>,
    /// 挂起操作队列（支持同一帧多个操作）
    pending_actions: Vec<PendingAction>,
    /// 调度器引用
    scheduler: Option<Scheduler>,
    /// 调度器消息接收端
    scheduler_rx: Option<std::sync::mpsc::Receiver<SchedulerMessage>>,
    /// 表单验证错误
    form_error: Option<String>,
    /// 托盘恢复窗口标志
    show_window_flag: Option<Arc<Mutex<bool>>>,
}

#[derive(Debug)]
pub enum PendingAction {
    SaveTask,
    CancelEdit,
    TestHotkey { modifiers: Vec<ModifierKey>, key: String },
    DeleteTask(usize),
    ToggleTask(usize),
}

impl CronAskApp {
    pub fn new(
        config: AppConfig,
        scheduler: Scheduler,
        scheduler_rx: std::sync::mpsc::Receiver<SchedulerMessage>,
        show_window_flag: Arc<Mutex<bool>>,
    ) -> Self {
        Self {
            config,
            next_triggers: HashMap::new(),
            last_trigger_info: None,
            editing_task_idx: None,
            new_task: Some(Task::default()),
            status_message: None,
            status_message_set_at: None,
            pending_actions: Vec::new(),
            scheduler: Some(scheduler),
            scheduler_rx: Some(scheduler_rx),
            form_error: None,
            show_window_flag: Some(show_window_flag),
        }
    }

    /// 设置状态消息（带自动清除）
    fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_message_set_at = Some(Instant::now());
    }

    /// 检查并清除过期的状态消息
    fn check_status_expiry(&mut self) {
        if let Some(set_at) = self.status_message_set_at {
            if set_at.elapsed().as_secs_f64() > STATUS_MESSAGE_TTL_SECS {
                self.status_message = None;
                self.status_message_set_at = None;
            }
        }
    }

    /// 处理调度器消息（非阻塞）
    fn poll_scheduler_messages(&mut self, ctx: &egui::Context) {
        // 先收集所有消息，避免 rx 借用与 self 借用冲突
        let (messages, disconnected) = if let Some(ref rx) = self.scheduler_rx {
            let mut msgs = Vec::new();
            let mut disconnected = false;
            loop {
                match rx.try_recv() {
                    Ok(msg) => msgs.push(msg),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            (msgs, disconnected)
        } else {
            return;
        };

        let mut need_repaint = false;

        for msg in &messages {
            match msg {
                SchedulerMessage::Triggered {
                    task_name,
                    hotkey,
                    ..
                } => {
                    keyboard::execute_hotkey_async(
                        hotkey.modifiers.clone(),
                        hotkey.key.clone(),
                    );
                    let info = format!("✅ {} ({}) 已触发", task_name, hotkey.display());
                    self.last_trigger_info = Some(info.clone());
                    self.set_status(info);
                }
                SchedulerMessage::NextTrigger { task_id, next_time } => {
                    if let Some(nt) = next_time {
                        self.next_triggers.insert(task_id.clone(), *nt);
                    }
                }
            }
            need_repaint = true;
        }

        if disconnected {
            self.scheduler_rx = None;
        }

        if need_repaint {
            ctx.request_repaint();
        }
    }

    /// 通知调度器配置变更，同时清理过期的 next_triggers 条目
    fn notify_scheduler_reload(&mut self) {
        if let Some(ref mut scheduler) = self.scheduler {
            scheduler.reload_tasks(self.config.tasks.clone());
        }
        // 清理不在当前任务列表中的 next_triggers 条目
        let task_ids: std::collections::HashSet<String> =
            self.config.tasks.iter().map(|t| t.id.clone()).collect();
        self.next_triggers.retain(|id, _| task_ids.contains(id));
    }

    /// 验证任务表单，返回错误信息（None 表示合法）
    fn validate_task(task: &Task) -> Option<String> {
        if task.name.trim().is_empty() {
            return Some("任务名称不能为空".to_string());
        }
        match &task.trigger {
            TriggerMode::Clock { .. } => {}
            TriggerMode::Countdown { minutes, seconds } => {
                if *minutes == 0 && *seconds == 0 {
                    return Some("倒计时时长不能为 0".to_string());
                }
            }
            TriggerMode::CountdownRandom {
                base_minutes, base_seconds,
                offset_min_minutes, offset_min_seconds,
                offset_max_minutes, offset_max_seconds,
            } => {
                if *base_minutes == 0 && *base_seconds == 0 {
                    return Some("基础时长不能为 0".to_string());
                }
                let offset_min = offset_min_minutes * 60 + offset_min_seconds;
                let offset_max = offset_max_minutes * 60 + offset_max_seconds;
                if offset_min > offset_max {
                    return Some("最小偏移不能大于最大偏移".to_string());
                }
            }
        }
        if task.hotkey.key.is_empty() {
            return Some("请选择一个主键".to_string());
        }
        None
    }

    /// 格式化剩余时间
    fn format_remaining(next: &chrono::DateTime<chrono::Local>) -> String {
        let now = chrono::Local::now();
        let diff = *next - now;
        let total_secs = diff.num_seconds();
        if total_secs <= 0 {
            return "即将触发".to_string();
        }
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        if hours > 0 {
            format!("{}时{}分{}秒后", hours, mins, secs)
        } else if mins > 0 {
            format!("{}分{}秒后", mins, secs)
        } else {
            format!("{}秒后", secs)
        }
    }

    /// 处理挂起的操作队列
    pub fn process_pending_actions(&mut self) {
        // 先取走所有挂起操作，避免借用冲突
        let actions: Vec<PendingAction> = self.pending_actions.drain(..).collect();

        for action in actions {
            let need_reload = match action {
                PendingAction::SaveTask => {
                    if let Some(ref task) = self.new_task {
                        // 验证
                        if let Some(err) = Self::validate_task(task) {
                            self.form_error = Some(err);
                            // 不清除 new_task，让用户继续编辑
                            self.new_task = Some(task.clone());
                            continue;
                        }
                        self.form_error = None;

                        if let Some(idx) = self.editing_task_idx {
                            if idx < self.config.tasks.len() {
                                self.config.tasks[idx] = task.clone();
                            }
                            self.editing_task_idx = None;
                        } else {
                            self.config.tasks.push(task.clone());
                        }
                        self.config.save();
                        self.set_status(format!("✅ 已保存任务: {}", task.name));
                    }
                    self.new_task = Some(Task::default());
                    true
                }
                PendingAction::CancelEdit => {
                    self.editing_task_idx = None;
                    self.new_task = Some(Task::default());
                    self.form_error = None;
                    false
                }
                PendingAction::TestHotkey { modifiers, key } => {
                    keyboard::execute_hotkey_async(modifiers.clone(), key.clone());
                    let hk = Hotkey { modifiers, key };
                    self.set_status(format!("已测试: {}", hk.display()));
                    false
                }
                PendingAction::DeleteTask(idx) => {
                    if idx < self.config.tasks.len() {
                        let task_id = self.config.tasks[idx].id.clone();
                        let name = self.config.tasks[idx].name.clone();
                        self.config.tasks.remove(idx);
                        self.config.save();
                        self.next_triggers.remove(&task_id);
                        self.set_status(format!("已删除: {}", name));
                    }
                    true
                }
                PendingAction::ToggleTask(idx) => {
                    if idx < self.config.tasks.len() {
                        self.config.tasks[idx].enabled = !self.config.tasks[idx].enabled;
                        self.config.save();
                        let state = if self.config.tasks[idx].enabled { "启用" } else { "暂停" };
                        self.set_status(format!("已{}: {}", state, self.config.tasks[idx].name));
                    }
                    true
                }
            };

            if need_reload {
                self.notify_scheduler_reload();
            }
        }
    }
}

impl eframe::App for CronAskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 0. 检查托盘恢复窗口请求
        if let Some(ref flag) = self.show_window_flag {
            if let Ok(mut v) = flag.lock() {
                if *v {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    // 重置标志，避免重复发送
                    *v = false;
                }
            }
        }

        // 1. 处理调度器消息
        self.poll_scheduler_messages(ctx);

        // 2. 处理挂起操作
        self.process_pending_actions();

        // 3. 检查状态消息过期
        self.check_status_expiry();

        // 4. 控制重绘频率 — 简化逻辑：每帧直接安排下一次重绘
        let has_active_tasks = self.config.tasks.iter().any(|t| t.enabled);
        let repaint_interval = if has_active_tasks {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(5)
        };
        ctx.request_repaint_after(repaint_interval);

        // 5. 顶部标题栏
        egui::TopBottomPanel::top("title_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("⏰ Cron-Ask-AI");
                ui.label("定时快捷键执行工具 v0.1.0");
            });
        });

        // 6. 底部状态栏
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(ref msg) = self.status_message {
                    ui.label(msg);
                    // 显示倒计时
                    if let Some(set_at) = self.status_message_set_at {
                        let remaining = STATUS_MESSAGE_TTL_SECS - set_at.elapsed().as_secs_f64();
                        if remaining > 0.0 {
                            ui.label(format!("({:.0}s)", remaining));
                        }
                    }
                } else {
                    ui.label("就绪");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let active = self.config.tasks.iter().filter(|t| t.enabled).count();
                    ui.label(format!("任务: {}/{}", active, self.config.tasks.len()));
                });
            });
        });

        // 7. 主内容区
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_task_list(ui);
            ui.add_space(8.0);
            self.draw_task_form(ui);
        });
    }
}

impl CronAskApp {
    fn draw_task_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("📋 任务列表");
        ui.separator();

        if self.config.tasks.is_empty() {
            ui.colored_label(egui::Color32::GRAY, "暂无任务，点击下方添加");
            return;
        }

        let mut delete_idx: Option<usize> = None;
        let mut toggle_idx: Option<usize> = None;
        let mut edit_idx: Option<usize> = None;

        for (i, task) in self.config.tasks.iter().enumerate() {
            let next_str = if let Some(next) = self.next_triggers.get(&task.id) {
                if task.enabled {
                    Self::format_remaining(next)
                } else {
                    "-".to_string()
                }
            } else if task.enabled {
                "⏳ 等待中...".to_string()
            } else {
                "-".to_string()
            };

            egui::Frame::group(ui.style())
                .stroke(egui::Stroke::new(
                    1.0,
                    if task.enabled {
                        egui::Color32::from_rgb(100, 200, 100)
                    } else {
                        egui::Color32::GRAY
                    },
                ))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // 启用状态
                        let label = if task.enabled { "✅" } else { "⬜" };
                        if ui.button(label).clicked() {
                            toggle_idx = Some(i);
                        }

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(&task.name);
                                if task.enabled {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(100, 200, 100),
                                        "● 运行中",
                                    );
                                } else {
                                    ui.colored_label(egui::Color32::GRAY, "○ 已暂停");
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(format!("模式: {}", task.trigger.display_name()));
                                ui.label("|");
                                ui.label(format!("快捷键: {}", task.hotkey.display()));
                            });
                            ui.horizontal(|ui| {
                                match &task.trigger {
                                    TriggerMode::Clock { hour, minute } => {
                                        ui.label(format!("⏰ 每天 {:02}:{:02}", hour, minute));
                                    }
                                    TriggerMode::Countdown { minutes, seconds } => {
                                        ui.label(format!("⏳ 每 {}分{}秒", minutes, seconds));
                                    }
                                    TriggerMode::CountdownRandom {
                                        base_minutes, base_seconds,
                                        offset_min_minutes, offset_min_seconds,
                                        offset_max_minutes, offset_max_seconds,
                                    } => {
                                        ui.label(format!(
                                            "🎲 每 {}分{}秒 ± {}分{}秒~{}分{}秒",
                                            base_minutes, base_seconds,
                                            offset_min_minutes, offset_min_seconds,
                                            offset_max_minutes, offset_max_seconds,
                                        ));
                                    }
                                }
                            });
                            // 倒计时独立一行，更显眼
                            if task.enabled {
                                let color = if let Some(next) = self.next_triggers.get(&task.id) {
                                    let secs = (*next - chrono::Local::now()).num_seconds();
                                    if secs <= 10 {
                                        egui::Color32::from_rgb(255, 80, 80) // 即将触发，红色
                                    } else if secs <= 60 {
                                        egui::Color32::from_rgb(255, 200, 0) // 1分钟内，黄色
                                    } else {
                                        egui::Color32::from_rgb(100, 220, 255) // 正常，蓝色
                                    }
                                } else {
                                    egui::Color32::GRAY
                                };
                                ui.horizontal(|ui| {
                                    ui.label("⏱");
                                    ui.colored_label(color, &next_str);
                                });
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🗑").clicked() {
                                delete_idx = Some(i);
                            }
                            if ui.button("✏").clicked() {
                                edit_idx = Some(i);
                            }
                        });
                    });
                });

            ui.add_space(4.0);
        }

        // 在借用外应用操作 — 使用 Vec 支持同一帧多个操作
        if let Some(idx) = delete_idx {
            self.pending_actions.push(PendingAction::DeleteTask(idx));
        }
        if let Some(idx) = toggle_idx {
            self.pending_actions.push(PendingAction::ToggleTask(idx));
        }
        if let Some(idx) = edit_idx {
            self.editing_task_idx = Some(idx);
            self.new_task = Some(self.config.tasks[idx].clone());
            self.form_error = None;
        }
    }

    fn draw_task_form(&mut self, ui: &mut egui::Ui) {
        let is_editing = self.editing_task_idx.is_some();

        ui.heading(if is_editing { "✏ 编辑任务" } else { "➕ 添加任务" });
        ui.separator();

        // 显示验证错误
        if let Some(ref err) = self.form_error {
            ui.colored_label(egui::Color32::from_rgb(255, 80, 80), format!("⚠ {}", err));
        }

        if self.new_task.is_none() {
            self.new_task = Some(Task::default());
        }

        let mut task = self.new_task.take().unwrap();

        egui::Grid::new("task_form")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // 任务名称
                ui.label("任务名称:");
                let name_resp = ui.text_edit_singleline(&mut task.name);
                if name_resp.lost_focus() && task.name.trim().is_empty() {
                    self.form_error = Some("任务名称不能为空".to_string());
                }
                ui.end_row();

                // 触发模式
                ui.label("触发模式:");
                ui.horizontal(|ui| {
                    let modes = ["时钟定时", "倒计时", "倒计时+随机偏移"];
                    let current_idx = match task.trigger {
                        TriggerMode::Clock { .. } => 0,
                        TriggerMode::Countdown { .. } => 1,
                        TriggerMode::CountdownRandom { .. } => 2,
                    };
                    for (idx, label) in modes.iter().enumerate() {
                        if ui.selectable_label(idx == current_idx, *label).clicked() && idx != current_idx {
                            task.trigger = match idx {
                                0 => TriggerMode::Clock { hour: 9, minute: 0 },
                                1 => TriggerMode::Countdown { minutes: 45, seconds: 0 },
                                2 => TriggerMode::CountdownRandom {
                                    base_minutes: 45, base_seconds: 0,
                                    offset_min_minutes: 5, offset_min_seconds: 0,
                                    offset_max_minutes: 15, offset_max_seconds: 0,
                                },
                                _ => TriggerMode::default(),
                            };
                        }
                    }
                });
                ui.end_row();

                // 模式参数
                match &mut task.trigger {
                    TriggerMode::Clock { hour, minute } => {
                        ui.label("时间:");
                        ui.horizontal(|ui| {
                            ui.label("时:");
                            ui.add(egui::DragValue::new(hour).range(0..=23));
                            ui.label("分:");
                            ui.add(egui::DragValue::new(minute).range(0..=59));
                        });
                        ui.end_row();
                    }
                    TriggerMode::Countdown { minutes, seconds } => {
                        ui.label("倒计时时长:");
                        ui.horizontal(|ui| {
                            ui.label("分:");
                            ui.add(egui::DragValue::new(minutes).range(1..=1440));
                            ui.label("秒:");
                            ui.add(egui::DragValue::new(seconds).range(0..=59));
                        });
                        ui.end_row();
                    }
                    TriggerMode::CountdownRandom {
                        base_minutes, base_seconds,
                        offset_min_minutes, offset_min_seconds,
                        offset_max_minutes, offset_max_seconds,
                    } => {
                        ui.label("基础时长:");
                        ui.horizontal(|ui| {
                            ui.label("分:");
                            ui.add(egui::DragValue::new(base_minutes).range(1..=1440));
                            ui.label("秒:");
                            ui.add(egui::DragValue::new(base_seconds).range(0..=59));
                        });
                        ui.end_row();

                        ui.label("最小偏移:");
                        ui.horizontal(|ui| {
                            ui.label("分:");
                            ui.add(egui::DragValue::new(offset_min_minutes).range(0..=1440));
                            ui.label("秒:");
                            ui.add(egui::DragValue::new(offset_min_seconds).range(0..=59));
                        });
                        ui.end_row();

                        ui.label("最大偏移:");
                        ui.horizontal(|ui| {
                            ui.label("分:");
                            ui.add(egui::DragValue::new(offset_max_minutes).range(0..=1440));
                            ui.label("秒:");
                            ui.add(egui::DragValue::new(offset_max_seconds).range(0..=59));
                        });
                        ui.end_row();
                    }
                }

                // 快捷键 - 修饰键
                ui.label("修饰键:");
                ui.horizontal(|ui| {
                    let all_mods = ModifierKey::all();
                    for mod_key in all_mods.iter() {
                        let is_active = task.hotkey.modifiers.iter().any(|m| m == mod_key);
                        let label = mod_key.display_name();
                        if ui.selectable_label(is_active, label).clicked() {
                            if is_active {
                                task.hotkey.modifiers.retain(|m| m != mod_key);
                            } else {
                                task.hotkey.modifiers.push((*mod_key).clone());
                            }
                        }
                    }
                });
                ui.end_row();

                // 快捷键 - 主键
                ui.label("主键:");
                ui.horizontal(|ui| {
                    let available = keyboard::available_keys();
                    egui::ComboBox::from_id_salt("key_select")
                        .selected_text(&task.hotkey.key)
                        .show_ui(ui, |ui| {
                            for key in available {
                                if ui.selectable_label(task.hotkey.key == *key, *key).clicked() {
                                    task.hotkey.key = key.to_string();
                                }
                            }
                        });
                });
                ui.end_row();

                // 预览
                ui.label("快捷键预览:");
                ui.colored_label(
                    egui::Color32::from_rgb(255, 200, 0),
                    task.hotkey.display(),
                );
                ui.end_row();
            });

        ui.add_space(8.0);

        // 存回 task
        self.new_task = Some(task);

        ui.horizontal(|ui| {
            if ui.button(if is_editing { "💾 保存修改" } else { "➕ 添加任务" }).clicked() {
                self.pending_actions.push(PendingAction::SaveTask);
            }

            if is_editing && ui.button("❌ 取消").clicked() {
                self.pending_actions.push(PendingAction::CancelEdit);
            }

            if ui.button("🧪 测试快捷键").clicked() {
                // 只在点击时 clone，而非每帧
                if let Some(ref task) = self.new_task {
                    self.pending_actions.push(PendingAction::TestHotkey {
                        modifiers: task.hotkey.modifiers.clone(),
                        key: task.hotkey.key.clone(),
                    });
                }
            }
        });
    }

    /// 拦截窗口关闭事件：隐藏到托盘而非退出
    fn on_close_event(&mut self) -> bool {
        if let Some(ref flag) = self.show_window_flag {
            if let Ok(mut v) = flag.lock() {
                *v = false;
            }
        }
        log::info!("窗口已隐藏到系统托盘");
        false // 阻止关闭
    }
}
