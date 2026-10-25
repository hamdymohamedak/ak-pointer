#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::os::raw::{c_char, c_void};

pub type CFIndex = isize;
pub type CFTypeID = usize;
pub type CGWindowID = u32;
pub type CGEventMask = u64;
pub type CGEventFlags = u64;
pub type CGKeyCode = u16;
pub type Boolean = u8;
pub type AXError = i32;

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFMachPortRef = *mut c_void;
pub type CFRunLoopRef = *mut c_void;
pub type CFRunLoopSourceRef = *mut c_void;
pub type CFRunLoopTimerRef = *mut c_void;
pub type AXUIElementRef = *mut c_void;
pub type AXValueRef = *const c_void;
pub type CGEventRef = *mut c_void;
pub type CGEventSourceRef = *mut c_void;
pub type CGEventTapProxy = *mut c_void;

pub type CGEventTapCallBack =
    extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef;

pub type CFRunLoopTimerCallBack = extern "C" fn(CFRunLoopTimerRef, *mut c_void);

#[repr(C)]
pub struct CFRunLoopTimerContext {
    pub version: isize,
    pub info: *mut c_void,
    pub retain: *const c_void,
    pub release: *const c_void,
    pub copy_description: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[repr(C)]
pub struct CFDictionaryKeyCallBacks {
    _private: [u8; 0],
}
#[repr(C)]
pub struct CFDictionaryValueCallBacks {
    _private: [u8; 0],
}
#[repr(C)]
pub struct CFArrayCallBacks {
    _private: [u8; 0],
}

pub const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
pub const K_CF_NUMBER_SINT64: i32 = 4;
pub const K_CF_NUMBER_DOUBLE: i32 = 13;

pub const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
pub const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
pub const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
pub const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
pub const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
pub const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
pub const K_CG_EVENT_RIGHT_MOUSE_DRAGGED: u32 = 7;
pub const K_CG_EVENT_KEY_DOWN: u32 = 10;
pub const K_CG_EVENT_KEY_UP: u32 = 11;
pub const K_CG_EVENT_SCROLL_WHEEL: u32 = 22;
pub const K_CG_EVENT_OTHER_MOUSE_DOWN: u32 = 25;
pub const K_CG_EVENT_OTHER_MOUSE_UP: u32 = 26;
pub const K_CG_EVENT_OTHER_MOUSE_DRAGGED: u32 = 27;
pub const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

pub const K_CG_HID_EVENT_TAP: u32 = 0;
pub const K_CG_SESSION_EVENT_TAP: u32 = 1;
pub const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
pub const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

pub const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
pub const K_CG_MOUSE_BUTTON_RIGHT: u32 = 1;
pub const K_CG_MOUSE_BUTTON_CENTER: u32 = 2;

pub const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;
pub const K_CG_SCROLL_EVENT_UNIT_PIXEL: u32 = 0;
pub const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;

pub const K_CG_MOUSE_EVENT_CLICK_STATE: u32 = 1;
pub const K_CG_MOUSE_EVENT_DELTA_X: u32 = 4;
pub const K_CG_MOUSE_EVENT_DELTA_Y: u32 = 5;
pub const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
pub const K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1: u32 = 11;
pub const K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_2: u32 = 12;
pub const K_CG_EVENT_SOURCE_USER_DATA: u32 = 42;
pub const K_CG_SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1: u32 = 96;
pub const K_CG_SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2: u32 = 97;

pub const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
pub const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
pub const K_CG_NULL_WINDOW_ID: CGWindowID = 0;

pub const K_AX_ERROR_SUCCESS: AXError = 0;
pub const K_AX_VALUE_CG_POINT_TYPE: u32 = 1;

pub const fn event_mask_bit(event_type: u32) -> CGEventMask {
    1u64 << event_type
}

extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;
    pub static kCFBooleanTrue: CFTypeRef;
    pub static kCFBooleanFalse: CFTypeRef;
    pub static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    pub static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
    pub static kCFTypeArrayCallBacks: CFArrayCallBacks;
    pub static kCFRunLoopCommonModes: CFStringRef;
    pub static kCFRunLoopDefaultMode: CFStringRef;

    pub static kCGWindowNumber: CFStringRef;
    pub static kCGWindowOwnerPID: CFStringRef;
    pub static kCGWindowOwnerName: CFStringRef;
    pub static kCGWindowName: CFStringRef;
    pub static kCGWindowBounds: CFStringRef;
    pub static kCGWindowLayer: CFStringRef;
    pub static kCGWindowAlpha: CFStringRef;

