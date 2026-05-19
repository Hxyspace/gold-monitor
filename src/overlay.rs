use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::{config, price};

static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);
static CLICKTHROUGH: AtomicBool = AtomicBool::new(false);
static OVERLAY_HWND: Mutex<Option<isize>> = Mutex::new(None);
static OVERLAY_DPI: AtomicU32 = AtomicU32::new(96);

// Base dimensions at 96 DPI
const BASE_W: i32 = 72;
const BASE_H: i32 = 46;
const BASE_FONT: f32 = 13.0;
const RADIUS: f32 = 6.0;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_visible() -> bool {
    OVERLAY_VISIBLE.load(Ordering::Relaxed)
}

pub fn is_clickthrough() -> bool {
    CLICKTHROUGH.load(Ordering::Relaxed)
}

pub fn set_clickthrough(val: bool) {
    CLICKTHROUGH.store(val, Ordering::Relaxed);
    config::set("clickthrough", serde_json::json!(val));
    unsafe {
        if let Some(h) = *OVERLAY_HWND.lock().unwrap() {
            let hwnd = HWND(h as *mut _);
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            if val {
                SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TRANSPARENT.0 as i32);
            } else {
                SetWindowLongW(hwnd, GWL_EXSTYLE, style & !(WS_EX_TRANSPARENT.0 as i32));
            }
        }
    }
}

pub fn toggle_clickthrough() {
    set_clickthrough(!is_clickthrough());
}

pub fn show() {
    if OVERLAY_HWND.lock().unwrap().is_none() {
        std::thread::spawn(|| unsafe { run_overlay() });
    } else {
        unsafe {
            if let Some(h) = *OVERLAY_HWND.lock().unwrap() {
                let _ = ShowWindow(HWND(h as *mut _), SW_SHOWNOACTIVATE);
            }
        }
    }
    OVERLAY_VISIBLE.store(true, Ordering::Relaxed);
    config::set("visible", serde_json::json!(true));
}

pub fn hide() {
    OVERLAY_VISIBLE.store(false, Ordering::Relaxed);
    config::set("visible", serde_json::json!(false));
    unsafe {
        if let Some(h) = *OVERLAY_HWND.lock().unwrap() {
            let _ = ShowWindow(HWND(h as *mut _), SW_HIDE);
        }
    }
}

pub fn toggle() {
    if is_visible() { hide(); } else { show(); }
}

