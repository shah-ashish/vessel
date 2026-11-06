#![allow(static_mut_refs)]

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
        CreatePen, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
        FillRect, GetStockObject, InvalidateRect, RoundRect, SelectObject, SetBkMode,
        SetTextColor, TextOutW, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER, FW_BOLD,
        FW_NORMAL, HDC, HFONT, NULL_BRUSH, PS_SOLID, SRCCOPY, TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetMessageW, LoadCursorW, PostQuitMessage, RegisterClassW, SetCursor, SetTimer,
        ShowWindow, TranslateMessage, CW_USEDEFAULT, IDC_ARROW, IDC_HAND, MSG, SW_SHOW,
        WM_DESTROY, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_TIMER,
        WNDCLASSW, WS_CAPTION, WS_CLIPCHILDREN, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
        WS_VISIBLE,
    },
};

use super::components::{copy_to_clipboard, to_wide};
use crate::ReceiverInfo;

const ID_TIMER_FRAME: usize = 1;

// Color Palette - Midnight Control
const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

const COLOR_BG: u32 = rgb(11, 18, 32);         // #0B1220 (Main Canvas)
const COLOR_SURFACE: u32 = rgb(18, 28, 46);    // #121C2E (Cards)
const COLOR_BORDER: u32 = rgb(28, 41, 64);     // #1C2940 (Card Borders)
const COLOR_PILL_BG: u32 = rgb(13, 22, 38);    // #0D1626 (Input/Pill BG)
const COLOR_ACCENT: u32 = rgb(56, 189, 248);   // #38BDF8 (Sky Blue)
const COLOR_GREEN: u32 = rgb(34, 197, 94);     // #22C55E (Online/Active)
const COLOR_AMBER: u32 = rgb(245, 158, 11);    // #F59E0B (Transitional/Warning)
const COLOR_RED: u32 = rgb(248, 113, 113);     // #F87171 (Exit/Destructive)
const COLOR_TEXT_PRI: u32 = rgb(226, 232, 240);// #E2E8F0 (Main Text)
const COLOR_TEXT_SEC: u32 = rgb(148, 163, 184);// #94A3B8 (Labels)
const COLOR_TEXT_MUT: u32 = rgb(100, 116, 139);// #64748B (Footer/Muted)

static mut GUI_STATE: Option<GuiState> = None;

struct GuiState {
    peak_levels: Arc<Mutex<(f32, f32)>>,
    _connected_clients: Arc<std::sync::atomic::AtomicUsize>,
    is_streaming: Arc<AtomicBool>,
    receiver_active: Arc<AtomicBool>,
    receiver_info: Arc<Mutex<Option<ReceiverInfo>>>,
    server_url: String,
    smooth_l: f32,
    smooth_r: f32,
    hover_btn: u8, // 0=none, 1=copy, 2=toggle, 3=exit
    copied_until: Option<Instant>,
}

pub fn run_native_gui(
    peak_levels: Arc<Mutex<(f32, f32)>>,
    connected_clients: Arc<std::sync::atomic::AtomicUsize>,
    is_streaming: Arc<AtomicBool>,
    receiver_active: Arc<AtomicBool>,
    receiver_info: Arc<Mutex<Option<ReceiverInfo>>>,
    server_url: String,
) {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("VesselMidnightControlWindow");

        let bg_brush = CreateSolidBrush(COLOR_BG);

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0,
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: bg_brush,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wc);

        GUI_STATE = Some(GuiState {
            peak_levels,
            _connected_clients: connected_clients,
            is_streaming,
            receiver_active,
            receiver_info,
            server_url,
            smooth_l: 0.0,
            smooth_r: 0.0,
            hover_btn: 0,
            copied_until: None,
        });

        let title = to_wide("Vessel — Midnight Control Dashboard");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            516,
            595,
            0,
            0,
            instance,
            std::ptr::null(),
        );

        if hwnd == 0 {
            return;
        }

        ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, ID_TIMER_FRAME, 25, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// Button Hit-Test Regions
