use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use crate::{taskbar, overlay, config};

const WM_TRAY: u32 = WM_APP + 100;
const IDM_SHOW: u32 = 1001;
const IDM_CLICKTHROUGH: u32 = 1002;
const IDM_TASKBAR: u32 = 1003;
const IDM_QUIT: u32 = 1004;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn load_icon() -> HICON {
    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap();
        // Load embedded icon resource (ID 1)
        let icon = LoadImageW(
            HINSTANCE(hinstance.0),
            PCWSTR(1 as *const u16), // MAKEINTRESOURCE(1)
            IMAGE_ICON, 0, 0,
            LR_DEFAULTSIZE,
        );
        if let Ok(h) = icon {
            return HICON(h.0);
        }
        LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
    }
}

pub fn create_tray() {
    unsafe {
        let class_name = wide("GoldTrayWnd");
        let hinstance = GetModuleHandleW(None).unwrap();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tray_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            w!("GoldTray"),
            WINDOW_STYLE(0),
            0, 0, 0, 0,
            HWND_MESSAGE,
            None,
            HINSTANCE(hinstance.0),
            None,
        ).unwrap();

        let icon = load_icon();

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        let tip = "Gold Monitor";
        let tip_wide: Vec<u16> = tip.encode_utf16().collect();
        nid.szTip[..tip_wide.len()].copy_from_slice(&tip_wide);

        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

unsafe extern "system" fn tray_wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = (lp.0 & 0xFFFF) as u32;
            match event {
                WM_RBUTTONUP => show_tray_menu(hwnd),
                WM_LBUTTONUP => {
                    overlay::toggle();
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wp.0 & 0xFFFF) as u32;
            match id {
                IDM_SHOW => overlay::toggle(),
                IDM_CLICKTHROUGH => {
                    overlay::toggle_clickthrough();
                }
                IDM_TASKBAR => {
                    taskbar::toggle();
                    config::set("taskbar", serde_json::json!(taskbar::is_visible()));
                }
                IDM_QUIT => {
                    // Remove tray icon before quitting
                    let nid = NOTIFYICONDATAW {
                        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                        hWnd: hwnd,
                        uID: 1,
                        ..Default::default()
                    };
                    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                    PostQuitMessage(0);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu = CreatePopupMenu().unwrap();

    let show_text = wide("显示窗口");
    let click_text = wide("鼠标穿透");
    let taskbar_text = wide("任务栏显示");
    let quit_text = wide("退出");

    // Checkmark items
    let show_flags = MF_STRING | if overlay::is_visible() { MF_CHECKED } else { MF_UNCHECKED };
    let click_flags = MF_STRING | if overlay::is_clickthrough() { MF_CHECKED } else { MF_UNCHECKED };
    let taskbar_flags = MF_STRING | if taskbar::is_visible() { MF_CHECKED } else { MF_UNCHECKED };

    let _ = AppendMenuW(menu, show_flags, IDM_SHOW as usize, PCWSTR(show_text.as_ptr()));
    let _ = AppendMenuW(menu, click_flags, IDM_CLICKTHROUGH as usize, PCWSTR(click_text.as_ptr()));
    let _ = AppendMenuW(menu, taskbar_flags, IDM_TASKBAR as usize, PCWSTR(taskbar_text.as_ptr()));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    let _ = AppendMenuW(menu, MF_STRING, IDM_QUIT as usize, PCWSTR(quit_text.as_ptr()));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, None);
    let _ = DestroyMenu(menu);
}
