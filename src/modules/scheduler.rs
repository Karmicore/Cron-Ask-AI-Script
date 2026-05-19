use chrono::{Local, TimeZone};
use rand::Rng;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

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
    msg_tx: std::sync::mpsc::Sender<SchedulerMessage>,
    rt: tokio::runtime::Runtime,
    /// 活跃任务句柄 — task_id → JoinHandle，用于真正取消任务
    active_handles: Arc<Mutex<Vec<(String, JoinHandle<()>)>>>,
}

impl Scheduler {
    /// 创建调度器，返回 (Scheduler, 消息接收端)
    pub fn new(_tasks: Vec<Task>) -> (Self, std::sync::mpsc::Receiver<SchedulerMessage>) {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<SchedulerMessage>();
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        let active_handles = Arc::new(Mutex::new(Vec::new()));

        let scheduler = Scheduler {
            msg_tx,
            rt,
            active_handles,
        };

        (scheduler, msg_rx)
    }

    /// 启动所有当前启用的任务
    pub fn start_all(&mut self, tasks: &[Task]) {
        for task in tasks.iter().filter(|t| t.enabled) {
            self.spawn_task(task.clone());
        }
    }

    /// 停止所有任务 — 真正取消 tokio task（abort）
    pub fn stop_all(&mut self) {
        let mut handles = self.active_handles.lock().unwrap();
        for (_, handle) in handles.drain(..) {
            handle.abort();
        }
    }

    /// 停止指定任务
    #[allow(dead_code)]
    pub fn stop_task(&mut self, task_id: &str) {
        let mut handles = self.active_handles.lock().unwrap();
        if let Some(pos) = handles.iter().position(|(id, _)| id == task_id) {
            let (_, handle) = handles.remove(pos);
            handle.abort();
        }
    }

    /// 重新加载配置 — 停止旧任务，启动新任务
    pub fn reload_tasks(&mut self, new_tasks: Vec<Task>) {
        self.stop_all();
        self.start_all(&new_tasks);
    }

    fn spawn_task(&mut self, task: Task) {
        let task_id = task.id.clone();
        let msg_tx = self.msg_tx.clone();

        let handle = self.rt.spawn(async move {
            loop {
                let (delay, next_time) = compute_next_delay(&task.trigger);

                // 通知下次触发时间
                let _ = msg_tx.send(SchedulerMessage::NextTrigger {
                    task_id: task.id.clone(),
                    next_time,
                });

                // 等待（如果被 abort，这里会直接退出，不会继续执行）
                tokio::time::sleep(delay).await;

                // 触发
                log::info!("任务触发: {} ({})", task.name, task.id);
                let _ = msg_tx.send(SchedulerMessage::Triggered {
                    task_id: task.id.clone(),
                    task_name: task.name.clone(),
                    hotkey: task.hotkey.clone(),
                });
            }
        });

        self.active_handles.lock().unwrap().push((task_id, handle));
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stop_all();
    }
}
