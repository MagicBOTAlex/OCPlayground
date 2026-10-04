//! Terminal front-end: renders the [`TextBuffer`] via `ratatui` and translates
//! key events into OpenComputers key codes.

use crate::buffer::TextBuffer;
use crate::color::wcwidth;
use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::CellDiffOption;
use ratatui::layout::Position;
use ratatui::style::{Color, Style};
use ratatui::{Frame, Terminal as RatatuiTerminal};
use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;

const MOD_CONTROL: i32 = 0x1D;
const MOD_SHIFT: i32 = 0x2A;
const MOD_ALT: i32 = 0x38;

pub struct KeyInput {
    pub character: char,
    pub code: i32,
    pub down: bool,
}

/// A touch/pointer action on the emulated screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Drag,
    Release,
    Scroll,
}

/// A pointer event. Coordinates are terminal cells (0-based); `data` is the
/// button index for press/drag/release and the wheel delta for scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseInput {
    pub action: MouseAction,
    pub x: i32,
    pub y: i32,
    pub data: i32,
}

/// Anything the front-end can deliver to the machine.
pub enum InputEvent {
    Key(KeyInput),
    Mouse(MouseInput),
    /// Host-level force quit. Plain Ctrl+C is reserved for this so a stuck
    /// guest program can always be escaped; OpenOS interrupts programs with
    /// Ctrl+Alt+C instead.
    Quit,
}

pub struct Output {
    terminal: Option<RatatuiTerminal<CrosstermBackend<Stdout>>>,
    mouse: bool,
    modifiers: KeyModifiers,
}

