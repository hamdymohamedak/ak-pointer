use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::model::{Action, EventType, InputEvent, MouseButton, ScrollUnit, TargetWindow};

pub fn sanitize_action_name(name: &str) -> String {
    let mut bytes = Vec::new();
    for byte in name.as_bytes() {
        if *byte < 0x20 || *byte == 0x7f || *byte == b'/' || *byte == b'\\' || *byte == b':' {
            continue;
        }
        bytes.push(if *byte == b' ' || *byte == b'\t' {
            b'_'
        } else {
            *byte
        });
    }
    while matches!(bytes.last(), Some(b'.' | b'_')) {
        bytes.pop();
    }
    while matches!(bytes.first(), Some(b'.' | b'_')) {
        bytes.remove(0);
    }
    let mut out = String::from_utf8(bytes).unwrap_or_default();
    if out == "." || out == ".." {
        return String::new();
    }
    if out.len() > 120 {
        out.truncate(120);
        while !out.is_char_boundary(out.len()) {
            out.pop();
        }
    }
    out
}

fn one_line(value: &str) -> String {
    value.chars().filter(|c| *c != '\n' && *c != '\r').collect()
}

fn button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Left => "left",
    }
}

fn parse_button(name: &str) -> Option<MouseButton> {
    match name {
        "left" => Some(MouseButton::Left),
        "right" => Some(MouseButton::Right),
        "middle" => Some(MouseButton::Middle),
        _ => None,
    }
}

fn rounded(value: f64) -> i64 {
    value.round() as i64
}

fn field(value: &str) -> String {
    one_line(value).replace('\t', " ")
}

fn primary_target(action: &Action) -> TargetWindow {
    action.targets.first().cloned().unwrap_or(TargetWindow {
        owner: action.window_owner.clone(),
        title: action.window_title.clone(),
        w: action.window_w,
        h: action.window_h,
    })
}

pub fn format_action(action: &Action) -> String {
    let primary = primary_target(action);
    let owner = one_line(&primary.owner);
    let title = one_line(&primary.title);
    let mut body = format!("version {}\n", action.version);
    body.push_str("window_owner");
    if !owner.is_empty() {
        body.push(' ');
        body.push_str(&owner);
    }
    body.push('\n');
    body.push_str("window_title");
    if !title.is_empty() {
        body.push(' ');
        body.push_str(&title);
    }
    body.push('\n');
    body.push_str(&format!(
        "window_w {}\nwindow_h {}\n\n",
        rounded(primary.w),
        rounded(primary.h)
    ));
    let mut current_target = 0u16;
    for event in &action.events {
        if event.target != current_target {
            if let Some(target) = action.targets.get(event.target as usize) {
                body.push_str(&format!(
                    "target\t{}\t{}\t{}\t{}\n",
                    field(&target.owner),
                    field(&target.title),
                    rounded(target.w),
                    rounded(target.h)
                ));
                current_target = event.target;
            }
        }
        body.push_str(&format!("{} ", event.t_ms));
        match event.kind {
            EventType::Move => {
                body.push_str(&format!("MOVE {} {}", rounded(event.x), rounded(event.y)));
            }
            EventType::Down => {
                body.push_str(&format!(
                    "DOWN {} {} {} {}",
                    button_name(event.button),
                    event.click_count,
                    rounded(event.x),
                    rounded(event.y)
                ));
            }
            EventType::Up => {
                body.push_str(&format!(
                    "UP {} {} {} {}",
                    button_name(event.button),
                    event.click_count,
                    rounded(event.x),
                    rounded(event.y)
                ));
            }
            EventType::Drag => {
                body.push_str(&format!(
                    "DRAG {} {} {}",
                    button_name(event.button),
                    rounded(event.x),
                    rounded(event.y)
                ));
            }
            EventType::Scroll => {
                let unit = if event.scroll_unit == ScrollUnit::Pixel {
                    "pixel"
                } else {
                    "line"
                };
                body.push_str(&format!(
                    "SCROLL {} {} {} {} {}",
                    rounded(event.x),
                    rounded(event.y),
                    event.scroll_x,
                    event.scroll_y,
                    unit
                ));
            }
            EventType::Key => {
                let state = if event.key_down { "down" } else { "up" };
                body.push_str(&format!("KEY {} {} {}", state, event.keycode, event.flags));
            }
        }
        body.push('\n');
    }
    body
}

