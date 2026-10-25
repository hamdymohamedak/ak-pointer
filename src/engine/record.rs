use std::os::raw::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::time::{Duration, Instant};

use crate::model::{Action, EventType, InputEvent, MouseButton, ScrollUnit, TargetWindow, KEY_F8};
use crate::platform::ffi::*;
use crate::platform::{self as window, WindowInfo};

#[derive(Clone)]
pub struct AllowedWindow {
    pub owner: String,
    pub title: String,
}

pub enum RecordStatus {
    Ok,
    NoAccessibility,
    NoWindow,
}

pub struct RecordResult {
    pub status: RecordStatus,
    pub action: Action,
}

static RECORD_LOOP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static STOP_RECORDING: AtomicBool = AtomicBool::new(false);

pub fn prepare_recording() {
    STOP_RECORDING.store(false, Ordering::SeqCst);
}

pub fn stop_recording() {
    STOP_RECORDING.store(true, Ordering::SeqCst);
    let run_loop = RECORD_LOOP.load(Ordering::SeqCst);
    if !run_loop.is_null() {
        unsafe { CFRunLoopStop(run_loop) };
    }
}

extern "C" fn stop_timer(_timer: CFRunLoopTimerRef, _info: *mut c_void) {
    if STOP_RECORDING.load(Ordering::Relaxed) {
        unsafe { CFRunLoopStop(CFRunLoopGetCurrent()) };
    }
}

struct Recorder {
    tap: CFMachPortRef,
    self_pid: i32,
    allowed: Vec<AllowedWindow>,
    buttons: u32,
    target_index: usize,
    originals: Vec<WindowInfo>,
    locked: bool,
    original: WindowInfo,
    last_seen: WindowInfo,
    action: Action,
    origin: Instant,
    have_motion: bool,
    last_motion: Instant,
    last_motion_x: f64,
    last_motion_y: f64,
    bounds_at: Instant,
}

impl Recorder {
    fn new() -> Self {
        let blank = WindowInfo {
            id: 0,
            pid: 0,
            layer: 0,
            owner: String::new(),
            title: String::new(),
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
        let now = Instant::now();
        Self {
            tap: std::ptr::null_mut(),
            self_pid: 0,
            allowed: Vec::new(),
            buttons: 0,
            target_index: 0,
            originals: Vec::new(),
            locked: false,
            original: blank.clone(),
            last_seen: blank,
            action: Action::new(),
            origin: now,
            have_motion: false,
            last_motion: now,
            last_motion_x: 0.0,
            last_motion_y: 0.0,
            bounds_at: now,
        }
    }

    fn run(&mut self) -> RecordResult {
        if !window::ensure_input_permissions() {
            return RecordResult {
                status: RecordStatus::NoAccessibility,
                action: Action::new(),
            };
        }
        self.self_pid = std::process::id() as i32;
        if !self.prelock() {
            return RecordResult {
                status: RecordStatus::NoWindow,
                action: Action::new(),
            };
        }

        let watched = [
            K_CG_EVENT_MOUSE_MOVED,
            K_CG_EVENT_LEFT_MOUSE_DOWN,
            K_CG_EVENT_LEFT_MOUSE_UP,
            K_CG_EVENT_LEFT_MOUSE_DRAGGED,
            K_CG_EVENT_RIGHT_MOUSE_DOWN,
            K_CG_EVENT_RIGHT_MOUSE_UP,
            K_CG_EVENT_RIGHT_MOUSE_DRAGGED,
            K_CG_EVENT_OTHER_MOUSE_DOWN,
            K_CG_EVENT_OTHER_MOUSE_UP,
            K_CG_EVENT_OTHER_MOUSE_DRAGGED,
            K_CG_EVENT_SCROLL_WHEEL,
            K_CG_EVENT_KEY_DOWN,
            K_CG_EVENT_KEY_UP,
        ];
        let mask = watched
            .iter()
            .fold(0u64, |mask, event_type| mask | event_mask_bit(*event_type));
        let user = self as *mut Recorder as *mut c_void;

        unsafe {
            let tap_ptr = CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                mask,
                record_callback,
                user,
            );
            let Some(tap) = Owned::from_raw(tap_ptr) else {
                window::print_accessibility_help();
                return RecordResult {
                    status: RecordStatus::NoAccessibility,
                    action: Action::new(),
                };
            };
            let Some(source) = Owned::from_raw(CFMachPortCreateRunLoopSource(
                kCFAllocatorDefault,
                tap.as_ptr(),
                0,
            )) else {
                println!("Could not start monitoring.");
                return RecordResult {
                    status: RecordStatus::NoAccessibility,
                    action: Action::new(),
                };
            };
            self.tap = tap.as_ptr();
            CFRunLoopAddSource(
                CFRunLoopGetCurrent(),
                source.as_ptr(),
                kCFRunLoopCommonModes,
            );
            CGEventTapEnable(self.tap, true);
            let mut timer_context = CFRunLoopTimerContext {
                version: 0,
                info: std::ptr::null_mut(),
                retain: std::ptr::null(),
                release: std::ptr::null(),
                copy_description: std::ptr::null(),
            };
            let timer = Owned::from_raw(CFRunLoopTimerCreate(
                kCFAllocatorDefault,
                CFAbsoluteTimeGetCurrent() + 0.2,
                0.2,
                0,
                0,
                stop_timer,
                &mut timer_context,
            ));
            if let Some(timer) = &timer {
                CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer.as_ptr(), kCFRunLoopCommonModes);
            }
            self.origin = Instant::now();
            RECORD_LOOP.store(CFRunLoopGetCurrent(), Ordering::SeqCst);
            if STOP_RECORDING.load(Ordering::SeqCst) {
                CFRunLoopStop(CFRunLoopGetCurrent());
            }
            CFRunLoopRun();
            RECORD_LOOP.store(std::ptr::null_mut(), Ordering::SeqCst);
            if let Some(timer) = &timer {
                CFRunLoopRemoveTimer(CFRunLoopGetCurrent(), timer.as_ptr(), kCFRunLoopCommonModes);
            }
            CGEventTapEnable(self.tap, false);
            CFRunLoopRemoveSource(
                CFRunLoopGetCurrent(),
                source.as_ptr(),
                kCFRunLoopCommonModes,
            );
            self.tap = std::ptr::null_mut();
        }

