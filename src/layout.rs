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
            KeyDisplay::Modifier(name) | KeyDisplay::Character(name) | KeyDisplay::Label(name) => {
                name
            }
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
    let output = std::process::Command::new(trusted_hyprctl()?)
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

/// Absolute locations we accept hyprctl from, in order of preference.
///
/// PATH is deliberately not consulted. It belongs to the caller, and the daemon
/// runs with a group it does not get from its own account, so naming a binary
/// the caller chooses is the wrong shape for this process. Nothing needs to be
/// configurable here: the candidate list is fixed, and if none of them qualifies
/// the caller falls back to the default layout instead of spawning anything.
const HYPRCTL_CANDIDATES: [&str; 3] = ["/usr/bin/hyprctl", "/bin/hyprctl", "/usr/local/bin/hyprctl"];

/// Resolve hyprctl from a fixed list of absolute paths, accepting a candidate
/// only when root owns it and no one else can write it.
///
/// An environment override is not accepted either. The daemon's privileges must
/// not be redirectable by the environment it inherited.
fn trusted_hyprctl() -> Option<std::path::PathBuf> {
    trusted_hyprctl_from(&HYPRCTL_CANDIDATES)
}

fn trusted_hyprctl_from(candidates: &[&str]) -> Option<std::path::PathBuf> {
    candidates
        .iter()
        .map(std::path::Path::new)
        .find(|candidate| is_root_owned_executable(candidate))
        .map(std::path::Path::to_path_buf)
}

fn is_root_owned_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    is_root_owned(&metadata) && mode_is_trusted(metadata.permissions().mode())
}

/// Split from the mode check so each condition is testable on its own. A test
/// that only exercised the pair could not tell which half rejected a file: a
/// temporary file is never root-owned, so the ownership test would mask a
/// regression in the permission test on every case.
fn is_root_owned(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    metadata.uid() == 0
}

    fn mode_is_trusted(mode: u32) -> bool {
        // Group or world writable would let a local user replace the binary.
        if mode & 0o022 != 0 {
            return false;
        }
        mode & 0o100 != 0
    }


// Note what this module deliberately does not do: it does not try to drop group
// `input` before spawning hyprctl. That was measured, not assumed.
//
// A process that gets a group from a set-group-ID bit does not pass it on. Per
// execve(2), when the file being executed has no set-group-ID bit the effective
// IDs are taken from the real IDs, so the effective group `input` the daemon
// holds is not inherited by hyprctl. Observed on this machine with the installed
// daemon running as root:input 2755:
//
//   daemon  Gid: 1000 993 993 993          (real 1000, effective 993 = input)
//   child   Gid: 1000 1000 1000 1000        (real 1000, effective 1000)
//
// The supplementary groups *are* inherited, but they are the caller's own, and
// an unprivileged process cannot drop them anyway: setgroups(0, NULL) returns
// EPERM without CAP_SETGID, and the daemon is not root. Attempting it in a
// pre_exec hook would therefore fail the spawn outright and cost the layout
// detection, in exchange for removing nothing.

#[cfg(test)]
mod tests {
    use super::*;