pub fn save_action(path: &Path, action: &Action) -> bool {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let mut file = match File::create(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    file.write_all(format_action(action).as_bytes()).is_ok()
}

pub fn parse_action_text(text: &str) -> Option<Action> {
    let path = std::env::temp_dir().join(format!("pointer-draft-{}.txt", std::process::id()));
    if fs::write(&path, text).is_err() {
        return None;
    }
    let action = load_action(&path);
    let _ = fs::remove_file(&path);
    action
}

fn header_parts(line: &str) -> (&str, &str) {
    match line.find(' ') {
        Some(index) => (&line[..index], &line[index + 1..]),
        None => (line, ""),
    }
}

fn parse_event(line: &str) -> Option<InputEvent> {
    let mut parts = line.split_whitespace();
    let t_ms = parts.next()?.parse().ok()?;
    let kind = parts.next()?;
    let mut event = InputEvent::new(EventType::Move);
    event.t_ms = t_ms;
    match kind {
        "MOVE" => {
            event.kind = EventType::Move;
            event.x = parts.next()?.parse().ok()?;
            event.y = parts.next()?.parse().ok()?;
        }
        "DOWN" | "UP" | "DRAG" => {
            event.kind = match kind {
                "DOWN" => EventType::Down,
                "UP" => EventType::Up,
                _ => EventType::Drag,
            };
            event.button = parse_button(parts.next()?)?;
            if event.kind == EventType::Drag {
                event.x = parts.next()?.parse().ok()?;
                event.y = parts.next()?.parse().ok()?;
            } else {
                event.click_count = parts.next()?.parse().ok()?;
                event.x = parts.next()?.parse().ok()?;
                event.y = parts.next()?.parse().ok()?;
            }
        }
        "SCROLL" => {
            event.kind = EventType::Scroll;
            event.x = parts.next()?.parse().ok()?;
            event.y = parts.next()?.parse().ok()?;
            event.scroll_x = parts.next()?.parse().ok()?;
            event.scroll_y = parts.next()?.parse().ok()?;
            if let Some(unit) = parts.next() {
                event.scroll_unit = match unit {
                    "pixel" => ScrollUnit::Pixel,
                    "line" => ScrollUnit::Line,
                    _ => return None,
                };
            }
        }
        "KEY" => {
            event.kind = EventType::Key;
            let state = parts.next()?;
            if state != "down" && state != "up" {
                return None;
            }
            event.key_down = state == "down";
            event.keycode = parts.next()?.parse().ok()?;
            event.flags = parts.next()?.parse().ok()?;
        }
        _ => return None,
    }
    Some(event)
}

fn parse_target_line(line: &str) -> Option<TargetWindow> {
    let mut parts = line.split('\t');
    if parts.next()? != "target" {
        return None;
    }
    let owner = parts.next()?.to_string();
    let title = parts.next()?.to_string();
    let w = parts.next()?.parse().ok()?;
    let h = parts.next()?.parse().ok()?;
    if owner.is_empty() || w < 1.0 || h < 1.0 {
        return None;
    }
    Some(TargetWindow { owner, title, w, h })
}

pub fn load_action(path: &Path) -> Option<Action> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            println!("Could not open {}", path.display());
            return None;
        }
    };
    let mut action = Action::new();
    let mut saw_version = false;
    let mut header = true;
    let mut current_target = 0u16;
    for line in BufReader::new(file).lines() {
        let mut line = match line {
            Ok(line) => line,
            Err(_) => {
                println!("Could not open {}", path.display());
                return None;
            }
        };
        if line.ends_with('\r') {
            line.pop();
        }
        if header {
            if line.is_empty() {
                header = false;
                continue;
            }
            let (key, value) = header_parts(&line);
            match key {
                "version" => match value.parse::<i32>() {
                    Ok(version) => {
                        action.version = version;
                        saw_version = true;
                        continue;
                    }
                    Err(_) => {
                        println!("Could not parse line: {line}");
                        return None;
                    }
                },
                "window_owner" => {
                    action.window_owner = value.to_string();
                    continue;
                }
                "window_title" => {
                    action.window_title = value.to_string();
                    continue;
                }
                "window_w" => match value.parse::<f64>() {
                    Ok(width) => {
                        action.window_w = width;
                        continue;
                    }
                    Err(_) => {
                        println!("Could not parse line: {line}");
                        return None;
                    }
                },
                "window_h" => match value.parse::<f64>() {
                    Ok(height) => {
                        action.window_h = height;
                        continue;
                    }
                    Err(_) => {
                        println!("Could not parse line: {line}");
                        return None;
                    }
                },
                _ => header = false,
            }
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(target) = parse_target_line(&line) {
            if action.targets.is_empty() {
                action.targets.push(TargetWindow {
                    owner: action.window_owner.clone(),
                    title: action.window_title.clone(),
                    w: action.window_w,
                    h: action.window_h,
                });
            }
            action.targets.push(target);
            current_target = (action.targets.len() - 1) as u16;
            continue;
        }
        match parse_event(&line) {
            Some(mut event) => {
                event.target = current_target;
                action.events.push(event);
            }
            None => {
                println!("Could not parse line: {line}");
                return None;
            }
        }
    }
    if action.targets.is_empty() {
        action.targets.push(TargetWindow {
            owner: action.window_owner.clone(),
            title: action.window_title.clone(),
            w: action.window_w,
            h: action.window_h,
        });
    }
    if !saw_version
        || action.version != 1
        || action.window_owner.is_empty()
        || action.window_w < 1.0
        || action.window_h < 1.0
    {
        println!("The file is missing window data: {}", path.display());
        return None;
    }
    Some(action)
}

