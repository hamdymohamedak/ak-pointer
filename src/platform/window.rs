use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use super::ffi::*;

#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub id: u32,
    pub pid: i32,
    pub layer: i32,
    pub owner: String,
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ScreenPoint {
    pub x: f64,
    pub y: f64,
}

fn is_ignored_owner(owner: &str) -> bool {
    matches!(
        owner,
        "Dock"
            | "Window Server"
            | "SystemUIServer"
            | "Control Center"
            | "Notification Center"
            | "Spotlight"
    )
}

unsafe fn dict_string(dict: CFDictionaryRef, key: CFStringRef) -> String {
    let value = CFDictionaryGetValue(dict, key as *const c_void);
    if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
        return String::new();
    }
    cf_string(value)
}

unsafe fn dict_i64(dict: CFDictionaryRef, key: CFStringRef) -> i64 {
    let value = CFDictionaryGetValue(dict, key as *const c_void);
    if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
        return 0;
    }
    let mut out = 0i64;
    CFNumberGetValue(
        value,
        K_CF_NUMBER_SINT64,
        &mut out as *mut i64 as *mut c_void,
    );
    out
}

unsafe fn dict_f64(dict: CFDictionaryRef, key: CFStringRef, fallback: f64) -> f64 {
    let value = CFDictionaryGetValue(dict, key as *const c_void);
    if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
        return fallback;
    }
    let mut out = fallback;
    CFNumberGetValue(
        value,
        K_CF_NUMBER_DOUBLE,
        &mut out as *mut f64 as *mut c_void,
    );
    out
}

unsafe fn parse_window(dict: CFDictionaryRef) -> Option<WindowInfo> {
    if dict.is_null() {
        return None;
    }
    let bounds = CFDictionaryGetValue(dict, kCGWindowBounds as *const c_void);
    if bounds.is_null() || CFGetTypeID(bounds) != CFDictionaryGetTypeID() {
        return None;
    }
    let mut rect = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 0.0,
            height: 0.0,
        },
    };
    if !CGRectMakeWithDictionaryRepresentation(bounds, &mut rect) {
        return None;
    }
    if dict_f64(dict, kCGWindowAlpha, 1.0) <= 0.01 {
        return None;
    }
    Some(WindowInfo {
        id: dict_i64(dict, kCGWindowNumber) as u32,
        pid: dict_i64(dict, kCGWindowOwnerPID) as i32,
        layer: dict_i64(dict, kCGWindowLayer) as i32,
        owner: dict_string(dict, kCGWindowOwnerName),
        title: dict_string(dict, kCGWindowName),
        x: rect.origin.x,
        y: rect.origin.y,
        w: rect.size.width,
        h: rect.size.height,
    })
}

unsafe fn ax_string(element: AXUIElementRef, attribute: CFStringRef) -> String {
    let mut raw: CFTypeRef = std::ptr::null();
    if AXUIElementCopyAttributeValue(element, attribute, &mut raw) != K_AX_ERROR_SUCCESS
        || raw.is_null()
    {
        return String::new();
    }
    let out = if CFGetTypeID(raw) == CFStringGetTypeID() {
        cf_string(raw)
    } else {
        String::new()
    };
    CFRelease(raw);
    out
}

unsafe fn cf_str(text: &CStr) -> CFStringRef {
    CFStringCreateWithCString(
        kCFAllocatorDefault,
        text.as_ptr(),
        K_CF_STRING_ENCODING_UTF8,
    )
}

unsafe fn ax_point(element: AXUIElementRef, position: CFStringRef) -> CGPoint {
    let mut point = CGPoint { x: 0.0, y: 0.0 };
    let mut raw: CFTypeRef = std::ptr::null();
    if AXUIElementCopyAttributeValue(element, position, &mut raw) != K_AX_ERROR_SUCCESS
        || raw.is_null()
    {
        return point;
    }
    if CFGetTypeID(raw) == AXValueGetTypeID() {
        AXValueGetValue(
            raw,
            K_AX_VALUE_CG_POINT_TYPE,
            &mut point as *mut CGPoint as *mut c_void,
        );
    }
    CFRelease(raw);
    point
}

fn list_with(options: u32) -> Vec<WindowInfo> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(options, K_CG_NULL_WINDOW_ID);
        if list.is_null() {
            return Vec::new();
        }
        let count = CFArrayGetCount(list);
        let mut windows = Vec::with_capacity(count.max(0) as usize);
        for index in 0..count {
            let dict = CFArrayGetValueAtIndex(list, index);
            if let Some(info) = parse_window(dict) {
                windows.push(info);
            }
        }
        CFRelease(list);
        windows
    }
}

