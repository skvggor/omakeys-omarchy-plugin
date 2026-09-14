use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::layout::XkbMap;
use crate::names;

#[derive(Debug)]
pub enum DeviceEvent {
    Key { code: u16, pressed: bool },
    Btn { code: u16, pressed: bool },
    ScrollV { delta: i32 },
    ScrollH { delta: i32 },
}

#[derive(Clone)]
struct HeldEntry {
    display: String,
    order: u64,
}

#[derive(Clone, Serialize)]
struct EventEntry {
    text: String,
    at: u64,
}

const MAX_EVENTS: usize = 12;
const EVENT_TTL_MS: u64 = 3000;
const DEDUP_WINDOW_MS: u64 = 600;

pub struct AppState {
    mod_entries: BTreeMap<u16, HeldEntry>,
    key_entries: BTreeMap<u16, HeldEntry>,
    mouse_entries: BTreeMap<u16, HeldEntry>,
    events: VecDeque<EventEntry>,
    order: u64,
    last_activity_ms: u64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mod_entries: BTreeMap::new(),
            key_entries: BTreeMap::new(),
            mouse_entries: BTreeMap::new(),
            events: VecDeque::new(),
            order: 0,
            last_activity_ms: 0,
        }
    }

    pub fn reset(&mut self) {
        self.mod_entries.clear();
        self.key_entries.clear();
        self.mouse_entries.clear();
        self.events.clear();
        self.order = 0;
        self.last_activity_ms = 0;
    }

    pub fn handle(&mut self, xkb: &mut XkbMap, event: DeviceEvent) -> bool {
        match event {
            DeviceEvent::Key { code, pressed } => {
                xkb.feed(code, pressed);
                if xkb.is_modifier_code(code) {
                    self.set_mod(code, pressed, xkb);
                    if pressed {
                        self.push_combo_event();
                    }
                } else if pressed {
                    let display = xkb.display_for(code).into_display_name();
                    self.press_key(code, display);
                    self.push_combo_event();
                } else {
                    self.release_key(code);
                }
                true
            }
            DeviceEvent::Btn { code, pressed } => {
                let label = names::btn_label(code)
                    .map(String::from)
                    .unwrap_or_else(|| format!("Btn{code:#x}"));
                if pressed {
                    self.press_button(code, label);
                    self.push_mouse_event();
                } else {
                    self.release_button(code);
                }
                true
            }
            DeviceEvent::ScrollV { delta } => {
                self.push_text_event(names::wheel_label(delta));
                true
            }
            DeviceEvent::ScrollH { delta } => {
                self.push_text_event(names::wheel_h_label(delta));
                true
            }
        }
    }

    pub fn snapshot(&self, xkb: &XkbMap, registry: &crate::devices::DeviceRegistry) -> String {
        let mut error = xkb.error_message();
        let device_problem = registry.problem();
        if !device_problem.is_empty() {
            if !error.is_empty() {
                error.push_str(" · ");
            }
            error.push_str(&device_problem);
        }
        self.snapshot_json(&error)
    }

    fn set_mod(&mut self, code: u16, pressed: bool, xkb: &XkbMap) {
        if pressed {
            let display = xkb
                .mod_display_name(code)
                .unwrap_or_else(|| xkb.sym_name_of(code));
            self.order += 1;
            self.mod_entries.insert(code, HeldEntry { display, order: self.order });
            self.last_activity_ms = now_ms();
        } else {
            self.mod_entries.remove(&code);
        }
    }

    fn press_key(&mut self, code: u16, display: String) {
        self.order += 1;
        self.key_entries.insert(code, HeldEntry { display, order: self.order });
        self.last_activity_ms = now_ms();
    }

    fn release_key(&mut self, code: u16) {
        self.key_entries.remove(&code);
    }

    fn press_button(&mut self, code: u16, display: String) {
        self.order += 1;
        self.mouse_entries.insert(code, HeldEntry { display, order: self.order });
        self.last_activity_ms = now_ms();
    }

    fn release_button(&mut self, code: u16) {
        self.mouse_entries.remove(&code);
    }

    fn push_combo_event(&mut self) {
        let mut parts: Vec<String> = Vec::new();
        parts.extend(self.ordered_mods());
        parts.extend(self.key_entries.values().map(|entry| entry.display.clone()));
        if parts.is_empty() {
            return;
        }
        self.push_text_event(parts.join("+"));
    }

    fn push_mouse_event(&mut self) {
        let mut parts: Vec<String> = Vec::new();
        parts.extend(self.ordered_mods());
        parts.extend(self.mouse_entries.values().map(|entry| entry.display.clone()));
        if parts.is_empty() {
            return;
        }
        self.push_text_event(parts.join("+"));
    }

    fn push_text_event(&mut self, text: impl Into<String>) {
        let text = text.into();
        let at = now_ms();
        if let Some(front) = self.events.front() {
            if front.text == text && at.saturating_sub(front.at) <= DEDUP_WINDOW_MS {
                self.events.pop_front();
            }
        }
        self.events.push_front(EventEntry { text, at });
        self.last_activity_ms = at;
        self.prune_events(at);
    }

    fn prune_events(&mut self, at: u64) {
        while self.events.len() > MAX_EVENTS {
            self.events.pop_back();
        }
        while let Some(back) = self.events.back() {
            if at.saturating_sub(back.at) > EVENT_TTL_MS {
                self.events.pop_back();
            } else {
                break;
            }
        }
    }

    fn ordered_mods(&self) -> Vec<String> {
        let mut entries: Vec<&HeldEntry> = self.mod_entries.values().collect();
        entries.sort_by_key(|entry| named_mod_rank(&entry.display));
        entries.into_iter().map(|entry| entry.display.clone()).collect()
    }

    fn snapshot_json(&self, error: &str) -> String {
        let mut keys: Vec<String> = Vec::new();
        keys.extend(self.ordered_mods());
        let mut key_order: Vec<&HeldEntry> = self.key_entries.values().collect();
        key_order.sort_by_key(|entry| entry.order);
        keys.extend(key_order.into_iter().map(|entry| entry.display.clone()));

        let mut mouse: Vec<String> = Vec::new();
        let mut mouse_order: Vec<&HeldEntry> = self.mouse_entries.values().collect();
        mouse_order.sort_by_key(|entry| entry.order);
        mouse.extend(mouse_order.into_iter().map(|entry| entry.display.clone()));

        let events: Vec<EventEntry> = self.events.iter().cloned().collect();

        let snapshot = Snapshot {
            version: 1,
            ok: error.is_empty(),
            error: error.to_string(),
            keys,
            mouse,
            active_at: self.last_activity_ms,
            events,
        };
        serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string())
    }

    #[cfg(test)]
    pub fn events_texts(&self) -> Vec<String> {
        self.events.iter().map(|entry| entry.text.clone()).collect()
    }
}

