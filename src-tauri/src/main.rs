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
    umbilical_app::run();
}
