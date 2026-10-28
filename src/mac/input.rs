//! What a click on the card does to the queue. Pure: the view reports where it was
//! clicked, this decides what that means, so the AppKit side stays a thin adapter.
use crate::{
    card::{Action, Card},
    queue::Queue,
};

/// The card as it is drawn, in backing pixels.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub text_left: i32,
}

/// Where `point` (points from the card's top-left corner) lies in the drawn card, given
/// the window's size in points.
pub fn to_pixels(point: (f64, f64), window: (f64, f64), layout: &Layout) -> (i32, i32) {
    if window.0 <= 0.0 || window.1 <= 0.0 {
        return (0, 0);
    }
    (
        (point.0 * f64::from(layout.width) / window.0) as i32,
        (point.1 * f64::from(layout.height) / window.1) as i32,
    )
}

/// Whether card pixel `(x, y)` is on the card's body rather than on a button or off the card.
pub fn on_body(queue: &Queue, click_to_dismiss: bool, layout: &Layout, (x, y): (i32, i32)) -> bool {
    let mut controls = Card::new(
        "",
        queue.selected_index(),
        queue.items().len(),
        click_to_dismiss,
    );
    controls.text_left = Some(layout.text_left);
    queue.selected().is_some()
        && controls.body_hit_test(layout.width, layout.height, layout.scale, x, y)
}

/// Applies a click at card pixel `(x, y)` to the queue. With `click_to_dismiss` the whole
/// card closes it; otherwise only the buttons act. Returns whether it hit a control.
pub fn click(
    queue: &mut Queue,
    click_to_dismiss: bool,
    layout: &Layout,
    (x, y): (i32, i32),
) -> bool {
    let mut controls = Card::new(
        "",
        queue.selected_index(),
        queue.items().len(),
        click_to_dismiss,
    );
    controls.text_left = Some(layout.text_left);
    if queue.selected().is_none() {
        return false;
    }
    match controls.hit_test(layout.width, layout.height, layout.scale, x, y) {
        Some(Action::Previous) => queue.next(-1),
        Some(Action::Next) => queue.next(1),
        Some(Action::Dismiss) => queue.dismiss(),
        None => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;

    fn layout() -> Layout {
        Layout {
            width: 640,
            height: 300,
            scale: 2.0,
            text_left: 44,
        }
    }
    fn queue(count: usize) -> Queue {
        let mut queue = Queue::new(8, 0);
        let kinds = ["turn_complete", "input_required", "error", "interrupted"];
        for (i, kind) in kinds.iter().take(count).enumerate() {
            let event = Event::demo(kind).unwrap();
            assert!(queue.push(event, i as u64, 0).is_accepted());
        }
        queue
    }
    /// Centre of a button of the card the queue would show.
    fn centre(queue: &Queue, click_to_dismiss: bool, action: Action) -> (i32, i32) {
        let l = layout();
        let mut card = Card::new(
            "",
            queue.selected_index(),
            queue.items().len(),
            click_to_dismiss,
        );
        card.text_left = Some(l.text_left);
        let (_, a) = card
            .buttons(l.width, l.height, l.scale)
            .into_iter()
            .find(|(a, _)| *a == action)
            .unwrap();
        ((a.left + a.right) / 2, (a.top + a.bottom) / 2)
    }

    #[test]
    fn points_scale_to_card_pixels_in_both_axes() {
        let l = layout();
        assert_eq!(to_pixels((160.0, 75.0), (320.0, 150.0), &l), (320, 150));
        assert_eq!(to_pixels((0.0, 0.0), (320.0, 150.0), &l), (0, 0));
        // A window shrunk to fit the display maps back to the full card.
        assert_eq!(to_pixels((80.0, 75.0), (160.0, 150.0), &l), (320, 150));
        assert_eq!(to_pixels((1.0, 1.0), (0.0, 0.0), &l), (0, 0));
    }

    #[test]
    fn the_dismiss_button_removes_the_selected_card() {
        let mut q = queue(2);
        let at = centre(&q, false, Action::Dismiss);
        assert!(click(&mut q, false, &layout(), at));
        assert_eq!(q.items().len(), 1);
    }

    #[test]
    fn the_arrows_move_the_selection_both_ways_and_wrap() {
        let mut q = queue(3);
        let first = q.selected().unwrap().id;
        let next = centre(&q, false, Action::Next);
        assert!(click(&mut q, false, &layout(), next));
        assert_ne!(q.selected().unwrap().id, first);
        let previous = centre(&q, false, Action::Previous);
        assert!(click(&mut q, false, &layout(), previous));
        assert_eq!(q.selected().unwrap().id, first);
        assert!(click(&mut q, false, &layout(), previous));
        assert_eq!(q.selected_index(), 2);
        assert_eq!(q.items().len(), 3);
    }

    #[test]
    fn the_body_does_nothing_while_the_dismiss_button_is_shown() {
        let mut q = queue(2);
        assert!(!click(&mut q, false, &layout(), (300, 60)));
        assert_eq!(q.items().len(), 2);
    }

    #[test]
    fn click_to_dismiss_closes_the_card_from_anywhere_on_it() {
        let mut q = queue(2);
        assert!(click(&mut q, true, &layout(), (300, 60)));
        assert_eq!(q.items().len(), 1);
    }

    #[test]
    fn clicks_outside_the_card_and_on_an_empty_queue_do_nothing() {
        let mut q = queue(2);
        assert!(!click(&mut q, true, &layout(), (-1, 10)));
        assert!(!click(&mut q, true, &layout(), (640, 10)));
        assert_eq!(q.items().len(), 2);
        let mut empty = queue(0);
        assert!(!click(&mut empty, true, &layout(), (300, 60)));
    }

    #[test]
    fn only_the_body_counts_as_the_body() {
        let q = queue(3);
        let l = layout();
        assert!(on_body(&q, true, &l, (300, 60)));
        assert!(!on_body(&q, true, &l, centre(&q, true, Action::Next)));
        assert!(!on_body(&q, false, &l, centre(&q, false, Action::Dismiss)));
        assert!(!on_body(&q, true, &l, (-1, 10)));
        assert!(!on_body(&q, true, &l, (640, 10)));
        assert!(!on_body(&queue(0), true, &l, (300, 60)));
    }
}
