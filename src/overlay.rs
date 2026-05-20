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
static PREV_PRICES: Mutex<price::PriceData> = Mutex::new(price::PriceData {
    xau: 0.0, au9999: 0.0, paxg: 0.0, dxy: 0.0, us10y: 0.0, us10y_chg: 0.0,
});

// Base dimensions at 96 DPI
const BASE_W: i32 = 82;
const BASE_H: i32 = 78;
const BASE_FONT: f32 = 11.0;
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

    let dpi = GetDpiForWindow(hwnd);
    let dpi = if dpi == 0 { 96 } else { dpi };
    OVERLAY_DPI.store(dpi, Ordering::Relaxed);
    let (w, h) = scaled_size(dpi);
    let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);

    *OVERLAY_HWND.lock().unwrap() = Some(hwnd.0 as isize);
    OVERLAY_VISIBLE.store(true, Ordering::Relaxed);

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

    let factory: ID2D1Factory = D2D1CreateFactory(
        D2D1_FACTORY_TYPE_SINGLE_THREADED, None,
    ).unwrap();

    let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap();
    let text_format = dwrite.CreateTextFormat(
        w!("Consolas"),
        None,
        DWRITE_FONT_WEIGHT_BOLD,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        BASE_FONT,
        w!(""),
    ).unwrap();
    let _ = text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
    let _ = text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    let old_bmp = SelectObject(mem_dc, dib);

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

    let dip_w = BASE_W as f32;
    let dip_h = BASE_H as f32;

    // Background rounded rect
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

    // Border
    let border_brush = rt.CreateSolidColorBrush(
        &D2D1_COLOR_F { r: 1.0, g: 215.0/255.0, b: 0.0, a: 0.3 },
        None,
    ).unwrap();
    rt.DrawRoundedRectangle(&rounded, &border_brush, 1.0, None);

    // Gold color brush
    let gold_brush = rt.CreateSolidColorBrush(
        &D2D1_COLOR_F { r: 1.0, g: 215.0/255.0, b: 0.0, a: 1.0 },
        None,
    ).unwrap();
    // Cyan brush for DXY
    let dxy_brush = rt.CreateSolidColorBrush(
        &D2D1_COLOR_F { r: 0.4, g: 0.9, b: 1.0, a: 1.0 },
        None,
    ).unwrap();
    // Light green brush for US10Y
    let us10y_brush = rt.CreateSolidColorBrush(
        &D2D1_COLOR_F { r: 0.5, g: 1.0, b: 0.5, a: 1.0 },
        None,
    ).unwrap();

    let prices = *price::PRICES.lock().unwrap();

    let lines: [(&str, &ID2D1SolidColorBrush, f64); 5] = [
        ("XA", &gold_brush, prices.xau),
        ("AU", &gold_brush, prices.au9999),
        ("DX", &dxy_brush, prices.dxy),
        ("Y0", &us10y_brush, prices.us10y),
        ("PA", &gold_brush, prices.paxg),
    ];

    let n = lines.len() as f32;
    let gap = (dip_h - n * BASE_FONT) / (n + 1.0);
    for (i, (label, brush, val)) in lines.iter().enumerate() {
        let text = if *label == "Y0" {
            let chg = prices.us10y_chg;
            let sign = if chg >= 0.05 { '+' } else if chg <= -0.05 { '-' } else { ' ' };
            format!("{:<2} {:>7}", label, format!("{:.2}{}{:.0}", val, sign, chg.abs()))
        } else {
            format!("{:<2} {:>7.2}", label, val)
        };
        let text_wide: Vec<u16> = text.encode_utf16().collect();
        let top = gap + i as f32 * (BASE_FONT + gap);
        let layout_rect = D2D_RECT_F {
            left: 3.0,
            top,
            right: dip_w - 3.0,
            bottom: top + BASE_FONT,
        };
        rt.DrawText(
            &text_wide,
            &text_format,
            &layout_rect,
            *brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    let _ = rt.EndDraw(None, None);

    let pt_src = POINT { x: 0, y: 0 };
    let size = SIZE { cx: w, cy: h };
    let blend = BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1,
    };
    let _ = UpdateLayeredWindow(
        hwnd, screen_dc, None, Some(&size),
        mem_dc, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA,
    );

    SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(dib);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);

    *PREV_PRICES.lock().unwrap() = prices;
}

unsafe extern "system" fn overlay_wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_DPICHANGED => {
            let new_dpi = (wp.0 & 0xFFFF) as u32;
            OVERLAY_DPI.store(new_dpi, Ordering::Relaxed);
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
