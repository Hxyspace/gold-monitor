use std::sync::{atomic::{AtomicBool, AtomicU32, Ordering}, Mutex};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Controls::*;
use crate::price::PriceData;

static VISIBLE: AtomicBool = AtomicBool::new(false);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
pub static PRICES: Mutex<PriceData> = Mutex::new(PriceData {
    xau: 0.0, au9999: 0.0, paxg: 0.0, dxy: 0.0, us10y: 0.0, us10y_chg: 0.0,
});

const WND_WIDTH: i32 = 66;
const WM_APP_QUIT: u32 = WM_APP + 1;
const WM_APP_REPAINT: u32 = WM_APP + 2;
const WM_APP_HIDE: u32 = WM_APP + 3;
const WM_APP_SHOW: u32 = WM_APP + 4;
const TIMER_RECLAIM: usize = 2;
const RECLAIM_DELAY_MS: u32 = 600_000;

pub fn toggle() {
    if VISIBLE.load(Ordering::Relaxed) {
        hide();
    } else {
        show();
    }
}

pub fn is_visible() -> bool {
    VISIBLE.load(Ordering::Relaxed)
}

pub fn update_prices(data: PriceData) {
    *PRICES.lock().unwrap() = data;
    if VISIBLE.load(Ordering::Relaxed) {
        let tid = THREAD_ID.load(Ordering::Relaxed);
        if tid != 0 {
            unsafe { let _ = PostThreadMessageW(tid, WM_APP_REPAINT, WPARAM(0), LPARAM(0)); }
        }
    }
}

fn show() {
    VISIBLE.store(true, Ordering::Relaxed);
    let tid = THREAD_ID.load(Ordering::Relaxed);
    if tid != 0 {
        // Thread already running, just show the window
        unsafe { let _ = PostThreadMessageW(tid, WM_APP_SHOW, WPARAM(0), LPARAM(0)); }
    } else {
        std::thread::spawn(|| unsafe { run_taskbar_window() });
    }
}

fn hide() {
    VISIBLE.store(false, Ordering::Relaxed);
    let tid = THREAD_ID.load(Ordering::Relaxed);
    if tid != 0 {
        unsafe { let _ = PostThreadMessageW(tid, WM_APP_HIDE, WPARAM(0), LPARAM(0)); }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct DCompRenderer {
    swap_chain: IDXGISwapChain1,
    d2d_context: ID2D1DeviceContext,
    dcomp_device: IDCompositionDevice,
    _dcomp_target: IDCompositionTarget,
    _dcomp_visual: IDCompositionVisual,
    dwrite_factory: IDWriteFactory,
    width: u32,
    height: u32,
}

impl DCompRenderer {
    unsafe fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self> {
        let mut d3d_device = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut d3d_device),
            None,
            None,
        )?;
        let d3d_device = d3d_device.unwrap();
        let dxgi_device: IDXGIDevice = d3d_device.cast()?;

        let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory1()?;
        let swap_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            ..Default::default()
        };
        let swap_chain = dxgi_factory.CreateSwapChainForComposition(&dxgi_device, &swap_desc, None)?;

        let d2d_factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;
        let d2d_context = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;

        let dcomp_device: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
        let dcomp_target = dcomp_device.CreateTargetForHwnd(hwnd, true)?;
        let dcomp_visual = dcomp_device.CreateVisual()?;
        dcomp_visual.SetContent(&swap_chain)?;
        dcomp_target.SetRoot(&dcomp_visual)?;
        dcomp_device.Commit()?;

        let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

        Ok(Self {
            swap_chain,
            d2d_context,
            dcomp_device,
            _dcomp_target: dcomp_target,
            _dcomp_visual: dcomp_visual,
            dwrite_factory,
            width,
            height,
        })
    }

    unsafe fn render(&self) -> Result<()> {
        let surface: IDXGISurface = self.swap_chain.GetBuffer(0)?;
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        let bitmap = self.d2d_context.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
        self.d2d_context.SetTarget(&bitmap);

        self.d2d_context.BeginDraw();
        self.d2d_context.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));

        let text_format = self.dwrite_factory.CreateTextFormat(
            &HSTRING::from("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            12.0,  // 9pt = 12 DIPs
            &HSTRING::from("en-us"),
        )?;
        text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        // Line spacing: slightly more than font height for readable gap
        text_format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, 16.0, 12.0)?;

        let rt: ID2D1RenderTarget = self.d2d_context.cast()?;
        let brush = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
            None,
        )?;

        let prices = *PRICES.lock().unwrap();
        let text = format!("$:{:8.2}\n\u{00A5}:{:8.2}", prices.xau, prices.au9999);

        let rect = D2D_RECT_F { left: 2.0, top: 0.0, right: self.width as f32, bottom: self.height as f32 };
        let s: Vec<u16> = text.encode_utf16().collect();

        self.d2d_context.DrawText(&s, &text_format, &rect, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);

        self.d2d_context.EndDraw(None, None)?;
        self.swap_chain.Present(0, DXGI_PRESENT(0)).ok()?;
        self.dcomp_device.Commit()?;

        Ok(())
    }
}

