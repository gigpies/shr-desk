//! Controller-facing console commands. These are semantic intents, never key names.
//! A learned device adapter supplies gestures; this module opens no MIDI ports.
use crate::actions::Action;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Menu,
    Grant,
    ReleaseWriter,
    Reconnect,
    LegacyReconnect,
    ReviewPage(i32),
    FullscreenEq,
    AnalysisSource(i32),
    Meters,
    MeterTap,
    MeterBank(i32),
    SendsView(bool),
    BrainBus(i32),
    EntryAppend(String),
    EntryBackspace,
    EntryAccept,
    ReviewDraft,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub label: &'static str,
    pub action: Action,
}
impl Item {
    pub fn new(label: &'static str, action: Action) -> Self {
        Self { label, action }
    }
}

/// Logical controls, not assumed MiniLab notes/CCs. Pads are zero based.
#[derive(Clone, Copy, Debug)]
pub enum Gesture {
    CancelPress,
    Menu,
    Browse(i32),
    Character(i32),
    Pad { index: usize, pressed: bool },
}

#[derive(Default)]
pub struct Surface {
    context: String,
    pub open: bool,
    pub selected: usize,
    pending: Option<Action>,
    character: usize,
    held: [bool; 8],
    blocked: [bool; 8],
}
impl Surface {
    pub fn synchronize(&mut self, context: String) {
        if self.context != context {
            self.context = context;
            self.pending = None;
            for i in 0..8 {
                self.blocked[i] |= self.held[i];
            }
            self.held = [false; 8];
        }
    }
    pub fn lost(&mut self) {
        self.pending = None;
        self.open = false;
        self.held = [false; 8];
        self.blocked = [true; 8];
        self.selected = 0;
    }
    pub fn character(&self) -> char {
        (32 + self.character as u8) as char
    }
    /// Actions fire once on a full press/release, so a held pad cannot confirm twice.
    pub fn gesture(&mut self, event: Gesture, items: &[Item]) -> Option<Action> {
        self.selected = self.selected.min(items.len().saturating_sub(1));
        match event {
            Gesture::CancelPress => {
                self.pending = None;
                self.held = [false; 8];
            }
            Gesture::Menu => {
                for i in 0..8 {
                    self.blocked[i] |= self.held[i];
                }
                self.open = !self.open;
                self.pending = None;
                self.held = [false; 8];
            }
            Gesture::Browse(n) => {
                self.open = true;
                if !items.is_empty() {
                    self.selected = (self.selected as i64 + i64::from(n.clamp(-8, 8)))
                        .rem_euclid(items.len() as i64)
                        as usize;
                }
            }
            Gesture::Character(n) => {
                self.character =
                    (self.character as i64 + i64::from(n.clamp(-8, 8))).rem_euclid(95) as usize;
            }
            Gesture::Pad { index, pressed } if index < 8 => {
                if pressed {
                    if !self.blocked[index] && !self.held[index] {
                        self.held[index] = true;
                        if index == 2 {
                            self.pending = items.get(self.selected).map(|i| i.action.clone());
                        }
                    }
                } else {
                    self.blocked[index] = false;
                    if !std::mem::take(&mut self.held[index]) {
                        return None;
                    }
                    return match index {
                        0 => {
                            self.gesture(Gesture::Browse(-1), items);
                            None
                        }
                        1 => {
                            self.gesture(Gesture::Browse(1), items);
                            None
                        }
                        2 => {
                            let pending = self.pending.take();
                            if pending.as_ref() == items.get(self.selected).map(|i| &i.action) {
                                self.open = false;
                                pending
                            } else {
                                None
                            }
                        }
                        3 => Some(Action::Cancel),
                        4 => {
                            self.gesture(Gesture::Character(-1), items);
                            None
                        }
                        5 => {
                            self.gesture(Gesture::Character(1), items);
                            None
                        }
                        6 => Some(Action::Console(Command::EntryAppend(
                            self.character().to_string(),
                        ))),
                        7 => Some(Action::Console(Command::EntryBackspace)),
                        _ => None,
                    };
                }
            }
            _ => (),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_pad_and_lost_release_never_confirm_new_context() {
        let items = [Item::new("Confirm", Action::Confirm)];
        let mut s = Surface::default();
        let down = Gesture::Pad {
            index: 2,
            pressed: true,
        };
        let up = Gesture::Pad {
            index: 2,
            pressed: false,
        };
        assert!(s.gesture(down, &items).is_none());
        assert!(s.gesture(down, &items).is_none());
        assert_eq!(s.gesture(up, &items), Some(Action::Confirm));
        assert!(s.gesture(up, &items).is_none());
        s.gesture(down, &items);
        s.lost();
        assert!(s.gesture(up, &items).is_none());
        s.gesture(down, &items);
        assert_eq!(s.gesture(up, &items), Some(Action::Confirm));
    }
    #[test]
    fn changed_context_cancels_press_without_changing_the_visible_choice() {
        let items = [
            Item::new("Grant", Action::Console(Command::Grant)),
            Item::new("Cancel", Action::Cancel),
        ];
        let mut surface = Surface {
            selected: 1,
            ..Default::default()
        };
        surface.gesture(
            Gesture::Pad {
                index: 2,
                pressed: true,
            },
            &items,
        );
        surface.synchronize("new revision".into());
        assert_eq!(surface.selected, 1);
        assert!(
            surface
                .gesture(
                    Gesture::Pad {
                        index: 2,
                        pressed: false
                    },
                    &items
                )
                .is_none()
        );
    }
    #[test]
    fn printable_entry_has_no_shortcut_interpretation() {
        let mut s = Surface::default();
        for _ in 0..5 {
            s.gesture(Gesture::Character(8), &[]);
        }
        s.gesture(Gesture::Character(7), &[]);
        assert_eq!(s.character(), 'O');
        s.gesture(
            Gesture::Pad {
                index: 6,
                pressed: true,
            },
            &[],
        );
        assert_eq!(
            s.gesture(
                Gesture::Pad {
                    index: 6,
                    pressed: false
                },
                &[]
            ),
            Some(Action::Console(Command::EntryAppend("O".into())))
        );
    }
}

/// Explicit learned pad binding. No default preset, enumeration or MIDI output.
#[derive(Clone, Debug)]
pub struct MidiBinding {
    endpoint: String,
    channel: u8,
    notes: [u8; 8],
}
impl MidiBinding {
    pub fn new(endpoint: String, channel: u8, notes: [u8; 8]) -> Result<Self, String> {
        if endpoint.is_empty()
            || endpoint.len() > 128
            || channel > 15
            || notes
                .iter()
                .enumerate()
                .any(|(i, n)| *n > 127 || notes[..i].contains(n))
        {
            return Err("invalid or ambiguous controller pad binding".into());
        }
        Ok(Self {
            endpoint,
            channel,
            notes,
        })
    }
    pub fn decode(&self, endpoint: &str, bytes: &[u8]) -> Option<Gesture> {
        if endpoint != self.endpoint
            || bytes.len() != 3
            || bytes[1] > 127
            || bytes[2] > 127
            || bytes[0] & 15 != self.channel
            || !matches!(bytes[0] & 0xf0, 0x80 | 0x90)
        {
            return None;
        }
        let index = self.notes.iter().position(|n| *n == bytes[1])?;
        Some(Gesture::Pad {
            index,
            pressed: bytes[0] & 0xf0 == 0x90 && bytes[2] != 0,
        })
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;
    #[test]
    fn wrong_device_invalid_bytes_and_changed_action_cannot_execute() {
        let binding = MidiBinding::new("desk".into(), 9, [36, 37, 38, 39, 40, 41, 42, 43]).unwrap();
        assert!(binding.decode("lighting", &[0x99, 38, 127]).is_none());
        assert!(binding.decode("desk", &[0x99, 38, 128]).is_none());
        assert!(MidiBinding::new("desk".into(), 16, [36, 37, 38, 39, 40, 41, 42, 43]).is_err());
        let mut surface = Surface::default();
        let items = [Item::new("Cancel", Action::Cancel)];
        surface.gesture(binding.decode("desk", &[0x99, 38, 127]).unwrap(), &items);
        let changed = [Item::new("Confirm", Action::Confirm)];
        assert!(
            surface
                .gesture(binding.decode("desk", &[0x99, 38, 0]).unwrap(), &changed)
                .is_none()
        );
        surface.gesture(binding.decode("desk", &[0x99, 38, 127]).unwrap(), &changed);
        surface.synchronize("new review".into());
        assert!(
            surface
                .gesture(binding.decode("desk", &[0x89, 38, 0]).unwrap(), &changed)
                .is_none()
        );
    }
}