        if !self.locked {
            return RecordResult {
                status: RecordStatus::NoWindow,
                action: Action::new(),
            };
        }
        println!(
            "Recording stopped with {} events.",
            self.action.events.len()
        );
        if let Some(start) = self.action.events.iter().map(|event| event.t_ms).min() {
            for event in &mut self.action.events {
                event.t_ms -= start;
            }
        }
        RecordResult {
            status: RecordStatus::Ok,
            action: std::mem::take(&mut self.action),
        }
    }

    fn handle(&mut self, event_type: u32, event: CGEventRef) {
        if STOP_RECORDING.load(Ordering::Relaxed) {
            unsafe { CFRunLoopStop(CFRunLoopGetCurrent()) };
            return;
        }
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            if !self.tap.is_null() {
                unsafe { CGEventTapEnable(self.tap, true) };
            }
            return;
        }
        if event.is_null() {
            return;
        }
        if event_type == K_CG_EVENT_KEY_DOWN || event_type == K_CG_EVENT_KEY_UP {
            let code =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) } as u16;
            if code == KEY_F8 {
                if event_type == K_CG_EVENT_KEY_DOWN {
                    STOP_RECORDING.store(true, Ordering::SeqCst);
                    unsafe { CFRunLoopStop(CFRunLoopGetCurrent()) };
                }
                return;
            }
            let Some(front) = window::front_window() else {
                return;
            };
            let Some(target) = self.same_app_target(&front) else {
                return;
            };
            self.adopt(target);
            self.record(event_type, event);
            return;
        }

        let point = unsafe { CGEventGetLocation(event) };
        self.refresh_locked_bounds();
        let inside = self.locked && contains(&self.last_seen, point.x, point.y);
        if is_mouse_down(event_type) {
            if !inside {
                let Some(window) = self.target_at(point.x, point.y) else {
                    return;
                };
                self.adopt(window);
            }
            self.buttons += 1;
        } else if is_mouse_up(event_type) {
            if self.buttons == 0 || !self.locked {
                return;
            }
            self.buttons -= 1;
        } else if is_drag(event_type) {
            if self.buttons == 0 || !self.locked {
                return;
            }
        } else if !inside {
            let Some(window) = self.target_at(point.x, point.y) else {
                return;
            };
            self.adopt(window);
        }
        if self.locked {
            self.record(event_type, event);
        }
    }

    fn prelock(&mut self) -> bool {
        let mut primary = None;
        for chosen in self.allowed.iter().rev() {
            if let Some(window) = window::find_app_window(&chosen.owner, &chosen.title) {
                window::raise_window(&window);
                primary = Some(window);
            }
        }
        let Some(window) = primary else {
            return false;
        };
        self.adopt(window);
        true
    }

    fn refresh_locked_bounds(&mut self) {
        if !self.locked || self.bounds_at.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.bounds_at = Instant::now();
        if let Some(live) = window::window_by_id(self.original.id) {
            self.last_seen = live;
        }
    }

    fn is_allowed(&self, window: &WindowInfo) -> bool {
        if window.id != 0 && self.originals.iter().any(|known| known.id == window.id) {
            return true;
        }
        self.allowed
            .iter()
            .any(|item| item.owner == window.owner && title_matches(&item.title, &window.title))
    }

    fn same_app_target(&self, front: &WindowInfo) -> Option<WindowInfo> {
        if self.is_allowed(front) {
            return Some(front.clone());
        }
        window::list_windows()
            .into_iter()
            .find(|window| self.is_allowed(window) && window.pid != 0 && window.pid == front.pid)
    }

    fn target_at(&self, x: f64, y: f64) -> Option<WindowInfo> {
        let windows = window::list_windows();
        let front = windows.iter().find(|window| {
            window.layer == 0 && window.pid != self.self_pid && contains(window, x, y)
        })?;
        if self.is_allowed(front) {
            return Some(front.clone());
        }
        if self.same_app_target(front).is_none() {
            return None;
        }
        windows
            .iter()
            .find(|window| self.is_allowed(window) && contains(window, x, y))
            .cloned()
            .or_else(|| self.same_app_target(front))
    }

    fn adopt(&mut self, live: WindowInfo) {
        if let Some(index) = self.originals.iter().position(|window| {
            window.id == live.id || (window.owner == live.owner && window.title == live.title)
        }) {
            self.target_index = index;
            self.original = self.originals[index].clone();
            self.last_seen = live;
            self.locked = true;
            return;
        }
        self.originals.push(live.clone());
        self.target_index = self.originals.len() - 1;
        self.action.targets.push(TargetWindow {
            owner: live.owner.clone(),
            title: live.title.clone(),
            w: live.w,
            h: live.h,
        });
        if self.target_index == 0 {
            self.action.window_owner = live.owner.clone();
            self.action.window_title = live.title.clone();
            self.action.window_w = live.w;
            self.action.window_h = live.h;
        }
        self.original = live.clone();
        self.last_seen = live;
        self.locked = true;
        print!("Tracking window: {}", self.original.owner);
        if !self.original.title.is_empty() {
            print!(" — {}", self.original.title);
        }
        println!();
    }

    fn record(&mut self, event_type: u32, event: CGEventRef) {
        let point = unsafe { CGEventGetLocation(event) };
        let motion = matches!(
            event_type,
            K_CG_EVENT_MOUSE_MOVED
                | K_CG_EVENT_LEFT_MOUSE_DRAGGED
                | K_CG_EVENT_RIGHT_MOUSE_DRAGGED
                | K_CG_EVENT_OTHER_MOUSE_DRAGGED
        );
        if motion && !self.keep_motion(point.x, point.y) {
            return;
        }
        let relative = window::to_relative(&self.original, &self.last_seen, point.x, point.y);
        let mut stored = InputEvent::new(EventType::Move);
        stored.t_ms = self.origin.elapsed().as_millis() as i64;
        stored.x = relative.x;
        stored.y = relative.y;
        stored.target = self.target_index as u16;

        match event_type {
            K_CG_EVENT_MOUSE_MOVED
            | K_CG_EVENT_LEFT_MOUSE_DRAGGED
            | K_CG_EVENT_RIGHT_MOUSE_DRAGGED
            | K_CG_EVENT_OTHER_MOUSE_DRAGGED => {
                stored.kind = if event_type == K_CG_EVENT_MOUSE_MOVED {
                    EventType::Move
                } else {
                    EventType::Drag
                };
                stored.button = button_for(event_type);
            }
            K_CG_EVENT_LEFT_MOUSE_DOWN
            | K_CG_EVENT_RIGHT_MOUSE_DOWN
            | K_CG_EVENT_OTHER_MOUSE_DOWN
            | K_CG_EVENT_LEFT_MOUSE_UP
            | K_CG_EVENT_RIGHT_MOUSE_UP
            | K_CG_EVENT_OTHER_MOUSE_UP => {
                stored.kind = if is_mouse_down(event_type) {
                    EventType::Down
                } else {
                    EventType::Up
                };
                stored.button = button_for(event_type);
                let clicks =
                    unsafe { CGEventGetIntegerValueField(event, K_CG_MOUSE_EVENT_CLICK_STATE) }
                        as i32;
                stored.click_count = if clicks > 0 { clicks } else { 1 };
                self.note_motion(point.x, point.y);
            }
            K_CG_EVENT_SCROLL_WHEEL => {
                let point_x = unsafe {
                    CGEventGetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2)
                } as i32;
                let point_y = unsafe {
                    CGEventGetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1)
                } as i32;
                let line_x = unsafe {
                    CGEventGetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_2)
                } as i32;
                let line_y = unsafe {
                    CGEventGetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1)
                } as i32;
                stored.kind = EventType::Scroll;
                if point_x != 0 || point_y != 0 {
                    stored.scroll_x = point_x;
                    stored.scroll_y = point_y;
                    stored.scroll_unit = ScrollUnit::Pixel;
                } else if line_x != 0 || line_y != 0 {
                    stored.scroll_x = line_x;
                    stored.scroll_y = line_y;
                    stored.scroll_unit = ScrollUnit::Line;
                } else {
                    return;
                }
            }
            K_CG_EVENT_KEY_DOWN | K_CG_EVENT_KEY_UP => {
                stored.kind = EventType::Key;
                stored.key_down = event_type == K_CG_EVENT_KEY_DOWN;
                stored.keycode =
                    unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) }
                        as u16;
                stored.flags = unsafe { CGEventGetFlags(event) };
            }
            _ => return,
        }
        self.action.events.push(stored);
    }

    fn keep_motion(&mut self, x: f64, y: f64) -> bool {
        if !self.have_motion {
            self.note_motion(x, y);
            return true;
        }
        let dx = x - self.last_motion_x;
        let dy = y - self.last_motion_y;
        let ms = self.last_motion.elapsed().as_millis();
        if ms < 4 && dx * dx + dy * dy < 1.0 {
            return false;
        }
        self.note_motion(x, y);
        true
    }

    fn note_motion(&mut self, x: f64, y: f64) {
        self.have_motion = true;
        self.last_motion = Instant::now();
        self.last_motion_x = x;
        self.last_motion_y = y;
    }
}

