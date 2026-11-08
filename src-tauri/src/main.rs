#![windows_subsystem = "windows"]

fn main() {
    #[cfg(windows)]
    if !webview2_available() {
        show_webview2_prompt();
        return;
    }
    harness_lib::run()
}

#[cfg(windows)]
fn webview2_available() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    const CLIENT: &str =
        r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    const CLIENT_WOW: &str =
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    [
        (HKEY_CURRENT_USER, CLIENT),
        (HKEY_LOCAL_MACHINE, CLIENT),
        (HKEY_LOCAL_MACHINE, CLIENT_WOW),
    ]
    .into_iter()
    .any(|(root, path)| {
        RegKey::predef(root)
            .open_subkey(path)
            .and_then(|key| key.get_value::<String, _>("pv"))
            .map(|version| !version.trim().is_empty() && version != "0.0.0.0")
            .unwrap_or(false)
    })
}

#[cfg(windows)]
fn show_webview2_prompt() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO, SW_SHOWNORMAL,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    let text = wide("Harness 需要 Microsoft Edge WebView2 运行时。\n\nWin10 21H2 及以后版本通常已预装。是否打开官方下载页面？");
    let title = wide("Harness 电脑维修助手");
    unsafe {
        if MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONWARNING,
        ) == IDYES
        {
            let operation = wide("open");
            let url = wide("https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/");
            let _ = ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(url.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}
