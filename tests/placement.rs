use aherald::placement::{Area, layout};

#[test]
fn positions_use_the_selected_work_area_and_scale_margins() {
    let a = Area {
        left: -1920,
        top: -200,
        right: 0,
        bottom: 880,
    };
    assert_eq!(
        layout(a, (320, 110), 1.0, "center", (0, 0)),
        (-1120, 285, 320, 110)
    );
    assert_eq!(
        layout(a, (320, 110), 1.5, "top_center", (0, 0)),
        (-1200, -164, 480, 165)
    );
    assert_eq!(
        layout(a, (320, 110), 1.0, "bottom_right", (0, 0)),
        (-344, 746, 320, 110)
    );
    assert_eq!(
        layout(a, (320, 110), 1.0, "near_cursor", (-10, 870)),
        (-320, 770, 320, 110)
    );
}

#[test]
fn oversized_cards_fit_without_clipping_or_leaving_the_work_area() {
    let a = Area {
        left: -100,
        top: 10,
        right: 100,
        bottom: 110,
    };
    assert_eq!(
        layout(a, (400, 100), 2.0, "center", (0, 0)),
        (-100, 35, 200, 50)
    );
    assert_eq!(
        layout(a, (100, 400), 2.0, "near_cursor", (i32::MAX, i32::MAX)),
        (75, 10, 25, 100)
    );
}

mod stacking {
    use aherald::placement::{Area, stack};
    const AREA: Area = Area {
        left: 0,
        top: 0,
        right: 1000,
        bottom: 600,
    };
    fn card(x: i32, y: i32) -> Area {
        Area {
            left: x,
            top: y,
            right: x + 300,
            bottom: y + 100,
        }
    }

    #[test]
    fn free_spot_is_kept() {
        assert_eq!(
            stack((350, 24, 300, 100), AREA, &[], "top_center", 8),
            (350, 24)
        );
        // A card elsewhere on screen does not push this one.
        assert_eq!(
            stack((350, 24, 300, 100), AREA, &[card(0, 400)], "top_center", 8),
            (350, 24)
        );
    }

    #[test]
    fn top_anchored_cards_stack_downward_past_every_occupied_card() {
        let occupied = [card(350, 132), card(350, 24)];
        assert_eq!(
            stack((350, 24, 300, 100), AREA, &occupied, "top_center", 8),
            (350, 240)
        );
    }

    #[test]
    fn bottom_right_cards_stack_upward() {
        let occupied = [card(676, 476)];
        assert_eq!(
            stack((676, 476, 300, 100), AREA, &occupied, "bottom_right", 8),
            (676, 368)
        );
    }

    #[test]
    fn a_full_work_area_falls_back_to_the_base_spot() {
        let occupied = [
            card(350, 24),
            card(350, 132),
            card(350, 240),
            card(350, 348),
            card(350, 456),
        ];
        assert_eq!(
            stack((350, 24, 300, 100), AREA, &occupied, "top_center", 8),
            (350, 24)
        );
    }
}
