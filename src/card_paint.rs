//! The look of a card, independent of any window system: the shapes as BGRA pixels and the
//! text as a list of runs for the platform's text engine to draw on top.
//!
//! Windows draws the runs with GDI and macOS with AppKit; both start from the same pixels
//! and the same run geometry, so the cards cannot drift apart.
use crate::{
    card::{Action, Area, Card, Heading},
    config::{Color, Config},
    events::Event,
};

/// Everything drawn on a card's base surface.
pub struct CardText<'a> {
    pub heading: &'a Heading,
    /// The event kind, drawn large on one row; with `body` the usage text instead.
    pub label: &'a str,
    /// `label` is several lines of body text rather than the event kind.
    pub body: bool,
    pub index: usize,
    pub count: usize,
    pub dismiss_button: bool,
    /// Coarse age of the event ("2 min ago"); absent for cards that live seconds.
    pub age: Option<&'a str>,
    pub color: Color,
    pub background_color: Color,
    /// Outline of the card and frame of its buttons; absent draws no card outline.
    pub border: Option<Color>,
}

/// What an event card shows: its heading, kind, queue position and (resident) age.
pub fn event_card_text<'a>(
    config: &Config,
    event: &Event,
    controls: &'a Card,
    age: Option<&'a str>,
) -> CardText<'a> {
    CardText {
        heading: &controls.heading,
        label: event.kind.label(),
        body: false,
        index: controls.index,
        count: controls.count,
        dismiss_button: controls.dismiss_button,
        age,
        color: config.event_color(event.kind),
        background_color: config.event_background_color(event.kind),
        border: config.event_border_color(event.kind),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// One piece of text to draw: inside `area`, `size` pixels tall, in a grey of `gray`.
#[derive(Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// Font height in device pixels (logical size times the scale).
    pub size: f64,
    pub gray: u8,
    pub area: Area,
    pub align: Align,
    /// Wrap onto further lines instead of keeping one line.
    pub wrap: bool,
    /// Centre the line between the top and bottom of `area`.
    pub center_vertical: bool,
    /// Replace an overlong single line's end with an ellipsis.
    pub ellipsis: bool,
}

pub struct Painted {
    /// Straight (not premultiplied) BGRA of the card's shapes, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
    pub text: Vec<TextRun>,
}

