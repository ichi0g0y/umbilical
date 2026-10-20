// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--daemon`: run only the session supervisor, no GUI.
    if std::env::args().any(|a| a == "--daemon") {
        let config = umbilical_core::Config::default_path();
        let paths = umbilical_core::daemon::Paths::default();
        if let Err(e) = umbilical_core::daemon::serve(config, &paths) {
            eprintln!("umbilical daemon: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    // `--stop-daemon`: stop every session and the daemon (the Windows uninstaller).
    if umbilical_app::is_stop_daemon_run() {
        let stopped = umbilical_app::stop_daemon();
        std::process::exit(if stopped { 0 } else { 1 });
    }
    // `--permission-states`: print the macOS permission states for the GUI.
    if umbilical_app::is_permission_states_run() {
        umbilical_app::print_permission_states();
        return;
    }
    umbilical_app::run();
}
