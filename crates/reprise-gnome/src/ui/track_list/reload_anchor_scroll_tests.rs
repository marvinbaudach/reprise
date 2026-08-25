use super::*;

fn adoption_geometry(
    guard_position: u32,
    row_count: usize,
    section_count: usize,
    preceding_sections: usize,
    row_height: f64,
    before: f64,
) -> Option<ScrollAdoptionGeometry> {
    let represented_sections = section_count.max(preceding_sections);
    let mut starts = (0..preceding_sections)
        .map(|index| u32::try_from(index).unwrap())
        .collect::<Vec<_>>();
    starts.extend((preceding_sections..represented_sections).map(|index| {
        guard_position
            .checked_add(1)
            .and_then(|position| position.checked_add(u32::try_from(index).unwrap()))
            .unwrap()
    }));
    let row_height = RowHeight::new(row_height).unwrap();
    let layout = Rc::new(ListLayout::sectioned(row_height, row_height, starts));
    ScrollAdoptionGeometry::new(guard_position, row_count, section_count, layout, before)
}

#[test]
fn adoption_match_decisions_are_pinned_across_concrete_inputs() {
    struct Case {
        name: &'static str,
        geometry: ScrollAdoptionGeometry,
        candidate: f64,
        lower: f64,
        upper: f64,
        page_size: f64,
        expected: bool,
    }

    let cases = [
        Case {
            name: "realistic sectioned queue with fractional rows",
            geometry: adoption_geometry(1_101, 2_276, 2, 2, 34.5, 38_000.0).unwrap(),
            candidate: 38_056.5,
            lower: 0.0,
            upper: 78_594.0,
            page_size: 249.0,
            expected: true,
        },
        Case {
            name: "the previous value is not adopted",
            geometry: adoption_geometry(1_101, 2_276, 2, 2, 34.5, 38_000.0).unwrap(),
            candidate: 38_000.0,
            lower: 0.0,
            upper: 78_594.0,
            page_size: 249.0,
            expected: false,
        },
        Case {
            name: "the lower adjustment edge clamps the request",
            geometry: adoption_geometry(0, 10, 1, 1, 10.0, 5.0).unwrap(),
            candidate: 20.0,
            lower: 20.0,
            upper: 110.0,
            page_size: 20.0,
            expected: true,
        },
        Case {
            name: "the upper adjustment edge clamps the request",
            geometry: adoption_geometry(9, 10, 1, 1, 10.0, 50.0).unwrap(),
            candidate: 80.0,
            lower: 0.0,
            upper: 110.0,
            page_size: 30.0,
            expected: true,
        },
        Case {
            name: "a sub-epsilon row shortfall keeps zero-height headers",
            geometry: adoption_geometry(5, 10, 1, 1, 10.0, 40.0).unwrap(),
            candidate: 50.0,
            lower: 0.0,
            upper: 99.75,
            page_size: 10.0,
            expected: true,
        },
    ];

    for case in cases {
        assert_eq!(
            case.geometry
                .matches(case.candidate, case.lower, case.upper, case.page_size,),
            case.expected,
            "{}",
            case.name
        );
    }
}

#[test]
fn adoption_rejects_zero_rows() {
    assert!(adoption_geometry(0, 0, 1, 1, 34.0, 0.0).is_none());
}

#[test]
fn adoption_rejects_zero_sections() {
    assert!(adoption_geometry(0, 1, 0, 0, 34.0, 0.0).is_none());
}

#[test]
fn adoption_rejects_more_preceding_sections_than_total_sections() {
    assert!(adoption_geometry(0, 1, 1, 2, 34.0, 0.0).is_none());
}

#[test]
fn adoption_rejects_a_guard_outside_the_rows() {
    assert!(adoption_geometry(1, 1, 1, 1, 34.0, 0.0).is_none());
}

#[test]
fn adoption_rejects_each_non_finite_adjustment_input() {
    let geometry = adoption_geometry(0, 1, 1, 1, 34.0, 0.0).unwrap();
    assert!(!geometry.matches(f64::NAN, 0.0, 70.0, 0.0));
    assert!(!geometry.matches(36.0, f64::NEG_INFINITY, 70.0, 0.0));
    assert!(!geometry.matches(36.0, 0.0, f64::INFINITY, 0.0));
    assert!(!geometry.matches(36.0, 0.0, 70.0, f64::NAN));

    let non_finite_before = adoption_geometry(0, 1, 1, 1, 34.0, f64::INFINITY).unwrap();
    assert!(!non_finite_before.matches(36.0, 0.0, 70.0, 0.0));
}

#[test]
fn adoption_rejects_an_upper_below_the_lower_bound() {
    let geometry = adoption_geometry(0, 1, 1, 1, 34.0, 0.0).unwrap();
    assert!(!geometry.matches(36.0, 71.0, 70.0, 0.0));
}

#[test]
fn adoption_rejects_a_negative_page_size() {
    let geometry = adoption_geometry(0, 1, 1, 1, 34.0, 0.0).unwrap();
    assert!(!geometry.matches(36.0, 0.0, 70.0, -1.0));
}

#[test]
fn adoption_rejects_an_upper_more_than_epsilon_shorter_than_the_rows() {
    let geometry = adoption_geometry(5, 10, 1, 1, 10.0, 40.0).unwrap();
    assert!(!geometry.matches(50.0, 0.0, 99.49, 10.0));
}

#[test]
fn adoption_accepts_only_the_value_explained_by_the_requested_guard_row() {
    let geometry = adoption_geometry(1_101, 2_276, 2, 2, 34.0, 37_454.0).unwrap();

    assert!(geometry.matches(37_488.0, 0.0, 77_438.0, 249.0));
    assert!(!geometry.matches(37_454.0, 0.0, 77_438.0, 249.0));
    assert!(!geometry.matches(36_000.0, 0.0, 77_438.0, 249.0));
}
