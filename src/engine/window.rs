//! The game's windows – what pygame/SDL and the Python files `layered_window.py` and `view_window.py` did.
//!
//! **Layered windows.** A window's picture, position and size are put on the screen in ONE step with
//! `UpdateLayeredWindow`. Moving and resizing in separate steps lets Windows redraw in between, and the window
//! visibly jumps – e.g. first to the left, then growing back to the right when a shot pushes the left wall.
//! The game draws straight into the bitmap that is shown (no copy); the bitmap is only made anew when the window gets
//! bigger than ever before – important when the window spans several monitors. It may therefore be wider and higher
//! than the window; only the top left part is shown.
//!
//! **Extra windows** ("views", the watchtower's sight): borderless, no taskbar button, owned by the main window (so
//! Windows keeps them above it), and they never become active – not even when clicked (`WM_MOUSEACTIVATE` →
//! `MA_NOACTIVATE`, plus `WS_EX_NOACTIVATE`). Clicks on them do not fall through to the desktop.
//!
//! **The mouse lock** (`ClipCursor`): Windows can only lock the cursor into one rectangle, so with several
//! rectangles (the arena and the views connected to it) it is locked into the box around them and put back onto the
//! nearest rectangle whenever it gets into a gap between them.
//!
//! Everything that arrives as a Windows message becomes an [`Event`] in a queue the engine empties every frame.

use std::cell::RefCell;
use std::collections::HashSet;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, ReleaseDC,
    SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MAPVK_VSC_TO_VK_EX, MapVirtualKeyW, ReleaseCapture, SetCapture, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_RETURN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::geom::Rect;
use super::gfx::Surface;

/// A key, by Windows' virtual-key code – with the two shift keys and the keypad's Enter told apart, as pygame did.
pub type Key = u32;

pub const K_W: Key = b'W' as u32;
pub const K_A: Key = b'A' as u32;
pub const K_S: Key = b'S' as u32;
pub const K_D: Key = b'D' as u32;
pub const K_B: Key = b'B' as u32;
pub const K_T: Key = b'T' as u32;
pub const K_SPACE: Key = 0x20;
pub const K_ESCAPE: Key = 0x1B;
pub const K_TAB: Key = 0x09;
pub const K_RETURN: Key = 0x0D;
/// Not a Windows code: Enter on the keypad (Windows sends `VK_RETURN` with the "extended" flag).
pub const K_KP_ENTER: Key = 0x1_000D;
pub const K_LSHIFT: Key = 0xA0;
pub const K_RSHIFT: Key = 0xA1;
pub const K_LEFT: Key = 0x25;
pub const K_UP: Key = 0x26;
pub const K_RIGHT: Key = 0x27;
pub const K_DOWN: Key = 0x28;
pub const K_F3: Key = 0x72;

/// What happened – pygame's events, as far as the game uses them. Positions are relative to the main window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    KeyDown(Key),
    KeyUp(Key),
    MouseMotion { x: i32, y: i32 },
    /// Buttons as in pygame: 1 left, 2 middle, 3 right.
    MouseButtonDown { button: u8, x: i32, y: i32 },
    MouseButtonUp { button: u8, x: i32, y: i32 },
    /// Clicks of the wheel: positive = away from you.
    MouseWheel { y: i32 },
    FocusGained,
    /// `to`: the window that became active (one of the game's own, or another program's).
    FocusLost { to: isize },
    Quit,
}

#[derive(Default)]
struct Input {
    events: Vec<Event>,
    keys: HashSet<Key>,
    main: isize,
    /// Every window of the game (main and views).
    own: HashSet<isize>,
}

thread_local! {
    static INPUT: RefCell<Input> = RefCell::new(Input::default());
}

/// Is `key` held right now (pygame's `key.get_pressed()`)? Only while the game's window is active.
pub fn key_held(key: Key) -> bool {
    INPUT.with(|i| i.borrow().keys.contains(&key))
}

