fn main() {
    // `UMBILICAL_RELEASE` turns the updater on (see `update/guard.rs`).
    println!("cargo:rerun-if-env-changed=UMBILICAL_RELEASE");
    tauri_build::build()
}
