//! Native Win32 L/R balance slider window.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetStockObject, SetBkMode, COLOR_BTNFACE, DEFAULT_GUI_FONT, HBRUSH, HDC, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX,
    TBS_AUTOTICKS, TBS_HORZ,
    TBM_SETLINESIZE, TBM_SETPAGESIZE, TBM_SETPOS, TBM_SETRANGEMAX, TBM_SETRANGEMIN,
    TBM_SETTICFREQ, TB_BOTTOM, TB_ENDTRACK, TB_LINEDOWN, TB_LINEUP, TB_PAGEDOWN, TB_PAGEUP,
    TB_THUMBPOSITION, TB_THUMBTRACK, TB_TOP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, BringWindowToTop, CallWindowProcW, CreateWindowExW, DefWindowProcW,
    DestroyWindow, GetCursorPos, GetParent, IsWindow, LoadCursorW, PostMessageW,
    RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW,
    ShowWindow, SystemParametersInfoW, CS_HREDRAW, CS_VREDRAW, GWLP_WNDPROC, HICON, HMENU,
    IDC_ARROW, SPI_GETWORKAREA, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORSTATIC, WM_DESTROY, WM_HSCROLL, WM_KEYDOWN,
    WM_SETFONT, WNDCLASSEXW, WNDPROC, WS_CAPTION, WS_CHILD, WS_EX_TOPMOST, WS_OVERLAPPED,
    WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

const TBM_GETPOS: u32 = 0x0400;
const VK_ESCAPE: usize = 0x1B;
const SS_CENTER: u32 = 0x00000001;
const BS_PUSHBUTTON: u32 = 0x00000000;

const IDC_STATIC_LEFT: usize = 101;
const IDC_TRACKBAR: usize = 102;
const IDC_STATIC_RIGHT: usize = 103;
const IDC_STATIC_VALUE: usize = 104;
const IDC_CENTER_BTN: usize = 105;

static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Localised strings required to render the slider window.
#[derive(Clone, Debug)]
pub struct BalanceTexts {
    pub title: String,
    pub left: String,
    pub right: String,
    pub center: String,
    pub value_left: String,
    pub value_right: String,
    pub center_button: String,
}

struct WindowState {
    hwnd: Option<HWND>,
    trackbar_hwnd: Option<HWND>,
    value_label_hwnd: Option<HWND>,
    button_hwnd: Option<HWND>,
    texts: Option<BalanceTexts>,
    on_change: Option<fn(i8)>,
    last_drag_time: Instant,
    last_sent_step: Option<i8>,
    orig_trackbar_proc: Option<isize>,
    orig_button_proc: Option<isize>,
}

