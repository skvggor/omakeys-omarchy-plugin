use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use evdev::{Device, EventSummary, KeyCode, RelativeAxisCode};

use crate::state::DeviceEvent;

#[derive(Clone, Copy, PartialEq, Debug)]
enum DeviceKind {
    Keyboard,
    Mouse,
}

pub struct DeviceRegistry {
    open: Arc<Mutex<HashSet<String>>>,
    sender: Sender<DeviceEvent>,
    notice: Arc<Mutex<String>>,
    debug: bool,
}

impl DeviceRegistry {
    pub fn new(sender: Sender<DeviceEvent>, debug: bool) -> Self {
        Self {
            open: Arc::new(Mutex::new(HashSet::new())),
            sender,
            notice: Arc::new(Mutex::new(String::new())),
            debug,
        }
    }

    pub fn problem(&self) -> String {
        self.notice
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn rescan(&self) {
        let Ok(entries) = std::fs::read_dir("/dev/input") else {
            self.set_notice("cannot list /dev/input".to_string());
            return;
        };

        let held = self.open.lock().expect("device registry lock");
        let mut candidates: Vec<(PathBuf, DeviceKind)> = Vec::new();
        let mut saw_event_device = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.file_name().map(|n| n.to_string_lossy().starts_with("event")).unwrap_or(false) {
                continue;
            }
            saw_event_device = true;
            if let Some(kind) = classify(&path) {
                candidates.push((path, kind));
            }
        }
        drop(held);

        if candidates.is_empty() {
            if saw_event_device {
                self.set_notice(
                    "no readable input devices: grant evdev access with './bin/omarchy-install-omakeys --install' (setgid input) or make sure this user can read /dev/input/event*"
                        .to_string(),
                );
            }
            return;
        }
        self.set_notice(String::new());

        for (path, kind) in candidates {
            let key = path.to_string_lossy().to_string();
            let mut held = self.open.lock().expect("device registry lock");
            if held.contains(&key) {
                continue;
            }
            held.insert(key.clone());
            drop(held);
            let sender = self.sender.clone();
            let open_set = self.open.clone();
            let debug = self.debug;
            spawn_reader(sender, open_set, key, kind, debug);
        }
    }

    fn set_notice(&self, message: String) {
        let mut guard = self
            .notice
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = message;
    }
}

fn classify(path: &Path) -> Option<DeviceKind> {
    let device = Device::open(path).ok()?;
    let has_keys = device
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::KEY_A));
    if has_keys {
        return Some(DeviceKind::Keyboard);
    }
    let has_mouse_button = device.supported_keys().is_some_and(|keys| {
        keys.contains(KeyCode::BTN_LEFT)
            || keys.contains(KeyCode::BTN_RIGHT)
            || keys.contains(KeyCode::BTN_MIDDLE)
    });
    let has_relative_axes = device.supported_relative_axes().is_some_and(|axes| {
        axes.contains(RelativeAxisCode::REL_X) || axes.contains(RelativeAxisCode::REL_WHEEL)
    });
    if has_mouse_button && has_relative_axes {
        return Some(DeviceKind::Mouse);
    }
    None
}

fn spawn_reader(sender: Sender<DeviceEvent>, open_set: Arc<Mutex<HashSet<String>>>, path: String, kind: DeviceKind, debug: bool) {
    std::thread::spawn(move || {
        let mut device = match Device::open(Path::new(&path)) {
            Ok(device) => device,
            Err(_) => {
                detach(&open_set, &path);
                return;
            }
        };
        if debug {
            eprintln!(
                "[debug] {:?} opened {}",
                kind,
                std::fs::canonicalize(&path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| path.clone())
                );
        }
        loop {
            let events = match device.fetch_events() {
                Ok(events) => events,
                Err(_) => break,
            };
            for event in events {
                if debug {
                    eprintln!("[debug] {:?} {} {:?}", kind, path, event.destructure());
                }
                if let Some(translated) = translate(event, kind) {
                    if sender.send(translated).is_err() {
                        return;
                    }
                }
            }
        }
        detach(&open_set, &path);
    });
}

fn detach(open_set: &Arc<Mutex<HashSet<String>>>, path: &str) {
    if let Ok(mut held) = open_set.lock() {
        held.remove(path);
    }
}

fn high_res_delta(value: i32) -> i32 {
    const HIGH_RES_UNIT: i32 = 120;
    if value == 0 {
        return 0;
    }
    let notches = value.abs() / HIGH_RES_UNIT.max(1);
    if notches == 0 {
        return if value > 0 { 1 } else { -1 };
    }
    notches * value.signum()
}

