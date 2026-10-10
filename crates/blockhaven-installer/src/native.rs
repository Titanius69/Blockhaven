use crate::{
    engine::{self, Integration, Options, Progress},
    network::GithubSource,
    platform::{self, InstallLock, WindowsIntegration},
    Result,
};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::DwmSetWindowAttribute, Gdi::*},
    System::{LibraryLoader::GetModuleHandleW, SystemServices::SS_LEFT},
    UI::{
        Controls::{
            BST_CHECKED, BST_UNCHECKED, DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED,
        },
        HiDpi::{
            GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
        },
        Input::KeyboardAndMouse::EnableWindow,
        WindowsAndMessaging::*,
    },
};

const DESKTOP: usize = 101;
const START_MENU: usize = 102;
const WEBVIEW: usize = 103;
const PRIMARY: usize = 201;
const SECONDARY: usize = 202;
const CLOSE: usize = 203;
const TIMER: usize = 1;
const AUTO_TIMER: usize = 2;
const LOADER: &[u8] = include_bytes!("../runtime/windows-x64/WebView2Loader.dll");

fn text(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
const fn color(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
const BACKGROUND: u32 = color(247, 248, 250);
const HEADER: u32 = color(22, 30, 42);
const INK: u32 = color(35, 45, 60);
const GREEN: u32 = color(35, 126, 85);

enum Event {
    Progress(Progress),
    Finished(Result<()>),
}
struct WindowState {
    root: PathBuf,
    current: PathBuf,
    options: Options,
    status: String,
    percent: Option<u32>,
    busy: bool,
    closing: bool,
    cancel: Arc<AtomicBool>,
    events: Option<Receiver<Event>>,
    scale: f64,
    font: HFONT,
    heading_font: HFONT,
    background: HBRUSH,
    icon: HICON,
    path_label: HWND,
    status_label: HWND,
    checkboxes: [HWND; 3],
    primary: HWND,
    secondary: HWND,
}
impl Drop for WindowState {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.font);
            DeleteObject(self.heading_font);
            DeleteObject(self.background);
        }
    }
}
impl WindowState {
    fn px(&self, value: i32) -> i32 {
        (value as f64 * self.scale).round() as i32
    }
    unsafe fn status(&mut self, hwnd: HWND, message: &str) {
        self.status = message.into();
        SetWindowTextW(self.status_label, text(message).as_ptr());
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
    unsafe fn controls(&self, enabled: bool) {
        for control in self.checkboxes {
            EnableWindow(control, enabled as i32);
        }
        EnableWindow(self.primary, enabled as i32);
        SetWindowTextW(
            self.primary,
            text(if enabled {
                if engine::installed_launcher(&self.root) {
                    "Check & launch"
                } else {
                    "Install & launch"
                }
            } else {
                "Please wait…"
            })
            .as_ptr(),
        );
        SetWindowTextW(
            self.secondary,
            text(if !enabled {
                "Cancel"
            } else if engine::installed_launcher(&self.root) {
                "Launch installed version"
            } else {
                "Cancel"
            })
            .as_ptr(),
        );
    }
    unsafe fn start(&mut self, hwnd: HWND) {
        if self.busy {
            return;
        }
        self.options = Options {
            desktop: SendMessageW(self.checkboxes[0], BM_GETCHECK, 0, 0) == BST_CHECKED as isize,
            start_menu: SendMessageW(self.checkboxes[1], BM_GETCHECK, 0, 0) == BST_CHECKED as isize,
            install_webview: SendMessageW(self.checkboxes[2], BM_GETCHECK, 0, 0)
                == BST_CHECKED as isize,
        };
        self.busy = true;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.percent = None;
        self.controls(false);
        self.status(hwnd, "Checking for Blockhaven updates…");
        let (send, receive) = mpsc::channel();
        self.events = Some(receive);
        let root = self.root.clone();
        let current = self.current.clone();
        let options = self.options.clone();
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let result = GithubSource::new().and_then(|source| {
                engine::run_install(
                    &source,
                    &WindowsIntegration,
                    engine::InstallRequest {
                        root: &root,
                        current: &current,
                        loader: LOADER,
                        options: &options,
                        cancel: &cancel,
                    },
                    &mut |progress| {
                        let _ = send.send(Event::Progress(progress));
                    },
                )
            });
            let _ = send.send(Event::Finished(result));
        });
    }
    unsafe fn poll(&mut self, hwnd: HWND) -> bool {
        let messages: Vec<_> = self
            .events
            .as_ref()
            .map(|events| events.try_iter().collect())
            .unwrap_or_default();
        for event in messages {
            match event {
                Event::Progress(progress) => {
                    self.percent = progress.percent;
                    if !self.cancel.load(Ordering::Relaxed) {
                        self.status(hwnd, &progress.message);
                    }
                }
                Event::Finished(result) => {
                    self.events = None;
                    self.busy = false;
                    if result.is_ok() || self.closing {
                        return true;
                    }
                    self.controls(true);
                    self.percent = Some(0);
                    self.status(hwnd, &result.unwrap_err());
                }
            }
        }
        if self.busy && self.percent.is_none() {
            InvalidateRect(hwnd, std::ptr::null(), 0);
        }
        false
    }
    unsafe fn request_close(&mut self, hwnd: HWND) -> bool {
        if self.busy {
            self.closing = true;
            self.cancel.store(true, Ordering::Relaxed);
            self.status(
                hwnd,
                "Cancelling… Waiting for the current operation to finish safely.",
            );
            false
        } else {
            true
        }
    }
}

