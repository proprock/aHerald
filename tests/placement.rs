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
