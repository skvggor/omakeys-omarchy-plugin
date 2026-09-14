use crate::names;

use xkbcommon::xkb;
use xkbcommon::xkb::keysyms::*;

const LEVEL_MODIFIER_SYMS: [xkb::Keysym; 7] = [
    xkb::Keysym::new(KEY_Shift_L),
    xkb::Keysym::new(KEY_Shift_R),
    xkb::Keysym::new(KEY_Caps_Lock),
    xkb::Keysym::new(KEY_Num_Lock),
    xkb::Keysym::new(KEY_ISO_Level3_Shift),
    xkb::Keysym::new(KEY_ISO_Level3_Lock),
    xkb::Keysym::new(KEY_ISO_Level5_Shift),
];

const GROUP_SYMS: [xkb::Keysym; 9] = [
    xkb::Keysym::new(KEY_ISO_Next_Group),
    xkb::Keysym::new(KEY_ISO_Prev_Group),
    xkb::Keysym::new(KEY_ISO_First_Group),
    xkb::Keysym::new(KEY_ISO_Last_Group),
    xkb::Keysym::new(KEY_ISO_Next_Group_Lock),
    xkb::Keysym::new(KEY_ISO_Prev_Group_Lock),
    xkb::Keysym::new(KEY_ISO_First_Group_Lock),
    xkb::Keysym::new(KEY_ISO_Last_Group_Lock),
    xkb::Keysym::new(KEY_ISO_Group_Latch),
];

pub struct XkbMap {
    keymap: xkb::Keymap,
    state: xkb::State,
    error: String,
}

pub enum KeyDisplay {
    Modifier(String),
    Character(String),
    Label(String),
}

impl KeyDisplay {
    pub fn into_display_name(self) -> String {
        match self {
            KeyDisplay::Modifier(name)
            | KeyDisplay::Character(name)
            | KeyDisplay::Label(name) => name,
        }
    }
}

impl XkbMap {
    pub fn current(layout: Option<&str>, variant: Option<&str>, options: Option<&str>) -> Self {
        let fetched = fetch_layout_from_hyprctl();
        let (layout, variant, options) = (
            layout.map(String::from).unwrap_or_else(|| fetched.0),
            variant.map(String::from).unwrap_or_else(|| fetched.1),
            options.map(String::from).unwrap_or_else(|| fetched.2),
        );
        Self::build(layout, variant, options)
    }

    pub fn error_message(&self) -> String {
        self.error.clone()
    }

    fn build(layout: String, variant: String, options: String) -> Self {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let compiled = xkb::Keymap::new_from_names(
            &context,
            &"",
            &"",
            &layout.as_str(),
            &variant.as_str(),
            if options.is_empty() {
                None
            } else {
                Some(options.clone())
            },
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        );
        match compiled {
            Some(keymap) => {
                let state = xkb::State::new(&keymap);
                Self {
                    keymap,
                    state,
                    error: String::new(),
                }
            }
            None => {
                let message = format!("cannot compile keymap for layout \"{layout}\"");
                let fallback = xkb::Keymap::new_from_names(
                    &context,
                    &"",
                    &"",
                    &"us",
                    &"",
                    None,
                    xkb::KEYMAP_COMPILE_NO_FLAGS,
                )
                .unwrap_or_else(|| panic!("cannot compile even the default keymap"));
                let state = xkb::State::new(&fallback);
                Self {
                    keymap: fallback,
                    state,
                    error: message,
                }
            }
        }
    }

    fn to_xkb_keycode(code: u16) -> xkb::Keycode {
        xkb::Keycode::new(code as u32 + 8)
    }

    pub fn feed(&mut self, code: u16, pressed: bool) {
        if !self.is_level_or_group_code(code) {
            return;
        }
        let keycode = Self::to_xkb_keycode(code);
        let direction = if pressed {
            xkb::KeyDirection::Down
        } else {
            xkb::KeyDirection::Up
        };
        self.state.update_key(keycode, direction);
    }

    pub fn sym_name_of(&self, code: u16) -> String {
        let sym = self.base_sym(code);
        xkb::keysym_get_name(sym)
    }

    pub fn is_modifier_code(&self, code: u16) -> bool {
        let sym_name = self.sym_name_of(code);
        names::is_modifier(&sym_name)
    }

