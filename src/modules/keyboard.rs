use rdev::{simulate, EventType, Key};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use super::config::ModifierKey;

/// 全局互斥锁，确保快捷键串行执行（避免并发冲突和线程爆炸）
static HOTKEY_LOCK: Mutex<()> = Mutex::new(());

/// 将配置中的修饰键映射到 rdev 的 Key
fn modifier_to_key(modifier: &ModifierKey) -> Key {
    match modifier {
        ModifierKey::Ctrl => Key::ControlLeft,
        ModifierKey::Alt => Key::Alt,
        ModifierKey::Shift => Key::ShiftLeft,
        ModifierKey::Super => Key::MetaLeft,
    }
}

/// 将配置中的主键字符串映射到 rdev 的 Key（零分配，直接比较字节）
fn key_from_str(key: &str) -> Option<Key> {
    match key.as_bytes() {
        // 字母键（大写）
        b"A" => Some(Key::KeyA), b"B" => Some(Key::KeyB), b"C" => Some(Key::KeyC),
        b"D" => Some(Key::KeyD), b"E" => Some(Key::KeyE), b"F" => Some(Key::KeyF),
        b"G" => Some(Key::KeyG), b"H" => Some(Key::KeyH), b"I" => Some(Key::KeyI),
        b"J" => Some(Key::KeyJ), b"K" => Some(Key::KeyK), b"L" => Some(Key::KeyL),
        b"M" => Some(Key::KeyM), b"N" => Some(Key::KeyN), b"O" => Some(Key::KeyO),
        b"P" => Some(Key::KeyP), b"Q" => Some(Key::KeyQ), b"R" => Some(Key::KeyR),
        b"S" => Some(Key::KeyS), b"T" => Some(Key::KeyT), b"U" => Some(Key::KeyU),
        b"V" => Some(Key::KeyV), b"W" => Some(Key::KeyW), b"X" => Some(Key::KeyX),
        b"Y" => Some(Key::KeyY), b"Z" => Some(Key::KeyZ),
        // 字母键（小写）
        b"a" => Some(Key::KeyA), b"b" => Some(Key::KeyB), b"c" => Some(Key::KeyC),
        b"d" => Some(Key::KeyD), b"e" => Some(Key::KeyE), b"f" => Some(Key::KeyF),
        b"g" => Some(Key::KeyG), b"h" => Some(Key::KeyH), b"i" => Some(Key::KeyI),
        b"j" => Some(Key::KeyJ), b"k" => Some(Key::KeyK), b"l" => Some(Key::KeyL),
        b"m" => Some(Key::KeyM), b"n" => Some(Key::KeyN), b"o" => Some(Key::KeyO),
        b"p" => Some(Key::KeyP), b"q" => Some(Key::KeyQ), b"r" => Some(Key::KeyR),
        b"s" => Some(Key::KeyS), b"t" => Some(Key::KeyT), b"u" => Some(Key::KeyU),
        b"v" => Some(Key::KeyV), b"w" => Some(Key::KeyW), b"x" => Some(Key::KeyX),
        b"y" => Some(Key::KeyY), b"z" => Some(Key::KeyZ),
        // 数字键
        b"0" => Some(Key::Num0), b"1" => Some(Key::Num1), b"2" => Some(Key::Num2),
        b"3" => Some(Key::Num3), b"4" => Some(Key::Num4), b"5" => Some(Key::Num5),
        b"6" => Some(Key::Num6), b"7" => Some(Key::Num7), b"8" => Some(Key::Num8),
        b"9" => Some(Key::Num9),
        // 功能键 & 特殊键（多字节，按 &str 匹配）
        _ => match key {
            "F1" => Some(Key::F1), "F2" => Some(Key::F2), "F3" => Some(Key::F3),
            "F4" => Some(Key::F4), "F5" => Some(Key::F5), "F6" => Some(Key::F6),
            "F7" => Some(Key::F7), "F8" => Some(Key::F8), "F9" => Some(Key::F9),
            "F10" => Some(Key::F10), "F11" => Some(Key::F11), "F12" => Some(Key::F12),
            "Space" | "SPACE" => Some(Key::Space),
            "Enter" | "ENTER" | "Return" | "RETURN" => Some(Key::Return),
            "Tab" | "TAB" => Some(Key::Tab),
            "Escape" | "ESC" | "Esc" => Some(Key::Escape),
            "Backspace" | "BACKSPACE" => Some(Key::Backspace),
            "Delete" | "DEL" | "Del" => Some(Key::Delete),
            "Home" | "HOME" => Some(Key::Home),
            "End" | "END" => Some(Key::End),
            "PageUp" | "PAGEUP" => Some(Key::PageUp),
            "PageDown" | "PAGEDOWN" => Some(Key::PageDown),
            "Up" | "UP" => Some(Key::UpArrow),
            "Down" | "DOWN" => Some(Key::DownArrow),
            "Left" | "LEFT" => Some(Key::LeftArrow),
            "Right" | "RIGHT" => Some(Key::RightArrow),
            _ => None,
        },
    }
}

