mod devices;
mod layout;
mod names;
mod state;

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use devices::DeviceRegistry;
use layout::XkbMap;
use state::{AppState, DeviceEvent};

struct Options {
    state_path: PathBuf,
    enable_path: PathBuf,
    layout: Option<String>,
    variant: Option<String>,
    xkb_options: Option<String>,
    debug: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    run_daemon(parse_options(&args));
}

fn parse_options(args: &[String]) -> Options {
    let state_path = arg_value(args, "--state")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let enable_path = arg_value(args, "--enable")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_enable_path(&state_path));
    Options {
        state_path,
        enable_path,
        layout: arg_value(args, "--layout"),
        variant: arg_value(args, "--variant"),
        xkb_options: arg_value(args, "--options"),
        debug: args.contains(&"--debug".to_string()),
    }
}

fn run_daemon(options: Options) {
    let _lock = acquire_lock(&options.state_path);
    let mut xkb = XkbMap::current(
        options.layout.as_deref(),
        options.variant.as_deref(),
        options.xkb_options.as_deref(),
    );
    let (sender, receiver) = mpsc::channel::<DeviceEvent>();
    let registry = DeviceRegistry::new(sender.clone(), options.debug);
    devices::install_signal_handlers();
    let mut app_state = AppState::new();
    run(
        &options.state_path,
        &options.enable_path,
        &mut xkb,
        &registry,
        &receiver,
        &mut app_state,
        devices::shutdown_requested,
    );
}

// Process events until `should_stop` turns true. Lives on the single daemon
// thread: the evdev readers only send DeviceEvent values through the channel.
fn run<F: Fn() -> bool>(
    state_path: &std::path::Path,
    enable_path: &std::path::Path,
    xkb: &mut XkbMap,
    registry: &DeviceRegistry,
    receiver: &mpsc::Receiver<DeviceEvent>,
    app_state: &mut AppState,
    should_stop: F,
) {
    let mut rescan_deadline = std::time::Instant::now() + Duration::from_secs(0);
    let mut last_enabled = read_enabled(enable_path);

    loop {
        if should_stop() {
            write_state(state_path, &app_state.snapshot(xkb, registry));
            return;
        }

        if rescan_deadline <= std::time::Instant::now() {
            registry.rescan();
            rescan_deadline = std::time::Instant::now() + Duration::from_secs(2);
        }

        let enabled = read_enabled(enable_path);
        if enabled != last_enabled {
            if !enabled {
                app_state.reset();
                write_state(state_path, &app_state.snapshot(xkb, registry));
            }
            last_enabled = enabled;
        }

        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                if enabled && app_state.handle(xkb, event) {
                    write_state(state_path, &app_state.snapshot(xkb, registry));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn default_enable_path(state_path: &std::path::Path) -> std::path::PathBuf {
    state_path
        .parent()
        .map(|parent| parent.join("enabled"))
        .unwrap_or_else(|| std::path::PathBuf::from("enabled"))
}

fn read_enabled(path: &std::path::Path) -> bool {
    match std::fs::read_to_string(path) {
        Ok(content) => !matches!(content.trim().to_ascii_lowercase().as_str(), "false" | "0"),
        Err(_) => true,
    }
}

fn default_state_path() -> std::path::PathBuf {
    match std::env::var("HOME") {
        Ok(home) => state_path_from_home(&home),
        Err(_) => {
            eprintln!("omakeys-daemon: no HOME and no --state given");
            std::process::exit(2);
        }
    }
}

fn state_path_from_home(home: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(home)
        .join(".local/state/omarchy/current/plugins/skvggor.omakeys/keys.json")
}

// Single-instance guard. Restarting the shell orphans the previous daemon
// (quickshell does not always reap its children), and two daemons reading the
// same input devices double-report every key. The last instance to start takes
// the lock, SIGTERMs a stale predecessor whose pid it holds in the lock file,
// then retries until the lock is free.
fn acquire_lock(state_path: &std::path::Path) -> std::fs::File {
    let lock_path = state_path.with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let file = match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(error) => panic!(
                "omakeys-daemon: cannot open lock {}: {error}",
                lock_path.display()
            ),
        };

        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            let _ = file.set_len(0);
            let _ = write_all_ignoring_errors(&file, format!("{}\n", std::process::id()).as_bytes());
            return file;
        }

        if let Ok(pid) = held_pid(&file) {
            if is_same_daemon(pid) {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                std::thread::sleep(Duration::from_millis(200));
            } else {
                eprintln!(
                    "omakeys-daemon: another instance (pid {pid}) holds {}",
                    lock_path.display()
                );
                std::process::exit(0);
            }
        } else {
            let _ = std::fs::remove_file(&lock_path);
        }

        if std::time::Instant::now() >= deadline {
            eprintln!(
                "omakeys-daemon: could not acquire {} in time",
                lock_path.display()
            );
            std::process::exit(1);
        }
    }
}

