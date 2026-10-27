use crate::engine::{HintItem, WebEngine};
use crate::ui::style::*;
use iced::widget::{container, text};
use iced::{Alignment, Element, Length};

pub const VIEWPORT_SCROLL_ID: &str = "viewport_scroll";

#[derive(Debug)]
struct HintCanvasOverlay {
    hints: Vec<HintItem>,
    hint_query: String,
}

impl<Message> iced::widget::canvas::Program<Message> for HintCanvasOverlay {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<iced::widget::canvas::Geometry> {
        let mut frame = iced::widget::canvas::Frame::new(renderer, bounds.size());
        for hint in &self.hints {
            let is_match = hint.key.starts_with(&self.hint_query);
            let bg_color = if is_match { ACCENT_YELLOW } else { FG_MUTED };

            let text_str = format!("[{}]", hint.key);
            let badge_w = (text_str.len() as f32 * 8.5 + 8.0).max(28.0);
            let badge_h = 18.0;

            let rect_path = iced::widget::canvas::Path::rectangle(
                iced::Point::new(hint.x, hint.y),
                iced::Size::new(badge_w, badge_h),
            );
            frame.fill(&rect_path, bg_color);
            frame.stroke(
                &rect_path,
                iced::widget::canvas::Stroke::default()
                    .with_color(BG_DARK)
                    .with_width(1.0),
            );
            frame.fill_text(iced::widget::canvas::Text {
                content: text_str,
                position: iced::Point::new(hint.x + 3.0, hint.y + 2.0),
                color: iced::Color::BLACK,
                size: 11.0.into(),
                font: iced::Font::MONOSPACE,
                ..Default::default()
            });
        }
        vec![frame.into_geometry()]
    }
}

pub fn view_viewport<'a, Message: Clone + 'static>(
    engine: &'a dyn WebEngine,
    is_follow_mode: bool,
    hints: Vec<HintItem>,
    hint_query: &str,
    on_bounds: Option<fn(iced::Rectangle) -> Message>,
) -> Element<'a, Message> {
    let frame_opt = engine.render_frame();
    let servo_elem = crate::ui::servo_shader::view_servo_texture(frame_opt.as_ref(), on_bounds);

    if let Some(_frame) = frame_opt {
        if is_follow_mode && !hints.is_empty() {
            let overlay = iced::widget::canvas(HintCanvasOverlay {
                hints,
                hint_query: hint_query.to_string(),
            })
            .width(Length::Fill)
            .height(Length::Fill);

            return iced::widget::stack![servo_elem, overlay].into();
        }
        servo_elem
    } else {
        let connecting = container(
            text(format!("Connecting to {}...", engine.uri()))
                .size(16)
                .color(FG_MUTED),
        )
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fill);

        iced::widget::stack![servo_elem, connecting].into()
    }
}
