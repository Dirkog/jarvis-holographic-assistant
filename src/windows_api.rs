//! Windows API обёртки — ПОЛНАЯ Windows-реализация
//! Управление окнами, курсором, громкостью, питанием

use std::ffi::CString;
use tracing::{info, error, warn};

#[cfg(windows)]
use winapi::um::winuser::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, 
    VK_LWIN, VK_SHIFT, VK_MENU, VK_F4, VK_TAB, VK_SNAPSHOT,
    VK_VOLUME_UP, VK_VOLUME_DOWN, VK_VOLUME_MUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_ABSOLUTE, SetCursorPos, mouse_event,
    keybd_event, GetForegroundWindow, GetWindowTextW, GetWindowRect,
    SetWindowPos, SWP_SHOWWINDOW, HWND_TOPMOST, HWND_NOTOPMOST,
    SendMessageW, WM_CLOSE, WM_SYSCOMMAND, SC_MONITORPOWER,
    GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};
#[cfg(windows)]
use winapi::um::shellapi::ShellExecuteW;
#[cfg(windows)]
use winapi::um::powrprof::SetSuspendState;
#[cfg(windows)]
use winapi::um::processthreadsapi::GetCurrentProcess;
#[cfg(windows)]
use winapi::um::handleapi::CloseHandle;
#[cfg(windows)]
use winapi::um::securitybaseapi::GetTokenInformation;
#[cfg(windows)]
use winapi::um::winbase::LookupPrivilegeValueW;
#[cfg(windows)]
use winapi::shared::minwindef::{DWORD, LPARAM, WPARAM, UINT, HINSTANCE, LPVOID, FALSE, TRUE};
#[cfg(windows)]
use winapi::shared::windef::{HWND, POINT, RECT};
#[cfg(windows)]
use winapi::shared::ntdef::NULL;

pub fn open_application(name: &str) {
    #[cfg(windows)]
    unsafe {
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let wide_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();

        ShellExecuteW(
            NULL as HWND,
            operation.as_ptr(),
            wide_name.as_ptr(),
            NULL,
            NULL,
            1,
        );
    }
    info!("[WINAPI] Открытие приложения: {}", name);
}

pub fn close_active_window() {
    #[cfg(windows)]
    unsafe {
        press_key_combo(&[
            (VK_MENU as u16, false),
            (VK_F4 as u16, false),
            (VK_F4 as u16, true),
            (VK_MENU as u16, true),
        ]);
    }
    info!("[WINAPI] Закрытие активного окна");
}

pub fn show_desktop() {
    #[cfg(windows)]
    unsafe {
        press_key_combo(&[
            (VK_LWIN as u16, false),
            (b'D' as u16, false),
            (b'D' as u16, true),
            (VK_LWIN as u16, true),
        ]);
    }
    info!("[WINAPI] Показать рабочий стол");
}

pub fn switch_desktop_left() {
    #[cfg(windows)]
    unsafe {
        press_key_combo(&[
            (0x11, false),
            (VK_LWIN as u16, false),
            (VK_LEFT as u16, false),
            (VK_LEFT as u16, true),
            (VK_LWIN as u16, true),
            (0x11, true),
        ]);
    }
    info!("[WINAPI] Рабочий стол влево");
}

pub fn switch_desktop_right() {
    #[cfg(windows)]
    unsafe {
        press_key_combo(&[
            (0x11, false),
            (VK_LWIN as u16, false),
            (VK_RIGHT as u16, false),
            (VK_RIGHT as u16, true),
            (VK_LWIN as u16, true),
            (0x11, true),
        ]);
    }
    info!("[WINAPI] Рабочий стол вправо");
}

pub fn move_cursor(x: f32, y: f32) {
    #[cfg(windows)]
    unsafe {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let screen_x = (x * screen_w as f32) as i32;
        let screen_y = (y * screen_h as f32) as i32;
        SetCursorPos(screen_x, screen_y);
    }
}