/// Which mouse buttons are held: left, middle, right (pygame's `mouse.get_pressed()`) – no matter which of the
/// game's windows the mouse is over.
pub fn mouse_held() -> [bool; 3] {
    let down = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 };
    let active = INPUT.with(|i| {
        let i = i.borrow();
        let fg = unsafe { GetForegroundWindow() } as isize;
        i.own.contains(&fg)
    });
    if !active {
        return [false; 3];
    }
    // Swapped buttons (left-handed setting) are already applied by Windows to the messages; GetAsyncKeyState
    // reports the physical ones.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let (l, r) = if swapped { (down(VK_RBUTTON), down(VK_LBUTTON)) } else { (down(VK_LBUTTON), down(VK_RBUTTON)) };
    [l, down(VK_MBUTTON), r]
}

/// Where the mouse cursor is on the screen – no matter over which window.
pub fn cursor_position() -> (i32, i32) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

pub(super) fn take_events() -> Vec<Event> {
    INPUT.with(|i| std::mem::take(&mut i.borrow_mut().events))
}

/// Lets every key go – Windows does not send "released" for keys that were held while another window became active.
pub(super) fn release_keys() {
    INPUT.with(|i| i.borrow_mut().keys.clear());
}

fn push(e: Event) {
    INPUT.with(|i| i.borrow_mut().events.push(e));
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

const CLASS: &str = "WindowsToolbox.CubeGame";
const VIEW_CLASS: &str = "WindowsToolbox.CubeGame.View";

fn register() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        // The icon compiled into the exe (resource 1, from the build script), if there is one.
        let icon = LoadIconW(instance, 1 as _);
        for (name, proc_) in [(CLASS, main_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT), (VIEW_CLASS, view_proc as _)] {
            let name = wide(name);
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(proc_),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: icon,
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: name.as_ptr(),
                hIconSm: icon,
            };
            RegisterClassExW(&class);
        }
    });
}

fn button_pos(lparam: LPARAM) -> (i32, i32) {
    ((lparam & 0xFFFF) as i16 as i32, ((lparam >> 16) & 0xFFFF) as i16 as i32)
}

/// The key of a key message: the two shift keys and the keypad's Enter told apart.
fn key_of(wparam: WPARAM, lparam: LPARAM) -> Key {
    let vk = wparam as u32;
    let extended = lparam & (1 << 24) != 0;
    if vk == VK_SHIFT as u32 {
        let scan = ((lparam >> 16) & 0xFF) as u32;
        return unsafe { MapVirtualKeyW(scan, MAPVK_VSC_TO_VK_EX) };
    }
    if vk == VK_RETURN as u32 && extended {
        return K_KP_ENTER;
    }
    vk
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let key = key_of(wparam, lparam);
            let repeat = lparam & (1 << 30) != 0;
            INPUT.with(|i| i.borrow_mut().keys.insert(key));
            if !repeat {
                push(Event::KeyDown(key));
            }
            if msg == WM_SYSKEYDOWN && wparam as u32 == 0x73 {
                // Alt+F4
                push(Event::Quit);
            }
            return 0;
        }
        WM_KEYUP | WM_SYSKEYUP => {
            let key = key_of(wparam, lparam);
            INPUT.with(|i| i.borrow_mut().keys.remove(&key));
            push(Event::KeyUp(key));
            return 0;
        }
        WM_SYSCHAR => return 0, // no beep for Alt+key
        WM_MOUSEMOVE => {
            let (x, y) = button_pos(lparam);
            push(Event::MouseMotion { x, y });
            return 0;
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let (x, y) = button_pos(lparam);
            let button = match msg { WM_LBUTTONDOWN => 1, WM_MBUTTONDOWN => 2, _ => 3 };
            unsafe { SetCapture(hwnd) };
            push(Event::MouseButtonDown { button, x, y });
            return 0;
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let (x, y) = button_pos(lparam);
            let button = match msg { WM_LBUTTONUP => 1, WM_MBUTTONUP => 2, _ => 3 };
            unsafe { ReleaseCapture() };
            push(Event::MouseButtonUp { button, x, y });
            return 0;
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xFFFF) as i16 as i32;
            let y = if delta > 0 { (delta + 119) / 120 } else { (delta - 119) / 120 };
            push(Event::MouseWheel { y });
            return 0;
        }
        WM_ACTIVATE => {
            if wparam & 0xFFFF == WA_INACTIVE as usize {
                release_keys();
                push(Event::FocusLost { to: lparam as isize });
            } else {
                push(Event::FocusGained);
            }
        }
        WM_CLOSE => {
            push(Event::Quit);
            return 0;
        }
        WM_SETCURSOR if lparam & 0xFFFF == HTCLIENT as isize => {
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW)) };
            return 1;
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn view_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // The click still arrives; the window just does not become active (the game would pause).
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_SETCURSOR if lparam & 0xFFFF == HTCLIENT as isize => {
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW)) };
            return 1;
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// The memory a window's picture is drawn into and shown from.
struct Bitmap {
    screen_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    w: u32,
    h: u32,
}

