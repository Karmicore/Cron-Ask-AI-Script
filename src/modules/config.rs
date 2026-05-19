use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 触发模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TriggerMode {
    /// 时钟定时：到指定时间点触发
    Clock {
        hour: u32,
        minute: u32,
    },
    /// 倒计时：每隔固定时长触发
    Countdown {
        minutes: u64,
        seconds: u64,
    },
    /// 倒计时+随机偏移：基础倒计时 ± 随机偏移
    CountdownRandom {
        base_minutes: u64,
        base_seconds: u64,
        /// 随机偏移的最小分钟数
        offset_min_minutes: u64,
        offset_min_seconds: u64,
        /// 随机偏移的最大分钟数
        offset_max_minutes: u64,
        offset_max_seconds: u64,
    },
}

impl Default for TriggerMode {
    fn default() -> Self {
        TriggerMode::Countdown {
            minutes: 45,
            seconds: 0,
        }
    }
}

impl TriggerMode {
    pub fn display_name(&self) -> &str {
        match self {
            TriggerMode::Clock { .. } => "时钟定时",
            TriggerMode::Countdown { .. } => "倒计时",
            TriggerMode::CountdownRandom { .. } => "倒计时+随机偏移",
        }
    }
}

/// 修饰键
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModifierKey {
    Ctrl,
    Alt,
    Shift,
    Super, // Win键
}

/// 修饰键静态列表（零分配）
static ALL_MODIFIERS: &[ModifierKey] = &[
    ModifierKey::Ctrl,
    ModifierKey::Alt,
    ModifierKey::Shift,
    ModifierKey::Super,
];

impl ModifierKey {
    pub fn display_name(&self) -> &str {
        match self {
            ModifierKey::Ctrl => "Ctrl",
            ModifierKey::Alt => "Alt",
            ModifierKey::Shift => "Shift",
            ModifierKey::Super => "Win",
        }
    }

    /// 获取所有修饰键（返回静态切片，零分配）
    pub fn all() -> &'static [ModifierKey] {
        ALL_MODIFIERS
    }
}

/// 快捷键组合
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hotkey {
    pub modifiers: Vec<ModifierKey>,
    pub key: String, // 主键，如 "Q", "Space", "Tab"
}

impl Default for Hotkey {
    fn default() -> Self {
        Hotkey {
            modifiers: vec![ModifierKey::Ctrl],
            key: "Q".to_string(),
        }
    }
}

impl Hotkey {
    pub fn display(&self) -> String {
        if self.modifiers.is_empty() {
            self.key.clone()
        } else {
            let mut s = String::with_capacity(32);
            for (i, m) in self.modifiers.iter().enumerate() {
                if i > 0 {
                    s.push('+');
                }
                s.push_str(m.display_name());
            }
            s.push('+');
            s.push_str(&self.key);
            s
        }
    }
}

/// 单个定时任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: TriggerMode,
    pub hotkey: Hotkey,
}

impl Default for Task {
    fn default() -> Self {
        Task {
            id: uuid_short(),
            name: "新任务".to_string(),
            enabled: true,
            trigger: TriggerMode::default(),
            hotkey: Hotkey::default(),
        }
    }
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_true")]
    pub minimize_on_close: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            tasks: vec![Task {
                id: uuid_short(),
                name: "打开通义千问".to_string(),
                enabled: true,
                trigger: TriggerMode::CountdownRandom {
                    base_minutes: 45,
                    base_seconds: 0,
                    offset_min_minutes: 5,
                    offset_min_seconds: 0,
                    offset_max_minutes: 15,
                    offset_max_seconds: 0,
                },
                hotkey: Hotkey {
                    modifiers: vec![ModifierKey::Ctrl],
                    key: "Q".to_string(),
                },
            }],
            autostart: false,
            minimize_on_close: true,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let mut path = dirs_config_dir();
        path.push("cron-ask-ai");
        fs::create_dir_all(&path).ok();
        path.push("config.toml");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        log::error!("配置文件解析失败: {}, 使用默认配置", e);
                    }
                },
                Err(e) => {
                    log::error!("配置文件读取失败: {}, 使用默认配置", e);
                }
            }
        }
        let default = Self::default();
        default.save();
        default
    }

    pub fn save(&self) {
        let path = Self::config_path();
        match toml::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = fs::write(&path, content) {
                    log::error!("配置文件保存失败: {}", e);
                }
            }
            Err(e) => {
                log::error!("配置序列化失败: {}", e);
            }
        }
    }
}

/// 跨平台配置目录
fn dirs_config_dir() -> PathBuf {
    // Windows: %APPDATA%
    // macOS: ~/Library/Application Support
    // Linux: $XDG_CONFIG_HOME 或 ~/.config
    if let Ok(appdata) = std::env::var("APPDATA") {
        // Windows
        return PathBuf::from(appdata);
    }

    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        // Linux (XDG)
        return PathBuf::from(xdg);
    }

    // macOS / Linux fallback
    if let Ok(home) = std::env::var("HOME") {
        // macOS: ~/Library/Application Support
        let mac_path = PathBuf::from(&home)
            .join("Library")
            .join("Application Support");
        if mac_path.exists() {
            return mac_path;
        }
        // Linux fallback: ~/.config
        return PathBuf::from(home).join(".config");
    }

    // 最终 fallback
    PathBuf::from(".")
}

/// 生成短 UUID — 使用计数器+纳秒避免冲突
fn uuid_short() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // 组合纳秒时间戳 + 原子计数器，避免冲突
    format!("{:x}{:02x}", (ts as u64) % 0xFFFFFFFF, count % 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_display() {
        let hk = Hotkey {
            modifiers: vec![ModifierKey::Ctrl, ModifierKey::Alt],
            key: "Q".to_string(),
        };
        assert_eq!(hk.display(), "Ctrl+Alt+Q");
    }

    #[test]
    fn test_hotkey_display_no_modifiers() {
        let hk = Hotkey {
            modifiers: vec![],
            key: "F1".to_string(),
        };
        assert_eq!(hk.display(), "F1");
    }

    #[test]
    fn test_config_default() {
        let config = AppConfig::default();
        assert!(!config.tasks.is_empty());
        assert_eq!(config.tasks[0].name, "打开通义千问");
    }

    #[test]
    fn test_config_serialize() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.tasks.len(), config.tasks.len());
    }

    #[test]
    fn test_uuid_uniqueness() {
        let id1 = uuid_short();
        let id2 = uuid_short();
        assert_ne!(id1, id2, "连续生成的 UUID 不应重复");
    }

    #[test]
    fn test_modifier_all_static() {
        let all = ModifierKey::all();
        assert_eq!(all.len(), 4);
        // 证明是静态切片（编译期检查）
        let _static_ref: &'static [ModifierKey] = all;
    }
}
