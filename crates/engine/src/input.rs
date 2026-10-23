use crate::{EventKind, InputEvent};

/// The three pointer buttons with standard camera-control meanings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

/// Platform-independent pointer bookkeeping for translating host events into
/// normalized [`InputEvent`] values. Hosts still translate their native event
/// types, but do not need to duplicate cursor/button state management.
#[derive(Clone, Debug, Default)]
pub struct PointerInputState {
    cursor: Option<(f32, f32)>,
    buttons: [bool; 3],
}

impl PointerInputState {
    pub fn cursor(&mut self, x: f32, y: f32) -> Option<InputEvent> {
        let previous = self.cursor;
        self.cursor = Some((x, y));
        let (old_x, old_y) = previous?;
        let (dx, dy) = (x - old_x, y - old_y);
        Some(self.event(EventKind::MouseMove, x, y, dx, dy))
    }

    pub fn button(&mut self, button: PointerButton, pressed: bool) -> InputEvent {
        let index = button.index();
        self.buttons[index] = pressed;
        let (x, y) = self.cursor.unwrap_or((0.0, 0.0));
        let mut event = self.event(EventKind::MouseButton, x, y, 0.0, 0.0);
        event.button = Some(index as u8);
        event.pressed = pressed;
        event
    }

    pub fn wheel(&self, amount: f32) -> InputEvent {
        let (x, y) = self.cursor.unwrap_or((0.0, 0.0));
        let mut event = self.event(EventKind::MouseWheel, x, y, 0.0, 0.0);
        event.wheel = amount;
        event
    }

    pub fn reset(&mut self) {
        self.cursor = None;
        self.buttons = [false; 3];
    }

    fn event(&self, kind: EventKind, x: f32, y: f32, dx: f32, dy: f32) -> InputEvent {
        InputEvent {
            kind,
            x,
            y,
            dx,
            dy,
            left_pressed: self.buttons[0],
            middle_pressed: self.buttons[1],
            right_pressed: self.buttons[2],
            ..Default::default()
        }
    }
}

impl PointerButton {
    const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_state_normalizes_motion_buttons_and_wheel() {
        let mut input = PointerInputState::default();
        assert!(input.cursor(10.0, 20.0).is_none());
        let moved = input.cursor(13.0, 18.0).unwrap();
        assert_eq!((moved.dx, moved.dy), (3.0, -2.0));
        let pressed = input.button(PointerButton::Left, true);
        assert!(pressed.left_pressed && pressed.pressed);
        assert!(input.wheel(1.0).left_pressed);
    }
}