pub fn list_action_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return files,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("txt") && path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_action_name("  login flow  "), "login_flow");
        assert!(!sanitize_action_name("../etc/passwd").contains('/'));
        assert_eq!(sanitize_action_name("open app"), "open_app");
        assert_eq!(sanitize_action_name("..."), "");
    }

    #[test]
    fn round_trips_an_action_file() {
        let dir = std::env::temp_dir().join(format!("pointer-storage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("login.txt");

        let mut action = Action::new();
        action.window_owner = "qemu-system-aarch64".to_string();
        action.window_title = "Pixel 7".to_string();
        action.window_w = 411.4;
        action.window_h = 823.6;

        let mut move_event = InputEvent::new(EventType::Move);
        move_event.x = 120.2;
        move_event.y = 340.6;

        let mut down = InputEvent::new(EventType::Down);
        down.t_ms = 180;
        down.x = 120.0;
        down.y = 340.0;

        let mut up = down.clone();
        up.t_ms = 250;
        up.kind = EventType::Up;

        let mut drag = InputEvent::new(EventType::Drag);
        drag.t_ms = 300;
        drag.button = MouseButton::Right;
        drag.x = 10.0;
        drag.y = 20.0;

        let mut scroll = InputEvent::new(EventType::Scroll);
        scroll.t_ms = 900;
        scroll.x = 200.0;
        scroll.y = 500.0;
        scroll.scroll_y = -3;
        scroll.scroll_unit = ScrollUnit::Pixel;

        let mut key = InputEvent::new(EventType::Key);
        key.t_ms = 980;
        key.keycode = 36;

        action.events = vec![move_event, down, up, drag, scroll, key];
        assert!(save_action(&path, &action));
        let loaded = load_action(&path).expect("load");
        assert_eq!(loaded.window_owner, "qemu-system-aarch64");
        assert_eq!(loaded.window_title, "Pixel 7");
        assert_eq!(loaded.window_w, 411.0);
        assert_eq!(loaded.window_h, 824.0);
        assert_eq!(loaded.events.len(), 6);
        assert_eq!(loaded.events[0].kind, EventType::Move);
        assert_eq!(loaded.events[0].x, 120.0);
        assert_eq!(loaded.events[0].y, 341.0);
        assert_eq!(loaded.events[1].kind, EventType::Down);
        assert_eq!(loaded.events[1].click_count, 1);
        assert_eq!(loaded.events[3].kind, EventType::Drag);
        assert_eq!(loaded.events[3].button, MouseButton::Right);
        assert_eq!(loaded.events[4].scroll_y, -3);
        assert_eq!(loaded.events[4].scroll_unit, ScrollUnit::Pixel);
        assert!(loaded.events[5].key_down);
        assert_eq!(loaded.events[5].keycode, 36);
        assert!(load_action(&dir.join("missing.txt")).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trips_a_second_window() {
        let dir = std::env::temp_dir().join(format!("pointer-targets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("two.txt");
        let mut action = Action::new();
        action.targets = vec![
            TargetWindow {
                owner: "Simulator".into(),
                title: "iPhone 16".into(),
                w: 402.0,
                h: 874.0,
            },
            TargetWindow {
                owner: "Android Studio".into(),
                title: "Pixel 7".into(),
                w: 400.0,
                h: 800.0,
            },
        ];
        let mut first = InputEvent::new(EventType::Down);
        first.x = 10.0;
        first.y = 20.0;
        let mut second = InputEvent::new(EventType::Down);
        second.t_ms = 400;
        second.x = 30.0;
        second.y = 40.0;
        second.target = 1;
        action.events = vec![first, second];
        assert!(save_action(&path, &action));
        let loaded = load_action(&path).expect("load");
        assert_eq!(loaded.window_owner, "Simulator");
        assert_eq!(loaded.events[0].target, 0);
        assert_eq!(loaded.events[1].target, 1);
        assert_eq!(loaded.targets[1].owner, "Android Studio");
        assert_eq!(loaded.targets[1].title, "Pixel 7");
        let _ = fs::remove_dir_all(&dir);
    }
}