    pub static kAXTrustedCheckOptionPrompt: CFStringRef;

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    pub fn CFStringGetTypeID() -> CFTypeID;
    pub fn CFDictionaryGetTypeID() -> CFTypeID;
    pub fn CFArrayGetTypeID() -> CFTypeID;
    pub fn CFNumberGetTypeID() -> CFTypeID;
    pub fn CFStringGetCStringPtr(the_string: CFStringRef, encoding: u32) -> *const c_char;
    pub fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> Boolean;
    pub fn CFStringCreateWithCString(
        allocator: CFAllocatorRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    pub fn CFStringGetLength(the_string: CFStringRef) -> CFIndex;
    pub fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
    pub fn CFDictionaryGetValue(the_dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    pub fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        key_call_backs: *const CFDictionaryKeyCallBacks,
        value_call_backs: *const CFDictionaryValueCallBacks,
    ) -> CFDictionaryRef;
    pub fn CFArrayGetCount(the_array: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(the_array: CFArrayRef, idx: CFIndex) -> *const c_void;
    pub fn CFArrayCreate(
        allocator: CFAllocatorRef,
        values: *const *const c_void,
        num_values: CFIndex,
        call_backs: *const CFArrayCallBacks,
    ) -> CFArrayRef;
    pub fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: i32,
        value_ptr: *const c_void,
    ) -> CFNumberRef;
    pub fn CFNumberGetValue(number: CFNumberRef, the_type: i32, value_ptr: *mut c_void) -> Boolean;

    pub fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
    pub fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRemoveSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRun();
    pub fn CFRunLoopRunInMode(
        mode: CFStringRef,
        seconds: f64,
        return_after_source_handled: Boolean,
    ) -> i32;
    pub fn CFRunLoopStop(rl: CFRunLoopRef);
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFAbsoluteTimeGetCurrent() -> f64;
    pub fn CFRunLoopTimerCreate(
        allocator: CFAllocatorRef,
        fire_date: f64,
        interval: f64,
        flags: u64,
        order: CFIndex,
        callout: CFRunLoopTimerCallBack,
        context: *mut CFRunLoopTimerContext,
    ) -> CFRunLoopTimerRef;
    pub fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: CFRunLoopTimerRef, mode: CFStringRef);
    pub fn CFRunLoopRemoveTimer(rl: CFRunLoopRef, timer: CFRunLoopTimerRef, mode: CFStringRef);

    pub fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: CGEventMask,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
    pub fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> CGEventRef;
    pub fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: bool,
    ) -> CGEventRef;
    pub fn CGEventCreateScrollWheelEvent(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        ...
    ) -> CGEventRef;
    pub fn CGEventPost(tap: u32, event: CGEventRef);
    pub fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    pub fn CGEventSetLocation(event: CGEventRef, location: CGPoint);
    pub fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
    pub fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub fn CGEventSourceCreate(state_id: i32) -> CGEventSourceRef;
    pub fn CGEventSourceSetLocalEventsSuppressionInterval(source: CGEventSourceRef, seconds: f64);
    pub fn CGEventSourceSetUserData(source: CGEventSourceRef, user_data: i64);
    pub fn CGPreflightListenEventAccess() -> bool;
    pub fn CGRequestListenEventAccess() -> bool;
    pub fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: CGWindowID) -> CFArrayRef;
    pub fn CGWindowListCreateDescriptionFromArray(window_array: CFArrayRef) -> CFArrayRef;
    pub fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect)
        -> bool;

    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(
        element: AXUIElementRef,
        timeout_in_seconds: f32,
    ) -> AXError;
    pub fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    pub fn AXValueGetValue(value: AXValueRef, the_type: u32, value_ptr: *mut c_void) -> Boolean;
    pub fn AXValueGetTypeID() -> CFTypeID;
}

pub struct Owned(*mut c_void);

impl Owned {
    pub unsafe fn from_raw(ptr: *mut c_void) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self(ptr))
        }
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.0
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0 as CFTypeRef) }
    }
}

pub unsafe fn cf_string(value: CFStringRef) -> String {
    if value.is_null() {
        return String::new();
    }
    let fast = CFStringGetCStringPtr(value, K_CF_STRING_ENCODING_UTF8);
    if !fast.is_null() {
        return std::ffi::CStr::from_ptr(fast)
            .to_string_lossy()
            .into_owned();
    }
    let length = CFStringGetLength(value);
    let mut bytes = CFStringGetMaximumSizeForEncoding(length, K_CF_STRING_ENCODING_UTF8) + 1;
    if bytes < 1 {
        bytes = 1;
    }
    let mut buffer = vec![0u8; bytes as usize];
    let ok = CFStringGetCString(
        value,
        buffer.as_mut_ptr() as *mut c_char,
        bytes,
        K_CF_STRING_ENCODING_UTF8,
    );
    if ok == 0 {
        return String::new();
    }
    std::ffi::CStr::from_ptr(buffer.as_ptr() as *const c_char)
        .to_string_lossy()
        .into_owned()
}
