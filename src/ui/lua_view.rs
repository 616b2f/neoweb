use crate::ui::style::*;
use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Element, Length};
use mlua::Table;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

static IMAGE_CACHE: LazyLock<Mutex<HashMap<u64, iced::widget::image::Handle>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn get_or_create_image_handle(bytes: &[u8]) -> iced::widget::image::Handle {
    use std::hash::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let key = hasher.finish();

    let mut cache = IMAGE_CACHE.lock().unwrap();
    if cache.len() > 256 {
        cache.clear();
    }
    cache
        .entry(key)
        .or_insert_with(|| iced::widget::image::Handle::from_bytes(bytes.to_vec()))
        .clone()
}

/// Represents a parsed node from Lua's declarative Iced UI primitives
#[derive(Debug, Clone, PartialEq)]
pub enum LuaUiNode {
    Text {
        content: String,
        size: u32,
        color: String,
        width: Option<Length>,
        height: Option<Length>,
        wrapping: Option<String>,
        align: Option<String>,
    },
    Space {
        width: Option<Length>,
        height: Option<Length>,
    },
    Row {
        children: Vec<LuaUiNode>,
        spacing: f32,
        align: String,
        width: Option<Length>,
        height: Option<Length>,
    },
    Column {
        children: Vec<LuaUiNode>,
        spacing: f32,
        align: String,
        width: Option<Length>,
        height: Option<Length>,
    },
    Container {
        child: Box<LuaUiNode>,
        padding: Option<(f32, f32)>,
        width: Option<Length>,
        height: Option<Length>,
        style: String,
    },
    Button {
        child: Box<LuaUiNode>,
        padding: (f32, f32),
        width: Option<Length>,
        height: Option<Length>,
        style: String,
        callback_id: Option<usize>,
    },
    Image {
        handle: iced::widget::image::Handle,
        width: Option<Length>,
        height: Option<Length>,
    },
    Spinner {
        width: Option<Length>,
        height: Option<Length>,
        circle_radius: Option<f32>,
    },
}

fn parse_length(val: Option<mlua::Value>) -> Option<Length> {
    match val {
        Some(mlua::Value::String(s)) => {
            let str_val = s.to_str().ok()?;
            match &str_val[..] {
                "fill" => Some(Length::Fill),
                "shrink" => Some(Length::Shrink),
                _ => None,
            }
        }
        Some(mlua::Value::Integer(n)) => Some(Length::Fixed(n as f32)),
        Some(mlua::Value::Number(n)) => Some(Length::Fixed(n as f32)),
        _ => None,
    }
}

fn parse_padding(tbl: &Table) -> Option<(f32, f32)> {
    if let Ok(p_num) = tbl.get::<f32>("padding") {
        return Some((p_num, p_num));
    }
    if let Ok(p_tbl) = tbl.get::<Table>("padding") {
        let v: f32 = p_tbl.get(1).unwrap_or(0.0);
        let h: f32 = p_tbl.get(2).unwrap_or(0.0);
        return Some((v, h));
    }
    None
}

