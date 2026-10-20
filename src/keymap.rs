use iced::keyboard::Key;
use iced::keyboard::key::Named;

/// Named/control keys supported by Neoweb in Neovim notation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKeyKind {
    Enter,
    Backspace,
    Tab,
    Space,
    Escape,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

/// The core key identity: either a standard named key or character string
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyKind {
    Named(NamedKeyKind),
    Character(String),
}

/// A parsed keystroke with normalized key identity and modifier flags
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: KeyKind,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyStroke {
    /// Constructs a `KeyStroke` from an Iced window keyboard event
    pub fn from_iced(
        key: &Key,
        modifiers: &iced::keyboard::Modifiers,
        text: Option<&str>,
    ) -> Option<Self> {
        let ctrl = modifiers.control();
        let alt = modifiers.alt();
        let shift = modifiers.shift();

        match key {
            Key::Named(Named::Escape) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Escape),
                ctrl: false,
                alt: false,
                shift: false,
            }),
            Key::Named(Named::Tab) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Tab),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::Enter) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Enter),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::Backspace) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Backspace),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::Space) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Space),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::Delete) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Delete),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::Insert) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Insert),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::Home) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::Home),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::End) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::End),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::PageUp) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::PageUp),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::PageDown) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::PageDown),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::ArrowUp) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::ArrowUp),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::ArrowDown) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::ArrowDown),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::ArrowLeft) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::ArrowLeft),
                ctrl,
                alt,
                shift,
            }),
            Key::Named(Named::ArrowRight) => Some(Self {
                key: KeyKind::Named(NamedKeyKind::ArrowRight),
                ctrl,
                alt,
                shift,
            }),
            _ => {
                if ctrl {
                    if let Key::Character(c) = key {
                        Some(Self {
                            key: KeyKind::Character(if shift {
                                c.to_ascii_uppercase().to_string()
                            } else {
                                c.to_ascii_lowercase().to_string()
                            }),
                            ctrl: true,
                            alt,
                            shift,
                        })
                    } else if let Some(txt) = text
                        && !txt.is_empty()
                        && txt.chars().all(|ch| ch.is_ascii_alphanumeric())
                    {
                        Some(Self {
                            key: KeyKind::Character(txt.to_ascii_lowercase()),
                            ctrl: true,
                            alt,
                            shift,
                        })
                    } else {
                        None
                    }
                } else if alt {
                    if let Key::Character(c) = key {
                        Some(Self {
                            key: KeyKind::Character(c.to_string()),
                            ctrl: false,
                            alt: true,
                            shift,
                        })
                    } else if let Some(txt) = text
                        && !txt.is_empty()
                    {
                        Some(Self {
                            key: KeyKind::Character(txt.to_string()),
                            ctrl: false,
                            alt: true,
                            shift,
                        })
                    } else {
                        None
                    }
                } else if let Some(txt) = text
                    && !txt.is_empty()
                {
                    if txt == " " {
                        Some(Self {
                            key: KeyKind::Named(NamedKeyKind::Space),
                            ctrl: false,
                            alt: false,
                            shift: false,
                        })
                    } else {
                        Some(Self {
                            key: KeyKind::Character(txt.to_string()),
                            ctrl: false,
                            alt: false,
                            shift,
                        })
                    }
                } else if let Key::Character(c) = key {
                    Some(Self {
                        key: KeyKind::Character(if shift {
                            c.to_ascii_uppercase().to_string()
                        } else {
                            c.to_string()
                        }),
                        ctrl: false,
                        alt: false,
                        shift,
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Formats the keystroke into standardized Neovim key notation
    pub fn to_neovim_string(&self) -> String {
        match &self.key {
            KeyKind::Named(named) => {
                let base = match named {
                    NamedKeyKind::Enter => "CR",
                    NamedKeyKind::Backspace => "BS",
                    NamedKeyKind::Tab => "Tab",
                    NamedKeyKind::Space => "Space",
                    NamedKeyKind::Escape => "Esc",
                    NamedKeyKind::Delete => "Del",
                    NamedKeyKind::Insert => "Ins",
                    NamedKeyKind::Home => "Home",
                    NamedKeyKind::End => "End",
                    NamedKeyKind::PageUp => "PageUp",
                    NamedKeyKind::PageDown => "PageDown",
                    NamedKeyKind::ArrowUp => "Up",
                    NamedKeyKind::ArrowDown => "Down",
                    NamedKeyKind::ArrowLeft => "Left",
                    NamedKeyKind::ArrowRight => "Right",
                };
                if self.ctrl {
                    format!("<C-{base}>")
                } else if self.alt {
                    format!("<A-{base}>")
                } else if self.shift && *named == NamedKeyKind::Tab {
                    "<S-Tab>".to_string()
                } else {
                    format!("<{base}>")
                }
            }
            KeyKind::Character(c) => {
                if self.ctrl {
                    format!("<C-{c}>")
                } else if self.alt {
                    format!("<A-{c}>")
                } else {
                    c.clone()
                }
            }
        }
    }

    /// Parses a Neovim key notation string (e.g. `<CR>`, `<BS>`, `<C-o>`, `<S-Tab>`, `j`) into a `KeyStroke`
    pub fn from_neovim_string(s: &str) -> Option<Self> {
        if s.starts_with('<') && s.ends_with('>') && s.len() >= 3 {
            let inner = &s[1..s.len() - 1];
            if let Some(rest) = inner.strip_prefix("C-") {
                if rest == "Tab" {
                    return Some(Self {
                        key: KeyKind::Named(NamedKeyKind::Tab),
                        ctrl: true,
                        alt: false,
                        shift: false,
                    });
                }
                if rest == "Space" {
                    return Some(Self {
                        key: KeyKind::Named(NamedKeyKind::Space),
                        ctrl: true,
                        alt: false,
                        shift: false,
                    });
                }
                return Some(Self {
                    key: KeyKind::Character(rest.to_string()),
                    ctrl: true,
                    alt: false,
                    shift: rest.chars().any(|c| c.is_ascii_uppercase()),
                });
            }
            if let Some(rest) = inner.strip_prefix("A-") {
                if rest == "Space" {
                    return Some(Self {
                        key: KeyKind::Named(NamedKeyKind::Space),
                        ctrl: false,
                        alt: true,
                        shift: false,
                    });
                }
                return Some(Self {
                    key: KeyKind::Character(rest.to_string()),
                    ctrl: false,
                    alt: true,
                    shift: rest.chars().any(|c| c.is_ascii_uppercase()),
                });
            }
            if let Some(rest) = inner.strip_prefix("S-")
                && rest == "Tab"
            {
                return Some(Self {
                    key: KeyKind::Named(NamedKeyKind::Tab),
                    ctrl: false,
                    alt: false,
                    shift: true,
                });
            }

            let named = match inner {
                "CR" | "Return" | "Enter" => NamedKeyKind::Enter,
                "BS" | "Backspace" => NamedKeyKind::Backspace,
                "Tab" => NamedKeyKind::Tab,
                "Space" => NamedKeyKind::Space,
                "Esc" | "Escape" => NamedKeyKind::Escape,
                "Del" | "Delete" => NamedKeyKind::Delete,
                "Ins" | "Insert" => NamedKeyKind::Insert,
                "Home" => NamedKeyKind::Home,
                "End" => NamedKeyKind::End,
                "PageUp" => NamedKeyKind::PageUp,
                "PageDown" => NamedKeyKind::PageDown,
                "Up" => NamedKeyKind::ArrowUp,
                "Down" => NamedKeyKind::ArrowDown,
                "Left" => NamedKeyKind::ArrowLeft,
                "Right" => NamedKeyKind::ArrowRight,
                _ => return None,
            };
            Some(Self {
                key: KeyKind::Named(named),
                ctrl: false,
                alt: false,
                shift: false,
            })
        } else {
            Some(Self {
                key: KeyKind::Character(s.to_string()),
                ctrl: false,
                alt: false,
                shift: s.chars().any(|c| c.is_ascii_uppercase()),
            })
        }
    }
}

/// Formats an Iced keyboard event into standardized Neovim key notation
/// (`<CR>`, `<BS>`, `<Esc>`, `<Space>`, `<Tab>`, `<S-Tab>`, `<C-w>`, `<C-u>`, `<C-o>`, `<C-i>`, etc.)
pub fn format_iced_key(
    key: &Key,
    modifiers: &iced::keyboard::Modifiers,
    text: Option<&str>,
) -> Option<String> {
    KeyStroke::from_iced(key, modifiers, text).map(|k| k.to_neovim_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::Named;
    use iced::keyboard::{Key, Modifiers};

    #[test]
    fn test_format_iced_key_shift_g() {
        let key = Key::Character("g".into());
        let mods = Modifiers::SHIFT;
        assert_eq!(
            format_iced_key(&key, &mods, Some("G")),
            Some("G".to_string())
        );
        assert_eq!(format_iced_key(&key, &mods, None), Some("G".to_string()));
    }

    #[test]
    fn test_format_iced_key_normal_chars() {
        let key = Key::Character("g".into());
        let mods = Modifiers::default();
        assert_eq!(
            format_iced_key(&key, &mods, Some("g")),
            Some("g".to_string())
        );

        let key_o = Key::Character("o".into());
        assert_eq!(
            format_iced_key(&key_o, &mods, Some("o")),
            Some("o".to_string())
        );
    }

    #[test]
    fn test_format_iced_key_special_keys() {
        let mods = Modifiers::default();
        assert_eq!(
            format_iced_key(&Key::Named(Named::Enter), &mods, None),
            Some("<CR>".to_string())
        );
        assert_eq!(
            format_iced_key(&Key::Named(Named::Backspace), &mods, None),
            Some("<BS>".to_string())
        );
        assert_eq!(
            format_iced_key(&Key::Named(Named::Escape), &mods, None),
            Some("<Esc>".to_string())
        );
        assert_eq!(
            format_iced_key(&Key::Named(Named::Space), &mods, Some(" ")),
            Some("<Space>".to_string())
        );
        assert_eq!(
            format_iced_key(&Key::Named(Named::Tab), &mods, None),
            Some("<Tab>".to_string())
        );

        let shift_mods = Modifiers::SHIFT;
        assert_eq!(
            format_iced_key(&Key::Named(Named::Tab), &shift_mods, None),
            Some("<S-Tab>".to_string())
        );

        let ctrl_mods = Modifiers::CTRL;
        let key_w = Key::Character("w".into());
        assert_eq!(
            format_iced_key(&key_w, &ctrl_mods, None),
            Some("<C-w>".to_string())
        );

        let key_o = Key::Character("o".into());
        assert_eq!(
            format_iced_key(&key_o, &ctrl_mods, None),
            Some("<C-o>".to_string())
        );

        let key_i = Key::Character("i".into());
        assert_eq!(
            format_iced_key(&key_i, &ctrl_mods, None),
            Some("<C-i>".to_string())
        );

        let key_n = Key::Character("n".into());
        assert_eq!(
            format_iced_key(&key_n, &ctrl_mods, None),
            Some("<C-n>".to_string())
        );

        let key_p = Key::Character("p".into());
        assert_eq!(
            format_iced_key(&key_p, &ctrl_mods, None),
            Some("<C-p>".to_string())
        );

        assert_eq!(
            format_iced_key(&Key::Named(Named::Tab), &ctrl_mods, None),
            Some("<C-Tab>".to_string())
        );
    }

    #[test]
    fn test_neovim_key_parsing_and_roundtrip() {
        let keys = vec![
            "<CR>", "<BS>", "<Esc>", "<Space>", "<Tab>", "<S-Tab>", "<C-Tab>",
            "<C-w>", "<C-o>", "<C-i>", "<A-Space>", "a", "G", "5",
        ];
        for k in keys {
            let stroke = KeyStroke::from_neovim_string(k)
                .unwrap_or_else(|| panic!("Failed to parse {}", k));
            assert_eq!(stroke.to_neovim_string(), k, "Mismatch on roundtrip for {}", k);
        }
    }
}
