use windows_sys::Win32::System::{
    DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
    Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND},
};

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub unsafe fn copy_to_clipboard(text: &str) {
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