impl Output {
    pub fn new(_interactive: bool, mouse: bool) -> Result<Output> {
        if !io::stdout().is_terminal() {
            return Ok(Output {
                terminal: None,
                mouse,
                modifiers: KeyModifiers::empty(),
            });
        }
        // Some terminals and multiplexers report a 0x0 size before the first
        // resize; rendering into an empty area shows nothing, so treat that as
        // a non-interactive (plain text) session instead.
        if matches!(crossterm::terminal::size(), Ok((cols, rows)) if cols == 0 || rows == 0) {
            return Ok(Output {
                terminal: None,
                mouse,
                modifiers: KeyModifiers::empty(),
            });
        }
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        let _ = execute!(
            stdout,
            event::PushKeyboardEnhancementFlags(
                event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        );
        if mouse {
            let _ = execute!(stdout, crossterm::event::EnableMouseCapture);
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = RatatuiTerminal::new(backend)?;
        Ok(Output {
            terminal: Some(terminal),
            mouse,
            modifiers: KeyModifiers::empty(),
        })
    }

    pub fn poll(&mut self, timeout: Duration) -> Result<Vec<InputEvent>> {
        if self.terminal.is_none() {
            std::thread::sleep(timeout.min(Duration::from_millis(20)));
            return Ok(Vec::new());
        }
        let mut inputs = Vec::new();
        if !event::poll(timeout)? {
            return Ok(inputs);
        }
        loop {
            match event::read()? {
                Event::Key(key) => {
                    if is_host_quit(&key) {
                        inputs.push(InputEvent::Quit);
                    } else {
                        self.emit_modifier_diff(key.modifiers, &mut inputs);
                        if let Some(input) = map_key(key) {
                            inputs.push(InputEvent::Key(input));
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    if self.mouse {
                        if let Some(input) = map_mouse(mouse) {
                            inputs.push(InputEvent::Mouse(input));
                        }
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
        Ok(inputs)
    }

    fn emit_modifier_diff(&mut self, want: KeyModifiers, inputs: &mut Vec<InputEvent>) {
        for (flag, code) in [
            (KeyModifiers::CONTROL, MOD_CONTROL),
            (KeyModifiers::SHIFT, MOD_SHIFT),
            (KeyModifiers::ALT, MOD_ALT),
        ] {
            let was = self.modifiers.contains(flag);
            let is = want.contains(flag);
            if is && !was {
                inputs.push(InputEvent::Key(KeyInput {
                    character: '\0',
                    code,
                    down: true,
                }));
            } else if !is && was {
                inputs.push(InputEvent::Key(KeyInput {
                    character: '\0',
                    code,
                    down: false,
                }));
            }
        }
        self.modifiers = want;
    }

    pub fn render(&mut self, buffer: &TextBuffer) -> Result<()> {
        let terminal = match self.terminal.as_mut() {
            Some(terminal) => terminal,
            None => return Ok(()),
        };
        terminal.draw(|frame| draw_buffer(frame, buffer))?;
        Ok(())
    }

    pub fn restore(&mut self) {
        if self.terminal.is_some() {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, event::PopKeyboardEnhancementFlags);
            if self.mouse {
                let _ = execute!(stdout, crossterm::event::DisableMouseCapture);
            }
            let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
            let _ = disable_raw_mode();
            self.terminal = None;
        }
    }

    pub fn is_tty(&self) -> bool {
        self.terminal.is_some()
    }
}

/// Print the visible screen buffer as plain text (used when stdout is not a
/// terminal, e.g. for CI or piping).
pub fn dump_screen(buffer: &TextBuffer) {
    for y in 0..buffer.height {
        let mut line = String::new();
        for x in 0..buffer.width {
            let cp = buffer.get(x, y).unwrap_or(0x20);
            let ch = char::from_u32(cp).unwrap_or(' ');
            line.push(if (ch as u32) < 0x20 { ' ' } else { ch });
        }
        println!("{}", line.trim_end());
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.restore();
    }
}

fn draw_buffer(frame: &mut Frame, buffer: &TextBuffer) {
    let area = frame.area();
    let buf = frame.buffer_mut();

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.reset();
            }
        }
    }

    let cols = buffer.width.min(area.width as i32);
    let rows = buffer.height.min(area.height as i32);
    if cols <= 0 || rows <= 0 {
        return;
    }
    for y in 0..rows {
        let mut x = 0;
        while x < cols {
            let cp = match buffer.get(x, y) {
                Some(cp) => cp,
                None => {
                    x += 1;
                    continue;
                }
            };
            let width = wcwidth(cp).max(1);
            let pos = Position::new(area.x + x as u16, area.y + y as u16);
            let (fg, bg) = buffer.cell_rgb(x, y);
            let style = Style::default()
                .fg(Color::Rgb((fg >> 16) as u8, (fg >> 8) as u8, fg as u8))
                .bg(Color::Rgb((bg >> 16) as u8, (bg >> 8) as u8, bg as u8));

            let ch = char::from_u32(cp).unwrap_or(' ');
            let symbol = if (ch as u32) < 0x20 || ch == '\u{7f}' {
                " ".to_string()
            } else {
                ch.to_string()
            };

            if let Some(cell) = buf.cell_mut(pos) {
                cell.set_style(style);
                cell.set_symbol(&symbol);
                if width >= 2 {
                    cell.set_diff_option(CellDiffOption::ForcedWidth(
                        std::num::NonZeroU16::new(2).unwrap(),
                    ));
                    if let Some(next) = buf.cell_mut(Position::new(pos.x + 1, pos.y)) {
                        next.set_style(style);
                        next.set_symbol(" ");
                        next.set_diff_option(CellDiffOption::Skip);
                    }
                }
            }
            x += width;
        }
    }
}

/// Plain Ctrl+C is a host-level force quit. Ctrl+Alt+C is left alone so OpenOS
/// can still interrupt a guest program with it.
fn is_host_quit(key: &KeyEvent) -> bool {
    if matches!(key.kind, KeyEventKind::Release) {
        return false;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        return false;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));
    }
    // Terminals without keyboard enhancement report Ctrl+C as ETX.
    matches!(key.code, KeyCode::Char('\u{3}'))
}

fn map_key(key: KeyEvent) -> Option<KeyInput> {
    let down = !matches!(key.kind, KeyEventKind::Release);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(c) => {
            // Terminals may deliver Enter/Tab/Backspace as control characters.
            if c == '\r' || (c == '\n' && !ctrl) {
                return Some(KeyInput {
                    character: '\n',
                    code: 0x1C,
                    down,
                });
            }
            if c == '\t' {
                return Some(KeyInput {
                    character: '\t',
                    code: 0x0F,
                    down,
                });
            }
            if c == '\u{8}' || c == '\u{7f}' {
                return Some(KeyInput {
                    character: '\u{8}',
                    code: 0x0E,
                    down,
                });
            }
            // Ctrl+<letter> arrives either as a control char or as a letter
            // with the CONTROL modifier; normalise to a control code so OpenOS
            // does not insert it as text, while still using the scan code.
            let (character, code) = if (c as u32) < 0x20 {
                let letter = char::from_u32('a' as u32 + c as u32 - 1)?;
                (c, char_scan_code(letter)?)
            } else if ctrl && c.is_ascii_alphabetic() {
                let control = char::from_u32(c.to_ascii_uppercase() as u32 & 0x1f)?;
                (control, char_scan_code(c)?)
            } else {
                (c, char_scan_code(c)?)
            };
            Some(KeyInput {
                character,
                code,
                down,
            })
        }
        KeyCode::Enter => Some(KeyInput {
            character: '\n',
            code: 0x1C,
            down,
        }),
        KeyCode::Backspace => Some(KeyInput {
            character: '\u{8}',
            code: 0x0E,
            down,
        }),
        KeyCode::Tab | KeyCode::BackTab => Some(KeyInput {
            character: '\t',
            code: 0x0F,
            down,
        }),
        KeyCode::Esc => Some(KeyInput {
            character: '\u{1b}',
            code: 0x01,
            down,
        }),
        KeyCode::Up => special(0xC8, down),
        KeyCode::Down => special(0xD0, down),
        KeyCode::Left => special(0xCB, down),
        KeyCode::Right => special(0xCD, down),
        KeyCode::Home => special(0xC7, down),
        KeyCode::End => special(0xCF, down),
        KeyCode::PageUp => special(0xC9, down),
        KeyCode::PageDown => special(0xD1, down),
        KeyCode::Insert => special(0xD2, down),
        KeyCode::Delete => special(0xD3, down),
        KeyCode::F(n) => {
            let code = match n {
                1 => 0x3B,
                2 => 0x3C,
                3 => 0x3D,
                4 => 0x3E,
                5 => 0x3F,
                6 => 0x40,
                7 => 0x41,
                8 => 0x42,
                9 => 0x43,
                10 => 0x44,
                11 => 0x57,
                12 => 0x58,
                _ => return None,
            };
            special(code, down)
        }
        _ => None,
    }
}

fn special(code: i32, down: bool) -> Option<KeyInput> {
    Some(KeyInput {
        character: '\0',
        code,
        down,
    })
}

fn mouse_button(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
    }
}