pub fn list_windows() -> Vec<WindowInfo> {
    list_with(K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS)
}

pub fn list_windows_all() -> Vec<WindowInfo> {
    list_with(K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS)
}

#[derive(Clone, Debug)]
pub struct AppChoice {
    pub owner: String,
    pub title: String,
    pub w: f64,
    pub h: f64,
    pub visible: bool,
}

fn usable_app_window(window: &WindowInfo, self_pid: i32) -> bool {
    window.layer == 0
        && window.w >= 40.0
        && window.h >= 40.0
        && window.pid != 0
        && window.pid != self_pid
        && !is_ignored_owner(&window.owner)
        && !window.owner.is_empty()
}

pub fn list_app_choices() -> Vec<AppChoice> {
    let self_pid = std::process::id() as i32;
    let mut best: HashMap<(String, String), AppChoice> = HashMap::new();
    for window in list_windows_all() {
        if !usable_app_window(&window, self_pid) {
            continue;
        }
        let key = (window.owner.clone(), window.title.clone());
        let area = window.w * window.h;
        if let Some(existing) = best.get(&key) {
            if existing.w * existing.h >= area {
                continue;
            }
        }
        best.insert(
            key,
            AppChoice {
                owner: window.owner,
                title: window.title,
                w: window.w,
                h: window.h,
                visible: false,
            },
        );
    }
    for window in list_windows() {
        if let Some(choice) = best.get_mut(&(window.owner.clone(), window.title.clone())) {
            choice.visible = true;
        }
    }
    let mut choices: Vec<_> = best.into_values().collect();
    choices.sort_by(|left, right| {
        left.owner
            .cmp(&right.owner)
            .then(left.title.cmp(&right.title))
    });
    choices
}