impl Area {
    fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

fn bgra(color: Color, alpha: u8) -> [u8; 4] {
    [color.0[2], color.0[1], color.0[0], alpha]
}

/// Draws a button face: a slightly lighter fill with a one-pixel border and rounded
/// corners, over the card background (BGRA, not yet premultiplied).
fn draw_button_frame(
    pixels: &mut [u8],
    width: i32,
    area: &Area,
    background: &[u8; 4],
    border_color: Option<Color>,
    scale: f64,
) {
    let lighten =
        |value: u8, amount: u32| (value as u32 + (255 - value as u32) * amount / 100) as u8;
    let fill = [
        lighten(background[0], 8),
        lighten(background[1], 8),
        lighten(background[2], 8),
        255,
    ];
    let border = match border_color {
        Some(c) => bgra(c, 255),
        None => [
            lighten(background[0], 32),
            lighten(background[1], 32),
            lighten(background[2], 32),
            255,
        ],
    };
    let (w, h) = (area.right - area.left, area.bottom - area.top);
    let radius = ((4.0 * scale) as i32).min(w / 2).min(h / 2);
    let line = (scale.round() as i32).max(1);
    // Distance of a pixel outside the rounded rectangle (negative inside).
    let inside = |x: i32, y: i32, inset: i32| {
        let (left, top) = (inset, inset);
        let (right, bottom) = (w - inset, h - inset);
        let r = (radius - inset).max(0);
        let cx = x.clamp(left + r, right - r - 1);
        let cy = y.clamp(top + r, bottom - r - 1);
        x >= left
            && y >= top
            && x < right
            && y < bottom
            && (x - cx).pow(2) + (y - cy).pow(2) <= r.pow(2)
    };
    for y in 0..h {
        for x in 0..w {
            let colour = if inside(x, y, line) {
                &fill
            } else if inside(x, y, 0) {
                &border
            } else {
                continue;
            };
            let i = (((area.top + y) * width + area.left + x) * 4) as usize;
            if let Some(pixel) = pixels.get_mut(i..i + 4) {
                pixel.copy_from_slice(colour);
            }
        }
    }
}

/// The remaining-seconds badge in a card's top-right corner.
pub fn countdown_run(width: i32, seconds: u64, scale: f64) -> TextRun {
    TextRun {
        text: format!("{seconds}s"),
        size: 16.0 * scale,
        gray: 0xf5,
        area: Area::new(
            width - (80.0 * scale) as i32,
            (16.0 * scale) as i32,
            width - (18.0 * scale) as i32,
            (40.0 * scale) as i32,
        ),
        align: Align::Right,
        wrap: false,
        center_vertical: false,
        ellipsis: false,
    }
}

/// Paints the shapes of a `width` x `height` card and lays out its text.
pub fn paint(width: i32, height: i32, scale: f64, text_left: i32, content: &CardText) -> Painted {
    let CardText {
        heading,
        label,
        body,
        index,
        count,
        dismiss_button,
        age,
        color,
        background_color,
        border,
    } = content;
    let controls = Card {
        heading: (*heading).clone(),
        index: *index,
        count: *count,
        navigation: *count > 1,
        dismiss_button: *dismiss_button,
        text_left: Some(text_left),
    };
    let buttons = controls.buttons(width, height, scale);
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let background = bgra(*background_color, 242);
    let radius = (18.0 * scale) as i32;
    let stripe = (8.0 * scale) as i32;
    for y in 0..height {
        for x in 0..width {
            let cx = x.clamp(radius, width - radius - 1);
            let cy = y.clamp(radius, height - radius - 1);
            let inside = (x - cx).pow(2) + (y - cy).pow(2) <= radius.pow(2);
            let i = ((y * width + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(if inside { &background } else { &[0, 0, 0, 0] });
            if inside && x < stripe {
                pixels[i..i + 3].copy_from_slice(&[color.0[2], color.0[1], color.0[0]]);
            }
        }
    }
    if let Some(border) = border {
        let outline = bgra(*border, 255);
        let line = (scale.round() as i32).max(1);
        let within = |x: i32, y: i32, inset: i32| {
            let r = (radius - inset).max(0);
            let cx = x.clamp(inset + r, width - inset - r - 1);
            let cy = y.clamp(inset + r, height - inset - r - 1);
            x >= inset
                && y >= inset
                && x < width - inset
                && y < height - inset
                && (x - cx).pow(2) + (y - cy).pow(2) <= r.pow(2)
        };
        for y in 0..height {
            for x in 0..width {
                if within(x, y, 0) && !within(x, y, line) {
                    let i = ((y * width + x) * 4) as usize;
                    pixels[i..i + 4].copy_from_slice(&outline);
                }
            }
        }
    }
    for (_, area) in &buttons {
        draw_button_frame(&mut pixels, width, area, &background, *border, scale);
    }

    let px = |v: f64| (v * scale) as i32;
    let mut text = Vec::new();
    let run = |text: &str, size: f64, gray: u8, area: Area, align: Align| TextRun {
        text: text.to_owned(),
        size: size * scale,
        gray,
        area,
        align,
        wrap: false,
        center_vertical: false,
        ellipsis: false,
    };
    // Row 1: harness and folder; the right edge keeps clear of the countdown badge.
    text.push(TextRun {
        ellipsis: true,
        ..run(
            &heading.title,
            18.0,
            0xf5,
            Area::new(text_left, px(14.0), width - px(82.0), px(38.0)),
            Align::Left,
        )
    });
    // Row 2 is reserved so the card never changes height; empty without a name.
    if let Some(name) = &heading.name {
        text.push(TextRun {
            ellipsis: true,
            ..run(
                name,
                16.0,
                0xc4,
                Area::new(text_left, px(38.0), width - px(18.0), px(60.0)),
                Align::Left,
            )
        });
    }
    // Row 3: the event kind, set apart and larger; usage cards show their body instead.
    if !label.is_empty() {
        let (size, top) = if *body { (18.0, 66.0) } else { (22.0, 72.0) };
        text.push(TextRun {
            wrap: *body,
            ellipsis: !*body,
            ..run(
                label,
                size,
                0xf5,
                Area::new(text_left, px(top), width - px(18.0), height - px(40.0)),
                Align::Left,
            )
        });
    }
    for (action, r) in &buttons {
        let caption = match action {
            Action::Previous => "<",
            Action::Next => ">",
            Action::Dismiss => "Dismiss",
        };
        text.push(TextRun {
            center_vertical: true,
            ..run(
                caption,
                15.0,
                0xf5,
                Area::new(r.left, r.top, r.right, r.bottom),
                Align::Center,
            )
        });
    }
    if let Some(counter) = controls.counter() {
        let r = controls.counter_area(width, height, scale);
        text.push(TextRun {
            center_vertical: true,
            ..run(
                &counter,
                15.0,
                0xc4,
                Area::new(r.left, r.top, r.right, r.bottom),
                Align::Center,
            )
        });
    }
    if let Some(age) = age {
        let r = controls.age_area(width, height, scale);
        text.push(TextRun {
            center_vertical: true,
            ellipsis: true,
            ..run(
                age,
                15.0,
                0xa0,
                Area::new(r.left, r.top, r.right, r.bottom),
                Align::Left,
            )
        });
    }
    Painted { pixels, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: i32 = 320;
    const H: i32 = 150;

    fn content<'a>(heading: &'a Heading, count: usize, dismiss: bool) -> CardText<'a> {
        CardText {
            heading,
            label: "Permission required",
            body: false,
            index: 0,
            count,
            dismiss_button: dismiss,
            age: None,
            color: Color([250, 180, 55]),
            background_color: crate::config::Notifications::default().background_color,
            border: None,
        }
    }

    fn pixel(painted: &Painted, x: i32, y: i32) -> [u8; 4] {
        let i = ((y * W + x) * 4) as usize;
        painted.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn the_corners_are_transparent_and_the_stripe_has_the_event_colour() {
        let heading = Heading::from("Codex");
        let painted = paint(W, H, 1.0, 22, &content(&heading, 1, false));
        assert_eq!(painted.pixels.len(), (W * H * 4) as usize);
        assert_eq!(pixel(&painted, 0, 0), [0, 0, 0, 0]);
        // BGRA of the stripe colour (R 250, G 180, B 55), left of the 8 px stripe width.
        assert_eq!(&pixel(&painted, 3, H / 2)[..3], &[55, 180, 250]);
        let background = crate::config::Notifications::default().background_color;
        assert_eq!(
            pixel(&painted, 40, H / 2),
            [background.0[2], background.0[1], background.0[0], 242]
        );
    }

    #[test]
    fn a_border_outlines_the_card_in_its_colour() {
        let heading = Heading::from("Codex");
        let plain = paint(W, H, 1.0, 22, &content(&heading, 1, false));
        let mut bordered = content(&heading, 1, false);
        bordered.border = Some(Color([255, 0, 0]));
        let bordered = paint(W, H, 1.0, 22, &bordered);
        assert_ne!(pixel(&plain, W / 2, 0), pixel(&bordered, W / 2, 0));
        assert_eq!(pixel(&bordered, W / 2, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn a_dismiss_button_is_framed_and_captioned() {
        let heading = Heading::from("Codex");
        let without = paint(W, H, 1.0, 22, &content(&heading, 1, false));
        let with = paint(W, H, 1.0, 22, &content(&heading, 1, true));
        assert_ne!(without.pixels, with.pixels);
        assert!(with.text.iter().any(|run| run.text == "Dismiss"));
        assert!(!without.text.iter().any(|run| run.text == "Dismiss"));
    }

    #[test]
    fn navigation_adds_arrows_and_a_counter_only_for_several_events() {
        let heading = Heading::from("Codex");
        let one = paint(W, H, 1.0, 22, &content(&heading, 1, false));
        let three = paint(W, H, 1.0, 22, &content(&heading, 3, false));
        let texts = |p: &Painted| p.text.iter().map(|r| r.text.clone()).collect::<Vec<_>>();
        assert!(!texts(&one).iter().any(|t| t == "<" || t == ">"));
        assert!(texts(&three).iter().any(|t| t == "<"));
        assert!(texts(&three).iter().any(|t| t == ">"));
        assert!(texts(&three).iter().any(|t| t == "1/3"));
    }

    #[test]
    fn the_rows_follow_the_heading_the_kind_and_the_age() {
        let heading = Heading::new("Codex", Some("repo"), Some("Fix the build"));
        let mut text = content(&heading, 1, false);
        text.age = Some("2 min ago");
        let painted = paint(W, H, 1.0, 22, &text);
        let by = |s: &str| painted.text.iter().find(|r| r.text == s).unwrap();
        assert_eq!(by("Codex, repo").area.top, 14);
        assert_eq!(by("Fix the build").area.top, 38);
        let kind = by("Permission required");
        assert_eq!((kind.area.top, kind.size, kind.wrap), (72, 22.0, false));
        assert!(by("2 min ago").ellipsis);
        // Every row keeps to the text column.
        assert!(painted.text.iter().take(3).all(|r| r.area.left == 22));
    }

    #[test]
    fn a_usage_body_wraps_in_the_body_font_and_a_nameless_card_has_no_second_row() {
        let heading = Heading::from("Usage");
        let mut text = content(&heading, 1, false);
        text.label = "5h 20%\n7d 40%";
        text.body = true;
        let painted = paint(W, H, 1.0, 22, &text);
        let body = painted.text.iter().find(|r| r.wrap).unwrap();
        assert_eq!((body.area.top, body.size), (66, 18.0));
        assert!(!body.ellipsis);
        assert_eq!(painted.text.iter().filter(|r| r.area.top == 38).count(), 0);
    }

    #[test]
    fn runs_scale_with_the_display() {
        let heading = Heading::from("Codex");
        let painted = paint(W * 2, H * 2, 2.0, 44, &content(&heading, 1, false));
        let title = &painted.text[0];
        assert_eq!((title.size, title.area.top), (36.0, 28));
    }

    #[test]
    fn the_countdown_badge_sits_in_the_top_right_corner() {
        let run = countdown_run(W, 7, 1.0);
        assert_eq!(run.text, "7s");
        assert_eq!(run.align, Align::Right);
        assert_eq!((run.area.right, run.area.top), (W - 18, 16));
        assert!(run.area.left > W / 2 && run.area.bottom > run.area.top);
    }

    #[test]
    fn an_event_card_shows_the_kind_the_queue_position_and_the_configured_colours() {
        let mut config = Config::default();
        let event = Event::demo("input_required").unwrap();
        config.events[event.kind].color = Some(Color([1, 2, 3]));
        let controls = Card::new("Codex", 1, 3, false);
        let text = event_card_text(&config, &event, &controls, Some("2 min ago"));
        assert_eq!(text.label, event.kind.label());
        assert_eq!((text.index, text.count), (1, 3));
        assert!(text.dismiss_button && !text.body);
        assert_eq!(text.age, Some("2 min ago"));
        assert_eq!(text.color, Color([1, 2, 3]));
        assert_eq!(
            text.background_color,
            config.event_background_color(event.kind)
        );
        assert_eq!(text.border, config.event_border_color(event.kind));
        assert_eq!(text.heading, &controls.heading);
    }
}
