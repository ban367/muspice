// リリースビルドのWindowsで、余分なコンソールウィンドウを開かない（消さないこと）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    muspice_lib::run()
}
