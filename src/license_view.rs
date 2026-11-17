use std::{cell::Cell, mem, ptr};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{DEFAULT_GUI_FONT, GetStockObject},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
    },
    core::w,
};

const CLASS: *const u16 = w!("WillowVillageThirdPartyNotices");
const TITLE: *const u16 = w!("柳溪村 · 第三方许可与鸣谢");
const TEXT: &str = include_str!("../THIRD_PARTY_NOTICES.txt");
const TEXT_CONTROL: i32 = 1;

thread_local! {
    static WINDOW: Cell<HWND> = const { Cell::new(ptr::null_mut()) };
}

pub(super) fn show(owner: HWND) {
    unsafe {
        let mut window = WINDOW.get();
        if window.is_null() {
            let instance = GetModuleHandleW(ptr::null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                hIcon: LoadIconW(instance, 1 as *const u16),
                hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
                lpszClassName: CLASS,
                ..mem::zeroed()
            };
            RegisterClassW(&class);
            window = CreateWindowExW(
                0,
                CLASS,
                TITLE,
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                760,
                610,
                owner,
                ptr::null_mut(),
                instance,
                ptr::null(),
            );
            if window.is_null() {
                MessageBoxW(
                    owner,
                    w!("无法打开许可窗口。发布页也提供完整许可文件。"),
                    TITLE,
                    MB_OK | MB_ICONWARNING,
                );
                return;
            }
            WINDOW.set(window);
        }
        ShowWindow(window, SW_RESTORE);
        SetForegroundWindow(window);
        SetFocus(GetDlgItem(window, TEXT_CONTROL));
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_CREATE => {
                let text = TEXT
                    .replace("\r\n", "\n")
                    .replace('\n', "\r\n")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<_>>();
                let edit = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    w!("EDIT"),
                    ptr::null(),
                    WS_CHILD
                        | WS_VISIBLE
                        | WS_VSCROLL
                        | ES_MULTILINE as u32
                        | ES_AUTOVSCROLL as u32
                        | ES_READONLY as u32
                        | ES_NOHIDESEL as u32,
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    TEXT_CONTROL as HMENU,
                    GetModuleHandleW(ptr::null()),
                    ptr::null(),
                );
                if edit.is_null() {
                    return -1;
                }
                if SendMessageW(edit, WM_SETTEXT, 0, text.as_ptr() as LPARAM) == 0 {
                    return -1;
                }
                SendMessageW(
                    edit,
                    WM_SETFONT,
                    GetStockObject(DEFAULT_GUI_FONT) as WPARAM,
                    1,
                );
                0
            }
            WM_SIZE => {
                let mut area: RECT = mem::zeroed();
                GetClientRect(hwnd, &mut area);
                MoveWindow(
                    GetDlgItem(hwnd, TEXT_CONTROL),
                    0,
                    0,
                    area.right,
                    area.bottom,
                    1,
                );
                0
            }
            WM_NCDESTROY => {
                if WINDOW.get() == hwnd {
                    WINDOW.set(ptr::null_mut());
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
