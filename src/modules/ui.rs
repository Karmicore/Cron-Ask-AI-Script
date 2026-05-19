use eframe::egui;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use super::config::*;
use super::keyboard;
use super::scheduler::{Scheduler, SchedulerMessage};

/// 主应用状态
pub struct CronAskApp {
    pub config: AppConfig,
    pub show_window: bool,
    pub next_triggers: HashMap<String, chrono::DateTime<chrono::Local>>,
    pub last_trigger_info: Option<String>,
    pub editing_task_idx: Option<usize>,
    pub new_task: Option<Task>,
    pub status_message: Option<String>,
    pub pending_action: Option<PendingAction>,
    /// 调度器引用
    scheduler: Option<Scheduler>,
    /// 调度器消息接收端
    scheduler_rx: Option<Receiver<SchedulerMessage>>,
    /// 上次重绘时间，用于控制刷新频率
    last_repaint: Instant,
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
    pub fn new(config: AppConfig, scheduler: Scheduler, scheduler_rx: Receiver<SchedulerMessage>) -> Self {
        Self {
            config,
            show_window: true,
            next_triggers: HashMap::new(),
            last_trigger_info: None,
            editing_task_idx: None,
            new_task: Some(Task::default()),
            status_message: None,
            pending_action: None,
            scheduler: Some(scheduler),
            scheduler_rx: Some(scheduler_rx),
            last_repaint: Instant::now(),
        }
    }

    /// 处理调度器消息（非阻塞）
    fn poll_scheduler_messages(&mut self, ctx: &egui::Context) {
        if let Some(ref rx) = self.scheduler_rx {
            // 非阻塞：一次取出所有可用消息
            while let Ok(msg) = rx.try_recv() {
                match &msg {
                    SchedulerMessage::Triggered {
                        task_name,
                        hotkey,
                        ..
                    } => {
                        // 在后台线程执行快捷键，不阻塞 UI
                        keyboard::execute_hotkey_async(
                            hotkey.modifiers.clone(),
                            hotkey.key.clone(),
                        );
                        self.last_trigger_info = Some(format!(
                            "✅ {} ({}) 已触发",
                            task_name,
                            hotkey.display()
                        ));
                        self.status_message = Some(self.last_trigger_info.clone().unwrap());
                    }
                    SchedulerMessage::NextTrigger { task_id, next_time } => {
                        if let Some(nt) = next_time {
                            self.next_triggers.insert(task_id.clone(), *nt);
                        }
                    }
                }
                ctx.request_repaint();
            }
        }
    }

    /// 通知调度器配置变更
    fn notify_scheduler_reload(&mut self) {
        if let Some(ref mut scheduler) = self.scheduler {
            scheduler.reload_tasks(self.config.tasks.clone());
        }
    }

