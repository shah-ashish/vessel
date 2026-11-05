#![allow(static_mut_refs)]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
        GetStockObject, InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW,
        DEFAULT_GUI_FONT, FW_BOLD, HBRUSH, HDC, PAINTSTRUCT, TRANSPARENT, WHITE_BRUSH,
    },
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        LibraryLoader::GetModuleHandleW,
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND},
    },
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, ShowWindow,
        TranslateMessage, BS_PUSHBUTTON, CW_USEDEFAULT, ES_AUTOHSCROLL, ES_READONLY,
        HMENU, IDC_ARROW, MSG, SW_SHOW, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_PAINT,
        WM_SETTEXT, WM_TIMER, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
        WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    },
};

const ID_BTN_COPY: usize = 101;
const ID_BTN_TOGGLE: usize = 102;
const ID_TIMER_METER: usize = 1;

static mut GUI_STATE: Option<GuiState> = None;

struct GuiState {
    peak_levels: Arc<Mutex<(f32, f32)>>,
    connected_clients: Arc<std::sync::atomic::AtomicUsize>,
    is_streaming: Arc<AtomicBool>,
    server_url: String,
    hwnd_btn_toggle: HWND,
    hwnd_btn_copy: HWND,
    hwnd_clients_lbl: HWND,
    smooth_l: f32,
    smooth_r: f32,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn run_native_gui(
    peak_levels: Arc<Mutex<(f32, f32)>>,
    connected_clients: Arc<std::sync::atomic::AtomicUsize>,
    is_streaming: Arc<AtomicBool>,
    server_url: String,
) {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("VesselNativeAppWindow");

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0,
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: GetStockObject(WHITE_BRUSH as i32) as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wc);

        GUI_STATE = Some(GuiState {
            peak_levels,
            connected_clients,
            is_streaming,
            server_url: server_url.clone(),
            hwnd_btn_toggle: 0,
            hwnd_btn_copy: 0,
            hwnd_clients_lbl: 0,
            smooth_l: 0.0,
            smooth_r: 0.0,
        });

