use chrono::{Local, TimeZone};
use rand::Rng;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::config::{Hotkey, Task, TriggerMode};

/// 调度器发送给主线程的消息
#[derive(Debug, Clone)]
pub enum SchedulerMessage {
    /// 任务触发
    Triggered {
        #[allow(dead_code)]
        task_id: String,
        task_name: String,
        hotkey: Hotkey,
    },
    /// 下次触发时间更新
    NextTrigger {
        task_id: String,
        next_time: Option<chrono::DateTime<Local>>,
    },
}

/// 共享任务状态 — 调度器读取，UI 写入
pub type SharedTasks = Arc<Mutex<Vec<Task>>>;

/// 计算下次触发的延迟时间
pub fn compute_next_delay(trigger: &TriggerMode) -> (Duration, Option<chrono::DateTime<Local>>) {
    match trigger {
        TriggerMode::Clock { hour, minute } => {
            let now = Local::now();
            let target_naive = now.date_naive()
                .and_hms_opt(*hour, *minute, 0)
                .unwrap();
            let target_dt = Local.from_local_datetime(&target_naive)
                .single()
                .unwrap_or_else(|| now);

            let target_dt = if target_dt <= now {
                let tomorrow = (now.date_naive() + chrono::Duration::days(1))
                    .and_hms_opt(*hour, *minute, 0)
                    .unwrap();
                Local.from_local_datetime(&tomorrow)
                    .single()
                    .unwrap_or_else(|| now)
            } else {
                target_dt
            };

            let delay = target_dt - now;
            (delay.to_std().unwrap_or(Duration::from_secs(60)), Some(target_dt))
        }
        TriggerMode::Countdown { minutes, seconds } => {
            let total_secs = minutes * 60 + seconds;
            let next = Local::now() + chrono::Duration::seconds(total_secs as i64);
            (Duration::from_secs(total_secs), Some(next))
        }
        TriggerMode::CountdownRandom {
            base_minutes,
            base_seconds,
            offset_min_minutes,
            offset_min_seconds,
            offset_max_minutes,
            offset_max_seconds,
        } => {
            let base_secs = base_minutes * 60 + base_seconds;
            let offset_min_secs = offset_min_minutes * 60 + offset_min_seconds;
            let offset_max_secs = offset_max_minutes * 60 + offset_max_seconds;

            let mut rng = rand::thread_rng();
            // 在 [offset_min, offset_max] 范围内随机选一个偏移量
            let offset: u64 = if offset_min_secs == offset_max_secs {
                offset_min_secs
            } else {
                rng.gen_range(offset_min_secs..=offset_max_secs)
            };

            // 随机方向：加上偏移 或 减去偏移
            let positive: bool = rng.gen();
            let final_offset = if positive { offset as i64 } else { -(offset as i64) };

            let total_secs = (base_secs as i64 + final_offset).max(1) as u64;
            let next = Local::now() + chrono::Duration::seconds(total_secs as i64);
            (Duration::from_secs(total_secs), Some(next))
        }
    }
}

/// 调度器 — 管理所有任务的生命周期
pub struct Scheduler {
    tasks: SharedTasks,
    msg_tx: std::sync::mpsc::Sender<SchedulerMessage>,
    rt: tokio::runtime::Runtime,
    /// 活跃的 task id → JoinHandle 标记（用于跟踪哪些在跑）
    active_ids: Arc<Mutex<Vec<String>>>,
}

impl Scheduler {
    /// 创建调度器，返回 (Scheduler, 消息接收端)
    pub fn new(tasks: Vec<Task>) -> (Self, std::sync::mpsc::Receiver<SchedulerMessage>) {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<SchedulerMessage>();
        let shared = Arc::new(Mutex::new(tasks));
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        let active_ids = Arc::new(Mutex::new(Vec::new()));

        let scheduler = Scheduler {
            tasks: shared,
            msg_tx,
            rt,
            active_ids,
        };

        (scheduler, msg_rx)
    }

    /// 启动所有当前启用的任务
    pub fn start_all(&mut self) {
        let tasks_to_start: Vec<Task> = {
            let tasks = self.tasks.lock().unwrap();
            tasks.iter()
                .filter(|t| t.enabled)
                .cloned()
                .collect()
        };
        for task in tasks_to_start {
            self.spawn_task(task);
        }
    }

    /// 停止所有任务（清空活跃列表）
    pub fn stop_all(&mut self) {
        self.active_ids.lock().unwrap().clear();
    }

    /// 重新加载配置 — 停止旧任务，启动新任务
    pub fn reload_tasks(&mut self, new_tasks: Vec<Task>) {
        self.stop_all();
        *self.tasks.lock().unwrap() = new_tasks;
        // 重新启动
        let tasks_to_start: Vec<Task> = {
            let tasks = self.tasks.lock().unwrap();
            tasks.iter()
                .filter(|t| t.enabled)
                .cloned()
                .collect()
        };
        for task in tasks_to_start {
            self.spawn_task(task);
        }
    }

    /// 获取共享任务引用（UI 侧读取/写入）
    #[allow(dead_code)]
    pub fn shared_tasks(&self) -> SharedTasks {
        self.tasks.clone()
    }

    fn spawn_task(&mut self, task: Task) {
        let task_id = task.id.clone();
        // 记录为活跃
        self.active_ids.lock().unwrap().push(task_id.clone());

        let msg_tx = self.msg_tx.clone();
        let active_ids = self.active_ids.clone();
        let task_id_check = task_id.clone();

        self.rt.spawn(async move {
            loop {
                // 检查此任务是否仍然活跃
                {
                    let ids = active_ids.lock().unwrap();
                    if !ids.contains(&task_id_check) {
                        log::info!("任务 {} 已被停止，退出循环", task_id_check);
                        break;
                    }
                }

                let (delay, next_time) = compute_next_delay(&task.trigger);

                // 通知下次触发时间
                let _ = msg_tx.send(SchedulerMessage::NextTrigger {
                    task_id: task.id.clone(),
                    next_time,
                });

                // 等待
                tokio::time::sleep(delay).await;

                // 再次检查是否仍活跃
                {
                    let ids = active_ids.lock().unwrap();
                    if !ids.contains(&task_id_check) {
                        break;
                    }
                }

                // 触发
                log::info!("任务触发: {} ({})", task.name, task.id);
                let _ = msg_tx.send(SchedulerMessage::Triggered {
                    task_id: task.id.clone(),
                    task_name: task.name.clone(),
                    hotkey: task.hotkey.clone(),
                });
            }
        });
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stop_all();
    }
}
