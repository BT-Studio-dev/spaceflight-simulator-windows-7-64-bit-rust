//! Native Windows 7 window, message pump, input and GDI presentation.
//!
//! Deliberately minimal: a single `HWND`, a 32-bit top-down DIB section that
//! owns the framebuffer, `StretchDIBits` to present, and `GetTickCount` for
//! timing. Everything here is available on Windows 7 SP1 (and is tested by CI
//! on real Windows runners).
//!
//! The API used is restricted to functions present in kernel32/user32/gdi32 on
//! Windows 7 and that are *not* subject to API-set redirection introduced in
//! Windows 8/10. Notably we do not call `GetDpiForWindow`, `SetProcessDpiAwarenessContext`
//! or any `*FromWindow` DPI helper.

#![allow(non_snake_case)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use crate::app::{Game, Key};
use crate::platform::rgb_to_bgr_inplace;
use crate::png;

type Hwnd = *mut core::ffi::c_void;
type Hdc = *mut core::ffi::c_void;
type Hinstance = *mut core::ffi::c_void;
type Hicon = *mut core::ffi::c_void;
type Hcursor = *mut core::ffi::c_void;
type Hbrush = *mut core::ffi::c_void;
type Hmenu = *mut core::ffi::c_void;
type Hbitmap = *mut core::ffi::c_void;
type Hpalette = *mut core::ffi::c_void;
type Hfont = *mut core::ffi::c_void;

#[repr(C)]
struct WndClassW {
    style: u32,
    lpfnWndProc: Option<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: Hinstance,
    hIcon: Hicon,
    hCursor: Hcursor,
    hbrBackground: Hbrush,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
}

#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Default)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wParam: usize,
    lParam: isize,
    time: u32,
    pt: Point,
    lPrivate: u32,
}

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct BitmapInfoHeader {
    biSize: u32,
    biWidth: i32,
    biHeight: i32,
    biPlanes: u16,
    biBitCount: u16,
    biCompression: u32,
    biSizeImage: u32,
    biXPelsPerMeter: i32,
    biYPelsPerMeter: i32,
    biClrUsed: u32,
    biClrImportant: u32,
}

#[repr(C)]
struct BitmapInfo {
    bmiHeader: BitmapInfoHeader,
    bmiColors: [u32; 3],
}

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const CW_USEDEFAULT: i32 = -0x8000_0000;
const CS_HREDRAW: u32 = 0x0002;
const CS_VREDRAW: u32 = 0x0001;
const CS_OWNDC: u32 = 0x0020;
const IDC_ARROW: usize = 32512;
const COLOR_WINDOW: usize = 5;
const SW_SHOW: i32 = 5;
const SW_MINIMIZE: i32 = 6;
const SW_RESTORE: i32 = 9;

const WM_DESTROY: u32 = 0x0002;
const WM_CLOSE: u32 = 0x0010;
const WM_SIZE: u32 = 0x0005;
const WM_PAINT: u32 = 0x000F;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_ACTIVATE: u32 = 0x0006;
const WM_ERASEBKGND: u32 = 0x0014;