impl Bitmap {
    fn new() -> Option<Bitmap> {
        let screen_dc = unsafe { GetDC(std::ptr::null_mut()) };
        let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if screen_dc.is_null() || memory_dc.is_null() {
            return None;
        }
        Some(Bitmap { screen_dc, memory_dc, bitmap: std::ptr::null_mut(), bits: std::ptr::null_mut(), w: 0, h: 0 })
    }
    /// At least `w` × `h`; made anew only when it gets bigger.
    fn ensure(&mut self, w: u32, h: u32) -> bool {
        if w <= self.w && h <= self.h && !self.bitmap.is_null() {
            return true;
        }
        let (w, h) = (w.max(self.w).max(1), h.max(self.h).max(1));
        let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // top-down: the first row in memory is the top one
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..unsafe { std::mem::zeroed() }
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = unsafe { CreateDIBSection(self.memory_dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0) };
        if bitmap.is_null() || bits.is_null() {
            return false;
        }
        unsafe { SelectObject(self.memory_dc, bitmap) };
        if !self.bitmap.is_null() {
            unsafe { DeleteObject(self.bitmap) };
        }
        self.bitmap = bitmap;
        self.bits = bits as *mut u8;
        self.w = w;
        self.h = h;
        true
    }
    fn canvas(&mut self, w: i32, h: i32) -> Option<Surface> {
        let (w, h) = (w.max(1) as u32, h.max(1) as u32);
        if !self.ensure(w, h) {
            return None;
        }
        Some(unsafe { Surface::window(self.bits, self.w, self.h, w, h) })
    }
    fn show(&self, hwnd: HWND, w: i32, h: i32, x: i32, y: i32) -> bool {
        let dst = POINT { x, y };
        let size = SIZE { cx: w.max(1), cy: h.max(1) };
        let src = POINT { x: 0, y: 0 };
        unsafe { UpdateLayeredWindow(hwnd, self.screen_dc, &dst, &size, self.memory_dc, &src, 0, std::ptr::null(), ULW_OPAQUE) != 0 }
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            DeleteDC(self.memory_dc);
            ReleaseDC(std::ptr::null_mut(), self.screen_dc);
        }
    }
}

/// The game's main window.
pub struct MainWindow {
    pub hwnd: HWND,
    bitmap: Bitmap,
    shown: bool,
    /// Where it is now (screen pixels).
    pub rect: Option<Rect>,
    locked: bool,
    /// Rectangles the locked mouse may move in – `None`: just the window.
    pub mouse_area: Option<Vec<Rect>>,
}

