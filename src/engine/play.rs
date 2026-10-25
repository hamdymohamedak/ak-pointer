use std::os::raw::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::model::{
    Action, EventType, InputEvent, MouseButton, ScrollUnit, KEY_ESCAPE, REPLAY_STAMP,
};
use crate::platform::ffi::*;
use crate::platform::{self as window, WindowInfo};

pub enum PlayStatus {
    Done,
    Cancelled,
    WindowNotFound,
    NoAccessibility,
}

struct SendPtr(*mut WatchState);
unsafe impl Send for SendPtr {}

struct WatchState {
    cancel: AtomicBool,
    stop: AtomicBool,
    ready: AtomicBool,
    active: AtomicBool,
    tap: AtomicPtr<c_void>,
}

struct EscapeWatcher {
    state: Box<WatchState>,
    thread: Option<JoinHandle<()>>,
}

impl EscapeWatcher {
    fn start() -> Self {
        let state = Box::new(WatchState {
            cancel: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            active: AtomicBool::new(false),
            tap: AtomicPtr::new(std::ptr::null_mut()),
        });
        let ptr = SendPtr(&*state as *const WatchState as *mut WatchState);
        let thread = thread::spawn(move || watcher_thread(ptr));
        let watcher = Self {
            state,
            thread: Some(thread),
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while !watcher.state.ready.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        watcher
    }

    fn cancelled(&self) -> bool {
        self.state.cancel.load(Ordering::SeqCst)
    }

    fn active(&self) -> bool {
        self.state.active.load(Ordering::SeqCst)
    }
}

impl Drop for EscapeWatcher {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn watcher_thread(ptr: SendPtr) {
    unsafe {
        let state = &mut *ptr.0;
        let tap = CGEventTapCreate(
            K_CG_SESSION_EVENT_TAP,
            K_CG_HEAD_INSERT_EVENT_TAP,
            K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
            event_mask_bit(K_CG_EVENT_KEY_DOWN),
            escape_callback,
            state as *mut WatchState as *mut c_void,
        );
        if tap.is_null() {
            state.ready.store(true, Ordering::SeqCst);
            return;
        }
        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
        if source.is_null() {
            CFRelease(tap as CFTypeRef);
            state.ready.store(true, Ordering::SeqCst);
            return;
        }
        state.tap.store(tap, Ordering::SeqCst);
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        state.active.store(true, Ordering::SeqCst);
        state.ready.store(true, Ordering::SeqCst);
        while !state.stop.load(Ordering::SeqCst) {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, 1u8);
        }
        CGEventTapEnable(tap, false);
        CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
        CFRelease(source as CFTypeRef);
        CFRelease(tap as CFTypeRef);
        state.tap.store(std::ptr::null_mut(), Ordering::SeqCst);
    }
}

extern "C" fn escape_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user: *mut c_void,
) -> CGEventRef {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if user.is_null() {
            return;
        }
        let state = unsafe { &*(user as *const WatchState) };
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            let tap = state.tap.load(Ordering::SeqCst);
            if !tap.is_null() {
                unsafe { CGEventTapEnable(tap, true) };
            }
            return;
        }
        if event.is_null() || event_type != K_CG_EVENT_KEY_DOWN {
            return;
        }
        unsafe {
            if CGEventGetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA) == REPLAY_STAMP {
                return;
            }
            let code = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;
            if code == KEY_ESCAPE {
                state.cancel.store(true, Ordering::SeqCst);
            }
        }
    }));
    event
}

fn wait_until(target: Instant, watcher: &EscapeWatcher, cancel: &AtomicBool) -> bool {
    while !watcher.cancelled() && !cancel.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= target {
            return true;
        }
        let remaining = target.saturating_duration_since(now);
        if remaining > Duration::from_millis(30) {
            thread::sleep(Duration::from_millis(30));
        } else {
            thread::sleep(remaining);
        }
    }
    false
}

fn cg_button(button: MouseButton) -> u32 {
    match button {
        MouseButton::Right => K_CG_MOUSE_BUTTON_RIGHT,
        MouseButton::Middle => K_CG_MOUSE_BUTTON_CENTER,
        MouseButton::Left => K_CG_MOUSE_BUTTON_LEFT,
    }
}