unsafe fn child(
    hwnd: HWND,
    state: &WindowState,
    class: &str,
    label: &str,
    style: u32,
    id: usize,
    bounds: [i32; 4],
) -> HWND {
    let control = CreateWindowExW(
        0,
        text(class).as_ptr(),
        text(label).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        state.px(bounds[0]),
        state.px(bounds[1]),
        state.px(bounds[2]),
        state.px(bounds[3]),
        hwnd,
        id as HMENU,
        GetModuleHandleW(std::ptr::null()),
        std::ptr::null(),
    );
    SendMessageW(control, WM_SETFONT, state.font as usize, 1);
    control
}
unsafe fn create_controls(hwnd: HWND, s: &mut WindowState) -> bool {
    let heading = child(
        hwnd,
        s,
        "STATIC",
        "Your launcher. Always up to date.",
        SS_LEFT,
        0,
        [32, 113, 494, 30],
    );
    SendMessageW(heading, WM_SETFONT, s.heading_font as usize, 1);
    s.path_label = child(
        hwnd,
        s,
        "STATIC",
        &format!("Install location\n{}", s.root.display()),
        SS_LEFT,
        0,
        [32, 155, 494, 49],
    );
    let labels = [
        "Create a desktop shortcut",
        "Add Blockhaven to the Start menu",
        "Install Microsoft WebView2 Runtime if missing",
    ];
    for (i, label) in labels.iter().enumerate() {
        let checked = [
            s.options.desktop,
            s.options.start_menu,
            s.options.install_webview,
        ][i];
        let handle = child(
            hwnd,
            s,
            "BUTTON",
            label,
            WS_TABSTOP | BS_AUTOCHECKBOX as u32,
            [DESKTOP, START_MENU, WEBVIEW][i],
            [32, 219 + i as i32 * 32, 494, 27],
        );
        SendMessageW(
            handle,
            BM_SETCHECK,
            if checked { BST_CHECKED } else { BST_UNCHECKED } as usize,
            0,
        );
        s.checkboxes[i] = handle;
    }
    let runtime_note = if platform::runtime_installed() {
        "WebView2 Runtime is already installed."
    } else {
        "Setup works without WebView2; the launcher requires it."
    };
    child(
        hwnd,
        s,
        "STATIC",
        runtime_note,
        SS_LEFT,
        0,
        [32, 319, 494, 25],
    );
    s.status_label = child(hwnd, s, "STATIC", &s.status, SS_LEFT, 0, [32, 360, 494, 53]);
    s.secondary = child(
        hwnd,
        s,
        "BUTTON",
        "Cancel",
        WS_TABSTOP | BS_OWNERDRAW as u32,
        SECONDARY,
        [32, 447, 239, 42],
    );
    s.primary = child(
        hwnd,
        s,
        "BUTTON",
        "Install & launch",
        WS_TABSTOP | BS_OWNERDRAW as u32,
        PRIMARY,
        [289, 447, 239, 42],
    );
    child(
        hwnd,
        s,
        "BUTTON",
        "×",
        WS_TABSTOP | BS_OWNERDRAW as u32,
        CLOSE,
        [507, 12, 36, 36],
    );
    s.controls(true);
    !s.primary.is_null() && !s.status_label.is_null() && s.checkboxes.iter().all(|h| !h.is_null())
}
unsafe fn fill(dc: HDC, bounds: &RECT, value: u32) {
    let brush = CreateSolidBrush(value);
    FillRect(dc, bounds, brush);
    DeleteObject(brush);
}
unsafe fn paint(hwnd: HWND, s: &WindowState) {
    let mut ps = std::mem::zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut bounds = std::mem::zeroed();
    GetClientRect(hwnd, &mut bounds);
    fill(dc, &bounds, BACKGROUND);
    fill(
        dc,
        &RECT {
            bottom: s.px(90),
            ..bounds
        },
        HEADER,
    );
    DrawIconEx(
        dc,
        s.px(32),
        s.px(25),
        s.icon,
        s.px(40),
        s.px(40),
        0,
        std::ptr::null_mut(),
        DI_NORMAL,
    );
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color(247, 250, 252));
    SelectObject(dc, s.heading_font);
    let mut title = RECT {
        left: s.px(87),
        top: s.px(20),
        right: s.px(480),
        bottom: s.px(49),
    };
    DrawTextW(
        dc,
        text("Blockhaven").as_ptr(),
        -1,
        &mut title,
        DT_SINGLELINE | DT_VCENTER,
    );
    SelectObject(dc, s.font);
    SetTextColor(dc, color(176, 190, 203));
    let mut subtitle = RECT {
        top: s.px(50),
        bottom: s.px(74),
        ..title
    };
    DrawTextW(
        dc,
        text("Native installer & updater").as_ptr(),
        -1,
        &mut subtitle,
        DT_SINGLELINE,
    );
    let track = RECT {
        left: s.px(32),
        top: s.px(426),
        right: s.px(528),
        bottom: s.px(431),
    };
    fill(dc, &track, color(222, 229, 234));
    if let Some(percent) = s.percent {
        fill(
            dc,
            &RECT {
                right: track.left + (track.right - track.left) * percent.min(100) as i32 / 100,
                ..track
            },
            GREEN,
        );
    } else if s.busy {
        // An indeterminate native progress bar, painted with GDI.
        let tick = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let span = track.right - track.left;
        let offset = (tick / 8 % span as u128) as i32;
        fill(
            dc,
            &RECT {
                left: track.left + offset,
                right: (track.left + offset + s.px(100)).min(track.right),
                ..track
            },
            GREEN,
        );
    }
    EndPaint(hwnd, &ps);
}
unsafe fn draw_button(s: &WindowState, item: &DRAWITEMSTRUCT) {
    let pressed = item.itemState & ODS_SELECTED != 0;
    let disabled = item.itemState & ODS_DISABLED != 0;
    let primary = item.CtlID as usize == PRIMARY;
    let close = item.CtlID as usize == CLOSE;
    let bg = if close {
        HEADER
    } else if disabled {
        color(218, 223, 228)
    } else if primary {
        if pressed {
            color(24, 99, 65)
        } else {
            GREEN
        }
    } else {
        if pressed {
            color(212, 220, 227)
        } else {
            color(229, 234, 239)
        }
    };
    fill(item.hDC, &item.rcItem, bg);
    SetBkMode(item.hDC, TRANSPARENT as i32);
    SetTextColor(
        item.hDC,
        if primary || close {
            color(255, 255, 255)
        } else {
            INK
        },
    );
    SelectObject(item.hDC, s.font);
    let mut buffer = [0u16; 256];
    let n = GetWindowTextW(item.hwndItem, buffer.as_mut_ptr(), buffer.len() as i32);
    let mut bounds = item.rcItem;
    DrawTextW(
        item.hDC,
        buffer.as_ptr(),
        n,
        &mut bounds,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    if item.itemState & ODS_FOCUS != 0 {
        InflateRect(&mut bounds, -s.px(4), -s.px(4));
        DrawFocusRect(item.hDC, &bounds);
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<WindowState>;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, w, l);
    }
    if message == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        drop(Box::from_raw(pointer));
        return DefWindowProcW(hwnd, message, w, l);
    }
    // Win32 calls can synchronously re-enter this procedure. Never alias mutable
    // state, and destroy the window only after releasing its state borrow.
    let Ok(mut state) = (*pointer).try_borrow_mut() else {
        if message == WM_CLOSE {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            return 0;
        }
        return DefWindowProcW(hwnd, message, w, l);
    };
    let s = &mut *state;
    let mut close = false;
    let result = match message {
        WM_CREATE => {
            if !create_controls(hwnd, s) {
                return -1;
            }
            SetTimer(hwnd, TIMER, 100, None);
            let installed_copy = std::fs::canonicalize(&s.current).ok()
                == std::fs::canonicalize(s.root.join("BlockhavenSetup.exe")).ok();
            let settings_mode = std::env::args().any(|a| a == "--settings");
            if installed_copy && !settings_mode {
                SetTimer(hwnd, AUTO_TIMER, 350, None);
            }
            0
        }
        WM_TIMER => {
            if w == AUTO_TIMER {
                KillTimer(hwnd, AUTO_TIMER);
                s.start(hwnd);
            } else {
                close = s.poll(hwnd);
            }
            0
        }
        WM_COMMAND => {
            match w & 0xffff {
                PRIMARY => {
                    KillTimer(hwnd, AUTO_TIMER);
                    s.start(hwnd);
                }
                SECONDARY => {
                    KillTimer(hwnd, AUTO_TIMER);
                    if s.busy {
                        s.cancel.store(true, Ordering::Relaxed);
                        s.status(
                            hwnd,
                            "Cancelling… Waiting for the current operation to finish safely.",
                        );
                    } else if engine::installed_launcher(&s.root) {
                        let result = if !platform::runtime_installed() {
                            Err("WebView2 Runtime is missing. Use Check & launch with runtime installation enabled.".into())
                        } else {
                            WindowsIntegration.launch(&s.root.join("blockhaven.exe"))
                        };
                        match result {
                            Ok(()) => {
                                close = true;
                            }
                            Err(e) => {
                                s.status(hwnd, &e);
                            }
                        }
                    } else {
                        close = s.request_close(hwnd);
                    }
                }
                CLOSE => close = s.request_close(hwnd),
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            close = s.request_close(hwnd);
            0
        }
        WM_PAINT => {
            paint(hwnd, s);
            0
        }
        WM_ERASEBKGND => 1,
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            SetBkColor(w as HDC, BACKGROUND);
            SetTextColor(w as HDC, INK);
            s.background as isize
        }
        WM_DRAWITEM => {
            draw_button(s, &*(l as *const DRAWITEMSTRUCT));
            1
        }
        WM_NCHITTEST => {
            let mut cursor = POINT {
                x: (l as u16) as i16 as i32,
                y: ((l >> 16) as u16) as i16 as i32,
            };
            ScreenToClient(hwnd, &mut cursor);
            if cursor.y < s.px(90) && cursor.x < s.px(500) {
                HTCAPTION as isize
            } else {
                HTCLIENT as isize
            }
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER);
            KillTimer(hwnd, AUTO_TIMER);
            PostQuitMessage(0);
            0
        }
        _ => {
            drop(state);
            return DefWindowProcW(hwnd, message, w, l);
        }
    };
    drop(state);
    if close {
        DestroyWindow(hwnd);
    }
    result
}
pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text(error).as_ptr(),
            text("Blockhaven Setup").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
