pub const MODIFIER_SYMS: &[&str] = &[
    "Control_L",
    "Control_R",
    "Shift_L",
    "Shift_R",
    "Alt_L",
    "Alt_R",
    "Meta_L",
    "Meta_R",
    "Super_L",
    "Super_R",
    "Hyper_L",
    "Hyper_R",
    "ISO_Level3_Shift",
    "ISO_Level5_Shift",
    "Caps_Lock",
];

pub const MODIFIER_DISPLAY: &[&str] = &[
    "Ctrl",
    "Ctrl",
    "Shift",
    "Shift",
    "Alt",
    "Alt",
    "Super",
    "Super",
    "Super",
    "Super",
    "Super",
    "Super",
    "AltGr",
    "AltGr",
    "Caps",
];

pub fn is_modifier(sym_name: &str) -> bool {
    MODIFIER_SYMS.contains(&sym_name)
}

pub fn modifier_display(sym_name: &str) -> Option<&'static str> {
    MODIFIER_SYMS
        .iter()
        .position(|&candidate| candidate == sym_name)
        .map(|index| MODIFIER_DISPLAY[index])
}

fn fn_display(index: u32) -> String {
    if index == 0 {
        "F1".to_string()
    } else {
        format!("F{}", index + 1)
    }
}

pub fn friendly(sym_name: &str) -> String {
    if let Some(display) = modifier_display(sym_name) {
        return display.to_string();
    }
    match sym_name {
        "Return" => "Enter",
        "KP_Enter" => "Enter",
        "Escape" => "Esc",
        "BackSpace" => "Backspace",
        "Tab" | "ISO_Left_Tab" => "Tab",
        "Delete" => "Del",
        "Insert" => "Ins",
        "Home" => "Home",
        "End" => "End",
        "Page_Up" | "Prior" => "Page Up",
        "Page_Down" | "Next" => "Page Down",
        "Up" => "Up",
        "Down" => "Down",
        "Left" => "Left",
        "Right" => "Right",
        "Print" => "Print Screen",
        "Pause" => "Pause",
        "Num_Lock" => "Num",
        "Scroll_Lock" => "ScrLk",
        "space" => "Space",
        "Multiply" => "*",
        "Add" => "+",
        "Subtract" => "-",
        "Divide" => "/",
        "KP_Add" => "+",
        "KP_Subtract" => "-",
        "KP_Multiply" => "*",
        "KP_Divide" => "/",
        "KP_0" => "0",
        "KP_1" => "1",
        "KP_2" => "2",
        "KP_3" => "3",
        "KP_4" => "4",
        "KP_5" => "5",
        "KP_6" => "6",
        "KP_7" => "7",
        "KP_8" => "8",
        "KP_9" => "9",
        "KP_Equal" => "=",
        "KP_Decimal" => ".",
        "KP_Separator" => ",",
        "XF86AudioPlay" => "Play",
        "XF86AudioPause" => "Pause",
        "XF86AudioStop" => "Stop",
        "XF86AudioNext" => "Next",
        "XF86AudioPrev" => "Previous",
        "XF86AudioRaiseVolume" => "Volume Up",
        "XF86AudioLowerVolume" => "Volume Down",
        "XF86AudioMute" => "Mute",
        "XF86MonBrightnessUp" => "Brightness Up",
        "XF86MonBrightnessDown" => "Brightness Down",
        other => match other.strip_prefix("XF86") {
            Some(rest) => return rest.to_string(),
            None => other,
        },
    }
    .to_string()
}

pub fn is_function_key(sym_name: &str) -> Option<String> {
    sym_name
        .strip_prefix('F')
        .and_then(|digits| digits.parse::<u32>().ok())
        .filter(|&number| (1..=35).contains(&number))
        .map(fn_display)
}