/// 发送按键按下事件（带错误日志）
fn send_key_press(key: Key) {
    match simulate(&EventType::KeyPress(key)) {
        Ok(()) => {}
        Err(e) => log::warn!("按键按下失败 {:?}: {:?}", key, e),
    }
}

/// 发送按键释放事件（带错误日志）
fn send_key_release(key: Key) {
    match simulate(&EventType::KeyRelease(key)) {
        Ok(()) => {}
        Err(e) => log::warn!("按键释放失败 {:?}: {:?}", key, e),
    }
}

/// RAII 按键守卫 — 即使 panic 也能确保释放按键
struct KeyGuard {
    key: Key,
    released: bool,
}

impl KeyGuard {
    fn press(key: Key) -> Self {
        send_key_press(key);
        KeyGuard {
            key,
            released: false,
        }
    }

    /// 手动释放（避免 Drop 中重复释放）
    fn release(&mut self) {
        if !self.released {
            send_key_release(self.key);
            self.released = true;
        }
    }
}

impl Drop for KeyGuard {
    fn drop(&mut self) {
        self.release();
    }
}

/// 在后台线程执行快捷键（避免阻塞 UI 线程），使用全局锁串行化
pub fn execute_hotkey_async(modifiers: Vec<ModifierKey>, key: String) {
    thread::spawn(move || {
        // 获取全局锁，确保同一时刻只有一个快捷键在执行
        let _lock = HOTKEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = execute_hotkey_sync(&modifiers, &key) {
            log::error!("快捷键执行失败: {}", e);
        }
    });
}

/// 同步执行快捷键（内部使用，会被 async 版本调用）
pub fn execute_hotkey_sync(modifiers: &[ModifierKey], key: &str) -> Result<(), String> {
    let main_key = key_from_str(key).ok_or_else(|| format!("未知按键: {}", key))?;
    let mod_keys: Vec<Key> = modifiers.iter().map(modifier_to_key).collect();

    log::info!("执行快捷键: {}",
        {
            let mut s = String::with_capacity(32);
            for (i, m) in modifiers.iter().enumerate() {
                if i > 0 { s.push('+'); }
                s.push_str(m.display_name());
            }
            s.push('+');
            s.push_str(key);
            s
        }
    );

    // 使用 RAII 守卫按下修饰键 — 即使 panic 也能释放
    let mut mod_guards: Vec<KeyGuard> = mod_keys.iter().map(|k| KeyGuard::press(*k)).collect();

    // 短暂延迟确保修饰键生效
    thread::sleep(Duration::from_millis(50));

    // 按下并释放主键
    {
        let mut main_guard = KeyGuard::press(main_key);
        thread::sleep(Duration::from_millis(30));
        main_guard.release();
    }

    // 逆序释放所有修饰键（RAII 也会在 drop 时释放，但显式释放更可控）
    for guard in mod_guards.iter_mut().rev() {
        guard.release();
    }

    Ok(())
}

/// 可用按键列表（静态，避免每帧分配）
static AVAILABLE_KEYS: &[&str] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M",
    "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "Space", "Enter", "Tab", "Escape", "Backspace", "Delete",
    "Home", "End", "PageUp", "PageDown",
    "Up", "Down", "Left", "Right",
];

/// 获取所有可用的主键列表（零分配）
pub fn available_keys() -> &'static [&'static str] {
    AVAILABLE_KEYS
}