fn cg_mouse_type(event: &InputEvent) -> u32 {
    let right = event.button == MouseButton::Right;
    let middle = event.button == MouseButton::Middle;
    match event.kind {
        EventType::Down => {
            if right {
                K_CG_EVENT_RIGHT_MOUSE_DOWN
            } else if middle {
                K_CG_EVENT_OTHER_MOUSE_DOWN
            } else {
                K_CG_EVENT_LEFT_MOUSE_DOWN
            }
        }
        EventType::Up => {
            if right {
                K_CG_EVENT_RIGHT_MOUSE_UP
            } else if middle {
                K_CG_EVENT_OTHER_MOUSE_UP
            } else {
                K_CG_EVENT_LEFT_MOUSE_UP
            }
        }
        EventType::Drag => {
            if right {
                K_CG_EVENT_RIGHT_MOUSE_DRAGGED
            } else if middle {
                K_CG_EVENT_OTHER_MOUSE_DRAGGED
            } else {
                K_CG_EVENT_LEFT_MOUSE_DRAGGED
            }
        }
        EventType::Move | EventType::Scroll | EventType::Key => K_CG_EVENT_MOUSE_MOVED,
    }
}

unsafe fn stamp(event: CGEventRef) {
    CGEventSetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA, REPLAY_STAMP);
}

unsafe fn post_mouse(
    source: CGEventSourceRef,
    event_type: u32,
    point: CGPoint,
    button: u32,
    click_count: i32,
    last: &mut CGPoint,
) {
    let Some(event) = Owned::from_raw(CGEventCreateMouseEvent(source, event_type, point, button))
    else {
        return;
    };
    let event = event.as_ptr();
    stamp(event);
    if event_type != K_CG_EVENT_MOUSE_MOVED {
        CGEventSetIntegerValueField(event, K_CG_MOUSE_EVENT_CLICK_STATE, i64::from(click_count));
    }
    CGEventSetIntegerValueField(
        event,
        K_CG_MOUSE_EVENT_DELTA_X,
        (point.x - last.x).round() as i64,
    );
    CGEventSetIntegerValueField(
        event,
        K_CG_MOUSE_EVENT_DELTA_Y,
        (point.y - last.y).round() as i64,
    );
    *last = point;
    CGEventPost(K_CG_HID_EVENT_TAP, event);
}