pub fn btn_label(code: u16) -> Option<&'static str> {
    match code {
        0x110 => Some("LEFT MOUSE BUTTON"),
        0x111 => Some("RIGHT MOUSE BUTTON"),
        0x112 => Some("MID MOUSE BUTTON"),
        0x113 => Some("Side1"),
        0x114 => Some("Side2"),
        0x115 => Some("Back"),
        0x116 => Some("Forward"),
        _ => None,
    }
}

pub fn wheel_label(delta: i32) -> &'static str {
    if delta > 0 {
        "Scroll Up"
    } else {
        "Scroll Down"
    }
}

pub fn wheel_h_label(delta: i32) -> &'static str {
    if delta > 0 {
        "Scroll Right"
    } else {
        "Scroll Left"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_modifiers_are_concise() {
        assert_eq!(friendly("Control_L"), "Ctrl");
        assert_eq!(friendly("Super_R"), "Super");
        assert_eq!(friendly("Shift_L"), "Shift");
        assert_eq!(friendly("ISO_Level3_Shift"), "AltGr");
        assert_eq!(friendly("Caps_Lock"), "Caps");
    }

    #[test]
    fn friendly_navigation_keys() {
        assert_eq!(friendly("Return"), "Enter");
        assert_eq!(friendly("Escape"), "Esc");
        assert_eq!(friendly("Page_Up"), "Page Up");
        assert_eq!(friendly("Page_Down"), "Page Down");
        assert_eq!(friendly("Prior"), "Page Up");
        assert_eq!(friendly("Next"), "Page Down");
        assert_eq!(friendly("Up"), "Up");
        assert_eq!(friendly("Tab"), "Tab");
        assert_eq!(friendly("ISO_Left_Tab"), "Tab");
        assert_eq!(friendly("Print"), "Print Screen");
    }

    #[test]
    fn friendly_function_keys() {
        assert_eq!(friendly("F5"), "F5");
        assert_eq!(friendly("F12"), "F12");
    }

    #[test]
    fn modifier_detection() {
        assert!(is_modifier("Control_R"));
        assert!(is_modifier("Super_L"));
        assert!(!is_modifier("Return"));
        assert!(!is_modifier("a"));
    }

    #[test]
    fn button_labels() {
        assert_eq!(btn_label(0x110), Some("LEFT MOUSE BUTTON"));
        assert_eq!(btn_label(0x111), Some("RIGHT MOUSE BUTTON"));
        assert_eq!(btn_label(0x112), Some("MID MOUSE BUTTON"));
        assert_eq!(btn_label(0x0a), None);
    }

    #[test]
    fn wheel_labels() {
        assert_eq!(wheel_label(1), "Scroll Up");
        assert_eq!(wheel_label(-1), "Scroll Down");
        assert_eq!(wheel_h_label(1), "Scroll Right");
        assert_eq!(wheel_h_label(-1), "Scroll Left");
    }

    #[test]
    fn media_and_brightness_labels() {
        assert_eq!(friendly("XF86AudioPlay"), "Play");
        assert_eq!(friendly("XF86AudioPause"), "Pause");
        assert_eq!(friendly("XF86AudioStop"), "Stop");
        assert_eq!(friendly("XF86AudioNext"), "Next");
        assert_eq!(friendly("XF86AudioPrev"), "Previous");
        assert_eq!(friendly("XF86AudioRaiseVolume"), "Volume Up");
        assert_eq!(friendly("XF86AudioLowerVolume"), "Volume Down");
        assert_eq!(friendly("XF86AudioMute"), "Mute");
        assert_eq!(friendly("XF86MonBrightnessUp"), "Brightness Up");
        assert_eq!(friendly("XF86MonBrightnessDown"), "Brightness Down");
    }

    #[test]
    fn keypad_and_arithmetic_labels_are_plain_ascii() {
        assert_eq!(friendly("KP_Multiply"), "*");
        assert_eq!(friendly("KP_Divide"), "/");
        assert_eq!(friendly("KP_Add"), "+");
        assert_eq!(friendly("KP_Subtract"), "-");
        assert_eq!(friendly("Multiply"), "*");
        assert_eq!(friendly("Subtract"), "-");
    }
}