impl LuaUiNode {
    pub fn from_lua(tbl: &Table) -> Option<Self> {
        let node_type: String = tbl.get("type").ok()?;
        match node_type.as_str() {
            "text" => {
                let content: String = tbl.get("content").unwrap_or_default();
                let size: u32 = tbl.get("size").unwrap_or(12);
                let color: String = tbl.get("color").unwrap_or_else(|_| "primary".to_string());
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                let wrapping: Option<String> = tbl.get("wrapping").ok();
                let align: Option<String> = tbl.get("align").ok();
                Some(Self::Text {
                    content,
                    size,
                    color,
                    width,
                    height,
                    wrapping,
                    align,
                })
            }
            "space" => {
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                Some(Self::Space { width, height })
            }
            "row" => {
                let children_tbl: Table = tbl.get("children").ok()?;
                let mut children = Vec::new();
                for val in children_tbl.sequence_values::<Table>() {
                    if let Ok(c) = val
                        && let Some(child_node) = Self::from_lua(&c)
                    {
                        children.push(child_node);
                    }
                }
                let spacing: f32 = tbl.get("spacing").unwrap_or(0.0);
                let align: String = tbl.get("align").unwrap_or_else(|_| "center".to_string());
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                Some(Self::Row {
                    children,
                    spacing,
                    align,
                    width,
                    height,
                })
            }
            "column" => {
                let children_tbl: Table = tbl.get("children").ok()?;
                let mut children = Vec::new();
                for val in children_tbl.sequence_values::<Table>() {
                    if let Ok(c) = val
                        && let Some(child_node) = Self::from_lua(&c)
                    {
                        children.push(child_node);
                    }
                }
                let spacing: f32 = tbl.get("spacing").unwrap_or(0.0);
                let align: String = tbl.get("align").unwrap_or_else(|_| "start".to_string());
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                Some(Self::Column {
                    children,
                    spacing,
                    align,
                    width,
                    height,
                })
            }
            "container" => {
                let child_tbl: Table = tbl.get("child").ok()?;
                let child = Box::new(Self::from_lua(&child_tbl)?);
                let padding = parse_padding(tbl);
                let style: String = tbl.get("style").unwrap_or_else(|_| "none".to_string());
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                Some(Self::Container {
                    child,
                    padding,
                    width,
                    height,
                    style,
                })
            }
            "button" => {
                let child_tbl: Table = tbl.get("child").ok()?;
                let child = Box::new(Self::from_lua(&child_tbl)?);
                let padding = parse_padding(tbl).unwrap_or((2.0, 6.0));
                let style: String = tbl
                    .get("style")
                    .unwrap_or_else(|_| "item_normal".to_string());
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                let callback_id: Option<usize> = tbl.get("callback_id").ok();
                Some(Self::Button {
                    child,
                    padding,
                    width,
                    height,
                    style,
                    callback_id,
                })
            }
            "image" => {
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                if let Ok(lua_str) = tbl.get::<mlua::LuaString>("data")
                    && !lua_str.as_bytes().is_empty()
                {
                    let handle = get_or_create_image_handle(&lua_str.as_bytes());
                    return Some(Self::Image {
                        handle,
                        width,
                        height,
                    });
                }
                if let Ok(path_str) = tbl.get::<String>("path") {
                    let handle = iced::widget::image::Handle::from_path(path_str);
                    return Some(Self::Image {
                        handle,
                        width,
                        height,
                    });
                }
                None
            }
            "spinner" => {
                let width = parse_length(tbl.get("width").ok());
                let height = parse_length(tbl.get("height").ok());
                let circle_radius: Option<f32> = tbl.get("circle_radius").ok();
                Some(Self::Spinner {
                    width,
                    height,
                    circle_radius,
                })
            }
            _ => None,
        }
    }
}