pub fn run() -> Result<()> {
    let root = platform::install_root()?;
    let _lock = InstallLock::acquire(&root)?;
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let options = engine::read_options(&root);
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        let scale = GetDpiForSystem() as f64 / 96.0;
        let instance = GetModuleHandleW(std::ptr::null());
        let icon = LoadIconW(instance, std::ptr::without_provenance::<u16>(1));
        let class_name = text("BlockhavenNativeSetup");
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hIcon: icon,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return Err(format!(
                "Could not register native window: {}",
                GetLastError()
            ));
        }
        let font = CreateFontW(
            -(14.0 * scale).round() as i32,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            text("Segoe UI").as_ptr(),
        );
        let heading_font = CreateFontW(
            -(21.0 * scale).round() as i32,
            0,
            0,
            0,
            600,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            text("Segoe UI").as_ptr(),
        );
        let state = Box::new(RefCell::new(WindowState {
            root,
            current,
            options,
            status: "Choose your options, then install and launch Blockhaven.".into(),
            percent: Some(0),
            busy: false,
            closing: false,
            cancel: Arc::new(AtomicBool::new(false)),
            events: None,
            scale,
            font,
            heading_font,
            background: CreateSolidBrush(BACKGROUND),
            icon,
            path_label: std::ptr::null_mut(),
            status_label: std::ptr::null_mut(),
            checkboxes: [std::ptr::null_mut(); 3],
            primary: std::ptr::null_mut(),
            secondary: std::ptr::null_mut(),
        }));
        let width = (560.0 * scale).round() as i32;
        let height = (520.0 * scale).round() as i32;
        let pointer = Box::into_raw(state);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT | WS_EX_APPWINDOW,
            class_name.as_ptr(),
            text("Blockhaven Setup").as_ptr(),
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
            (GetSystemMetrics(SM_CXSCREEN) - width) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - height) / 2,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            pointer.cast(),
        );
        if hwnd.is_null() {
            return Err(format!(
                "Could not create native window: {}",
                GetLastError()
            ));
        }
        let rounding: u32 = 2;
        DwmSetWindowAttribute(hwnd, 33, (&rounding as *const u32).cast(), 4);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        let mut message: MSG = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut message, std::ptr::null_mut(), 0, 0);
            if result == -1 {
                return Err("Native event loop failed".into());
            }
            if result == 0 {
                break;
            }
            if message.message == WM_KEYDOWN && message.wParam == 27 {
                SendMessageW(hwnd, WM_CLOSE, 0, 0);
                continue;
            }
            if IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    Ok(())
}