fn map_mouse(event: MouseEvent) -> Option<MouseInput> {
    let (action, data) = match event.kind {
        MouseEventKind::Down(button) => (MouseAction::Press, mouse_button(button)),
        MouseEventKind::Drag(button) => (MouseAction::Drag, mouse_button(button)),
        MouseEventKind::Up(button) => (MouseAction::Release, mouse_button(button)),
        MouseEventKind::ScrollUp => (MouseAction::Scroll, 1),
        MouseEventKind::ScrollDown => (MouseAction::Scroll, -1),
        // Horizontal wheel and move events are not part of the OC screen API.
        _ => return None,
    };
    Some(MouseInput {
        action,
        x: event.column as i32,
        y: event.row as i32,
        data,
    })
}

/// Map a printable character to its OpenComputers scan code.
fn char_scan_code(c: char) -> Option<i32> {
    let lower = c.to_ascii_lowercase();
    let base = match lower {
        'a' => 0x1E,
        'b' => 0x30,
        'c' => 0x2E,
        'd' => 0x20,
        'e' => 0x12,
        'f' => 0x21,
        'g' => 0x22,
        'h' => 0x23,
        'i' => 0x17,
        'j' => 0x24,
        'k' => 0x25,
        'l' => 0x26,
        'm' => 0x32,
        'n' => 0x31,
        'o' => 0x18,
        'p' => 0x19,
        'q' => 0x10,
        'r' => 0x13,
        's' => 0x1F,
        't' => 0x14,
        'u' => 0x16,
        'v' => 0x2F,
        'w' => 0x11,
        'x' => 0x2D,
        'y' => 0x15,
        'z' => 0x2C,
        '1' | '!' => 0x02,
        '2' | '@' => 0x03,
        '3' | '#' => 0x04,
        '4' | '$' => 0x05,
        '5' | '%' => 0x06,
        '6' | '^' => 0x07,
        '7' | '&' => 0x08,
        '8' | '*' => 0x09,
        '9' | '(' => 0x0A,
        '0' | ')' => 0x0B,
        '-' | '_' => 0x0C,
        '=' | '+' => 0x0D,
        '[' | '{' => 0x1A,
        ']' | '}' => 0x1B,
        '\\' | '|' => 0x2B,
        ';' | ':' => 0x27,
        '\'' | '"' => 0x28,
        '`' | '~' => 0x29,
        ',' | '<' => 0x33,
        '.' | '>' => 0x34,
        '/' | '?' => 0x35,
        ' ' => 0x39,
        '\n' => 0x1C,
        '\t' => 0x0F,
        _ => return None,
    };
    Some(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };

    fn event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    #[test]
    fn maps_press_drag_and_release() {
        let press = map_mouse(event(MouseEventKind::Down(MouseButton::Left), 3, 7)).unwrap();
        assert_eq!(press.action, MouseAction::Press);
        assert_eq!((press.x, press.y, press.data), (3, 7, 0));

        let drag = map_mouse(event(MouseEventKind::Drag(MouseButton::Right), 4, 8)).unwrap();
        assert_eq!(drag.action, MouseAction::Drag);
        assert_eq!(drag.data, 1);

        let up = map_mouse(event(MouseEventKind::Up(MouseButton::Middle), 5, 9)).unwrap();
        assert_eq!(up.action, MouseAction::Release);
        assert_eq!(up.data, 2);
    }

    #[test]
    fn maps_vertical_scroll_to_delta() {
        let up = map_mouse(event(MouseEventKind::ScrollUp, 1, 2)).unwrap();
        assert_eq!(up.action, MouseAction::Scroll);
        assert_eq!(up.data, 1);

        let down = map_mouse(event(MouseEventKind::ScrollDown, 1, 2)).unwrap();
        assert_eq!(down.data, -1);
    }

    #[test]
    fn ignores_move_events() {
        assert!(map_mouse(event(MouseEventKind::Moved, 1, 1)).is_none());
    }

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind,
            state: crossterm::event::KeyEventState::empty(),
        }
    }

    #[test]
    fn plain_ctrl_c_is_host_quit() {
        assert!(is_host_quit(&key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        )));
        // Terminals without keyboard enhancement report ETX directly.
        assert!(is_host_quit(&key(
            KeyCode::Char('\u{3}'),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        )));
    }

    #[test]
    fn ctrl_alt_c_is_left_for_the_guest() {
        assert!(!is_host_quit(&key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
            KeyEventKind::Press,
        )));
    }

    #[test]
    fn plain_c_and_key_release_do_not_quit() {
        assert!(!is_host_quit(&key(
            KeyCode::Char('c'),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        )));
        assert!(!is_host_quit(&key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        )));
    }
}
