#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "macos")]
    if topsap_next_lib::helper::is_helper_mode() {
        topsap_next_lib::helper::run();
        return;
    }

    topsap_next_lib::run();
}