pub fn render_lua_node<Message: Clone + 'static>(
    node: &LuaUiNode,
    on_callback: impl Fn(usize) -> Message + Copy + 'static,
) -> Element<'static, Message> {
    match node {
        LuaUiNode::Text {
            content,
            size,
            color,
            width,
            height,
            wrapping,
            align,
        } => {
            let col = match color.as_str() {
                "yellow" => ACCENT_YELLOW,
                "blue" => ACCENT_BLUE,
                "muted" => FG_MUTED,
                "green" => ACCENT_GREEN,
                "red" => ACCENT_RED,
                "purple" => ACCENT_PURPLE,
                _ => FG_PRIMARY,
            };
            let mut t = text(content.clone()).size(*size as f32).color(col);
            let wrap = match wrapping.as_deref() {
                Some("word") => iced::widget::text::Wrapping::Word,
                Some("glyph") => iced::widget::text::Wrapping::Glyph,
                _ => iced::widget::text::Wrapping::None,
            };
            t = t.wrapping(wrap);
            match align.as_deref() {
                Some("center") => {
                    t = t.align_x(Alignment::Center).align_y(Alignment::Center);
                }
                Some("end") | Some("right") => {
                    t = t.align_x(Alignment::End);
                }
                _ => {}
            }
            if let Some(w) = width {
                t = t.width(*w);
            }
            if let Some(h) = height {
                t = t.height(*h);
            }
            t.into()
        }
        LuaUiNode::Space { width, height } => {
            let mut s = Space::new();
            if let Some(w) = width {
                s = s.width(*w);
            }
            if let Some(h) = height {
                s = s.height(*h);
            }
            s.into()
        }
        LuaUiNode::Row {
            children,
            spacing,
            align,
            width,
            height,
        } => {
            let mut r = row![].spacing(*spacing);
            match align.as_str() {
                "start" => {
                    r = r.align_y(Alignment::Start);
                }
                "end" => {
                    r = r.align_y(Alignment::End);
                }
                _ => {
                    r = r.align_y(Alignment::Center);
                }
            }
            if let Some(w) = width {
                r = r.width(*w);
            }
            if let Some(h) = height {
                r = r.height(*h);
            }
            for c in children {
                r = r.push(render_lua_node(c, on_callback));
            }
            r.into()
        }
        LuaUiNode::Column {
            children,
            spacing,
            align,
            width,
            height,
        } => {
            let mut col = column![].spacing(*spacing);
            match align.as_str() {
                "center" => {
                    col = col.align_x(Alignment::Center);
                }
                "end" => {
                    col = col.align_x(Alignment::End);
                }
                _ => {
                    col = col.align_x(Alignment::Start);
                }
            }
            if let Some(w) = width {
                col = col.width(*w);
            }
            if let Some(h) = height {
                col = col.height(*h);
            }
            for c in children {
                col = col.push(render_lua_node(c, on_callback));
            }
            col.into()
        }
        LuaUiNode::Container {
            child,
            padding,
            width,
            height,
            style,
        } => {
            let inner = render_lua_node(child, on_callback);
            let mut c = container(inner).align_y(Alignment::Center);
            if let Some((v, h)) = padding {
                c = c.padding([*v, *h]);
            }
            if let Some(w) = width {
                c = c.width(*w);
            }
            if let Some(h) = height {
                c = c.height(*h);
            }
            match style.as_str() {
                "surface" => {
                    c = c.style(|_theme| container::Style {
                        background: Some(BG_SURFACE.into()),
                        border: iced::Border {
                            color: BORDER_COLOR,
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    });
                }
                "active" => {
                    c = c.style(|_theme| container::Style {
                        background: Some(BG_ACTIVE.into()),
                        border: iced::Border {
                            radius: 3.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                }
                "dark" => {
                    c = c.style(|_theme| container::Style {
                        background: Some(BG_DARK.into()),
                        ..Default::default()
                    });
                }
                "prompt_bar" | "status_bar" | "tab_strip" => {
                    c = c.style(|_theme| container::Style {
                        background: Some(BG_DARK.into()),
                        border: iced::Border {
                            color: BORDER_COLOR,
                            width: 1.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                }
                "tab_active" => {
                    c = c.style(|_theme| container::Style {
                        background: Some(BG_SURFACE.into()),
                        border: iced::Border {
                            color: ACCENT_BLUE,
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    });
                }
                "tab_inactive" => {
                    c = c.style(|_theme| container::Style {
                        background: Some(BG_DARK.into()),
                        border: iced::Border {
                            color: BORDER_COLOR,
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    });
                }
                _ => {}
            }
            c.into()
        }
        LuaUiNode::Button {
            child,
            padding,
            width,
            height,
            style,
            callback_id,
        } => {
            let inner = render_lua_node(child, on_callback);
            let style_str = style.clone();
            let mut inner_c = container(inner).align_y(Alignment::Center);
            if style_str == "close_btn" {
                inner_c = inner_c.align_x(Alignment::Center);
            } else {
                inner_c = inner_c.align_x(Alignment::Start);
            }
            if let Some(w) = width {
                inner_c = inner_c.width(*w);
            }
            if let Some(h) = height {
                inner_c = inner_c.height(*h);
            }
            let mut b =
                button(inner_c)
                    .padding([padding.0, padding.1])
                    .style(move |_theme, status| {
                        let is_hovered = matches!(status, iced::widget::button::Status::Hovered);
                        match style_str.as_str() {
                            "item_selected" => button::Style {
                                background: Some(BG_ACTIVE.into()),
                                text_color: FG_PRIMARY,
                                border: iced::Border {
                                    radius: 3.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            "item_normal" => button::Style {
                                background: if is_hovered {
                                    Some(BG_ACTIVE.into())
                                } else {
                                    None
                                },
                                text_color: FG_MUTED,
                                border: iced::Border {
                                    radius: 3.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            "tab_active" => button::Style {
                                background: Some(BG_SURFACE.into()),
                                text_color: FG_PRIMARY,
                                border: iced::Border {
                                    color: ACCENT_BLUE,
                                    width: 1.0,
                                    radius: 0.0.into(),
                                },
                                ..Default::default()
                            },
                            "tab_inactive" => button::Style {
                                background: if is_hovered {
                                    Some(BG_ACTIVE.into())
                                } else {
                                    Some(BG_DARK.into())
                                },
                                text_color: if is_hovered { FG_PRIMARY } else { FG_MUTED },
                                border: iced::Border {
                                    color: BORDER_COLOR,
                                    width: 1.0,
                                    radius: 0.0.into(),
                                },
                                ..Default::default()
                            },
                            "close_btn" => button::Style {
                                background: if is_hovered {
                                    Some(BG_ACTIVE.into())
                                } else {
                                    None
                                },
                                text_color: if is_hovered { ACCENT_RED } else { FG_MUTED },
                                border: iced::Border {
                                    radius: 3.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            "none" => button::Style {
                                background: None,
                                text_color: FG_PRIMARY,
                                ..Default::default()
                            },
                            _ => {
                                if is_hovered {
                                    button::Style {
                                        background: Some(BG_ACTIVE.into()),
                                        text_color: FG_PRIMARY,
                                        border: iced::Border {
                                            radius: 3.0.into(),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }
                                } else {
                                    button::Style {
                                        background: None,
                                        text_color: FG_PRIMARY,
                                        ..Default::default()
                                    }
                                }
                            }
                        }
                    });
            if let Some(w) = width {
                b = b.width(*w);
            }
            if let Some(h) = height {
                b = b.height(*h);
            }
            if let Some(id) = callback_id {
                b = b.on_press(on_callback(*id));
            }
            b.into()
        }
        LuaUiNode::Image {
            handle,
            width,
            height,
        } => {
            let mut img = iced::widget::image(handle.clone());
            if let Some(w) = width {
                img = img.width(*w);
            }
            if let Some(h) = height {
                img = img.height(*h);
            }
            img.into()
        }
        LuaUiNode::Spinner {
            width,
            height,
            circle_radius,
        } => {
            let mut s = iced_aw::Spinner::new();
            if let Some(w) = width {
                s = s.width(*w);
            }
            if let Some(h) = height {
                s = s.height(*h);
            }
            if let Some(r) = circle_radius {
                s = s.circle_radius(*r);
            }
            s.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestMsg {
        Clicked(usize),
    }

    #[test]
    fn test_render_lua_node_text_variants() {
        let colors = [
            "primary",
            "muted",
            "yellow",
            "red",
            "blue",
            "green",
            "magenta",
            "orange",
            "cyan",
            "white",
            "#1a1b26",
            "#e0af68",
            "unknown_color",
        ];
        let wraps = [None, Some("none"), Some("word"), Some("glyph")];
        let aligns = [
            None,
            Some("center"),
            Some("start"),
            Some("end"),
            Some("right"),
        ];

        for color in colors {
            for wrap in &wraps {
                for align in &aligns {
                    let node = LuaUiNode::Text {
                        content: "Test Label".to_string(),
                        size: 13,
                        color: color.to_string(),
                        width: Some(Length::Fill),
                        height: Some(Length::Fixed(20.0)),
                        wrapping: wrap.map(|s| s.to_string()),
                        align: align.map(|s| s.to_string()),
                    };
                    let _elem: Element<'_, TestMsg> = render_lua_node(&node, TestMsg::Clicked);
                }
            }
        }
    }

    #[test]
    fn test_render_lua_node_space() {
        let node = LuaUiNode::Space {
            width: Some(Length::Fixed(10.0)),
            height: Some(Length::Fill),
        };
        let _elem: Element<'_, TestMsg> = render_lua_node(&node, TestMsg::Clicked);
    }

    #[test]
    fn test_render_lua_node_containers_row_column() {
        let child = LuaUiNode::Text {
            content: "Inner".to_string(),
            size: 12,
            color: "primary".to_string(),
            width: None,
            height: None,
            wrapping: None,
            align: None,
        };

        let row = LuaUiNode::Row {
            children: vec![child.clone()],
            spacing: 4.0,
            align: "center".to_string(),
            width: Some(Length::Fill),
            height: Some(Length::Fixed(28.0)),
        };
        let _elem_row: Element<'_, TestMsg> = render_lua_node(&row, TestMsg::Clicked);

        let col = LuaUiNode::Column {
            children: vec![child.clone()],
            spacing: 2.0,
            align: "start".to_string(),
            width: Some(Length::Shrink),
            height: None,
        };
        let _elem_col: Element<'_, TestMsg> = render_lua_node(&col, TestMsg::Clicked);

        let styles = [
            "dark",
            "surface",
            "prompt_bar",
            "status_bar",
            "active",
            "item_selected",
            "item_normal",
            "none",
            "unknown_style",
        ];
        for style in styles {
            let container_node = LuaUiNode::Container {
                child: Box::new(child.clone()),
                padding: Some((4.0, 8.0)),
                width: Some(Length::Fill),
                height: Some(Length::Fixed(28.0)),
                style: style.to_string(),
            };
            let _elem_cont: Element<'_, TestMsg> =
                render_lua_node(&container_node, TestMsg::Clicked);
        }
    }

    #[test]
    fn test_render_lua_node_buttons() {
        let child = LuaUiNode::Text {
            content: "×".to_string(),
            size: 13,
            color: "primary".to_string(),
            width: None,
            height: None,
            wrapping: None,
            align: Some("center".to_string()),
        };

        // Close button: style == "close_btn", centered alignment
        let close_btn = LuaUiNode::Button {
            child: Box::new(child.clone()),
            padding: (0.0, 0.0),
            width: Some(Length::Fixed(16.0)),
            height: Some(Length::Fixed(16.0)),
            style: "close_btn".to_string(),
            callback_id: Some(42),
        };
        let _elem_close: Element<'_, TestMsg> = render_lua_node(&close_btn, TestMsg::Clicked);

        // Tab button: style == "tab_active", left-aligned inner container
        let tab_btn = LuaUiNode::Button {
            child: Box::new(child.clone()),
            padding: (2.0, 6.0),
            width: Some(Length::Fill),
            height: Some(Length::Fixed(28.0)),
            style: "tab_active".to_string(),
            callback_id: None,
        };
        let _elem_tab: Element<'_, TestMsg> = render_lua_node(&tab_btn, TestMsg::Clicked);
    }

    #[test]
    fn test_render_lua_node_spinner_and_image() {
        let spinner = LuaUiNode::Spinner {
            width: Some(Length::Fixed(16.0)),
            height: Some(Length::Fixed(16.0)),
            circle_radius: Some(1.5),
        };
        let _elem_spin: Element<'_, TestMsg> = render_lua_node(&spinner, TestMsg::Clicked);

        // 1x1 transparent PNG
        let png_bytes = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let handle = get_or_create_image_handle(&png_bytes);
        let img = LuaUiNode::Image {
            handle,
            width: Some(Length::Fixed(16.0)),
            height: Some(Length::Fixed(16.0)),
        };
        let _elem_img: Element<'_, TestMsg> = render_lua_node(&img, TestMsg::Clicked);
    }
}
