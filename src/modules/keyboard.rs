use rdev::{simulate, EventType, Key};
use std::thread;
use std::time::Duration;

use super::config::ModifierKey;

/// 将配置中的修饰键映射到 rdev 的 Key
fn modifier_to_key(modifier: &ModifierKey) -> Key {
    match modifier {
        ModifierKey::Ctrl => Key::ControlLeft,
        ModifierKey::Alt => Key::Alt,
        ModifierKey::Shift => Key::ShiftLeft,
        ModifierKey::Super => Key::MetaLeft,
    }
}

/// 将配置中的主键字符串映射到 rdev 的 Key
fn key_from_str(key: &str) -> Option<Key> {
    match key.to_uppercase().as_str() {
        // 字母键
        "A" => Some(Key::KeyA), "B" => Some(Key::KeyB), "C" => Some(Key::KeyC),
        "D" => Some(Key::KeyD), "E" => Some(Key::KeyE), "F" => Some(Key::KeyF),
        "G" => Some(Key::KeyG), "H" => Some(Key::KeyH), "I" => Some(Key::KeyI),
        "J" => Some(Key::KeyJ), "K" => Some(Key::KeyK), "L" => Some(Key::KeyL),
        "M" => Some(Key::KeyM), "N" => Some(Key::KeyN), "O" => Some(Key::KeyO),
        "P" => Some(Key::KeyP), "Q" => Some(Key::KeyQ), "R" => Some(Key::KeyR),
        "S" => Some(Key::KeyS), "T" => Some(Key::KeyT), "U" => Some(Key::KeyU),
        "V" => Some(Key::KeyV), "W" => Some(Key::KeyW), "X" => Some(Key::KeyX),
        "Y" => Some(Key::KeyY), "Z" => Some(Key::KeyZ),
        // 数字键
        "0" => Some(Key::Num0), "1" => Some(Key::Num1), "2" => Some(Key::Num2),
        "3" => Some(Key::Num3), "4" => Some(Key::Num4), "5" => Some(Key::Num5),
        "6" => Some(Key::Num6), "7" => Some(Key::Num7), "8" => Some(Key::Num8),
        "9" => Some(Key::Num9),
        // 功能键
        "F1" => Some(Key::F1), "F2" => Some(Key::F2), "F3" => Some(Key::F3),
        "F4" => Some(Key::F4), "F5" => Some(Key::F5), "F6" => Some(Key::F6),
        "F7" => Some(Key::F7), "F8" => Some(Key::F8), "F9" => Some(Key::F9),
        "F10" => Some(Key::F10), "F11" => Some(Key::F11), "F12" => Some(Key::F12),
        // 特殊键
        "SPACE" => Some(Key::Space),
        "ENTER" | "RETURN" => Some(Key::Return),
        "TAB" => Some(Key::Tab),
        "ESCAPE" | "ESC" => Some(Key::Escape),
        "BACKSPACE" => Some(Key::Backspace),
        "DELETE" | "DEL" => Some(Key::Delete),
        "HOME" => Some(Key::Home),
        "END" => Some(Key::End),
        "PAGEUP" => Some(Key::PageUp),
        "PAGEDOWN" => Some(Key::PageDown),
        "UP" => Some(Key::UpArrow),
        "DOWN" => Some(Key::DownArrow),
        "LEFT" => Some(Key::LeftArrow),
        "RIGHT" => Some(Key::RightArrow),
        _ => None,
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

/// 在后台线程执行快捷键（避免阻塞 UI 线程）
pub fn execute_hotkey_async(modifiers: Vec<ModifierKey>, key: String) {
    thread::spawn(move || {
        if let Err(e) = execute_hotkey_sync(&modifiers, &key) {
            log::error!("快捷键执行失败: {}", e);
        }
    });
}

/// 同步执行快捷键（内部使用，会被 async 版本调用）
pub fn execute_hotkey_sync(modifiers: &[ModifierKey], key: &str) -> Result<(), String> {
    let main_key = key_from_str(key).ok_or_else(|| format!("未知按键: {}", key))?;
    let mod_keys: Vec<Key> = modifiers.iter().map(modifier_to_key).collect();

    log::info!("执行快捷键: {}+{}",
        modifiers.iter().map(|m| m.display_name()).collect::<Vec<_>>().join("+"),
        key
    );

    // 按下所有修饰键
    for mk in &mod_keys {
        send_key_press(*mk);
    }

    // 短暂延迟确保修饰键生效
    thread::sleep(Duration::from_millis(50));

    // 按下主键
    send_key_press(main_key);
    thread::sleep(Duration::from_millis(30));
    send_key_release(main_key);

    // 释放所有修饰键（逆序）
    for mk in mod_keys.iter().rev() {
        send_key_release(*mk);
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