    pub fn display_for(&self, code: u16) -> KeyDisplay {
        let keycode = Self::to_xkb_keycode(code);
        let sym = self.base_sym(code);
        let sym_name = xkb::keysym_get_name(sym);

        if names::is_modifier(&sym_name) {
            let display = names::modifier_display(&sym_name)
                .map(String::from)
                .unwrap_or_else(|| names::friendly(&sym_name));
            return KeyDisplay::Modifier(display);
        }

        if let Some(custom) = names::is_function_key(&sym_name) {
            return KeyDisplay::Label(custom);
        }

        let utf8 = self.state.key_get_utf8(keycode);
        let trimmed = utf8.trim();
        if trimmed == " " {
            return KeyDisplay::Label("Space".to_string());
        }
        if !trimmed.is_empty() && !trimmed.chars().any(char::is_control) {
            return KeyDisplay::Character(utf8);
        }

        KeyDisplay::Label(names::friendly(&sym_name))
    }

    pub fn mod_display_name(&self, code: u16) -> Option<String> {
        let sym_name = self.sym_name_of(code);
        names::modifier_display(&sym_name).map(String::from)
    }

    fn base_sym(&self, code: u16) -> xkb::Keysym {
        let keycode = Self::to_xkb_keycode(code);
        let syms = self.keymap.key_get_syms_by_level(keycode, 0, 0);
        *syms.first().unwrap_or(&xkb::Keysym::new(KEY_NoSymbol))
    }

    fn is_level_or_group_code(&self, code: u16) -> bool {
        let sym = self.base_sym(code);
        if LEVEL_MODIFIER_SYMS.contains(&sym) {
            return true;
        }
        let keycode = Self::to_xkb_keycode(code);
        for level in 0..=3 {
            let syms = self.keymap.key_get_syms_by_level(keycode, 0, level);
            if syms.iter().any(|candidate| GROUP_SYMS.contains(candidate)) {
                return true;
            }
        }
        false
    }
}

fn fetch_layout_from_hyprctl() -> (String, String, String) {
    let layout = hyprctl_str("input:kb_layout");
    let variant = hyprctl_str("input:kb_variant");
    let options_rule = hyprctl_str("input:kb_options");
    (
        layout.unwrap_or_else(|| "us".to_string()),
        variant.unwrap_or_default(),
        options_rule.unwrap_or_default(),
    )
}

fn hyprctl_str(option: &str) -> Option<String> {
    let output = std::process::Command::new("hyprctl")
        .args(["getoption", option, "-j"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let value = parsed.get("str")?.as_str()?;
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_basic() -> XkbMap {
        XkbMap::build("us".to_string(), String::new(), String::new())
    }

    #[test]
    fn letter_codes_resolve_to_printable() {
        let mut map = map_basic();
        let a = 0x1e; // KEY_A
        assert!(!map.is_modifier_code(a));
        assert_eq!(map.display_for(a).into_display_name(), "a");
        map.feed(0x2a, true); // KEY_LEFTSHIFT
        assert_eq!(map.display_for(a).into_display_name(), "A");
    }

    #[test]
    fn modifiers_are_detected() {
        let map = map_basic();
        assert!(map.is_modifier_code(0x1d)); // KEY_LEFTCTRL
        assert!(map.is_modifier_code(0x38)); // KEY_LEFTALT
        assert!(map.is_modifier_code(0x7d)); // KEY_LEFTMETA
        assert!(map.is_modifier_code(0x3a)); // KEY_CAPSLOCK
        if let Some(name) = map.mod_display_name(0x1d) {
            assert_eq!(name, "Ctrl");
        }
    }

    #[test]
    fn ctrl_does_not_distort_the_character() {
        let mut map = map_basic();
        map.feed(0x1d, true); // KEY_LEFTCTRL
        assert_eq!(map.display_for(0x1e).into_display_name(), "a");
    }

    #[test]
    fn enter_is_a_label() {
        let map = map_basic();
        assert_eq!(map.display_for(0x1c).into_display_name(), "Enter"); // KEY_ENTER
    }

    #[test]
    fn control_keys_fall_back_to_labels() {
        let map = map_basic();
        assert_eq!(map.display_for(0x01).into_display_name(), "Esc"); // KEY_ESC
        assert_eq!(map.display_for(0x0e).into_display_name(), "Backspace"); // KEY_BACKSPACE
        assert_eq!(map.display_for(0x6f).into_display_name(), "Del"); // KEY_DELETE
    }
}