const PM_REMOVE: u32 = 0x0001;
const BI_RGB: u32 = 0;
const DIB_RGB_COLORS: u32 = 0;
const SRCCOPY: u32 = 0x00CC_0020;
const VK_F1: u32 = 0x70;
const VK_F2: u32 = 0x71;
const VK_F4: u32 = 0x73;
const VK_ESCAPE: u32 = 0x1B;
const VK_RETURN: u32 = 0x0D;
const VK_MENU: u32 = 0x12;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOZORDER: u32 = 0x0004;
const IDI_APPLICATION: usize = 32512;
const MB_ICONERROR: u32 = 0x10;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
    fn GetTickCount() -> u32;
    fn Sleep(ms: u32);
    fn GetLastError() -> u32;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(wc: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Hwnd,
        menu: Hmenu,
        instance: Hinstance,
        param: *mut core::ffi::c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize;
    fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
    fn UpdateWindow(hwnd: Hwnd) -> i32;
    fn PeekMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn ClientToScreen(hwnd: Hwnd, p: *mut Point) -> i32;
    fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn MessageBeep(kind: u32) -> i32;
    fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, flags: u32) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn AdjustWindowRect(rect: *mut Rect, style: u32, menu: i32) -> i32;
    fn LoadCursorW(instance: Hinstance, name: usize) -> Hcursor;
    fn LoadIconW(instance: Hinstance, name: usize) -> Hicon;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetDC(hwnd: Hwnd) -> Hdc;
    fn ReleaseDC(hwnd: Hwnd, hdc: Hdc) -> i32;
    fn CreateCompatibleDC(hdc: Hdc) -> Hdc;
    fn CreateDIBSection(
        hdc: Hdc,
        bmi: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut core::ffi::c_void,
        section: *mut core::ffi::c_void,
        offset: u32,
    ) -> Hbitmap;
    fn SelectObject(hdc: Hdc, obj: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn DeleteObject(obj: *mut core::ffi::c_void) -> i32;
    fn DeleteDC(hdc: Hdc) -> i32;
    fn StretchDIBits(
        hdc: Hdc,
        xDest: i32,
        yDest: i32,
        wDest: i32,
        hDest: i32,
        xSrc: i32,
        ySrc: i32,
        wSrc: i32,
        hSrc: i32,
        bits: *const core::ffi::c_void,
        bmi: *const BitmapInfo,
        usage: u32,
        rop: u32,
    ) -> i32;
    fn PatBlt(hdc: Hdc, x: i32, y: i32, w: i32, h: i32, rop: u32) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Global state shared with the window procedure. Only the fields the WndProc
/// needs live here; the game itself is owned by `run`.
struct WindowState {
    hwnd: Hwnd,
    game: *mut Game,
    /// Set when the user asks to close the window.
    closing: bool,
    last_w: i32,
    last_h: i32,
}

static mut STATE: *mut WindowState = null_mut();

unsafe fn state() -> &'static mut WindowState {
    &mut *STATE
}

/// A DIB section used as the framebuffer target.
struct Surface {
    mem_dc: Hdc,
    bitmap: Hbitmap,
    old: *mut core::ffi::c_void,
    bits: *mut u32,
    w: i32,
    h: i32,
    bmi: Box<BitmapInfo>,
}

impl Surface {
    unsafe fn new(hdc: Hdc, w: i32, h: i32) -> Surface {
        let bmi = Box::new(BitmapInfo {
            bmiHeader: BitmapInfoHeader {
                biSize: std::mem::size_of::<BitmapInfoHeader>() as u32,
                biWidth: w,
                // Negative height = top-down DIB, so row 0 is the top row.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: (w * h * 4) as u32,
                biXPelsPerMeter: 3780,
                biYPelsPerMeter: 3780,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [0; 3],
        });
        let mut bits: *mut core::ffi::c_void = null_mut();
        let bitmap = CreateDIBSection(hdc, &*bmi as *const BitmapInfo, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let mem_dc = CreateCompatibleDC(hdc);
        let old = SelectObject(mem_dc, bitmap);
        Surface {
            mem_dc,
            bitmap,
            old,
            bits: bits as *mut u32,
            w,
            h,
            bmi,
        }
    }

    unsafe fn pixels(&self) -> &'static mut [u32] {
        std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize)
    }

    unsafe fn free(&mut self) {
        if !self.mem_dc.is_null() {
            SelectObject(self.mem_dc, self.old);
            DeleteDC(self.mem_dc);
            self.mem_dc = null_mut();
        }
        if !self.bitmap.is_null() {
            DeleteObject(self.bitmap);
            self.bitmap = null_mut();
        }
    }
}

