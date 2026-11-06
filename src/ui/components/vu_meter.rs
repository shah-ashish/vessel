use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::Gdi::{
        CreateSolidBrush, DeleteObject, FillRect, GetStockObject, SelectObject, SetBkMode,
        SetTextColor, TextOutW, DEFAULT_GUI_FONT, HDC, TRANSPARENT,
    },
};

use super::clipboard::to_wide;

pub unsafe fn draw_native_vu_meter(hdc: HDC, smooth_l: f32, smooth_r: f32) {
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

pub unsafe fn draw_channel_bar(hdc: HDC, x: i32, y: i32, width: i32, height: i32, amp: f32) {
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