fn title_matches(wanted: &str, actual: &str) -> bool {
    if wanted.is_empty() || wanted == actual {
        return true;
    }
    if actual.is_empty() {
        return false;
    }
    actual.starts_with(wanted) || wanted.starts_with(actual)
}

fn contains(window: &WindowInfo, x: f64, y: f64) -> bool {
    window.w >= 1.0
        && window.h >= 1.0
        && x >= window.x
        && y >= window.y
        && x < window.x + window.w
        && y < window.y + window.h
}

fn is_mouse_up(event_type: u32) -> bool {
    matches!(
        event_type,
        K_CG_EVENT_LEFT_MOUSE_UP | K_CG_EVENT_RIGHT_MOUSE_UP | K_CG_EVENT_OTHER_MOUSE_UP
    )
}

fn is_drag(event_type: u32) -> bool {
    matches!(
        event_type,
        K_CG_EVENT_LEFT_MOUSE_DRAGGED
            | K_CG_EVENT_RIGHT_MOUSE_DRAGGED
            | K_CG_EVENT_OTHER_MOUSE_DRAGGED
    )
}

fn is_mouse_down(event_type: u32) -> bool {
    matches!(
        event_type,
        K_CG_EVENT_LEFT_MOUSE_DOWN | K_CG_EVENT_RIGHT_MOUSE_DOWN | K_CG_EVENT_OTHER_MOUSE_DOWN
    )
}

fn button_for(event_type: u32) -> MouseButton {
    match event_type {
        K_CG_EVENT_RIGHT_MOUSE_DOWN
        | K_CG_EVENT_RIGHT_MOUSE_UP
        | K_CG_EVENT_RIGHT_MOUSE_DRAGGED => MouseButton::Right,
        K_CG_EVENT_OTHER_MOUSE_DOWN
        | K_CG_EVENT_OTHER_MOUSE_UP
        | K_CG_EVENT_OTHER_MOUSE_DRAGGED => MouseButton::Middle,
        _ => MouseButton::Left,
    }
}

extern "C" fn record_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user: *mut c_void,
) -> CGEventRef {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if user.is_null() {
            return;
        }
        unsafe { &mut *(user as *mut Recorder) }.handle(event_type, event);
    }));
    event
}

pub fn record_action(allowed: Vec<AllowedWindow>) -> RecordResult {
    let mut recorder = Recorder::new();
    recorder.allowed = allowed;
    recorder.run()
}
