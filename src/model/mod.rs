mod edit;

pub use edit::{apply_text_run, describe_event, text_runs, TextRun};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventType {
    Move,
    Down,
    Up,
    Drag,
    Scroll,
    Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollUnit {
    Line,
    Pixel,
}

#[derive(Clone, Debug)]
pub struct InputEvent {
    pub t_ms: i64,
    pub kind: EventType,
    pub button: MouseButton,
    pub click_count: i32,
    pub x: f64,
    pub y: f64,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub scroll_unit: ScrollUnit,
    pub key_down: bool,
    pub keycode: u16,
    pub flags: u64,
    pub target: u16,
}

impl InputEvent {
    pub fn new(kind: EventType) -> Self {
        Self {
            t_ms: 0,
            kind,
            button: MouseButton::Left,
            click_count: 1,
            x: 0.0,
            y: 0.0,
            scroll_x: 0,
            scroll_y: 0,
            scroll_unit: ScrollUnit::Line,
            key_down: true,
            keycode: 0,
            flags: 0,
            target: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TargetWindow {
    pub owner: String,
    pub title: String,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub version: i32,
    pub window_owner: String,
    pub window_title: String,
    pub window_w: f64,
    pub window_h: f64,
    pub events: Vec<InputEvent>,
    pub targets: Vec<TargetWindow>,
}

impl Default for Action {
    fn default() -> Self {
        Self::new()
    }
}

impl Action {
    pub fn new() -> Self {
        Self {
            version: 1,
            window_owner: String::new(),
            window_title: String::new(),
            window_w: 0.0,
            window_h: 0.0,
            events: Vec::new(),
            targets: Vec::new(),
        }
    }

    pub fn target_window(&self, index: usize) -> TargetWindow {
        if let Some(target) = self.targets.get(index) {
            return target.clone();
        }
        TargetWindow {
            owner: self.window_owner.clone(),
            title: self.window_title.clone(),
            w: self.window_w,
            h: self.window_h,
        }
    }
}

pub const REPLAY_STAMP: i64 = 0x0070_6F69_6E74_6572;
pub const KEY_ESCAPE: u16 = 0x35;
pub const KEY_F8: u16 = 0x64;