impl MainWindow {
    pub fn new(title: &str, rect: Rect) -> Option<MainWindow> {
        register();
        let (class, title) = (wide(CLASS), wide(title));
        let hwnd = unsafe {
            CreateWindowExW(WS_EX_LAYERED | WS_EX_APPWINDOW, class.as_ptr(), title.as_ptr(), WS_POPUP, rect.x, rect.y, rect.w.max(1), rect.h.max(1), std::ptr::null_mut(), std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null())
        };
        if hwnd.is_null() {
            return None;
        }
        INPUT.with(|i| {
            let mut i = i.borrow_mut();
            i.main = hwnd as isize;
            i.own.insert(hwnd as isize);
        });
        let bitmap = Bitmap::new()?;
        Some(MainWindow { hwnd, bitmap, shown: false, rect: None, locked: false, mouse_area: None })
    }
    pub fn canvas(&mut self, w: i32, h: i32) -> Option<Surface> {
        self.bitmap.canvas(w, h)
    }
    /// Puts what was drawn on the canvas on the screen, with the window at `rect`.
    pub fn show(&mut self, rect: Rect) -> bool {
        if !self.bitmap.show(self.hwnd, rect.w, rect.h, rect.x, rect.y) {
            return false;
        }
        if !self.shown {
            self.shown = true;
            unsafe {
                ShowWindow(self.hwnd, SW_SHOW);
                SetForegroundWindow(self.hwnd);
            }
        }
        self.rect = Some(rect);
        self.apply_lock();
        true
    }
    pub fn focus(&self) {
        unsafe { SetForegroundWindow(self.hwnd) };
    }
    pub fn focused(&self) -> bool {
        unsafe { GetForegroundWindow() == self.hwnd }
    }
    pub fn lock_mouse(&mut self, locked: bool) {
        self.locked = locked;
        self.apply_lock();
    }
    fn apply_lock(&self) {
        let Some(rect) = self.rect.filter(|_| self.locked) else {
            unsafe { ClipCursor(std::ptr::null()) };
            return;
        };
        let area = self.mouse_area.clone().filter(|a| !a.is_empty()).unwrap_or_else(|| vec![rect]);
        let bx = area[0].union_all(&area[1..]);
        let r = RECT { left: bx.x, top: bx.y, right: bx.right(), bottom: bx.bottom() };
        unsafe { ClipCursor(&r) };
        let (x, y) = cursor_position();
        if area.len() > 1 && !area.iter().any(|a| a.collidepoint(x, y)) {
            // In a gap between the rectangles: onto the nearest one.
            let nearest = area
                .iter()
                .map(|a| (x.clamp(a.left(), a.right() - 1), y.clamp(a.top(), a.bottom() - 1)))
                .min_by_key(|(px, py)| ((px - x) as i64).pow(2) + ((py - y) as i64).pow(2));
            if let Some((px, py)) = nearest {
                unsafe { SetCursorPos(px, py) };
            }
        }
    }
}

impl Drop for MainWindow {
    fn drop(&mut self) {
        unsafe {
            ClipCursor(std::ptr::null()); // lets the mouse go, also after a crash
            DestroyWindow(self.hwnd);
        }
        INPUT.with(|i| i.borrow_mut().own.remove(&(self.hwnd as isize)));
    }
}

/// An extra window (see the top of this file).
pub struct ViewWindow {
    pub hwnd: HWND,
    bitmap: Bitmap,
    shown: bool,
}

impl ViewWindow {
    pub fn new(owner: HWND) -> Option<ViewWindow> {
        register();
        let (class, title) = (wide(VIEW_CLASS), wide("view"));
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                owner, // owned by the main window: always above it
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return None;
        }
        let Some(bitmap) = Bitmap::new() else {
            unsafe { DestroyWindow(hwnd) };
            return None;
        };
        INPUT.with(|i| i.borrow_mut().own.insert(hwnd as isize));
        Some(ViewWindow { hwnd, bitmap, shown: false })
    }
    pub fn canvas(&mut self, w: i32, h: i32) -> Option<Surface> {
        self.bitmap.canvas(w, h)
    }
    /// Puts the picture on the screen at `rect`; the first time the window appears – without becoming active.
    pub fn show(&mut self, rect: Rect) -> bool {
        if !self.bitmap.show(self.hwnd, rect.w, rect.h, rect.x, rect.y) {
            return false;
        }
        if !self.shown {
            self.shown = true;
            unsafe { ShowWindow(self.hwnd, SW_SHOWNA) };
        }
        true
    }
    pub fn hide(&mut self) {
        if self.shown {
            self.shown = false;
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
    }
}

impl Drop for ViewWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.hwnd) };
        INPUT.with(|i| i.borrow_mut().own.remove(&(self.hwnd as isize)));
    }
}

/// Is `hwnd` one of the game's windows?
pub fn is_own(hwnd: isize) -> bool {
    INPUT.with(|i| i.borrow().own.contains(&hwnd))
}

/// Hands every waiting Windows message to the window procedures (they turn them into events).
pub fn pump() {
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The whole screen sharp instead of blurry upscaling by Windows – before the first window.
pub fn dpi_aware() {
    unsafe { windows_sys::Win32::UI::HiDpi::SetProcessDpiAwareness(windows_sys::Win32::UI::HiDpi::PROCESS_SYSTEM_DPI_AWARE) };
}