fn translate(event: evdev::InputEvent, kind: DeviceKind) -> Option<DeviceEvent> {
    match event.destructure() {
        EventSummary::Key(_, code, value) => {
            if value == 2 {
                return None;
            }
            let pressed = value == 1;
            if code.code() >= 0x100 && kind == DeviceKind::Mouse {
                return Some(DeviceEvent::Btn { code: code.code(), pressed });
            }
            Some(DeviceEvent::Key { code: code.code(), pressed })
        }
        EventSummary::RelativeAxis(_, code, value) => {
            if kind != DeviceKind::Mouse {
                return None;
            }
            match code {
                RelativeAxisCode::REL_WHEEL => Some(DeviceEvent::ScrollV { delta: value }),
                RelativeAxisCode::REL_HWHEEL => Some(DeviceEvent::ScrollH { delta: value }),
                RelativeAxisCode::REL_WHEEL_HI_RES => {
                    Some(DeviceEvent::ScrollV { delta: high_res_delta(value) })
                }
                RelativeAxisCode::REL_HWHEEL_HI_RES => {
                    Some(DeviceEvent::ScrollH { delta: high_res_delta(value) })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

static SIGNAL_FLAG: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_signal: libc::c_int) {
    SIGNAL_FLAG.store(true, Ordering::Relaxed);
}

pub fn install_signal_handlers() {
    let handler = handle_signal as *const () as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

pub fn shutdown_requested() -> bool {
    SIGNAL_FLAG.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_ignores_unreadable_paths() {
        let result = classify(Path::new("/dev/input/does-not-exist"));
        assert_eq!(result, None);
    }

    #[test]
    fn key_repeat_is_dropped() {
        let event = evdev::InputEvent::new(evdev::EventType::KEY.0, 30, 2);
        let translated = translate(event, DeviceKind::Keyboard);
        assert!(translated.is_none());
    }

    #[test]
    fn key_down_is_described() {
        let pressed = evdev::InputEvent::new(evdev::EventType::KEY.0, 30, 1);
        let translated = translate(pressed, DeviceKind::Keyboard);
        assert!(matches!(translated, Some(DeviceEvent::Key { code: 30, pressed: true })));

        let released = evdev::InputEvent::new(evdev::EventType::KEY.0, 30, 0);
        assert!(matches!(
            translate(released, DeviceKind::Keyboard),
            Some(DeviceEvent::Key { code: 30, pressed: false })
        ));
    }

    #[test]
    fn mouse_button_codes_become_buttons() {
        let pressed = evdev::InputEvent::new(evdev::EventType::KEY.0, 0x110, 1);
        assert!(matches!(
            translate(pressed, DeviceKind::Mouse),
            Some(DeviceEvent::Btn { code: 0x110, pressed: true })
        ));
    }

    #[test]
    fn wheel_codes_become_scroll_events() {
        let delta = evdev::InputEvent::new(evdev::EventType::RELATIVE.0, 8, -1);
        assert!(matches!(
            translate(delta, DeviceKind::Mouse),
            Some(DeviceEvent::ScrollV { delta: -1 })
        ));
    }

    #[test]
    fn high_res_wheel_codes_become_scroll_events() {
        let up = evdev::InputEvent::new(evdev::EventType::RELATIVE.0, 0x0b, 120);
        assert!(matches!(
            translate(up, DeviceKind::Mouse),
            Some(DeviceEvent::ScrollV { delta: 1 })
        ));
        let down = evdev::InputEvent::new(evdev::EventType::RELATIVE.0, 0x0b, -120);
        assert!(matches!(
            translate(down, DeviceKind::Mouse),
            Some(DeviceEvent::ScrollV { delta: -1 })
        ));
    }

    #[test]
    fn high_res_hwheel_codes_become_scroll_events() {
        let left = evdev::InputEvent::new(evdev::EventType::RELATIVE.0, 0x0c, -120);
        assert!(matches!(
            translate(left, DeviceKind::Mouse),
            Some(DeviceEvent::ScrollH { delta: -1 })
        ));
    }

    #[test]
    fn high_res_delta_normalizes_partial_notches() {
        assert_eq!(high_res_delta(120), 1);
        assert_eq!(high_res_delta(-120), -1);
        assert_eq!(high_res_delta(240), 2);
        assert_eq!(high_res_delta(60), 1);
        assert_eq!(high_res_delta(-60), -1);
        assert_eq!(high_res_delta(0), 0);
    }
}