pub fn mouse_click() {
    #[cfg(windows)]
    unsafe {
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        std::thread::sleep(std::time::Duration::from_millis(50));
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
    info!("[WINAPI] Клик мыши");
}

pub fn mouse_down() {
    #[cfg(windows)]
    unsafe {
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
    }
    info!("[WINAPI] Mouse down");
}

pub fn mouse_up() {
    #[cfg(windows)]
    unsafe {
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
    info!("[WINAPI] Mouse up");
}

pub fn adjust_volume(delta: f32) {
    #[cfg(windows)]
    unsafe {
        let steps = (delta.abs() * 10.0) as i32;
        let vk = if delta > 0.0 { VK_VOLUME_UP } else { VK_VOLUME_DOWN };

        for _ in 0..steps.max(1) {
            keybd_event(vk as u8, 0, 0, 0);
            keybd_event(vk as u8, 0, 2, 0);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    info!("[WINAPI] Громкость изменена на {}", delta);
}

pub fn toggle_mute() {
    #[cfg(windows)]
    unsafe {
        keybd_event(VK_VOLUME_MUTE as u8, 0, 0, 0);
        keybd_event(VK_VOLUME_MUTE as u8, 0, 2, 0);
    }
    info!("[WINAPI] Mute toggled");
}

pub fn take_screenshot() {
    #[cfg(windows)]
    unsafe {
        press_key_combo(&[
            (VK_LWIN as u16, false),
            (VK_SHIFT as u16, false),
            (b'S' as u16, false),
            (b'S' as u16, true),
            (VK_SHIFT as u16, true),
            (VK_LWIN as u16, true),
        ]);
    }
    info!("[WINAPI] Скриншот");
}

pub fn shutdown_system() {
    #[cfg(windows)]
    unsafe {
        use winapi::um::winuser::ExitWindowsEx;
        use winapi::um::winnt::{TOKEN_ADJUST_PRIVILEGES, TOKEN_QUERY, SE_PRIVILEGE_ENABLED};
        use winapi::um::processthreadsapi::OpenProcessToken;

        let mut h_token = NULL;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut h_token) != 0 {
            let mut tkp = std::mem::zeroed::<winapi::um::winnt::TOKEN_PRIVILEGES>();
            let privilege = CString::new("SeShutdownPrivilege").unwrap();
            let mut luid = std::mem::zeroed::<winapi::um::winnt::LUID>();

            if LookupPrivilegeValueW(NULL, privilege.as_ptr() as *const u16, &mut luid) != 0 {
                tkp.PrivilegeCount = 1;
                tkp.Privileges[0].Luid = luid;
                tkp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;

                if GetTokenInformation(h_token, winapi::um::winnt::TokenPrivileges, 
                    &mut tkp as *mut _ as *mut _, std::mem::size_of::<winapi::um::winnt::TOKEN_PRIVILEGES>() as u32, 
                    &mut 0) != 0 {
                    ExitWindowsEx(0x00000001 | 0x00000004, 0);
                }
            }
            CloseHandle(h_token);
        }
        warn!("[WINAPI] Shutdown: требуются права администратора");
    }
    #[cfg(not(windows))]
    {
        warn!("[WINAPI] Shutdown доступен только на Windows");
    }
}

pub fn restart_system() {
    #[cfg(windows)]
    unsafe {
        use winapi::um::winuser::ExitWindowsEx;
        use winapi::um::winnt::{TOKEN_ADJUST_PRIVILEGES, TOKEN_QUERY, SE_PRIVILEGE_ENABLED};
        use winapi::um::processthreadsapi::OpenProcessToken;

        let mut h_token = NULL;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut h_token) != 0 {
            let mut tkp = std::mem::zeroed::<winapi::um::winnt::TOKEN_PRIVILEGES>();
            let privilege = CString::new("SeShutdownPrivilege").unwrap();
            let mut luid = std::mem::zeroed::<winapi::um::winnt::LUID>();

            if LookupPrivilegeValueW(NULL, privilege.as_ptr() as *const u16, &mut luid) != 0 {
                tkp.PrivilegeCount = 1;
                tkp.Privileges[0].Luid = luid;
                tkp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;

                if GetTokenInformation(h_token, winapi::um::winnt::TokenPrivileges, 
                    &mut tkp as *mut _ as *mut _, std::mem::size_of::<winapi::um::winnt::TOKEN_PRIVILEGES>() as u32, 
                    &mut 0) != 0 {
                    ExitWindowsEx(0x00000002 | 0x00000004, 0);
                }
            }
            CloseHandle(h_token);
        }
        warn!("[WINAPI] Restart: требуются права администратора");
    }
    #[cfg(not(windows))]
    {
        warn!("[WINAPI] Restart доступен только на Windows");
    }
}

pub fn sleep_system() {
    #[cfg(windows)]
    unsafe {
        SetSuspendState(FALSE, FALSE, FALSE);
    }
    info!("[WINAPI] Режим сна");
}

pub fn lock_workstation() {
    #[cfg(windows)]
    unsafe {
        press_key_combo(&[
            (VK_LWIN as u16, false),
            (b'L' as u16, false),
            (b'L' as u16, true),
            (VK_LWIN as u16, true),
        ]);
    }
    info!("[WINAPI] Блокировка рабочей станции");
}

#[cfg(windows)]
unsafe fn press_key_combo(keys: &[(u16, bool)]) {
    for &(vk, up) in keys {
        let mut input = INPUT {
            type_: INPUT_KEYBOARD,
            u: std::mem::zeroed(),
        };
        let ki = input.u.ki_mut();
        ki.wVk = vk;
        ki.dwFlags = if up { 2 } else { 0 };
        ki.time = 0;
        ki.dwExtraInfo = 0;
        SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;
