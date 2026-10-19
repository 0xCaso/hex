//! Platform-neutral key and modifier types for the command taxonomy.
//! Actions describe keystrokes with these; `keyboard` maps them to
//! CoreGraphics on macOS. `COMMAND` is the platform primary shortcut
//! modifier (Command on macOS, Control elsewhere); `OPTION` is Option/Alt.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Character(char),
    Home,
    End,
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const COMMAND: Self = Self(1 << 0);
    pub const SHIFT: Self = Self(1 << 1);
    pub const OPTION: Self = Self(1 << 2);
    pub const CONTROL: Self = Self(1 << 3);

    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}