unsafe fn run_taskbar_window() {
    if let Err(e) = run_taskbar_window_inner() {
        eprintln!("[taskbar] Error: {:?}", e);
    }
}

unsafe fn create_tooltip(hwnd: HWND) -> Result<HWND> {
    // TrafficMonitor pattern: create tooltip with our window as owner/parent
    let tip_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        TOOLTIPS_CLASSW,
        None,
        WINDOW_STYLE(WS_POPUP.0 | TTS_ALWAYSTIP | TTS_NOPREFIX),
        CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
        hwnd,  // Owner = our window (same as TrafficMonitor's m_tool_tips.Create(this, ...))
        None,
        None,
        None,
    )?;

    // Multi-line support (TrafficMonitor uses 600)
    SendMessageW(tip_hwnd, TTM_SETMAXTIPWIDTH, WPARAM(0), LPARAM(600));

    // TrafficMonitor pattern: TTF_IDISHWND, hwnd = parent of our window (Shell_TrayWnd)
    // uId = our window handle
    let parent = GetParent(hwnd).unwrap_or(hwnd);
    let mut ti = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND,
        hwnd: parent,
        uId: hwnd.0 as usize,
        ..Default::default()
    };

    let empty = wide("");
    ti.lpszText = PWSTR(empty.as_ptr() as *mut _);

    SendMessageW(tip_hwnd, TTM_ADDTOOLW, WPARAM(0), LPARAM(&ti as *const _ as isize));

    // TrafficMonitor: SetToolTipsTopMost() - SetWindowPos(&wndTopMost, 0,0,0,0, SWP_NOSIZE|SWP_NOMOVE|SWP_SHOWWINDOW)
    let _ = SetWindowPos(tip_hwnd, HWND_TOPMOST, 0, 0, 0, 0,
        SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE);

    Ok(tip_hwnd)
}

unsafe fn update_tooltip_text(tip_hwnd: HWND, hwnd: HWND) {
    let prices = *PRICES.lock().unwrap();
    let chg_str = if prices.us10y_chg >= 0.05 {
        format!("+{:.0}bp", prices.us10y_chg)
    } else if prices.us10y_chg <= -0.05 {
        format!("-{:.0}bp", prices.us10y_chg.abs())
    } else {
        ".0".to_string()
    };
    let text = format!(
        "XA:{:8.2}$\r\nAU:{:8.2}\u{00A5}\r\nPA:{:8.2}$\r\nDX:{:8.2}\r\nY0:{:8.2}% {}",
        prices.xau, prices.au9999, prices.paxg, prices.dxy, prices.us10y, chg_str
    );
    let tip_text = wide(&text);

    let parent = GetParent(hwnd).unwrap_or(hwnd);
    let mut ti = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND,
        hwnd: parent,
        uId: hwnd.0 as usize,
        ..Default::default()
    };
    ti.lpszText = PWSTR(tip_text.as_ptr() as *mut _);

    SendMessageW(tip_hwnd, TTM_UPDATETIPTEXTW, WPARAM(0), LPARAM(&ti as *const _ as isize));
}

unsafe fn relay_tooltip_event(tip_hwnd: HWND, msg: &MSG) {
    SendMessageW(tip_hwnd, TTM_RELAYEVENT, WPARAM(0), LPARAM(msg as *const _ as isize));
}

// fn hide_taskbar(hwnd: HWND) {
//     unsafe {
//         let _ = ShowWindow(hwnd, SW_HIDE);
//         VISIBLE.store(false, Ordering::Relaxed);
//         SetTimer(hwnd, TIMER_RECLAIM, RECLAIM_DELAY_MS, None);
//     }
// }

