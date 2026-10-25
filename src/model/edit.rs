use super::{EventType, InputEvent, MouseButton, ScrollUnit};

const SHIFT: u64 = 0x20000;
const CAPS: u64 = 0x10000;
const CONTROL: u64 = 0x40000;
const OPTION: u64 = 0x80000;
const COMMAND: u64 = 0x100000;

const KEY_RETURN: u16 = 0x24;
const KEY_TAB: u16 = 0x30;
const KEY_BACKSPACE: u16 = 0x33;

const KEYS: &[(u16, char, char)] = &[
    (0x00, 'a', 'A'),
    (0x01, 's', 'S'),
    (0x02, 'd', 'D'),
    (0x03, 'f', 'F'),
    (0x04, 'h', 'H'),
    (0x05, 'g', 'G'),
    (0x06, 'z', 'Z'),
    (0x07, 'x', 'X'),
    (0x08, 'c', 'C'),
    (0x09, 'v', 'V'),
    (0x0B, 'b', 'B'),
    (0x0C, 'q', 'Q'),
    (0x0D, 'w', 'W'),
    (0x0E, 'e', 'E'),
    (0x0F, 'r', 'R'),
    (0x10, 'y', 'Y'),
    (0x11, 't', 'T'),
    (0x12, '1', '!'),
    (0x13, '2', '@'),
    (0x14, '3', '#'),
    (0x15, '4', '$'),
    (0x16, '6', '^'),
    (0x17, '5', '%'),
    (0x18, '=', '+'),
    (0x19, '9', '('),
    (0x1A, '7', '&'),
    (0x1B, '-', '_'),
    (0x1C, '8', '*'),
    (0x1D, '0', ')'),
    (0x1E, ']', '}'),
    (0x1F, 'o', 'O'),
    (0x20, 'u', 'U'),
    (0x21, '[', '{'),
    (0x22, 'i', 'I'),
    (0x23, 'p', 'P'),
    (0x25, 'l', 'L'),
    (0x26, 'j', 'J'),
    (0x27, '\'', '"'),
    (0x28, 'k', 'K'),
    (0x29, ';', ':'),
    (0x2A, '\\', '|'),
    (0x2B, ',', '<'),
    (0x2C, '/', '?'),
    (0x2D, 'n', 'N'),
    (0x2E, 'm', 'M'),
    (0x2F, '.', '>'),
    (0x31, ' ', ' '),
    (0x32, '`', '~'),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

pub fn text_runs(events: &[InputEvent]) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut index = 0;
    while index < events.len() {
        if events[index].kind != EventType::Key || is_shortcut(events[index].flags) {
            index += 1;
            continue;
        }
        let start = index;
        let mut end = index;
        let mut text = String::new();
        let mut typed = false;
        while index < events.len() {
            let event = &events[index];
            if event.kind == EventType::Move {
                index += 1;
                continue;
            }
            if event.kind != EventType::Key || is_shortcut(event.flags) {
                break;
            }
            end = index;
            if event.key_down {
                if event.keycode == KEY_BACKSPACE {
                    text.pop();
                    typed = true;
                } else if let Some(ch) = char_for_key(event.keycode, event.flags) {
                    text.push(ch);
                    typed = true;
                }
            }
            index += 1;
        }
        if typed {
            runs.push(TextRun { start, end, text });
        }
        if index == start {
            index += 1;
        }
    }
    runs
}

pub fn apply_text_run(
    events: &mut Vec<InputEvent>,
    start: usize,
    end: usize,
    text: &str,
) -> Result<(), String> {
    if start > end || end >= events.len() {
        return Err("That typed text is no longer in this action.".into());
    }
    let start_ms = events[start].t_ms;
    let old_end_ms = events[end].t_ms;
    let target = events[start].target;
    let mut new_keys = encode_text(text, start_ms)?;
    for event in &mut new_keys {
        event.target = target;
    }
    let new_end_ms = new_keys.last().map(|event| event.t_ms).unwrap_or(start_ms);
    let shift_by = (new_end_ms - old_end_ms).max(0);

    let mut rebuilt = Vec::with_capacity(events.len() + new_keys.len());
    rebuilt.extend_from_slice(&events[..start]);
    rebuilt.extend(new_keys);
    for event in &events[start..=end] {
        if event.kind == EventType::Key {
            continue;
        }
        let mut kept = event.clone();
        if kept.t_ms < new_end_ms {
            kept.t_ms = new_end_ms;
        }
        rebuilt.push(kept);
    }
    for event in &events[end + 1..] {
        let mut later = event.clone();
        later.t_ms += shift_by;
        rebuilt.push(later);
    }
    *events = rebuilt;
    Ok(())
}

pub fn describe_event(event: &InputEvent) -> String {
    match event.kind {
        EventType::Move => format!("move  {:.0}, {:.0}", event.x, event.y),
        EventType::Down => format!(
            "down {} x{}  {:.0}, {:.0}",
            button_name(event.button),
            event.click_count,
            event.x,
            event.y
        ),
        EventType::Up => format!(
            "up {}  {:.0}, {:.0}",
            button_name(event.button),
            event.x,
            event.y
        ),
        EventType::Drag => format!(
            "drag {}  {:.0}, {:.0}",
            button_name(event.button),
            event.x,
            event.y
        ),
        EventType::Scroll => {
            let unit = if event.scroll_unit == ScrollUnit::Pixel {
                "pixel"
            } else {
                "line"
            };
            format!(
                "scroll {:.0}, {:.0}  {}, {} {unit}",
                event.x, event.y, event.scroll_x, event.scroll_y
            )
        }
        EventType::Key => {
            let state = if event.key_down { "down" } else { "up" };
            format!("key {state} {}", key_label(event))
        }
    }
}