unsafe fn paint(hwnd: Hwnd, surf: &mut Surface) {
    let mut rect = Rect::default();
    GetClientRect(hwnd, &mut rect);
    let cw = (rect.right - rect.left).max(1);
    let ch = (rect.bottom - rect.top).max(1);
    let hdc = GetDC(hwnd);
    if hdc.is_null() {
        return;
    }
    if surf.w != cw || surf.h != ch {
        surf.free();
        *surf = Surface::new(hdc, cw, ch);
    }
    StretchDIBits(
        hdc,
        0,
        0,
        cw,
        ch,
        0,
        0,
        surf.w,
        surf.h,
        surf.bits as *const core::ffi::c_void,
        &*surf.bmi as *const BitmapInfo,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
    ReleaseDC(hwnd, hdc);
}

unsafe extern "system" fn wndproc(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
    let st = STATE;
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_CLOSE => {
            if !st.is_null() {
                (*st).closing = true;
            }
            PostQuitMessage(0);
            0
        }
        WM_ERASEBKGND => 1,
        WM_ACTIVATE => {
            // Re-assert the palette-free 32-bit surface after alt-tab.
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => {
            if !st.is_null() && !(*st).game.is_null() {
                let game = &mut *(*st).game;
                match msg {
                    WM_SIZE => {
                        let w = (lp & 0xFFFF) as i32;
                        let h = ((lp >> 16) & 0xFFFF) as i32;
                        if w > 0 && h > 0 {
                            game.resize(w, h);
                            (*st).last_w = w;
                            (*st).last_h = h;
                        }
                        return 0;
                    }
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        let vk = wp as u32;
                        // Alt+F4 must still quit.
                        if vk == VK_F4 && lp & (1 << 29) != 0 {
                            return DefWindowProcW(hwnd, msg, wp, lp);
                        }
                        if let Some(key) = map_key(vk) {
                            if key.is_held() {
                                game.key_down(key, true);
                            }
                            if msg == WM_KEYDOWN || vk == VK_F1 {
                                game.key_pressed(key);
                            }
                        }
                        return 0;
                    }
                    WM_KEYUP | WM_SYSKEYUP => {
                        if let Some(key) = map_key(wp as u32) {
                            if key.is_held() {
                                game.key_down(key, false);
                            }
                        }
                        return 0;
                    }
                    WM_MOUSEMOVE => {
                        let x = (lp & 0xFFFF) as i16 as i32;
                        let y = ((lp >> 16) & 0xFFFF) as i16 as i32;
                        game.mouse_move(x, y);
                        return 0;
                    }
                    WM_LBUTTONDOWN => {
                        game.mouse_button(true);
                        return 0;
                    }
                    WM_LBUTTONUP => {
                        game.mouse_button(false);
                        return 0;
                    }
                    WM_RBUTTONDOWN => {
                        game.mouse_right_button(true);
                        return 0;
                    }
                    WM_RBUTTONUP => {
                        game.mouse_right_button(false);
                        return 0;
                    }
                    WM_MOUSEWHEEL => {
                        let delta = ((wp >> 16) & 0xFFFF) as i16 as i32;
                        game.wheel(delta / 120);
                        return 0;
                    }
                    _ => {}
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }
}

fn map_key(vk: u32) -> Option<Key> {
    // Escape closes menus but must never close the window directly.
    if vk == VK_ESCAPE {
        return Some(Key::Escape);
    }
    Key::from_vk(vk)
}

/// Milliseconds since boot, wrapped every ~49 days (fine for frame timing).
fn now_ms() -> u32 {
    unsafe { GetTickCount() }
}

fn screenshot_path() -> std::path::PathBuf {
    // Next to the executable, in a screenshots/ folder.
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let stamp = (now_ms() / 1000) % 100_000;
    dir.join("screenshots").join(format!("spaceflight-{:05}.png", stamp))
}

/// Write a screenshot next to the exe; returns the path on success.
pub fn save_screenshot(game: &Game) -> Option<std::path::PathBuf> {
    let path = screenshot_path();
    let src = &game.canvas.px;
    // PNG expects 0x00RRGGBB which is exactly what the canvas stores.
    png::write_png(&path, game.canvas.w as u32, game.canvas.h as u32, src)
        .ok()
        .map(|_| path)
}

pub struct Options {
    pub width: i32,
    pub height: i32,
    pub fullscreen: bool,
}

pub const APP_TITLE: &str = "Spaceflight Simulator";

/// Run the game loop. Returns the process exit code.
pub fn run(opts: Options) -> io::Result<i32> {
    unsafe {
        let hinstance = GetModuleHandleW(null());
        let class_name = wide("SpaceflightSimulatorWindowClass");
        let title = wide(&format!(
            "{} - {} [{}]",
            APP_TITLE,
            crate::VERSION,
            crate::TARGET_OS
        ));
        let wc = WndClassW {
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as Hbrush,
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };
        // RegisterClassW returns 0 if the class already exists, which is fine.
        RegisterClassW(&wc);

        let mut style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
        let mut rect = Rect {
            left: 0,
            top: 0,
            right: opts.width,
            bottom: opts.height,
        };
        if !opts.fullscreen {
            AdjustWindowRect(&mut rect, style, 0);
        } else {
            style = WS_VISIBLE;
        }
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            null_mut(),
            hinstance,
            null_mut(),
        );
        if hwnd.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("CreateWindowExW failed (error {})", GetLastError()),
            ));
        }

        let mut game = Box::new(Game::new(opts.width, opts.height));
        let mut state = Box::new(WindowState {
            hwnd,
            game: &mut *game as *mut Game,
            closing: false,
            last_w: opts.width,
            last_h: opts.height,
        });
        STATE = &mut *state;

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let hdc = GetDC(hwnd);
        let mut surf = Surface::new(hdc, opts.width, opts.height);
        ReleaseDC(hwnd, hdc);

        let mut last = now_ms();
        let mut last_title = String::new();
        let mut tick_accum = 0u32;
        let target_ms = 1000.0 / 60.0;

        'outer: loop {
            // ---- pump messages ------------------------------------------
            let mut msg = Msg::default();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_CLOSE {
                    break 'outer;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if state.closing {
                break 'outer;
            }

            // ---- timing --------------------------------------------------
            let now = now_ms();
            let elapsed = now.wrapping_sub(last);
            last = now;
            let dt = (elapsed as f64 / 1000.0).clamp(0.0, 0.1);

            // ---- simulate & render ---------------------------------------
            game.tick(dt);
            game.draw();
            if game.take_screenshot_request() {
                if let Some(p) = save_screenshot(&game) {
                    let m = format!("SCREENSHOT SAVED: {}", p.display());
                    game.set_message(m);
                }
            }
            if game.quit {
                break 'outer;
            }

            // ---- present --------------------------------------------------
            // Convert RGB -> BGR once into the DIB and blit.
            let n = (game.canvas.w * game.canvas.h) as usize;
            if n > 0 && surf.w * surf.h >= game.canvas.w * game.canvas.h {
                let dst = surf.pixels();
                let src = &game.canvas.px;
                let count = (surf.w * surf.h) as usize;
                let copy = n.min(count);
                dst[..copy].copy_from_slice(&src[..copy]);
                rgb_to_bgr_inplace(&mut dst[..copy]);
            }
            paint(hwnd, &mut surf);

            // Update the window caption with live telemetry, at most twice a
            // second to avoid flicker.
            tick_accum = tick_accum.wrapping_add(elapsed);
            if tick_accum >= 500 {
                tick_accum = 0;
                let s = format!("{} | {}", APP_TITLE, game.status_line());
                if s != last_title {
                    let w = wide(&s);
                    SetWindowTextW(hwnd, w.as_ptr());
                    last_title = s;
                }
            }

            // ---- frame limiter -----------------------------------------(60 Hz)
            let spent = now_ms().wrapping_sub(now);
            if (spent as f64) < target_ms {
                let sleep = (target_ms - spent as f64) as u32;
                if sleep > 0 {
                    Sleep(sleep);
                }
            }
        }

        // ---- shutdown ----------------------------------------------------
        surf.free();
        STATE = null_mut();
        Ok(0)
    }
}