        let title = to_wide("Vessel - Laptop Audio Stream");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            480,
            490,
            0,
            0,
            instance,
            std::ptr::null(),
        );

        if hwnd == 0 {
            return;
        }

        ShowWindow(hwnd, SW_SHOW);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let instance = GetModuleHandleW(std::ptr::null());
            let font = GetStockObject(DEFAULT_GUI_FONT as i32);

            let btn_class = to_wide("BUTTON");
            let static_class = to_wide("STATIC");
            let edit_class = to_wide("EDIT");

            // Title Header
            let lbl_title = to_wide("VESSEL AUDIO SERVER");
            let h_title = CreateWindowExW(
                0,
                static_class.as_ptr(),
                lbl_title.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                24,
                20,
                420,
                26,
                hwnd,
                0,
                instance,
                std::ptr::null(),
            );

            // Create Bold Font for Title
            let bold_font = CreateFontW(
                20, 0, 0, 0, FW_BOLD as i32, 0, 0, 0, 0, 0, 0, 0, 0,
                to_wide("Segoe UI").as_ptr(),
            );
            SendMessageW(h_title, 0x0030 /* WM_SETFONT */, bold_font as usize, 1);

            // Subtitle
            let lbl_sub = to_wide("Capturing Windows WASAPI loopback audio at 48,000 Hz Stereo");
            let h_sub = CreateWindowExW(
                0,
                static_class.as_ptr(),
                lbl_sub.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                24,
                48,
                420,
                20,
                hwnd,
                0,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_sub, 0x0030, font as usize, 1);

            // URL Label
            let lbl_url = to_wide("Wi-Fi / Hotspot Mobile URL:");
            let h_url_lbl = CreateWindowExW(
                0,
                static_class.as_ptr(),
                lbl_url.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                24,
                190,
                420,
                18,
                hwnd,
                0,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_url_lbl, 0x0030, font as usize, 1);

            // URL Edit Box (Read-only, selectable)
            let server_url = GUI_STATE.as_ref().map(|s| s.server_url.as_str()).unwrap_or("http://127.0.0.1:8080");
            let wide_url = to_wide(server_url);
            let h_edit = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                wide_url.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_BORDER | (ES_READONLY as u32) | (ES_AUTOHSCROLL as u32),
                24,
                212,
                300,
                28,
                hwnd,
                0,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_edit, 0x0030, font as usize, 1);

            // Copy Button
            let btn_copy_text = to_wide("Copy Link");
            let h_copy = CreateWindowExW(
                0,
                btn_class.as_ptr(),
                btn_copy_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_PUSHBUTTON as u32),
                334,
                211,
                100,
                30,
                hwnd,
                ID_BTN_COPY as HMENU,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_copy, 0x0030, font as usize, 1);

            // Connected Clients Label
            let clients_text = to_wide("Active Mobile Receivers: 0 connected");
            let h_clients = CreateWindowExW(
                0,
                static_class.as_ptr(),
                clients_text.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                24,
                260,
                420,
                20,
                hwnd,
                0,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_clients, 0x0030, font as usize, 1);

            // Stream Pause/Resume Toggle Button
            let toggle_text = to_wide("Stop Streaming");
            let h_toggle = CreateWindowExW(
                0,
                btn_class.as_ptr(),
                toggle_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_PUSHBUTTON as u32),
                24,
                310,
                190,
                38,
                hwnd,
                ID_BTN_TOGGLE as HMENU,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_toggle, 0x0030, font as usize, 1);

            // Exit Button
            let exit_text = to_wide("Exit Vessel");
            let h_exit = CreateWindowExW(
                0,
                btn_class.as_ptr(),
                exit_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_PUSHBUTTON as u32),
                244,
                310,
                190,
                38,
                hwnd,
                103 as HMENU,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_exit, 0x0030, font as usize, 1);

            // Status Notice Footer
            let footer_text = to_wide("UDP auto-discovery active on port 8082 · Phone auto-connects");
            let h_footer = CreateWindowExW(
                0,
                static_class.as_ptr(),
                footer_text.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                24,
                390,
                420,
                20,
                hwnd,
                0,
                instance,
                std::ptr::null(),
            );
            SendMessageW(h_footer, 0x0030, font as usize, 1);

            if let Some(state) = GUI_STATE.as_mut() {
                state.hwnd_btn_toggle = h_toggle;
                state.hwnd_btn_copy = h_copy;
                state.hwnd_clients_lbl = h_clients;
            }

            // Set 30ms timer for smooth VU meter animation
            SetTimer(hwnd, ID_TIMER_METER, 30, None);
            0
        }

        WM_COMMAND => {
            let cmd_id = (wparam & 0xFFFF) as usize;
            if cmd_id == ID_BTN_COPY {
                // Copy URL to Windows Clipboard
                if let Some(state) = GUI_STATE.as_ref() {
                    copy_to_clipboard(&state.server_url);
                    let copied_str = to_wide("✓ Copied!");
                    SendMessageW(state.hwnd_btn_copy, WM_SETTEXT, 0, copied_str.as_ptr() as LPARAM);
                }
            } else if cmd_id == ID_BTN_TOGGLE {
                // Toggle active streaming
                if let Some(state) = GUI_STATE.as_mut() {
                    let was_streaming = state.is_streaming.load(Ordering::SeqCst);
                    let new_state = !was_streaming;
                    state.is_streaming.store(new_state, Ordering::SeqCst);

                    let label = if new_state {
                        to_wide("Stop Streaming")
                    } else {
                        to_wide("▶ Resume Streaming")
                    };
                    SendMessageW(state.hwnd_btn_toggle, WM_SETTEXT, 0, label.as_ptr() as LPARAM);
                }
            } else if cmd_id == 103 {
                // Exit app
                DestroyWindow(hwnd);
            }
            0
        }

        WM_TIMER => {
            if wparam == ID_TIMER_METER {
                if let Some(state) = GUI_STATE.as_mut() {
                    let (raw_l, raw_r) = *state.peak_levels.lock().unwrap();

                    // Smooth ballistics
                    state.smooth_l = if raw_l > state.smooth_l { raw_l } else { (state.smooth_l * 0.85).max(0.0) };
                    state.smooth_r = if raw_r > state.smooth_r { raw_r } else { (state.smooth_r * 0.85).max(0.0) };

                    let clients = state.connected_clients.load(Ordering::Relaxed);
                    let lbl = to_wide(&format!("Active Mobile Receivers: {} connected", clients));
                    SendMessageW(state.hwnd_clients_lbl, 0x000C /* WM_SETTEXT */, 0, lbl.as_ptr() as LPARAM);

                    // Repaint the VU meter area only
                    let meter_rect = RECT { left: 24, top: 80, right: 440, bottom: 170 };
                    InvalidateRect(hwnd, &meter_rect, 0);
                }
            }
            0
        }

        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            draw_native_vu_meter(hdc);

            EndPaint(hwnd, &ps);
            0
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn draw_native_vu_meter(hdc: HDC) {
    let (smooth_l, smooth_r) = if let Some(state) = GUI_STATE.as_ref() {
        (state.smooth_l, state.smooth_r)
    } else {
        (0.0, 0.0)
    };

    let font = GetStockObject(DEFAULT_GUI_FONT as i32);
    SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, 0x00333333);

    // Left channel label
    let text_l = to_wide("L");
    TextOutW(hdc, 24, 88, text_l.as_ptr(), 1);

    // Right channel label
    let text_r = to_wide("R");
    TextOutW(hdc, 24, 128, text_r.as_ptr(), 1);

    let meter_x = 44;
    let meter_w = 320;
    let meter_h = 18;

    // Draw Left Channel Track
    draw_channel_bar(hdc, meter_x, 88, meter_w, meter_h, smooth_l);

    // Draw Right Channel Track
    draw_channel_bar(hdc, meter_x, 128, meter_w, meter_h, smooth_r);

    // Draw dB values
    let db_l = if smooth_l <= 0.0001 { "-∞ dB".to_string() } else { format!("{:.1} dB", 20.0 * smooth_l.log10().max(-48.0)) };
    let db_r = if smooth_r <= 0.0001 { "-∞ dB".to_string() } else { format!("{:.1} dB", 20.0 * smooth_r.log10().max(-48.0)) };

    let wide_dbl = to_wide(&db_l);
    let wide_dbr = to_wide(&db_r);

    TextOutW(hdc, meter_x + meter_w + 12, 88, wide_dbl.as_ptr(), (wide_dbl.len() - 1) as i32);
    TextOutW(hdc, meter_x + meter_w + 12, 128, wide_dbr.as_ptr(), (wide_dbr.len() - 1) as i32);
}