pub fn play_action(action: &Action, cancel: &AtomicBool, countdown_secs: u64) -> PlayStatus {
    if action.events.is_empty() {
        println!("The action is empty.");
        return PlayStatus::Done;
    }
    let Some(mut window) = window::find_window(
        &action.window_owner,
        &action.window_title,
        action.window_w,
        action.window_h,
    ) else {
        print!("Could not find the window");
        if !action.window_title.is_empty() {
            print!(" \"{}\"", action.window_title);
        }
        println!(" owned by {}.", action.window_owner);
        println!("Open that app and try again. It can be in the background.");
        return PlayStatus::WindowNotFound;
    };
    if !window::ensure_input_permissions() {
        return PlayStatus::NoAccessibility;
    }

    print!("Playing on: {}", window.owner);
    if !window.title.is_empty() {
        print!(" / {}", window.title);
    }
    println!();
    if window::size_changed(action.window_w, action.window_h, &window) {
        println!(
            "Window size changed from {}x{} to {}x{}. Coordinates will be scaled.",
            action.window_w.round() as i64,
            action.window_h.round() as i64,
            window.w.round() as i64,
            window.h.round() as i64
        );
    }

    let watcher = EscapeWatcher::start();
    if !watcher.active() {
        println!("Esc cannot cancel this run. Playback will continue.");
    }
    if !window::raise_window(&window) {
        println!("Could not raise the window. Keystrokes may not reach it.");
    }

    if countdown_secs > 0 {
        println!("Starting in {countdown_secs} seconds. Press Esc or Stop to cancel.");
        for second in (1..=countdown_secs).rev() {
            println!("{second}");
            if cancel.load(Ordering::Relaxed)
                || !wait_until(Instant::now() + Duration::from_secs(1), &watcher, cancel)
            {
                return PlayStatus::Cancelled;
            }
        }
    }

    if let Some(again) = window::window_by_id(window.id) {
        window = again;
    }
    window::raise_window(&window);

    unsafe {
        let Some(source) = Owned::from_raw(CGEventSourceCreate(
            K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
        )) else {
            println!("Could not send events.");
            return PlayStatus::NoAccessibility;
        };
        CGEventSourceSetLocalEventsSuppressionInterval(source.as_ptr(), 0.0);
        CGEventSourceSetUserData(source.as_ptr(), REPLAY_STAMP);

        let mut last = CGPoint { x: 0.0, y: 0.0 };
        if let Some(cursor) = Owned::from_raw(CGEventCreate(std::ptr::null_mut())) {
            last = CGEventGetLocation(cursor.as_ptr());
        }

        let start = Instant::now();
        let first_ms = action
            .events
            .iter()
            .map(|event| event.t_ms)
            .min()
            .unwrap_or(0);
        let mut windows: Vec<Option<WindowInfo>> = Vec::new();
        let mut current_index = usize::MAX;
        for event in &action.events {
            let at = (event.t_ms - first_ms).max(0) as u64;
            if !wait_until(start + Duration::from_millis(at), &watcher, cancel) {
                return PlayStatus::Cancelled;
            }
            let index = event.target as usize;
            if current_index != usize::MAX && index != current_index {
                if cancel.load(Ordering::Relaxed)
                    || !wait_until(
                        Instant::now() + Duration::from_millis(200),
                        &watcher,
                        cancel,
                    )
                {
                    return PlayStatus::Cancelled;
                }
            }
            if windows.len() <= index {
                windows.resize(index + 1, None);
            }
            if windows[index].is_none() || index != current_index {
                let spec = action.target_window(index);
                let Some(found) = window::find_window(&spec.owner, &spec.title, spec.w, spec.h)
                else {
                    print!("Could not find the window");
                    if !spec.title.is_empty() {
                        print!(" \"{}\"", spec.title);
                    }
                    println!(" owned by {}.", spec.owner);
                    return PlayStatus::WindowNotFound;
                };
                window::raise_window(&found);
                windows[index] = Some(found);
                current_index = index;
            }
            let spec = action.target_window(index);
            let Some(stored) = windows[index].clone() else {
                return PlayStatus::WindowNotFound;
            };
            let current = window::window_by_id(stored.id).unwrap_or(stored);
            windows[index] = Some(current.clone());
            let screen = window::to_screen(spec.w, spec.h, &current, event.x, event.y);
            let point = CGPoint {
                x: screen.x,
                y: screen.y,
            };
            if event.kind == EventType::Key {
                if let Some(key) = Owned::from_raw(CGEventCreateKeyboardEvent(
                    source.as_ptr(),
                    event.keycode,
                    event.key_down,
                )) {
                    CGEventSetFlags(key.as_ptr(), event.flags);
                    stamp(key.as_ptr());
                    CGEventPost(K_CG_HID_EVENT_TAP, key.as_ptr());
                }
                continue;
            }
            if event.kind == EventType::Scroll {
                if (last.x - point.x).abs() > 0.5 || (last.y - point.y).abs() > 0.5 {
                    post_mouse(
                        source.as_ptr(),
                        K_CG_EVENT_MOUSE_MOVED,
                        point,
                        K_CG_MOUSE_BUTTON_LEFT,
                        0,
                        &mut last,
                    );
                }
                let unit = if event.scroll_unit == ScrollUnit::Pixel {
                    K_CG_SCROLL_EVENT_UNIT_PIXEL
                } else {
                    K_CG_SCROLL_EVENT_UNIT_LINE
                };
                if let Some(scroll) = Owned::from_raw(CGEventCreateScrollWheelEvent(
                    source.as_ptr(),
                    unit,
                    2,
                    event.scroll_y,
                    event.scroll_x,
                )) {
                    CGEventSetLocation(scroll.as_ptr(), point);
                    stamp(scroll.as_ptr());
                    CGEventPost(K_CG_HID_EVENT_TAP, scroll.as_ptr());
                }
                continue;
            }
            post_mouse(
                source.as_ptr(),
                cg_mouse_type(event),
                point,
                cg_button(event.button),
                event.click_count,
                &mut last,
            );
        }
    }
    if watcher.cancelled() {
        PlayStatus::Cancelled
    } else {
        PlayStatus::Done
    }
}