/// Show a modal error box; used for fatal start-up problems.
pub fn error_box(title: &str, text: &str) {
    unsafe {
        let t = wide(title);
        let m = wide(text);
        MessageBoxW(null_mut(), m.as_ptr(), t.as_ptr(), MB_ICONERROR);
    }
}

/// Centre the window on the primary monitor.
pub fn center_window(hwnd: Hwnd) {
    unsafe {
        let sw = GetSystemMetrics(0);
        let sh = GetSystemMetrics(1);
        let mut rect = Rect::default();
        GetClientRect(hwnd, &mut rect);
        SetWindowPos(hwnd, null_mut(), (sw - 1024) / 2, (sh - 768) / 2, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
    }
}

#[allow(dead_code)]
fn unused(_: Hpalette, _: Hfont, _: Hbrush) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_mapping_is_total_for_letters() {
        assert_eq!(map_key(VK_ESCAPE), Some(Key::Escape));
        assert_eq!(map_key(VK_F1), Some(Key::Help));
        assert_eq!(map_key(VK_F2), Some(Key::Screenshot));
        assert_eq!(map_key(0x57), Some(Key::Up));
        assert_eq!(map_key(0x1234), None);
        assert_eq!(map_key(VK_RETURN), Some(Key::Enter));
        assert!(VK_MENU != 0);
    }

    #[test]
    fn wide_strings_are_nul_terminated() {
        let w = wide("ABC");
        assert_eq!(w, vec![65, 66, 67, 0]);
    }

    #[test]
    fn screenshot_path_is_next_to_the_exe() {
        let p = screenshot_path();
        assert!(p.to_string_lossy().contains("spaceflight-"));
        assert_eq!(p.extension().unwrap(), "png");
    }
}