unsafe fn draw_channel_bar(hdc: HDC, x: i32, y: i32, width: i32, height: i32, amp: f32) {
    // Background track (dark slate)
    let bg_rect = RECT { left: x, top: y, right: x + width, bottom: y + height };
    let bg_brush = CreateSolidBrush(0x002B2014); // Dark slate
    FillRect(hdc, &bg_rect, bg_brush);
    DeleteObject(bg_brush as _);

    // Calculate filled width: map -48dB..0dB -> 0..width
    let db = if amp <= 0.0001 { -48.0 } else { 20.0 * amp.log10().clamp(-48.0, 0.0) };
    let pct = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
    let fill_w = (width as f32 * pct) as i32;

    if fill_w > 0 {
        let fill_rect = RECT { left: x + 1, top: y + 1, right: x + fill_w - 1, bottom: y + height - 1 };
        // Emerald green / amber active color
        let color = if pct > 0.85 { 0x003333E6 /* Red/Amber */ } else { 0x0078B410 /* Green */ };
        let fill_brush = CreateSolidBrush(color);
        FillRect(hdc, &fill_rect, fill_brush);
        DeleteObject(fill_brush as _);
    }
}

unsafe fn copy_to_clipboard(text: &str) {
    let wide = to_wide(text);
    let bytes = wide.len() * 2;

    let hmem = GlobalAlloc(GHND, bytes);
    if hmem.is_null() {
        return;
    }

    let ptr = GlobalLock(hmem) as *mut u16;
    if !ptr.is_null() {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        GlobalUnlock(hmem);

        if OpenClipboard(0) != 0 {
            EmptyClipboard();
            SetClipboardData(13 /* CF_UNICODETEXT */, hmem as isize);
            CloseClipboard();
        }
    }
}