pub fn find_app_window(owner: &str, title: &str) -> Option<WindowInfo> {
    fn pick(windows: Vec<WindowInfo>, owner: &str, title: &str) -> Option<WindowInfo> {
        windows
            .into_iter()
            .filter(|window| {
                window.layer == 0
                    && window.w >= 40.0
                    && window.h >= 40.0
                    && window.owner == owner
                    && (title.is_empty() || window.title == title)
                    && !is_ignored_owner(&window.owner)
            })
            .max_by(|left, right| {
                (left.w * left.h)
                    .partial_cmp(&(right.w * right.h))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
    pick(list_windows(), owner, title).or_else(|| pick(list_windows_all(), owner, title))
}

pub fn front_window() -> Option<WindowInfo> {
    list_windows().into_iter().find(|window| {
        window.layer == 0
            && window.w >= 40.0
            && window.h >= 40.0
            && window.pid != 0
            && !is_ignored_owner(&window.owner)
    })
}

pub fn window_by_id(id: u32) -> Option<WindowInfo> {
    if id == 0 {
        return None;
    }
    unsafe {
        let wide = i64::from(id);
        let number = CFNumberCreate(
            kCFAllocatorDefault,
            K_CF_NUMBER_SINT64,
            &wide as *const i64 as *const c_void,
        );
        if number.is_null() {
            return None;
        }
        let values = [number as *const c_void];
        let ids = CFArrayCreate(
            kCFAllocatorDefault,
            values.as_ptr(),
            1,
            &kCFTypeArrayCallBacks,
        );
        let described = if ids.is_null() {
            std::ptr::null()
        } else {
            CGWindowListCreateDescriptionFromArray(ids)
        };
        let mut result = None;
        if !described.is_null() && CFArrayGetCount(described) > 0 {
            result = parse_window(CFArrayGetValueAtIndex(described, 0));
        }
        if !described.is_null() {
            CFRelease(described);
        }
        if !ids.is_null() {
            CFRelease(ids);
        }
        CFRelease(number);
        result.filter(|window| window.w >= 1.0)
    }
}

fn nearest_window(
    windows: Vec<WindowInfo>,
    owner: &str,
    title: &str,
    recorded_w: f64,
    recorded_h: f64,
) -> Option<WindowInfo> {
    let mut best: Option<WindowInfo> = None;
    let mut best_score = f64::MAX;
    for window in windows {
        if window.layer != 0 || window.owner != owner || window.title != title {
            continue;
        }
        let score = (window.w - recorded_w).abs() + (window.h - recorded_h).abs();
        if score < best_score {
            best_score = score;
            best = Some(window);
        }
    }
    best
}

pub fn find_window(
    owner: &str,
    title: &str,
    recorded_w: f64,
    recorded_h: f64,
) -> Option<WindowInfo> {
    nearest_window(list_windows(), owner, title, recorded_w, recorded_h)
        .or_else(|| nearest_window(list_windows_all(), owner, title, recorded_w, recorded_h))
}

pub fn raise_window(window: &WindowInfo) -> bool {
    unsafe {
        let app = AXUIElementCreateApplication(window.pid);
        if app.is_null() {
            return false;
        }
        let frontmost = cf_str(c"AXFrontmost");
        let hidden = cf_str(c"AXHidden");
        let minimized = cf_str(c"AXMinimized");
        let windows_attr = cf_str(c"AXWindows");
        let title_attr = cf_str(c"AXTitle");
        let position_attr = cf_str(c"AXPosition");
        let raise_action = cf_str(c"AXRaise");
        let main_attr = cf_str(c"AXMain");
        AXUIElementSetMessagingTimeout(app, 1.5);
        AXUIElementSetAttributeValue(app, hidden, kCFBooleanFalse);
        let front_err = AXUIElementSetAttributeValue(app, frontmost, kCFBooleanTrue);

        let mut raw: CFTypeRef = std::ptr::null();
        let mut best: AXUIElementRef = std::ptr::null_mut();
        let mut best_dist = f64::MAX;
        let mut title_matched = false;
        if AXUIElementCopyAttributeValue(app, windows_attr, &mut raw) == K_AX_ERROR_SUCCESS
            && !raw.is_null()
            && CFGetTypeID(raw) == CFArrayGetTypeID()
        {
            let windows = raw;
            let count = CFArrayGetCount(windows);
            for index in 0..count {
                let candidate = CFArrayGetValueAtIndex(windows, index) as AXUIElementRef;
                let title = ax_string(candidate, title_attr);
                let same_title = title == window.title;
                if !window.title.is_empty() && !same_title {
                    continue;
                }
                if window.title.is_empty() && !title.is_empty() {
                    continue;
                }
                let pos = ax_point(candidate, position_attr);
                let dist = (pos.x - window.x).powi(2) + (pos.y - window.y).powi(2);
                if dist < best_dist {
                    best = candidate;
                    best_dist = dist;
                    title_matched = true;
                }
            }
            if !title_matched {
                best = std::ptr::null_mut();
                best_dist = f64::MAX;
                for index in 0..count {
                    let candidate = CFArrayGetValueAtIndex(windows, index) as AXUIElementRef;
                    let pos = ax_point(candidate, position_attr);
                    let dist = (pos.x - window.x).powi(2) + (pos.y - window.y).powi(2);
                    if dist < best_dist {
                        best = candidate;
                        best_dist = dist;
                    }
                }
                if best_dist > 80.0 * 80.0 {
                    best = std::ptr::null_mut();
                }
            }
            if !best.is_null() {
                AXUIElementSetAttributeValue(best, minimized, kCFBooleanFalse);
                AXUIElementPerformAction(best, raise_action);
                AXUIElementSetAttributeValue(best, main_attr, kCFBooleanTrue);
            }
        }
        if !raw.is_null() {
            CFRelease(raw);
        }
        for value in [
            frontmost,
            hidden,
            minimized,
            windows_attr,
            title_attr,
            position_attr,
            raise_action,
            main_attr,
        ] {
            if !value.is_null() {
                CFRelease(value);
            }
        }
        CFRelease(app as CFTypeRef);
        front_err == K_AX_ERROR_SUCCESS
    }
}

extern "C" fn permission_probe_callback(
    _proxy: CGEventTapProxy,
    _event_type: u32,
    event: CGEventRef,
    _user: *mut c_void,
) -> CGEventRef {
    event
}

fn listen_tap_works() -> bool {
    unsafe {
        let mask = event_mask_bit(K_CG_EVENT_LEFT_MOUSE_DOWN) | event_mask_bit(K_CG_EVENT_KEY_DOWN);
        let tap = CGEventTapCreate(
            K_CG_SESSION_EVENT_TAP,
            K_CG_HEAD_INSERT_EVENT_TAP,
            K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
            mask,
            permission_probe_callback,
            std::ptr::null_mut(),
        );
        if tap.is_null() {
            return false;
        }
        CFRelease(tap as CFTypeRef);
        true
    }
}

fn warn_input_monitoring() {
    static REQUESTED: AtomicBool = AtomicBool::new(false);
    if !REQUESTED.swap(true, Ordering::Relaxed) {
        unsafe { CGRequestListenEventAccess() };
    }
    println!("Input Monitoring is off, so typed keys may not be recorded.");
    println!("System Settings → Privacy & Security → Input Monitoring");
    println!("Enable Cursor, then run this program again. Mouse clicks are still recorded.");
}

pub fn print_accessibility_help() {
    println!("Accessibility permission is required to watch the mouse and keyboard.");
    if let Ok(path) = std::env::current_exe() {
        println!("Enable this program:\n  {}", path.display());
    }
    println!("System Settings → Privacy & Security → Accessibility");
    println!("Also enable it under Input Monitoring if that list appears.");
    println!("Then quit and run the program again.");
}

pub fn keyboard_capture_ready() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

pub fn ensure_input_permissions() -> bool {
    unsafe {
        let trusted = AXIsProcessTrusted() != 0;
        let listen = CGPreflightListenEventAccess();
        if trusted || listen_tap_works() {
            if !listen {
                warn_input_monitoring();
            }
            return true;
        }
        if !trusted {
            let keys = [kAXTrustedCheckOptionPrompt as *const c_void];
            let values = [kCFBooleanTrue as *const c_void];
            let options = CFDictionaryCreate(
                kCFAllocatorDefault,
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            if !options.is_null() {
                AXIsProcessTrustedWithOptions(options);
                CFRelease(options);
            }
        }
        if !listen {
            CGRequestListenEventAccess();
        }
    }
    print_accessibility_help();
    false
}

pub fn to_relative(
    locked: &WindowInfo,
    current: &WindowInfo,
    screen_x: f64,
    screen_y: f64,
) -> ScreenPoint {
    if current.w < 1.0 || current.h < 1.0 || locked.w < 1.0 || locked.h < 1.0 {
        return ScreenPoint {
            x: screen_x - current.x,
            y: screen_y - current.y,
        };
    }
    ScreenPoint {
        x: (screen_x - current.x) * (locked.w / current.w),
        y: (screen_y - current.y) * (locked.h / current.h),
    }
}

pub fn to_screen(
    recorded_w: f64,
    recorded_h: f64,
    current: &WindowInfo,
    rel_x: f64,
    rel_y: f64,
) -> ScreenPoint {
    let scale_x = if recorded_w > 1.0 {
        current.w / recorded_w
    } else {
        1.0
    };
    let scale_y = if recorded_h > 1.0 {
        current.h / recorded_h
    } else {
        1.0
    };
    ScreenPoint {
        x: current.x + rel_x * scale_x,
        y: current.y + rel_y * scale_y,
    }
}

pub fn size_changed(recorded_w: f64, recorded_h: f64, current: &WindowInfo) -> bool {
    (current.w - recorded_w).abs() > 2.0 || (current.h - recorded_h).abs() > 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(x: f64, y: f64, w: f64, h: f64) -> WindowInfo {
        WindowInfo {
            id: 1,
            pid: 1,
            layer: 0,
            owner: String::new(),
            title: String::new(),
            x,
            y,
            w,
            h,
        }
    }

    #[test]
    fn converts_coordinates_when_the_window_moves_or_resizes() {
        let locked = window(0.0, 0.0, 400.0, 800.0);
        let mut current = window(300.0, 80.0, 400.0, 800.0);
        let relative = to_relative(&locked, &current, 420.0, 420.0);
        assert!((relative.x - 120.0).abs() < 0.01);
        assert!((relative.y - 340.0).abs() < 0.01);
        let screen = to_screen(400.0, 800.0, &current, relative.x, relative.y);
        assert!((screen.x - 420.0).abs() < 0.01);
        assert!((screen.y - 420.0).abs() < 0.01);

        current.w = 800.0;
        current.h = 1600.0;
        let scaled = to_relative(&locked, &current, 700.0, 880.0);
        assert!((scaled.x - 200.0).abs() < 0.01);
        assert!((scaled.y - 400.0).abs() < 0.01);

        let replay = window(10.0, 20.0, 200.0, 400.0);
        let back = to_screen(locked.w, locked.h, &replay, scaled.x, scaled.y);
        assert!((back.x - 110.0).abs() < 0.01);
        assert!((back.y - 220.0).abs() < 0.01);
        assert!(size_changed(400.0, 800.0, &replay));
        assert!(!size_changed(200.0, 400.0, &replay));
    }
}
