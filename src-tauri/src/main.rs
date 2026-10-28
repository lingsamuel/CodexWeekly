#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    codex_weekly_overlay::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Codex Weekly Overlay 桌面程序需要 Windows；核心测试可在 WSL 运行。");
}