pub fn update() {
    if let Some(h) = *OVERLAY_HWND.lock().unwrap() {
        unsafe {
            let _ = PostMessageW(HWND(h as *mut _), WM_APP + 200, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn run_overlay() {
    let class_name = wide("GoldOverlayWnd");
    let hinstance = GetModuleHandleW(None).unwrap();

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(overlay_wnd_proc),
        hInstance: hinstance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        hbrBackground: HBRUSH(std::ptr::null_mut()),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    RegisterClassExW(&wc);

    let x = config::get_i32("x", 100);
    let y = config::get_i32("y", 100);
    let clickthrough = config::get_bool("clickthrough", false);
    CLICKTHROUGH.store(clickthrough, Ordering::Relaxed);

    let mut ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED;
    if clickthrough {
        ex_style |= WS_EX_TRANSPARENT;
    }

    // Create at base size first, then resize after getting DPI
    let hwnd = CreateWindowExW(
        ex_style,
        PCWSTR(class_name.as_ptr()),
        w!("Gold"),
        WS_POPUP | WS_VISIBLE,
        x, y, BASE_W, BASE_H,
        None,
        None,
        HINSTANCE(hinstance.0),
        None,
    ).unwrap();

    // Get actual DPI and resize
    let dpi = GetDpiForWindow(hwnd);
    let dpi = if dpi == 0 { 96 } else { dpi };
    OVERLAY_DPI.store(dpi, Ordering::Relaxed);
    let (w, h) = scaled_size(dpi);
    let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);

    *OVERLAY_HWND.lock().unwrap() = Some(hwnd.0 as isize);
    OVERLAY_VISIBLE.store(true, Ordering::Relaxed);

    // Initial paint
    paint_layered(hwnd);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

fn scaled_size(dpi: u32) -> (i32, i32) {
    let scale = dpi as f32 / 96.0;
    ((BASE_W as f32 * scale) as i32, (BASE_H as f32 * scale) as i32)
}

unsafe fn paint_layered(hwnd: HWND) {
    let dpi = OVERLAY_DPI.load(Ordering::Relaxed);
    let (w, h) = scaled_size(dpi);

    // Create D2D factory
    let factory: ID2D1Factory = D2D1CreateFactory(
        D2D1_FACTORY_TYPE_SINGLE_THREADED, None,
    ).unwrap();

    // Create DWrite factory
    let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap();
    let text_format = dwrite.CreateTextFormat(
        w!("Consolas"),
        None,
        DWRITE_FONT_WEIGHT_BOLD,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        BASE_FONT, // DIPs - D2D1 handles scaling via DPI
        w!(""),
    ).unwrap();
    let _ = text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
    let _ = text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

    // Create a compatible DC and 32-bit DIB
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    let old_bmp = SelectObject(mem_dc, dib);

    // Create D2D DC render target
    let props = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: dpi as f32,
        dpiY: dpi as f32,
        ..Default::default()
    };
    let rt: ID2D1DCRenderTarget = factory.CreateDCRenderTarget(&props).unwrap();
    let rect = RECT { left: 0, top: 0, right: w, bottom: h };
    rt.BindDC(mem_dc, &rect).unwrap();

    rt.BeginDraw();
    rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));

    // All coordinates in DIPs (base size) - D2D1 scales via DPI setting
    let dip_w = BASE_W as f32;
    let dip_h = BASE_H as f32;

    // Background rounded rect: rgba(116,115,113, 0.88)
    let bg_brush = rt.CreateSolidColorBrush(
        &D2D1_COLOR_F { r: 116.0/255.0, g: 115.0/255.0, b: 113.0/255.0, a: 0.88 },
        None,
    ).unwrap();
    let rounded = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F { left: 0.5, top: 0.5, right: dip_w - 0.5, bottom: dip_h - 0.5 },
        radiusX: RADIUS,
        radiusY: RADIUS,
    };
    rt.FillRoundedRectangle(&rounded, &bg_brush);

    // Border: rgba(255,215,0, 0.3)
    let border_brush = rt.CreateSolidColorBrush(
        &D2D1_COLOR_F { r: 1.0, g: 215.0/255.0, b: 0.0, a: 0.3 },
        None,
    ).unwrap();
    rt.DrawRoundedRectangle(&rounded, &border_brush, 1.0, None);

    // Text: gold #ffd700
    let text_brush = rt.CreateSolidColorBrush(
        &D2D1_COLOR_F { r: 1.0, g: 215.0/255.0, b: 0.0, a: 1.0 },
        None,
    ).unwrap();

    let (xau, au9999, paxg) = *price::PRICES.lock().unwrap();
    let lines = [
        format!("{:.2}", xau),
        format!("{:.2}", au9999),
        format!("{:.2}", paxg),
    ];

    // Equal spacing: gap = (H - 3*font) / 4
    let gap = (dip_h - 3.0 * BASE_FONT) / 4.0;
    for (i, line) in lines.iter().enumerate() {
        let text_wide: Vec<u16> = line.encode_utf16().collect();
        let top = gap + i as f32 * (BASE_FONT + gap);
        let layout_rect = D2D_RECT_F {
            left: 4.0,
            top,
            right: dip_w - 4.0,
            bottom: top + BASE_FONT,
        };
        rt.DrawText(
            &text_wide,
            &text_format,
            &layout_rect,
            &text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    let _ = rt.EndDraw(None, None);

    // UpdateLayeredWindow with per-pixel alpha
    let pt_src = POINT { x: 0, y: 0 };
    let size = SIZE { cx: w, cy: h };
    let blend = BLENDFUNCTION {
        BlendOp: 0, // AC_SRC_OVER
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1, // AC_SRC_ALPHA
    };
    let _ = UpdateLayeredWindow(
        hwnd, screen_dc, None, Some(&size),
        mem_dc, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA,
    );

    // Cleanup
    SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(dib);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
}

unsafe extern "system" fn overlay_wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_DPICHANGED => {
            let new_dpi = (wp.0 & 0xFFFF) as u32;
            OVERLAY_DPI.store(new_dpi, Ordering::Relaxed);
            // Use the suggested rect from lp
            let suggested = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd, None,
                suggested.left, suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            paint_layered(hwnd);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            if !CLICKTHROUGH.load(Ordering::Relaxed) {
                LRESULT(HTCAPTION as isize)
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        WM_MOVE => {
            let mut rc = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rc);
            config::set("x", serde_json::json!(rc.left));
            config::set("y", serde_json::json!(rc.top));
            LRESULT(0)
        }
        _ if msg == WM_APP + 200 => {
            paint_layered(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