unsafe fn run_taskbar_window_inner() -> Result<()> {
    let class_name = wide("GoldTaskbarDComp");
    let hinstance = GetModuleHandleW(None)?;

    // hCursor = IDC_ARROW: standard Win32 way to declare a window's cursor.
    // Without it, class cursor is NULL, DefWindowProc doesn't call SetCursor,
    // parent Shell_TrayWnd then sets IDC_APPSTARTING for unrecognized children.
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    RegisterClassExW(&wc);

    let taskbar = FindWindowW(w!("Shell_TrayWnd"), None)?;
    let notify = FindWindowExW(taskbar, None, w!("TrayNotifyWnd"), None)?;

    let mut taskbar_rect = RECT::default();
    GetWindowRect(taskbar, &mut taskbar_rect)?;
    let tb_height = taskbar_rect.bottom - taskbar_rect.top;

    let mut nr = RECT::default();
    GetWindowRect(notify, &mut nr)?;
    let x = nr.left - taskbar_rect.left - WND_WIDTH - 2;

    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        PCWSTR(class_name.as_ptr()),
        w!("GoldTB"),
        WS_POPUP,
        0, 0, WND_WIDTH, tb_height,
        None,
        None,
        HINSTANCE(hinstance.0),
        None,
    )?;

    // Reparent into Shell_TrayWnd as WS_CHILD
    let _ = SetParent(hwnd, taskbar);
    let style = GetWindowLongW(hwnd, GWL_STYLE);
    SetWindowLongW(hwnd, GWL_STYLE, (style & !(WS_POPUP.0 as i32)) | WS_CHILD.0 as i32);
    SetWindowPos(hwnd, None, x, 0, WND_WIDTH, tb_height,
        SWP_SHOWWINDOW | SWP_FRAMECHANGED | SWP_NOZORDER)?;

    // Create tooltip (TrafficMonitor pattern: after window is positioned)
    let tip_hwnd = create_tooltip(hwnd)?;
    // Update tooltip text immediately if prices already available
    update_tooltip_text(tip_hwnd, hwnd);

    // Init DirectComposition renderer
    let renderer = DCompRenderer::new(hwnd, WND_WIDTH as u32, tb_height as u32)?;
    renderer.render()?;

    THREAD_ID.store(GetCurrentThreadId(), Ordering::Relaxed);

    let mut msg = MSG::default();
    loop {
        let ret = GetMessageW(&mut msg, None, 0, 0);
        if ret.0 <= 0 { break; }

        // Thread messages (hwnd == null)
        if msg.hwnd.0 == std::ptr::null_mut() {
            if msg.message == WM_APP_QUIT {
                let _ = DestroyWindow(hwnd);
                break;
            } else if msg.message == WM_APP_HIDE {
                let _ = ShowWindow(hwnd, SW_HIDE);
                VISIBLE.store(false, Ordering::Relaxed);
                // Start delayed resource reclaim timer
                SetTimer(hwnd, TIMER_RECLAIM, RECLAIM_DELAY_MS, None);
                continue;
            } else if msg.message == WM_APP_SHOW {
                let _ = KillTimer(hwnd, TIMER_RECLAIM);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = renderer.render();
                continue;
            } else if msg.message == WM_APP_REPAINT {
                let _ = renderer.render();
                update_tooltip_text(tip_hwnd, hwnd);
                continue;
            }
        }

        // Timer handling
        if msg.hwnd == hwnd && msg.message == WM_TIMER && msg.wParam.0 == TIMER_RECLAIM {
            // Reclaim resources after being hidden for a while
            let _ = KillTimer(hwnd, TIMER_RECLAIM);
            let _ = DestroyWindow(hwnd);
            break;
        }

        // Relay mouse events to tooltip
        if msg.message == WM_MOUSEMOVE
            || msg.message == WM_LBUTTONDOWN
            || msg.message == WM_LBUTTONUP
        {
            relay_tooltip_event(tip_hwnd, &msg);
        }

        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    THREAD_ID.store(0, Ordering::Relaxed);
    VISIBLE.store(false, Ordering::Relaxed);
    Ok(())
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, _wp: WPARAM, _lp: LPARAM) -> LRESULT {
    match msg {
        // WM_RBUTTONUP => {
        //     hide_taskbar(hwnd);
        //     LRESULT(0)
        // }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, _wp, _lp),
    }
}