fn hit_test(x: i32, y: i32) -> u8 {
    // Copy Button: x [340..460], y [94..130]
    if x >= 340 && x <= 460 && y >= 94 && y <= 130 {
        return 1;
    }
    // Toggle Button: x [20..235], y [458..504]
    if x >= 20 && x <= 235 && y >= 458 && y <= 504 {
        return 2;
    }
    // Exit Button: x [255..470], y [458..504]
    if x >= 255 && x <= 470 && y >= 458 && y <= 504 {
        return 3;
    }
    0
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let hit = hit_test(x, y);

            if let Some(state) = GUI_STATE.as_mut() {
                if state.hover_btn != hit {
                    state.hover_btn = hit;
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            0
        }

        WM_SETCURSOR => {
            if let Some(state) = GUI_STATE.as_ref() {
                if state.hover_btn != 0 {
                    SetCursor(LoadCursorW(0, IDC_HAND));
                    return 1;
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let hit = hit_test(x, y);

            if let Some(state) = GUI_STATE.as_mut() {
                if hit == 1 {
                    copy_to_clipboard(&state.server_url);
                    state.copied_until = Some(Instant::now());
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                } else if hit == 2 {
                    let current = state.is_streaming.load(Ordering::SeqCst);
                    state.is_streaming.store(!current, Ordering::SeqCst);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                } else if hit == 3 {
                    DestroyWindow(hwnd);
                }
            }
            0
        }

        WM_TIMER => {
            if wparam == ID_TIMER_FRAME {
                if let Some(state) = GUI_STATE.as_mut() {
                    let (raw_l, raw_r) = *state.peak_levels.lock().unwrap();
                    state.smooth_l = if raw_l > state.smooth_l {
                        raw_l
                    } else {
                        (state.smooth_l * 0.82).max(0.0)
                    };
                    state.smooth_r = if raw_r > state.smooth_r {
                        raw_r
                    } else {
                        (state.smooth_r * 0.82).max(0.0)
                    };

                    // Auto-clear copied toast after 2 seconds
                    if let Some(t) = state.copied_until {
                        if t.elapsed().as_secs() >= 2 {
                            state.copied_until = None;
                        }
                    }

                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            0
        }

        WM_PAINT => {
            let mut ps: windows_sys::Win32::Graphics::Gdi::PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut client_rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client_rect);
            let width = client_rect.right;
            let height = client_rect.bottom;

            // Double Buffering to guarantee 0% flicker
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            // 1. Paint Window Canvas Background
            let bg_brush = CreateSolidBrush(COLOR_BG);
            FillRect(mem_dc, &client_rect, bg_brush);
            DeleteObject(bg_brush as _);

            if let Some(state) = GUI_STATE.as_ref() {
                render_dashboard(mem_dc, state, width, height);
            }

            // Blit buffer to screen
            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

            // Clean up buffer objects
            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp as _);
            DeleteDC(mem_dc);

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

// ---------------------------------------------------------------------------
// Modern UI Dashboard Renderer
// ---------------------------------------------------------------------------
unsafe fn render_dashboard(hdc: HDC, state: &GuiState, _width: i32, _height: i32) {
    SetBkMode(hdc, TRANSPARENT as i32);

    let font_title = create_font("Segoe UI", 18, FW_BOLD);
    let font_card_header = create_font("Segoe UI", 9, FW_BOLD);
    let font_body_bold = create_font("Segoe UI", 12, FW_BOLD);
    let font_body = create_font("Segoe UI", 11, FW_NORMAL);
    let font_mono = create_font("Consolas", 11, FW_NORMAL);
    let font_small = create_font("Segoe UI", 9, FW_NORMAL);

    // =======================================================================
    // 1. TOP HEADER SECTION
    // =======================================================================
    SelectObject(hdc, font_title as _);
    SetTextColor(hdc, COLOR_TEXT_PRI);
    let title_text = to_wide("VESSEL");
    TextOutW(hdc, 24, 18, title_text.as_ptr(), (title_text.len() - 1) as i32);

    SelectObject(hdc, font_body as _);
    SetTextColor(hdc, COLOR_TEXT_SEC);
    let sub_text = to_wide("Midnight Control · Laptop Audio Server");
    TextOutW(hdc, 100, 22, sub_text.as_ptr(), (sub_text.len() - 1) as i32);

    // Online Status Pill (Right aligned)
    draw_card_box(hdc, 386, 16, 88, 26, rgb(16, 26, 42), COLOR_GREEN, 12);
    SelectObject(hdc, font_small as _);
    SetTextColor(hdc, COLOR_GREEN);
    let status_pill = to_wide("● RUNNING");
    let mut pill_rect = RECT { left: 386, top: 16, right: 474, bottom: 42 };
    DrawTextW(hdc, status_pill.as_ptr(), -1, &mut pill_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

    // =======================================================================
    // 2. CARD 1: SERVER NETWORK ENDPOINT
    // =======================================================================
    draw_card_box(hdc, 20, 54, 454, 88, COLOR_SURFACE, COLOR_BORDER, 8);

    SelectObject(hdc, font_card_header as _);
    SetTextColor(hdc, COLOR_TEXT_MUT);
    let srv_label = to_wide("SERVER NETWORK ENDPOINT");
    TextOutW(hdc, 36, 66, srv_label.as_ptr(), (srv_label.len() - 1) as i32);

    // URL Display Pill
    draw_card_box(hdc, 34, 92, 296, 36, COLOR_PILL_BG, COLOR_BORDER, 6);
    SelectObject(hdc, font_mono as _);
    SetTextColor(hdc, COLOR_ACCENT);
    let url_wide = to_wide(&state.server_url);
    let mut url_rect = RECT { left: 44, top: 92, right: 326, bottom: 128 };
    DrawTextW(hdc, url_wide.as_ptr(), -1, &mut url_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

    // Copy Button
    let is_copied = state.copied_until.is_some();
    let (copy_bg, copy_border, copy_fg, copy_txt) = if is_copied {
        (rgb(16, 40, 30), COLOR_GREEN, COLOR_GREEN, "✓ Copied!")
    } else if state.hover_btn == 1 {
        (rgb(28, 48, 76), COLOR_ACCENT, COLOR_ACCENT, "Copy Link")
    } else {
        (COLOR_PILL_BG, COLOR_BORDER, COLOR_TEXT_PRI, "Copy Link")
    };
    draw_card_box(hdc, 340, 92, 120, 36, copy_bg, copy_border, 6);
    SelectObject(hdc, font_body_bold as _);
    SetTextColor(hdc, copy_fg);
    let copy_text_wide = to_wide(copy_txt);
    let mut copy_rect = RECT { left: 340, top: 92, right: 460, bottom: 128 };
    DrawTextW(hdc, copy_text_wide.as_ptr(), -1, &mut copy_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

    // =======================================================================
    // 3. CARD 2: CONNECTED RECEIVER (Dedicated Device Card)
    // =======================================================================
    draw_card_box(hdc, 20, 152, 454, 98, COLOR_SURFACE, COLOR_BORDER, 8);

    SelectObject(hdc, font_card_header as _);
    SetTextColor(hdc, COLOR_TEXT_MUT);
    let rx_title = to_wide("MOBILE RECEIVER (SINGLE-DEVICE LOCK)");
    TextOutW(hdc, 36, 164, rx_title.as_ptr(), (rx_title.len() - 1) as i32);

    let is_active = state.receiver_active.load(Ordering::Relaxed);
    if is_active {
        let info_lock = state.receiver_info.lock();
        let (name, ip) = if let Ok(ref guard) = info_lock {
            if let Some(ref info) = **guard {
                (info.name.clone(), info.ip.clone())
            } else {
                ("Android receiver".to_string(), "127.0.0.1".to_string())
            }
        } else {
            ("Android receiver".to_string(), "127.0.0.1".to_string())
        };

        // Green Indicator Badge
        draw_card_box(hdc, 36, 188, 72, 22, rgb(16, 40, 30), COLOR_GREEN, 10);
        SelectObject(hdc, font_small as _);
        SetTextColor(hdc, COLOR_GREEN);
        let active_badge = to_wide("● ACTIVE");
        let mut ab_rect = RECT { left: 36, top: 188, right: 108, bottom: 210 };
        DrawTextW(hdc, active_badge.as_ptr(), -1, &mut ab_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        // Device Name
        SelectObject(hdc, font_body_bold as _);
        SetTextColor(hdc, COLOR_TEXT_PRI);
        let name_wide = to_wide(&name);
        TextOutW(hdc, 118, 189, name_wide.as_ptr(), (name_wide.len() - 1) as i32);

        // Device IP & Lock info
        SelectObject(hdc, font_body as _);
        SetTextColor(hdc, COLOR_TEXT_SEC);
        let detail_text = format!("IP: {} · Control WebSocket connected · Second device rejected", ip);
        let detail_wide = to_wide(&detail_text);
        TextOutW(hdc, 36, 218, detail_wide.as_ptr(), (detail_wide.len() - 1) as i32);
    } else {
        // Honest Empty State
        draw_card_box(hdc, 36, 188, 64, 22, rgb(20, 28, 40), COLOR_TEXT_MUT, 10);
        SelectObject(hdc, font_small as _);
        SetTextColor(hdc, COLOR_TEXT_MUT);
        let idle_badge = to_wide("○ IDLE");
        let mut ib_rect = RECT { left: 36, top: 188, right: 100, bottom: 210 };
        DrawTextW(hdc, idle_badge.as_ptr(), -1, &mut ib_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        SelectObject(hdc, font_body_bold as _);
        SetTextColor(hdc, COLOR_TEXT_SEC);
        let no_rx = to_wide("No receiver connected");
        TextOutW(hdc, 112, 189, no_rx.as_ptr(), (no_rx.len() - 1) as i32);

        SelectObject(hdc, font_body as _);
        SetTextColor(hdc, COLOR_TEXT_MUT);
        let hint = to_wide("Open Vessel on your Android phone — it will auto-discover this PC");
        TextOutW(hdc, 36, 218, hint.as_ptr(), (hint.len() - 1) as i32);
    }

    // =======================================================================
    // 4. CARD 3: AUDIO STREAM & LIVE VU METERS
    // =======================================================================
    draw_card_box(hdc, 20, 260, 454, 178, COLOR_SURFACE, COLOR_BORDER, 8);

    SelectObject(hdc, font_card_header as _);
    SetTextColor(hdc, COLOR_TEXT_MUT);
    let audio_title = to_wide("AUDIO PIPELINE (WASAPI LOOPBACK)");
    TextOutW(hdc, 36, 272, audio_title.as_ptr(), (audio_title.len() - 1) as i32);

    let is_streaming = state.is_streaming.load(Ordering::Relaxed);
    let (srv_badge_bg, srv_badge_border, srv_badge_color, srv_badge_txt) = if is_streaming {
        (rgb(16, 40, 30), COLOR_GREEN, COLOR_GREEN, "● SPEAKER STREAMING")
    } else {
        (rgb(40, 30, 16), COLOR_AMBER, COLOR_AMBER, "❚❚ SPEAKER PAUSED")
    };
    draw_card_box(hdc, 310, 268, 150, 22, srv_badge_bg, srv_badge_border, 10);
    SelectObject(hdc, font_small as _);
    SetTextColor(hdc, srv_badge_color);
    let srv_pill = to_wide(srv_badge_txt);
    let mut sp_rect = RECT { left: 310, top: 268, right: 460, bottom: 290 };
    DrawTextW(hdc, srv_pill.as_ptr(), -1, &mut sp_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

    // VU Meter - Left Channel
    SelectObject(hdc, font_body_bold as _);
    SetTextColor(hdc, COLOR_TEXT_PRI);
    let l_lbl = to_wide("L");
    TextOutW(hdc, 36, 307, l_lbl.as_ptr(), 1);
    draw_meter_bar(hdc, 56, 308, 318, 16, state.smooth_l);

    let dbl_str = format_db(state.smooth_l);
    let dbl_wide = to_wide(&dbl_str);
    SelectObject(hdc, font_mono as _);
    SetTextColor(hdc, COLOR_TEXT_SEC);
    TextOutW(hdc, 386, 308, dbl_wide.as_ptr(), (dbl_wide.len() - 1) as i32);

    // VU Meter - Right Channel
    SelectObject(hdc, font_body_bold as _);
    SetTextColor(hdc, COLOR_TEXT_PRI);
    let r_lbl = to_wide("R");
    TextOutW(hdc, 36, 341, r_lbl.as_ptr(), 1);
    draw_meter_bar(hdc, 56, 342, 318, 16, state.smooth_r);

    let dbr_str = format_db(state.smooth_r);
    let dbr_wide = to_wide(&dbr_str);
    SelectObject(hdc, font_mono as _);
    SetTextColor(hdc, COLOR_TEXT_SEC);
    TextOutW(hdc, 386, 342, dbr_wide.as_ptr(), (dbr_wide.len() - 1) as i32);

    // Audio Technical Parameters
    SelectObject(hdc, font_small as _);
    SetTextColor(hdc, COLOR_TEXT_MUT);
    let tech_info = to_wide("Capture: 48,000 Hz · 2 Channels Stereo · Float32 WASAPI -> PCM16 LAUD");
    TextOutW(hdc, 36, 382, tech_info.as_ptr(), (tech_info.len() - 1) as i32);

    let flow_note = to_wide("Independent Stream: Toggling speaker does not disconnect receiver");
    TextOutW(hdc, 36, 404, flow_note.as_ptr(), (flow_note.len() - 1) as i32);

    // =======================================================================
    // 5. ACTION BUTTONS (MODERN DARK PILLS)
    // =======================================================================
    // Button 1: Toggle Audio Streaming
    let (t_bg, t_border, t_fg, t_txt) = if is_streaming {
        if state.hover_btn == 2 {
            (rgb(40, 24, 28), COLOR_RED, COLOR_RED, "Stop Audio Stream")
        } else {
            (COLOR_SURFACE, COLOR_RED, COLOR_RED, "Stop Audio Stream")
        }
    } else {
        if state.hover_btn == 2 {
            (rgb(16, 40, 30), COLOR_GREEN, COLOR_GREEN, "▶ Resume Audio Stream")
        } else {
            (COLOR_SURFACE, COLOR_GREEN, COLOR_GREEN, "▶ Resume Audio Stream")
        }
    };
    draw_card_box(hdc, 20, 452, 220, 46, t_bg, t_border, 8);
    SelectObject(hdc, font_body_bold as _);
    SetTextColor(hdc, t_fg);
    let t_wide = to_wide(t_txt);
    let mut tb_rect = RECT { left: 20, top: 452, right: 240, bottom: 498 };
    DrawTextW(hdc, t_wide.as_ptr(), -1, &mut tb_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

    // Button 2: Exit Vessel Server
    let (e_bg, e_border, e_fg) = if state.hover_btn == 3 {
        (rgb(36, 20, 28), COLOR_RED, COLOR_RED)
    } else {
        (COLOR_SURFACE, COLOR_BORDER, COLOR_TEXT_SEC)
    };
    draw_card_box(hdc, 254, 452, 220, 46, e_bg, e_border, 8);
    SelectObject(hdc, font_body_bold as _);
    SetTextColor(hdc, e_fg);
    let e_wide = to_wide("Exit Server");
    let mut eb_rect = RECT { left: 254, top: 452, right: 474, bottom: 498 };
    DrawTextW(hdc, e_wide.as_ptr(), -1, &mut eb_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

    // =======================================================================
    // 6. FOOTER
    // =======================================================================
    SelectObject(hdc, font_small as _);
    SetTextColor(hdc, COLOR_TEXT_MUT);
    let footer_txt = to_wide("Vessel v1.0.0 · UDP Auto-Discovery Active (Port 8082) · Midnight Control");
    let mut foot_rect = RECT { left: 20, top: 512, right: 474, bottom: 536 };
    DrawTextW(hdc, footer_txt.as_ptr(), -1, &mut foot_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

    // Clean up created fonts
    DeleteObject(font_title as _);
    DeleteObject(font_card_header as _);
    DeleteObject(font_body_bold as _);
    DeleteObject(font_body as _);
    DeleteObject(font_mono as _);
    DeleteObject(font_small as _);
}

// ---------------------------------------------------------------------------
// Drawing Primitives
// ---------------------------------------------------------------------------
unsafe fn draw_card_box(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fill_color: u32,
    border_color: u32,
    radius: i32,
) {
    let brush = CreateSolidBrush(fill_color);
    let pen = CreatePen(PS_SOLID, 1, border_color);

    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, pen);

    RoundRect(hdc, x, y, x + w, y + h, radius, radius);

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(brush as _);
    DeleteObject(pen as _);
}

unsafe fn draw_meter_bar(hdc: HDC, x: i32, y: i32, width: i32, height: i32, amp: f32) {
    // Dark meter background track
    let bg_rect = RECT { left: x, top: y, right: x + width, bottom: y + height };
    let bg_brush = CreateSolidBrush(COLOR_PILL_BG);
    FillRect(hdc, &bg_rect, bg_brush);
    DeleteObject(bg_brush as _);

    // Meter track border
    let pen = CreatePen(PS_SOLID, 1, COLOR_BORDER);
    let old_pen = SelectObject(hdc, pen);
    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH as i32));
    windows_sys::Win32::Graphics::Gdi::Rectangle(hdc, x, y, x + width, y + height);
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(pen as _);

    // Fill level: -48dB to 0dB
    let db = if amp <= 0.0001 { -48.0 } else { 20.0 * amp.log10().clamp(-48.0, 0.0) };
    let pct = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
    let fill_w = (width as f32 * pct) as i32;

    if fill_w > 2 {
        let fill_color = if pct > 0.88 {
            COLOR_RED
        } else if pct > 0.70 {
            COLOR_AMBER
        } else {
            COLOR_GREEN
        };
        let fill_rect = RECT {
            left: x + 1,
            top: y + 1,
            right: x + fill_w - 1,
            bottom: y + height - 1,
        };
        let fill_brush = CreateSolidBrush(fill_color);
        FillRect(hdc, &fill_rect, fill_brush);
        DeleteObject(fill_brush as _);
    }
}

fn format_db(amp: f32) -> String {
    if amp <= 0.0001 {
        "-∞ dB".to_string()
    } else {
        let db = 20.0 * amp.log10();
        format!("{:>5.1} dB", db.max(-48.0))
    }
}

unsafe fn create_font(name: &str, size_pt: i32, weight: u32) -> HFONT {
    CreateFontW(
        -size_pt * 96 / 72,
        0,
        0,
        0,
        weight as i32,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        to_wide(name).as_ptr(),
    )
}