thread_local! {
    static STATE: RefCell<WindowState> = RefCell::new(WindowState {
        hwnd: None,
        trackbar_hwnd: None,
        value_label_hwnd: None,
        button_hwnd: None,
        texts: None,
        on_change: None,
        last_drag_time: Instant::now(),
        last_sent_step: None,
        orig_trackbar_proc: None,
        orig_button_proc: None,
    });
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn send_msg(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> LRESULT {
    SendMessageW(hwnd, msg, Some(WPARAM(wparam)), Some(LPARAM(lparam)))
}

/// Clamps a step value to the valid scale [-16, 16].
pub fn clamp_step(v: i32) -> i8 {
    v.clamp(-16, 16) as i8
}

/// Formats the slider value label: "L {n}" for negative, "R {n}" for positive, or centre text for 0.
pub fn step_label(step: i8, tpl_left: &str, tpl_right: &str, centre: &str) -> String {
    let s = clamp_step(step as i32);
    if s < 0 {
        crate::i18n::format(tpl_left, &[("n", &(-s).to_string())])
    } else if s > 0 {
        crate::i18n::format(tpl_right, &[("n", &s.to_string())])
    } else {
        centre.to_string()
    }
}

/// Checks whether the balance slider window is currently open.
pub fn is_open() -> bool {
    STATE.with(|s| {
        let b = s.borrow();
        if let Some(h) = b.hwnd {
            unsafe { IsWindow(Some(h)).as_bool() }
        } else {
            false
        }
    })
}

/// Updates the slider position and value label if the window is currently open.
pub fn set_step(step: i8) {
    let step = clamp_step(step as i32);
    STATE.with(|s| {
        let mut b = s.borrow_mut();
        if let Some(hwnd) = b.hwnd {
            unsafe {
                if IsWindow(Some(hwnd)).as_bool() {
                    if let Some(tb) = b.trackbar_hwnd {
                        let _ = send_msg(tb, TBM_SETPOS, 1, step as isize);
                    }
                    if let Some(vl) = b.value_label_hwnd {
                        if let Some(texts) = &b.texts {
                            let text = step_label(step, &texts.value_left, &texts.value_right, &texts.center);
                            let wide = to_wide(&text);
                            let _ = SetWindowTextW(vl, PCWSTR(wide.as_ptr()));
                        }
                    }
                    b.last_sent_step = Some(step);
                }
            }
        }
    });
}

/// Opens the balance slider window or brings it to the front if already open.
pub fn open(texts: BalanceTexts, initial_step: i8, on_change: fn(i8)) {
    let initial_step = clamp_step(initial_step as i32);
    if is_open() {
        STATE.with(|s| {
            let mut b = s.borrow_mut();
            b.texts = Some(texts.clone());
            b.on_change = Some(on_change);
            if let Some(hwnd) = b.hwnd {
                unsafe {
                    let title_wide = to_wide(&texts.title);
                    let _ = SetWindowTextW(hwnd, PCWSTR(title_wide.as_ptr()));
                    if let Some(tb) = b.trackbar_hwnd {
                        let _ = send_msg(tb, TBM_SETPOS, 1, initial_step as isize);
                    }
                    if let Some(vl) = b.value_label_hwnd {
                        let text = step_label(initial_step, &texts.value_left, &texts.value_right, &texts.center);
                        let wide = to_wide(&text);
                        let _ = SetWindowTextW(vl, PCWSTR(wide.as_ptr()));
                    }
                    b.last_sent_step = Some(initial_step);
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = BringWindowToTop(hwnd);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        });
        return;
    }

    unsafe {
        let _ = create_window(texts, initial_step, on_change);
    }
}

#[allow(dead_code)]
pub struct BalanceWindow;

#[allow(dead_code)]
impl BalanceWindow {
    pub fn open(texts: BalanceTexts, initial_step: i8, on_change: fn(i8)) {
        open(texts, initial_step, on_change);
    }
    pub fn set_step(step: i8) {
        set_step(step);
    }
    pub fn is_open() -> bool {
        is_open()
    }
}

unsafe fn create_window(texts: BalanceTexts, initial_step: i8, on_change: fn(i8)) -> Option<HWND> {
    let hinstance: HINSTANCE = GetModuleHandleW(PCWSTR::null()).ok()?.into();

    // Ensure common controls (trackbar) are initialised
    let icex = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_BAR_CLASSES,
    };
    let _ = InitCommonControlsEx(&icex);

    let class_name = w!("AKG_N9_Balance_Window");

    if !CLASS_REGISTERED.load(Ordering::SeqCst) {
        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: HICON::default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 as isize + 1) as *mut _),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name,
            hIconSm: HICON::default(),
        };
        let _ = RegisterClassExW(&wnd_class);
        CLASS_REGISTERED.store(true, Ordering::SeqCst);
    }

    // Client area size: 340 x 150
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 340,
        bottom: 150,
    };
    let style: WINDOW_STYLE = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let ex_style: WINDOW_EX_STYLE = WS_EX_TOPMOST;
    let _ = AdjustWindowRectEx(&mut rect, style, false, ex_style);
    let win_w = rect.right - rect.left;
    let win_h = rect.bottom - rect.top;

    // Position near mouse cursor, clamped inside the monitor's work area
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let mut work_area = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut work_area as *mut _ as *mut _),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );

    let mut x = pt.x - win_w / 2;
    let mut y = pt.y - win_h / 2;
    if x + win_w > work_area.right {
        x = work_area.right - win_w;
    }
    if x < work_area.left {
        x = work_area.left;
    }
    if y + win_h > work_area.bottom {
        y = work_area.bottom - win_h;
    }
    if y < work_area.top {
        y = work_area.top;
    }

    let title_wide = to_wide(&texts.title);
    let hwnd = CreateWindowExW(
        ex_style,
        class_name,
        PCWSTR(title_wide.as_ptr()),
        style,
        x,
        y,
        win_w,
        win_h,
        None,
        None,
        Some(hinstance),
        None,
    ).ok()?;

    // Create child controls
    let left_wide = to_wide(&texts.left);
    let left_lbl = CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        PCWSTR(left_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_CENTER),
        12,
        22,
        24,
        20,
        Some(hwnd),
        Some(HMENU(IDC_STATIC_LEFT as *mut _)),
        Some(hinstance),
        None,
    ).ok()?;

    let trackbar = CreateWindowExW(
        Default::default(),
        w!("msctls_trackbar32"),
        PCWSTR::null(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(TBS_AUTOTICKS | TBS_HORZ),
        38,
        18,
        264,
        30,
        Some(hwnd),
        Some(HMENU(IDC_TRACKBAR as *mut _)),
        Some(hinstance),
        None,
    ).ok()?;

    let right_wide = to_wide(&texts.right);
    let right_lbl = CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        PCWSTR(right_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_CENTER),
        304,
        22,
        24,
        20,
        Some(hwnd),
        Some(HMENU(IDC_STATIC_RIGHT as *mut _)),
        Some(hinstance),
        None,
    ).ok()?;

    let initial_val_text = step_label(initial_step, &texts.value_left, &texts.value_right, &texts.center);
    let val_wide = to_wide(&initial_val_text);
    let value_lbl = CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        PCWSTR(val_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_CENTER),
        40,
        56,
        260,
        20,
        Some(hwnd),
        Some(HMENU(IDC_STATIC_VALUE as *mut _)),
        Some(hinstance),
        None,
    ).ok()?;

    let btn_wide = to_wide(&texts.center_button);
    let center_btn = CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(btn_wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON),
        120,
        92,
        100,
        28,
        Some(hwnd),
        Some(HMENU(IDC_CENTER_BTN as *mut _)),
        Some(hinstance),
        None,
    ).ok()?;

    // Apply standard GUI font to all children
    let hfont = GetStockObject(DEFAULT_GUI_FONT);
    for h in [left_lbl, trackbar, right_lbl, value_lbl, center_btn] {
        let _ = send_msg(h, WM_SETFONT, hfont.0 as usize, 1);
    }

    // Configure trackbar: range -16..=16, page size 4, line size 1, tick every 4
    let _ = send_msg(trackbar, TBM_SETRANGEMIN, 0, -16);
    let _ = send_msg(trackbar, TBM_SETRANGEMAX, 1, 16);
    let _ = send_msg(trackbar, TBM_SETPAGESIZE, 0, 4);
    let _ = send_msg(trackbar, TBM_SETLINESIZE, 0, 1);
    let _ = send_msg(trackbar, TBM_SETTICFREQ, 4, 0);
    let _ = send_msg(trackbar, TBM_SETPOS, 1, initial_step as isize);

    // Subclass trackbar and button to intercept Escape key
    let orig_trackbar = SetWindowLongPtrW(trackbar, GWLP_WNDPROC, child_subclass_proc as *const () as isize);
    let orig_button = SetWindowLongPtrW(center_btn, GWLP_WNDPROC, child_subclass_proc as *const () as isize);

    STATE.with(|s| {
        let mut b = s.borrow_mut();
        b.hwnd = Some(hwnd);
        b.trackbar_hwnd = Some(trackbar);
        b.value_label_hwnd = Some(value_lbl);
        b.button_hwnd = Some(center_btn);
        b.texts = Some(texts);
        b.on_change = Some(on_change);
        b.last_drag_time = Instant::now();
        b.last_sent_step = Some(initial_step);
        b.orig_trackbar_proc = Some(orig_trackbar);
        b.orig_button_proc = Some(orig_button);
    });

    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);

    Some(hwnd)
}