fn named_mod_rank(display: &str) -> usize {
    names::MODIFIER_DISPLAY
        .iter()
        .position(|&candidate| candidate == display)
        .unwrap_or(usize::MAX)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Serialize)]
struct Snapshot {
    version: u32,
    ok: bool,
    error: String,
    keys: Vec<String>,
    mouse: Vec<String>,
    active_at: u64,
    events: Vec<EventEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (AppState, XkbMap) {
        let xkb = XkbMap::current(Some("us"), None, None);
        (AppState::new(), xkb)
    }

    #[test]
    fn combo_builds_mods_first_then_keys() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x38, pressed: true });
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1d, pressed: true });
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: true });
        let json = app_state.snapshot_json("");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["keys"], serde_json::json!(["Ctrl", "Alt", "a"]));
    }

    #[test]
    fn releasing_a_key_drops_it_from_the_combo() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: true });
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: false });
        let parsed: serde_json::Value = serde_json::from_str(&app_state.snapshot_json("")).unwrap();
        assert_eq!(parsed["keys"], serde_json::json!([]));
    }

    #[test]
    fn modifier_press_alone_generates_an_event() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x3a, pressed: true }); // Caps_Lock
        let texts = app_state.events_texts();
        assert_eq!(texts[0], "Caps");
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x3a, pressed: false });
        let parsed: serde_json::Value = serde_json::from_str(&app_state.snapshot_json("")).unwrap();
        assert_eq!(parsed["keys"], serde_json::json!([]));
    }

    #[test]
    fn chained_modifier_presses_build_the_combo_incrementally() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1d, pressed: true }); // Ctrl
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x38, pressed: true }); // Alt
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: true }); // 'a'
        let texts = app_state.events_texts();
        assert_eq!(texts[0], "Ctrl+Alt+a");
        assert_eq!(texts[1], "Ctrl+Alt");
        assert_eq!(texts[2], "Ctrl");
    }

    #[test]
    fn tapping_a_modifier_keeps_its_event_for_the_overlay() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x3a, pressed: true }); // Caps press
        let pressed: serde_json::Value =
            serde_json::from_str(&app_state.snapshot_json("")).unwrap();
        assert_eq!(pressed["keys"], serde_json::json!(["Caps"]));
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x3a, pressed: false }); // Caps release
        let released: serde_json::Value =
            serde_json::from_str(&app_state.snapshot_json("")).unwrap();
        assert_eq!(released["keys"], serde_json::json!([]));
        assert_eq!(released["events"][0]["text"], "Caps");
    }

    #[test]
    fn mouse_buttons_surface_separately() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1d, pressed: true });
        app_state.handle(&mut xkb, DeviceEvent::Btn { code: 0x110, pressed: true });
        let json = app_state.snapshot_json("");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["mouse"], serde_json::json!(["LEFT MOUSE BUTTON"]));
        assert_eq!(parsed["events"][0]["text"], "Ctrl+LEFT MOUSE BUTTON");
    }

    #[test]
    fn scroll_produces_transient_events() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::ScrollV { delta: -1 });
        let parsed: serde_json::Value = serde_json::from_str(&app_state.snapshot_json("")).unwrap();
        assert_eq!(parsed["events"][0]["text"], "Scroll Down");
        assert_eq!(parsed["keys"], serde_json::json!([]));
    }

    #[test]
    fn rapid_near_identical_combos_are_deduplicated() {
        let (mut app_state, mut xkb) = state();
        for _ in 0..4 {
            app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: true });
            app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: false });
        }
        let texts = app_state.events_texts();
        assert_eq!(texts[0], "a");
        assert_eq!(texts.iter().filter(|text| text.as_str() == "a").count(), 1);
    }

    #[test]
    fn snapshot_is_well_formed() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: true });
        let parsed: serde_json::Value = serde_json::from_str(&app_state.snapshot_json("")).unwrap();
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["ok"], true);
        assert!(parsed["active_at"].as_u64().unwrap() > 0);
    }

    #[test]
    fn reset_clears_held_keys_and_history() {
        let (mut app_state, mut xkb) = state();
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1d, pressed: true });
        app_state.handle(&mut xkb, DeviceEvent::Key { code: 0x1e, pressed: true });
        app_state.handle(&mut xkb, DeviceEvent::ScrollV { delta: -1 });
        app_state.reset();
        let parsed: serde_json::Value = serde_json::from_str(&app_state.snapshot_json("")).unwrap();
        assert_eq!(parsed["keys"], serde_json::json!([]));
        assert_eq!(parsed["mouse"], serde_json::json!([]));
        assert_eq!(parsed["events"], serde_json::json!([]));
        assert_eq!(parsed["active_at"], 0);
    }

    #[test]
    fn snapshot_reports_xkb_compile_errors() {
        let (app_state, _xkb) = state();
        let failing = XkbMap::current(Some("not-a-real-layout"), None, None);
        let json = app_state.snapshot(&failing, &registry());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["ok"], false);
        assert_eq!(
            parsed["error"].as_str().unwrap(),
            "cannot compile keymap for layout \"not-a-real-layout\""
        );
    }

    fn registry() -> crate::devices::DeviceRegistry {
        let (sender, _receiver) = std::sync::mpsc::channel::<DeviceEvent>();
        crate::devices::DeviceRegistry::new(sender, false)
    }
}