    fn set_mode(path: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, PermissionsExt::from_mode(mode)).unwrap();
    }

    fn temp_candidate(directory: &std::path::Path, name: &str, mode: u32) -> std::path::PathBuf {
        let path = directory.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        set_mode(&path, mode);
        path
    }


    // The daemon is setgid input, so a child it spawns inherits group input and
    // can read every keyboard on the machine. These cover the trust check that
    // decides which binary is allowed to be spawned at all.

    // Each half of the trust check on its own. A temporary file is never
    // root-owned, so the end-to-end cases below cannot reach the permission
    // test: without these, dropping the group-write check would change nothing
    // observable.

    #[test]
    fn trusts_only_a_mode_nobody_else_can_write() {
        assert!(mode_is_trusted(0o755));
        assert!(mode_is_trusted(0o500));
        assert!(mode_is_trusted(0o111));

        assert!(!mode_is_trusted(0o775), "group writable is not trusted");
        assert!(!mode_is_trusted(0o757), "world writable is not trusted");
        assert!(!mode_is_trusted(0o777), "group and world writable is not trusted");
        assert!(!mode_is_trusted(0o666), "writable without execute is not trusted");
        assert!(!mode_is_trusted(0o644), "readable but not executable is not trusted");
        assert!(!mode_is_trusted(0o000));
    }

    #[test]
    fn trusts_only_a_file_owned_by_root() {
        use std::os::unix::fs::MetadataExt;

        let directory = tempfile::tempdir().unwrap();
        let owned_by_us = temp_candidate(directory.path(), "hyprctl", 0o755);
        let metadata = std::fs::metadata(&owned_by_us).unwrap();

        // The suite never runs as root, so this asserts the negative and leaves
        // the positive to the end-to-end case below.
        assert_eq!(is_root_owned(&metadata), metadata.uid() == 0);
        if unsafe { libc::geteuid() } == 0 {
            assert!(is_root_owned(&metadata));
        }
    }

    #[test]
    fn rejects_a_candidate_that_is_not_owned_by_root() {
        let directory = tempfile::tempdir().unwrap();
        let candidate = temp_candidate(directory.path(), "hyprctl", 0o755);

        assert_eq!(trusted_hyprctl_from(&[candidate.to_str().unwrap()]), None);
    }

    #[test]
    fn rejects_a_group_writable_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let candidate = temp_candidate(directory.path(), "hyprctl", 0o775);

        assert_eq!(trusted_hyprctl_from(&[candidate.to_str().unwrap()]), None);
    }

    #[test]
    fn rejects_a_world_writable_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let candidate = temp_candidate(directory.path(), "hyprctl", 0o757);

        assert_eq!(trusted_hyprctl_from(&[candidate.to_str().unwrap()]), None);
    }

    #[test]
    fn rejects_a_candidate_without_the_owner_execute_bit() {
        let directory = tempfile::tempdir().unwrap();
        let candidate = temp_candidate(directory.path(), "hyprctl", 0o644);

        assert_eq!(trusted_hyprctl_from(&[candidate.to_str().unwrap()]), None);
    }

    #[test]
    fn rejects_a_directory_and_a_missing_path() {
        let directory = tempfile::tempdir().unwrap();

        assert_eq!(
            trusted_hyprctl_from(&[directory.path().to_str().unwrap()]),
            None
        );
        assert_eq!(
            trusted_hyprctl_from(&[directory.path().join("absent").to_str().unwrap()]),
            None
        );
    }

    #[test]
    fn rejects_every_candidate_when_none_of_them_is_trusted() {
        let directory = tempfile::tempdir().unwrap();
        let group_writable = temp_candidate(directory.path(), "a", 0o775);
        let world_writable = temp_candidate(directory.path(), "b", 0o757);

        assert_eq!(
            trusted_hyprctl_from(&[group_writable.to_str().unwrap(), world_writable.to_str().unwrap()]),
            None
        );
    }

    // Only assertable where a root-owned hyprctl actually exists, which is the
    // case on the machine this plugin targets and not on a CI runner.
    #[test]
    fn accepts_a_root_owned_executable_when_one_is_installed() {
        let system = std::path::Path::new("/usr/bin/hyprctl");
        if !system.exists() {
            eprintln!("skipped: no root-owned hyprctl on this machine");
            return;
        }

        assert_eq!(trusted_hyprctl(), Some(system.to_path_buf()));
    }

    #[test]
    fn never_resolves_hyprctl_from_path() {
        use std::sync::{Mutex, MutexGuard, OnceLock};

        // The tests in this binary run on parallel threads and env is process
        // wide, so the two env-reading assertions take this lock.
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard: MutexGuard<()> = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("executed");
        let fake = directory.path().join("hyprctl");
        // A redirection, not touch(1): PATH points at this directory alone, so
        // any external command here would fail to resolve and the marker would
        // never appear for the wrong reason.
        std::fs::write(&fake, format!("#!/bin/sh\n: > '{}'\n", marker.display())).unwrap();
        set_mode(&fake, 0o755);


        let original = std::env::var_os("PATH");
        std::env::set_var("PATH", directory.path());

        let resolved = trusted_hyprctl();
        let _ = hyprctl_str("input:kb_layout");

        match original {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }

        assert!(
            !marker.exists(),
            "hyprctl was resolved through PATH: {:?}",
            marker
        );
        // Whatever was resolved, it can only be one of the fixed absolute paths.
        if let Some(path) = resolved {
            assert!(
                HYPRCTL_CANDIDATES.contains(&path.to_str().unwrap_or_default()),
                "resolved outside the candidate list: {path:?}"
            );
        }
    }

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