unsafe extern "system" fn child_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = std::panic::catch_unwind(|| {
        if msg == WM_KEYDOWN && wparam.0 == VK_ESCAPE {
            if let Ok(parent) = GetParent(hwnd) {
                let _ = PostMessageW(Some(parent), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return Some(LRESULT(0));
        }
        None
    });
    if let Ok(Some(res)) = result {
        return res;
    }

    let orig = STATE.with(|s| {
        let b = s.borrow();
        if b.trackbar_hwnd == Some(hwnd) {
            b.orig_trackbar_proc
        } else if b.button_hwnd == Some(hwnd) {
            b.orig_button_proc
        } else {
            None
        }
    });

    if let Some(orig_proc) = orig {
        let prev_wndproc: WNDPROC = std::mem::transmute(orig_proc);
        CallWindowProcW(prev_wndproc, hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let result = std::panic::catch_unwind(|| {
        wndproc_inner(hwnd, msg, wparam, lparam)
    });
    result.unwrap_or_else(|_| DefWindowProcW(hwnd, msg, wparam, lparam))
}

unsafe fn wndproc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as *mut _);
            let _ = SetBkMode(hdc, TRANSPARENT);
            return LRESULT(COLOR_BTNFACE.0 as isize + 1);
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as usize;
            if id == IDC_CENTER_BTN {
                STATE.with(|s| {
                    let mut b = s.borrow_mut();
                    if let Some(tb) = b.trackbar_hwnd {
                        let _ = send_msg(tb, TBM_SETPOS, 1, 0);
                    }
                    if let Some(vl) = b.value_label_hwnd {
                        if let Some(texts) = &b.texts {
                            let text = step_label(0, &texts.value_left, &texts.value_right, &texts.center);
                            let wide = to_wide(&text);
                            let _ = SetWindowTextW(vl, PCWSTR(wide.as_ptr()));
                        }
                    }
                    b.last_sent_step = Some(0);
                    b.last_drag_time = Instant::now();
                    if let Some(cb) = b.on_change {
                        cb(0);
                    }
                });
                return LRESULT(0);
            }
        }
        WM_HSCROLL => {
            let code = (wparam.0 & 0xFFFF) as u32;
            let (tb_hwnd, on_change_cb) = STATE.with(|s| {
                let b = s.borrow();
                (b.trackbar_hwnd, b.on_change)
            });

            if let Some(tb) = tb_hwnd {
                let pos = send_msg(tb, TBM_GETPOS, 0, 0).0 as i32;
                let step = clamp_step(pos);

                // Update value label text on every change
                STATE.with(|s| {
                    let b = s.borrow();
                    if let Some(vl) = b.value_label_hwnd {
                        if let Some(texts) = &b.texts {
                            let text = step_label(step, &texts.value_left, &texts.value_right, &texts.center);
                            let wide = to_wide(&text);
                            let _ = SetWindowTextW(vl, PCWSTR(wide.as_ptr()));
                        }
                    }
                });

                match code {
                    TB_THUMBTRACK => {
                        // Dragging: throttle at most every 100 ms
                        let now = Instant::now();
                        let should_call = STATE.with(|s| {
                            let mut b = s.borrow_mut();
                            if now.duration_since(b.last_drag_time) >= Duration::from_millis(100) {
                                b.last_drag_time = now;
                                b.last_sent_step = Some(step);
                                true
                            } else {
                                false
                            }
                        });
                        if should_call {
                            if let Some(cb) = on_change_cb {
                                cb(step);
                            }
                        }
                    }
                    TB_ENDTRACK | TB_THUMBPOSITION => {
                        // ALWAYS once more on release with the final value
                        STATE.with(|s| {
                            let mut b = s.borrow_mut();
                            b.last_drag_time = Instant::now();
                            b.last_sent_step = Some(step);
                        });
                        if let Some(cb) = on_change_cb {
                            cb(step);
                        }
                    }
                    TB_LINEUP | TB_LINEDOWN | TB_PAGEUP | TB_PAGEDOWN | TB_TOP | TB_BOTTOM => {
                        // Keyboard / arrows / page keys: call on_change immediately
                        STATE.with(|s| {
                            let mut b = s.borrow_mut();
                            b.last_drag_time = Instant::now();
                            b.last_sent_step = Some(step);
                        });
                        if let Some(cb) = on_change_cb {
                            cb(step);
                        }
                    }
                    _ => {}
                }
            }
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            return LRESULT(0);
        }
        WM_DESTROY => {
            STATE.with(|s| {
                let mut b = s.borrow_mut();
                b.hwnd = None;
                b.trackbar_hwnd = None;
                b.value_label_hwnd = None;
                b.button_hwnd = None;
                b.orig_trackbar_proc = None;
                b.orig_button_proc = None;
            });
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_step() {
        assert_eq!(clamp_step(0), 0);
        assert_eq!(clamp_step(-16), -16);
        assert_eq!(clamp_step(16), 16);
        assert_eq!(clamp_step(-25), -16);
        assert_eq!(clamp_step(25), 16);
        assert_eq!(clamp_step(5), 5);
        assert_eq!(clamp_step(-7), -7);
        assert_eq!(clamp_step(i32::MAX), 16);
        assert_eq!(clamp_step(i32::MIN), -16);
    }

    #[test]
    fn test_step_label() {
        let tpl_l = "L {n}";
        let tpl_r = "R {n}";
        let ctr = "Center";

        assert_eq!(step_label(0, tpl_l, tpl_r, ctr), "Center");
        assert_eq!(step_label(-8, tpl_l, tpl_r, ctr), "L 8");
        assert_eq!(step_label(5, tpl_l, tpl_r, ctr), "R 5");
        assert_eq!(step_label(-16, tpl_l, tpl_r, ctr), "L 16");
        assert_eq!(step_label(16, tpl_l, tpl_r, ctr), "R 16");
        // Check out of range values clamp to 16
        assert_eq!(step_label(-20, tpl_l, tpl_r, ctr), "L 16");
        assert_eq!(step_label(20, tpl_l, tpl_r, ctr), "R 16");
    }
}