fn encode_text(text: &str, mut t_ms: i64) -> Result<Vec<InputEvent>, String> {
    let mut events = Vec::new();
    for ch in text.chars() {
        let (keycode, flags) = key_for_char(ch).ok_or_else(|| {
            format!("This editor cannot type '{ch}'. Use the file text for that character.")
        })?;
        let mut down = InputEvent::new(EventType::Key);
        down.t_ms = t_ms;
        down.key_down = true;
        down.keycode = keycode;
        down.flags = flags;
        events.push(down);
        t_ms += 25;
        let mut up = InputEvent::new(EventType::Key);
        up.t_ms = t_ms;
        up.key_down = false;
        up.keycode = keycode;
        up.flags = flags;
        events.push(up);
        t_ms += 55;
    }
    Ok(events)
}

fn key_for_char(ch: char) -> Option<(u16, u64)> {
    if ch == '\n' {
        return Some((KEY_RETURN, 0));
    }
    if ch == '\t' {
        return Some((KEY_TAB, 0));
    }
    for (keycode, lower, upper) in KEYS {
        if ch == *lower {
            return Some((*keycode, 0));
        }
        if ch == *upper && *upper != *lower {
            return Some((*keycode, SHIFT));
        }
    }
    None
}

fn char_for_key(keycode: u16, flags: u64) -> Option<char> {
    if keycode == KEY_RETURN {
        return Some('\n');
    }
    if keycode == KEY_TAB {
        return Some('\t');
    }
    let (_, lower, upper) = KEYS.iter().find(|(code, _, _)| *code == keycode)?;
    let shift = flags & SHIFT != 0;
    if lower.is_ascii_alphabetic() {
        let caps = flags & CAPS != 0;
        Some(if shift ^ caps { *upper } else { *lower })
    } else if shift {
        Some(*upper)
    } else {
        Some(*lower)
    }
}

fn key_label(event: &InputEvent) -> String {
    if event.keycode == KEY_BACKSPACE {
        return "backspace".into();
    }
    match char_for_key(event.keycode, event.flags) {
        Some('\n') => "return".into(),
        Some('\t') => "tab".into(),
        Some(' ') => "space".into(),
        Some(ch) => ch.to_string(),
        None => format!("code {}", event.keycode),
    }
}

fn is_shortcut(flags: u64) -> bool {
    flags & (CONTROL | OPTION | COMMAND) != 0
}

fn button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Left => "left",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_down(t_ms: i64, keycode: u16, flags: u64) -> InputEvent {
        let mut event = InputEvent::new(EventType::Key);
        event.t_ms = t_ms;
        event.keycode = keycode;
        event.flags = flags;
        event.key_down = true;
        event
    }

    fn click(t_ms: i64) -> InputEvent {
        let mut event = InputEvent::new(EventType::Down);
        event.t_ms = t_ms;
        event.x = 10.0;
        event.y = 20.0;
        event
    }

    #[test]
    fn backspace_is_applied_in_the_text() {
        let events = vec![
            key_down(0, 0x04, 0),
            key_down(80, 0x0E, 0),
            key_down(160, 0x25, 0),
            key_down(240, 0x25, 0),
            key_down(320, 0x1F, 0),
            key_down(400, KEY_BACKSPACE, 0),
            key_down(480, KEY_BACKSPACE, 0),
            key_down(560, 0x23, 0),
        ];
        let runs = text_runs(&events);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "help");
    }

    #[test]
    fn longer_replacement_shifts_the_following_click() {
        let mut events = vec![key_down(0, 0x00, 0), key_down(80, 0x0B, 0), click(500)];
        apply_text_run(&mut events, 0, 1, "abcd").unwrap();
        assert_eq!(text_runs(&events)[0].text, "abcd");
        let click = events
            .iter()
            .find(|event| event.kind == EventType::Down)
            .unwrap();
        assert!(click.t_ms > 500);
    }

    #[test]
    fn command_shortcut_splits_plain_text() {
        let events = vec![
            key_down(0, 0x04, 0),
            key_down(80, 0x0E, 0),
            key_down(160, 0x08, COMMAND),
            key_down(240, 0x25, 0),
            key_down(320, 0x1F, 0),
        ];
        let runs = text_runs(&events);
        assert_eq!(
            runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>(),
            vec!["he", "lo"]
        );
    }

    #[test]
    fn shifted_letter_is_uppercase() {
        let events = vec![key_down(0, 0x00, SHIFT)];
        assert_eq!(text_runs(&events)[0].text, "A");
        let mut events = events;
        apply_text_run(&mut events, 0, 0, "A").unwrap();
        let down = events.iter().find(|event| event.key_down).unwrap();
        assert_eq!(down.keycode, 0x00);
        assert_eq!(down.flags & SHIFT, SHIFT);
    }
}
