use super::{row_widget_counts_from_observations, RowWidgetCounts};

#[test]
fn zero_height_rows_are_present_but_not_allocated() {
    let observations = std::iter::repeat_n(("row", 0), 206);

    assert_eq!(
        row_widget_counts_from_observations(observations),
        RowWidgetCounts {
            present: 206,
            allocated: 0,
        }
    );
}

#[test]
fn an_allocated_row_ends_the_healthy_probe() {
    let observations = [("box", 0), ("row", 0), ("row", 53), ("row", 0)];

    assert_eq!(
        row_widget_counts_from_observations(observations),
        RowWidgetCounts {
            present: 2,
            allocated: 1,
        }
    );
}