fn held_pid(file: &std::fs::File) -> Result<i32, ()> {
    use std::io::Read;
    let mut contents = String::new();
    file.try_clone()
        .map_err(|_| ())?
        .read_to_string(&mut contents)
        .map_err(|_| ())?;
    let pid = contents.trim().parse::<i32>().map_err(|_| ())?;
    if pid <= 1 {
        return Err(());
    }
    Ok(pid)
}

fn is_same_daemon(pid: i32) -> bool {
    let Ok(cmdline) = std::fs::read_to_string(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    cmdline.contains("omakeys-daemon")
}

fn write_all_ignoring_errors(mut file: &std::fs::File, data: &[u8]) -> std::io::Result<()> {
    file.write_all(data)
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(item) = iter.next() {
        if item == name {
            return iter.next().cloned();
        }
    }
    None
}

fn write_state(path: &std::path::Path, json: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut file) => {
            let _ = file.write_all(json.as_bytes());
            let _ = file.flush();
        }
        Err(error) => {
            eprintln!(
                "omakeys-daemon: cannot write {}: {error}",
                path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn unique_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omakeys-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_enable(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("enabled");
        std::fs::write(&path, content.as_bytes()).unwrap();
        path
    }

    fn held_file(content: &str) -> std::fs::File {
        let dir = unique_dir("held");
        let path = dir.join("keys.lock");
        std::fs::write(&path, content.as_bytes()).unwrap();
        std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap()
    }

    #[test]
    fn read_enabled_defaults_to_true_when_file_is_missing() {
        let dir = unique_dir("missing");
        assert!(read_enabled(&dir.join("enabled")));
    }

    #[test]
    fn read_enabled_parses_true_and_false() {
        assert!(read_enabled(&write_enable(&unique_dir("parse"), "true")));
        assert!(read_enabled(&write_enable(&unique_dir("parse1"), "1")));
        assert!(!read_enabled(&write_enable(&unique_dir("parse0"), "false")));
        assert!(!read_enabled(&write_enable(&unique_dir("parse00"), "0")));
    }

    #[test]
    fn read_enabled_treats_garbage_and_empty_as_true() {
        assert!(read_enabled(&write_enable(&unique_dir("garbage"), "")));
        assert!(read_enabled(&write_enable(&unique_dir("garbage1"), "maybe")));
    }

    #[test]
    fn default_enable_path_sits_next_to_state_file() {
        let state = std::path::PathBuf::from("/some/plugin/keys.json");
        assert_eq!(default_enable_path(&state), std::path::PathBuf::from("/some/plugin/enabled"));
    }

    #[test]
    fn arg_value_reads_flag_values() {
        let args: Vec<String> = vec!["--state".to_string(), "s.json".to_string(), "--debug".to_string()];
        assert_eq!(arg_value(&args, "--state"), Some("s.json".to_string()));
        assert_eq!(arg_value(&args, "--debug"), None);
        assert_eq!(arg_value(&["--debug".to_string()], "--state"), None);
    }

    #[test]
    fn parse_options_applies_defaults_and_overrides() {
        let options = parse_options(&["--state".to_string(), "keys.json".to_string()]);
        assert_eq!(options.state_path, std::path::PathBuf::from("keys.json"));
        assert_eq!(options.enable_path, std::path::PathBuf::from("enabled"));
        assert!(!options.debug);
        assert_eq!(options.layout, None);

        let full = parse_options(&[
            "--state".to_string(),
            "k.json".to_string(),
            "--layout".to_string(),
            "br".to_string(),
            "--debug".to_string(),
        ]);
        assert_eq!(full.layout.as_deref(), Some("br"));
        assert!(full.debug);
    }

    #[test]
    fn state_path_from_home_builds_plugin_state_path() {
        assert_eq!(
            state_path_from_home("/home/user"),
            std::path::PathBuf::from(
                "/home/user/.local/state/omarchy/current/plugins/skvggor.omakeys/keys.json"
            )
        );
    }

    #[test]
    fn write_state_keeps_keystrokes_owner_only() {
        let dir = unique_dir("state-mode");
        let path = dir.join("keys.json");
        write_state(&path, r#"{"keys":["a"]}"#);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "state file must not be group/world readable");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"keys":["a"]}"#);
    }

    #[test]
    fn write_state_survives_unwritable_paths() {
        let dir = unique_dir("state-error");
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "file").unwrap();
        write_state(&blocker.join("keys.json"), "{}");
    }

    #[test]
    fn write_all_ignoring_errors_persists_data() {
        let dir = unique_dir("write-all");
        let path = dir.join("data");
        let file = std::fs::File::create(&path).unwrap();
        write_all_ignoring_errors(&file, b"abc").unwrap();
        drop(file);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "abc");
    }

    #[test]
    fn held_pid_parses_a_valid_pid() {
        assert_eq!(held_pid(&held_file("4242\n")), Ok(4242));
    }

    #[test]
    fn held_pid_rejects_garbage_and_useless_pids() {
        assert!(held_pid(&held_file("not-a-pid")).is_err());
        assert!(held_pid(&held_file("0\n")).is_err());
        assert!(held_pid(&held_file("1\n")).is_err());
    }

    #[test]
    fn acquire_lock_creates_lock_with_own_pid() {
        let dir = unique_dir("acquire-new");
        let state = dir.join("keys.json");
        let file = acquire_lock(&state);
        let content = std::fs::read_to_string(state.with_extension("lock")).unwrap();
        assert_eq!(content.trim(), std::process::id().to_string());
        drop(file);
    }

    #[test]
    fn acquire_lock_recovers_a_stale_lock() {
        let dir = unique_dir("acquire-stale");
        let state = dir.join("keys.json");
        std::fs::write(state.with_extension("lock"), "not a pid\n").unwrap();
        let _file = acquire_lock(&state);
        let content = std::fs::read_to_string(state.with_extension("lock")).unwrap();
        assert_eq!(content.trim(), std::process::id().to_string());
    }

    #[test]
    fn is_same_daemon_rejects_unknown_pids() {
        assert!(!is_same_daemon(999_999));
    }

    fn test_xkb() -> XkbMap {
        XkbMap::current(Some("us"), None, None)
    }

    #[test]
    fn run_translates_keys_and_writes_state() {
        let dir = unique_dir("run-engine");
        let state = dir.join("keys.json");
        let enable = write_enable(&dir, "true");
        let mut xkb = test_xkb();
        let (tx, rx) = mpsc::channel::<DeviceEvent>();
        let (registry_tx, _registry_rx) = mpsc::channel::<DeviceEvent>();
        let registry = DeviceRegistry::new(registry_tx, false);
        let mut app_state = AppState::new();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_sender = stop.clone();
        let sender = std::thread::spawn(move || {
            tx.send(DeviceEvent::Key { code: 0x1e, pressed: true }).unwrap();
            std::thread::sleep(Duration::from_millis(250));
            stop_sender.store(true, Ordering::Relaxed);
        });
        run(
            &state,
            &enable,
            &mut xkb,
            &registry,
            &rx,
            &mut app_state,
            || stop.load(Ordering::Relaxed),
        );
        sender.join().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
        assert_eq!(parsed["keys"], serde_json::json!(["a"]));
    }

    #[test]
    fn run_resets_held_keys_when_disabled() {
        let dir = unique_dir("run-disable");
        let state = dir.join("keys.json");
        let enable = write_enable(&dir, "true");
        let mut xkb = test_xkb();
        let (tx, rx) = mpsc::channel::<DeviceEvent>();
        let (registry_tx, _registry_rx) = mpsc::channel::<DeviceEvent>();
        let registry = DeviceRegistry::new(registry_tx, false);
        let mut app_state = AppState::new();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_sender = stop.clone();
        let enable_sender = enable.clone();
        let sender = std::thread::spawn(move || {
            tx.send(DeviceEvent::Key { code: 0x1e, pressed: true }).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            std::fs::write(&enable_sender, "false").unwrap();
            std::thread::sleep(Duration::from_millis(300));
            stop_sender.store(true, Ordering::Relaxed);
        });
        run(
            &state,
            &enable,
            &mut xkb,
            &registry,
            &rx,
            &mut app_state,
            || stop.load(Ordering::Relaxed),
        );
        sender.join().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
        assert_eq!(parsed["keys"], serde_json::json!([]));
    }
}