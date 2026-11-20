use crate::overlay_recovery;
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    ptr::null_mut,
    rc::Rc,
};
use tauri::{AppHandle, Emitter, LogicalPosition, PhysicalPosition, WebviewWindow};
use windows_sys::{
    core::w,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            ClientToScreen, CombineRgn, CreateRectRgn, DeleteObject, GetStockObject, SetWindowRgn,
            BLACK_BRUSH, RGN_OR,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::WM_MOUSELEAVE,
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_LBUTTON,
                VK_RBUTTON,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos,
                GetSystemMetrics, GetWindowRect, IsIconic, IsWindowVisible, KillTimer, LoadCursorW,
                RegisterClassW, SetCursor, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
                ShowWindow, HWND_TOPMOST, IDC_ARROW, IDC_SIZEALL, LWA_ALPHA, MA_NOACTIVATE,
                SM_SWAPBUTTON, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE,
                WM_CANCELMODE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
                WM_MOUSEWHEEL, WM_NCDESTROY, WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

const WINDOW_CLASS: windows_sys::core::PCWSTR = w!("CodexWeeklyOverlayInput");
const DRAG_TIMER: usize = 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Target {
    Working,
    Completed,
    Details,
}

#[derive(Deserialize)]
pub struct HitRegion {
    target: Target,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PhysicalRegion {
    target: Target,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl HitRegion {
    fn to_physical(&self, scale: f64, client: RECT) -> Option<PhysicalRegion> {
        let left = ((self.x * scale).floor() as i32).clamp(0, client.right);
        let top = ((self.y * scale).floor() as i32).clamp(0, client.bottom);
        let right = (((self.x + self.width) * scale).ceil() as i32).clamp(0, client.right);
        let bottom = (((self.y + self.height) * scale).ceil() as i32).clamp(0, client.bottom);
        (left < right && top < bottom).then_some(PhysicalRegion {
            target: self.target,
            left,
            top,
            right,
            bottom,
        })
    }
}

#[derive(Clone, Copy)]
struct PointerDrag {
    grab_offset: LogicalPosition<f64>,
    window_start: RECT,
}

struct InputWindow {
    hwnd: HWND,
    overlay: HWND,
    app: AppHandle,
    enabled: Cell<bool>,
    regions: RefCell<Vec<HitRegion>>,
    physical_regions: RefCell<Vec<PhysicalRegion>>,
    hovered: Cell<Option<Target>>,
    tracking_mouse: Cell<bool>,
    drag: Cell<Option<PointerDrag>>,
}

thread_local! {
    // Windows 在悬浮窗 UI 线程上派发输入消息，原生命中窗口也只在该线程访问。
    static INPUT: RefCell<Option<Rc<InputWindow>>> = const { RefCell::new(None) };
}

pub fn install(app: &AppHandle, overlay: &WebviewWindow, enabled: bool) -> Result<(), String> {
    let overlay = overlay.hwnd().map_err(|error| error.to_string())?.0;
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: GetStockObject(BLACK_BRUSH),
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            WINDOW_CLASS,
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            overlay,
            null_mut(),
            module,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // 透明度为零的层会透传鼠标。以最低非零透明度覆盖原生区域，锁定的 WebView 始终穿透。
        if SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA) == 0 {
            let error = std::io::Error::last_os_error().to_string();
            DestroyWindow(hwnd);
            return Err(error);
        }
        INPUT.with(|input| {
            input.replace(Some(Rc::new(InputWindow {
                hwnd,
                overlay,
                app: app.clone(),
                enabled: Cell::new(enabled),
                regions: RefCell::default(),
                physical_regions: RefCell::default(),
                hovered: Cell::default(),
                tracking_mouse: Cell::default(),
                drag: Cell::default(),
            })))
        });
    }
    Ok(())
}

fn input() -> Option<Rc<InputWindow>> {
    INPUT.with(|state| state.borrow().clone())
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if let Some(input) = input() {
        input.enabled.set(enabled);
        input.sync()?;
    }
    Ok(())
}

pub fn set_regions(regions: Vec<HitRegion>) -> Result<(), String> {
    if let Some(input) = input() {
        input.regions.replace(regions);
        input.sync()?;
    }
    Ok(())
}

pub fn sync() -> Result<(), String> {
    input().map_or(Ok(()), |input| input.sync())
}

impl InputWindow {
    fn sync(&self) -> Result<(), String> {
        let mut client = RECT::default();
        let mut origin = POINT::default();
        unsafe {
            if !self.enabled.get()
                || IsWindowVisible(self.overlay) == 0
                || IsIconic(self.overlay) != 0
            {
                self.hide();
                return Ok(());
            }
            if GetClientRect(self.overlay, &mut client) == 0
                || ClientToScreen(self.overlay, &mut origin) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let scale = f64::from(GetDpiForWindow(self.overlay)) / 96.0;
            let regions: Vec<_> = self
                .regions
                .borrow()
                .iter()
                .filter_map(|region| region.to_physical(scale, client))
                .collect();
            if regions.is_empty() {
                if self.drag.get().is_none() {
                    self.hide();
                }
                return Ok(());
            }
            if regions != *self.physical_regions.borrow() {
                let combined = CreateRectRgn(0, 0, 0, 0);
                if combined.is_null() {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                for region in &regions {
                    let next = CreateRectRgn(region.left, region.top, region.right, region.bottom);
                    let merged =
                        !next.is_null() && CombineRgn(combined, combined, next, RGN_OR) != 0;
                    if !next.is_null() {
                        DeleteObject(next);
                    }
                    if !merged {
                        DeleteObject(combined);
                        return Err("无法设置悬浮窗鼠标区域。".into());
                    }
                }
                // SetWindowRgn 成功后，Windows 接管区域的释放。
                if SetWindowRgn(self.hwnd, combined, 1) == 0 {
                    DeleteObject(combined);
                    return Err(std::io::Error::last_os_error().to_string());
                }
                self.physical_regions.replace(regions);
            }
            if SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                origin.x,
                origin.y,
                client.right,
                client.bottom,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        if self.drag.get().is_none() {
            self.refresh_hover();
        }
        Ok(())
    }

    fn hide(&self) {
        self.stop_drag(false);
        self.set_hover(None);
        self.tracking_mouse.set(false);
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    fn set_hover(&self, target: Option<Target>) {
        if self.hovered.replace(target) != target {
            let _ = self.app.emit_to("overlay", "overlay-region-hover", target);
        }
    }

    fn refresh_hover(&self) {
        let target = self.target_under_cursor();
        if target.is_some() {
            self.track_mouse();
        }
        self.set_hover(target);
    }

    fn target_under_cursor(&self) -> Option<Target> {
        let mut cursor = POINT::default();
        let mut origin = POINT::default();
        unsafe {
            if GetCursorPos(&mut cursor) == 0 || ClientToScreen(self.hwnd, &mut origin) == 0 {
                return None;
            }
        }
        self.target_at(cursor.x - origin.x, cursor.y - origin.y)
    }

    fn target_at(&self, x: i32, y: i32) -> Option<Target> {
        self.physical_regions
            .borrow()
            .iter()
            .find(|region| {
                (region.left..region.right).contains(&x) && (region.top..region.bottom).contains(&y)
            })
            .map(|region| region.target)
    }

    fn track_mouse(&self) {
        if !self.tracking_mouse.get() {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            self.tracking_mouse
                .set(unsafe { TrackMouseEvent(&mut tracking) != 0 });
        }
    }

    fn start_drag(&self) {
        if self.drag.get().is_some() {
            return;
        }
        let mut cursor_start = POINT::default();
        let mut window_start = RECT::default();
        unsafe {
            if GetCursorPos(&mut cursor_start) == 0
                || GetWindowRect(self.overlay, &mut window_start) == 0
            {
                return;
            }
            // 后台窗口不能跨过自身区域捕获鼠标。仅在气泡已按下期间读取指针，移动时不激活窗口。
            if SetTimer(self.hwnd, DRAG_TIMER, 16, None) == 0 {
                return;
            }
        }
        overlay_recovery::begin_drag(&self.app);
        let offset = PhysicalPosition::new(
            cursor_start.x - window_start.left,
            cursor_start.y - window_start.top,
        );
        self.drag.set(Some(PointerDrag {
            grab_offset: offset.to_logical(window_scale(self.overlay)),
            window_start,
        }));
    }

    fn update_drag(&self) {
        let Some(drag) = self.drag.get() else {
            return;
        };
        if !overlay_recovery::is_dragging(&self.app) {
            self.stop_drag(false);
            return;
        }
        let mut cursor = POINT::default();
        unsafe {
            if GetCursorPos(&mut cursor) == 0
                || IsWindowVisible(self.overlay) == 0
                || IsIconic(self.overlay) != 0
            {
                self.stop_drag(false);
                return;
            }
            // 以逻辑坐标记录按下的位置，跨 DPI 后指针继续抓住同一个气泡。
            let offset: PhysicalPosition<i32> =
                drag.grab_offset.to_physical(window_scale(self.overlay));
            let x = cursor.x - offset.x;
            let y = cursor.y - offset.y;
            let mut current = RECT::default();
            if GetWindowRect(self.overlay, &mut current) != 0
                && (current.left != x || current.top != y)
            {
                SetWindowPos(
                    self.overlay,
                    null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
                );
            }
            let button = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
                VK_RBUTTON
            } else {
                VK_LBUTTON
            };
            if GetAsyncKeyState(i32::from(button)) >= 0 {
                self.stop_drag(true);
            }
        }
    }

    fn stop_drag(&self, save: bool) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let mut current = RECT::default();
        unsafe { KillTimer(self.hwnd, DRAG_TIMER) };
        let moved = unsafe { GetWindowRect(self.overlay, &mut current) } != 0
            && (current.left != drag.window_start.left || current.top != drag.window_start.top);
        if save && moved {
            overlay_recovery::finish_drag(&self.app, self.overlay);
        } else {
            overlay_recovery::cancel_drag(&self.app);
        }
        if save {
            self.refresh_hover();
        }
    }
}

fn window_scale(hwnd: HWND) -> f64 {
    f64::from(unsafe { GetDpiForWindow(hwnd) }) / 96.0
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let Some(input) = input().filter(|input| input.hwnd == hwnd) else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };
    let mouse_target = || {
        let x = i32::from(lparam as i16);
        let y = i32::from((lparam >> 16) as i16);
        input.target_at(x, y)
    };
    match message {
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_MOUSEMOVE => {
            input.track_mouse();
            if input.drag.get().is_none() {
                input.set_hover(mouse_target());
            }
        }
        WM_MOUSELEAVE => {
            input.tracking_mouse.set(false);
            if input.drag.get().is_none() {
                input.set_hover(None);
            }
        }
        WM_SETCURSOR => {
            let cursor = match input.target_under_cursor() {
                Some(Target::Working | Target::Completed) => IDC_SIZEALL,
                _ => IDC_ARROW,
            };
            SetCursor(LoadCursorW(null_mut(), cursor));
            return 1;
        }
        WM_LBUTTONDOWN => {
            if matches!(mouse_target(), Some(Target::Working | Target::Completed)) {
                input.start_drag();
            }
        }
        WM_LBUTTONUP => input.update_drag(),
        WM_TIMER if wparam == DRAG_TIMER => input.update_drag(),
        WM_CANCELMODE => input.stop_drag(false),
        WM_MOUSEWHEEL => {
            if input.target_under_cursor() == Some(Target::Details) {
                let u = ((wparam >> 16) as i16) as f64;
                let _ = input.app.emit_to(
                    "overlay",
                    "overlay-region-scroll",
                    serde_json::json!({"deltaY": -u * 0.4}),
                );
            }
        }
        WM_NCDESTROY => {
            input.stop_drag(false);
            INPUT.with(|state| state.replace(None));
        }
        _ => {}
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}