    /// 处理挂起的操作
    pub fn process_pending_actions(&mut self) {
        if let Some(action) = self.pending_action.take() {
            let need_reload = match action {
                PendingAction::SaveTask => {
                    if let Some(ref task) = self.new_task {
                        if let Some(idx) = self.editing_task_idx {
                            if idx < self.config.tasks.len() {
                                self.config.tasks[idx] = task.clone();
                            }
                            self.editing_task_idx = None;
                        } else {
                            self.config.tasks.push(task.clone());
                        }
                        self.config.save();
                        self.status_message = Some(format!("✅ 已保存任务: {}", task.name));
                    }
                    self.new_task = Some(Task::default());
                    true
                }
                PendingAction::CancelEdit => {
                    self.editing_task_idx = None;
                    self.new_task = Some(Task::default());
                    false
                }
                PendingAction::TestHotkey { modifiers, key } => {
                    // 测试快捷键用异步方式，避免阻塞
                    keyboard::execute_hotkey_async(modifiers.clone(), key.clone());
                    let hk = Hotkey { modifiers, key };
                    self.status_message = Some(format!("已测试: {}", hk.display()));
                    false
                }
                PendingAction::DeleteTask(idx) => {
                    if idx < self.config.tasks.len() {
                        let task_id = self.config.tasks[idx].id.clone();
                        let name = self.config.tasks[idx].name.clone();
                        self.config.tasks.remove(idx);
                        self.config.save();
                        // 清理 next_triggers
                        self.next_triggers.remove(&task_id);
                        self.status_message = Some(format!("已删除: {}", name));
                    }
                    true
                }
                PendingAction::ToggleTask(idx) => {
                    if idx < self.config.tasks.len() {
                        self.config.tasks[idx].enabled = !self.config.tasks[idx].enabled;
                        self.config.save();
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
        // 1. 处理调度器消息（非阻塞）
        self.poll_scheduler_messages(ctx);

        // 2. 处理挂起操作
        self.process_pending_actions();

        if !self.show_window {
            return;
        }

        // 3. 控制重绘频率：有启用任务时每秒刷新，无任务时降低频率
        let has_active_tasks = self.config.tasks.iter().any(|t| t.enabled);
        let repaint_interval = if has_active_tasks {
            std::time::Duration::from_secs(1)
        } else {
            std::time::Duration::from_secs(5) // 无活跃任务时 5 秒刷新一次
        };

        if self.last_repaint.elapsed() >= repaint_interval {
            ctx.request_repaint_after(repaint_interval);
            self.last_repaint = Instant::now();
        }

        // 4. 绘制 UI
        // 顶部标题栏
        egui::TopBottomPanel::top("title_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("⏰ Cron-Ask-AI");
                ui.label("定时快捷键执行工具 v0.1.0");
            });
        });

        // 底部状态栏
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(ref msg) = self.status_message {
                    ui.label(msg);
                } else {
                    ui.label("就绪");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let active = self.config.tasks.iter().filter(|t| t.enabled).count();
                    ui.label(format!("任务: {}/{}", active, self.config.tasks.len()));
                });
            });
        });

        // 主内容区
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
            let next_str = self.next_triggers.get(&task.id).map_or(
                if task.enabled { "计算中...".to_string() } else { "-".to_string() },
                |t| t.format("%H:%M:%S").to_string(),
            );

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
                                ui.label("|");
                                ui.label(format!("下次触发: {}", next_str));
                            });
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

        // 在借用外应用操作
        if let Some(idx) = delete_idx {
            self.pending_action = Some(PendingAction::DeleteTask(idx));
        }
        if let Some(idx) = toggle_idx {
            self.pending_action = Some(PendingAction::ToggleTask(idx));
        }
        if let Some(idx) = edit_idx {
            self.editing_task_idx = Some(idx);
            self.new_task = Some(self.config.tasks[idx].clone());
        }
    }

    fn draw_task_form(&mut self, ui: &mut egui::Ui) {
        let is_editing = self.editing_task_idx.is_some();

        ui.heading(if is_editing { "✏ 编辑任务" } else { "➕ 添加任务" });
        ui.separator();

        if self.new_task.is_none() {
            self.new_task = Some(Task::default());
        }

        // 取出 task，编辑完放回
        let mut task = self.new_task.take().unwrap();

        egui::Grid::new("task_form")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // 任务名称
                ui.label("任务名称:");
                ui.text_edit_singleline(&mut task.name);
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
                            ui.add(egui::DragValue::new(offset_min_minutes).range(0..=60));
                            ui.label("秒:");
                            ui.add(egui::DragValue::new(offset_min_seconds).range(0..=59));
                        });
                        ui.end_row();

                        ui.label("最大偏移:");
                        ui.horizontal(|ui| {
                            ui.label("分:");
                            ui.add(egui::DragValue::new(offset_max_minutes).range(0..=60));
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
                    for mod_key in &all_mods {
                        let is_active = task.hotkey.modifiers.iter().any(|m| m == mod_key);
                        let label = mod_key.display_name();
                        if ui.selectable_label(is_active, label).clicked() {
                            if is_active {
                                task.hotkey.modifiers.retain(|m| m != mod_key);
                            } else {
                                task.hotkey.modifiers.push(mod_key.clone());
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
        let task_clone = task.clone();
        self.new_task = Some(task);

        ui.horizontal(|ui| {
            if ui.button(if is_editing { "💾 保存修改" } else { "➕ 添加任务" }).clicked() {
                self.pending_action = Some(PendingAction::SaveTask);
            }

            if is_editing && ui.button("❌ 取消").clicked() {
                self.pending_action = Some(PendingAction::CancelEdit);
            }

            if ui.button("🧪 测试快捷键").clicked() {
                self.pending_action = Some(PendingAction::TestHotkey {
                    modifiers: task_clone.hotkey.modifiers.clone(),
                    key: task_clone.hotkey.key.clone(),
                });
            }
        });
    }
}
