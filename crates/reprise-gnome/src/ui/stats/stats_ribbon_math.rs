#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct RibbonLayout {
    pub points: Vec<Point>,
    pub peak_index: Option<usize>,
    pub open_index: Option<usize>,
}

pub(in crate::ui) fn ribbon_layout(
    values: &[i64],
    width: f64,
    height: f64,
    open_index: Option<usize>,
) -> RibbonLayout {
    let maximum = values.iter().copied().max().unwrap_or(0).max(0);
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = if values.len() <= 1 {
                width / 2.0
            } else {
                index as f64 * width / (values.len() - 1) as f64
            };
            let magnitude = if maximum == 0 {
                0.0
            } else {
                (*value).max(0) as f64 / maximum as f64
            };
            Point {
                x,
                y: height - magnitude * height,
            }
        })
        .collect();
    let peak_index = (maximum > 0).then(|| {
        values
            .iter()
            .enumerate()
            .max_by(|(left_index, left), (right_index, right)| {
                left.cmp(right).then_with(|| right_index.cmp(left_index))
            })
            .map_or(0, |(index, _)| index)
    });
    RibbonLayout {
        points,
        peak_index,
        open_index: open_index.filter(|index| *index < values.len()),
    }
}

pub(in crate::ui) fn bucket_at_x(x: f64, width: f64, bucket_count: usize) -> Option<usize> {
    if bucket_count == 0 || width <= 0.0 || x < 0.0 || x >= width {
        return None;
    }
    Some(((x / width * bucket_count as f64).floor() as usize).min(bucket_count - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ribbon_area_path_spans_every_bucket() {
        let layout = ribbon_layout(&[10, 20, 5], 300.0, 100.0, None);

        assert_eq!(layout.points.len(), 3);
        assert_eq!(layout.points[0].x, 0.0);
        assert_eq!(layout.points[1].x, 150.0);
        assert_eq!(layout.points[2].x, 300.0);
    }

    #[test]
    fn ribbon_marks_the_open_bucket_and_the_peak() {
        let layout = ribbon_layout(&[10, 30, 20], 300.0, 100.0, Some(2));

        assert_eq!(layout.peak_index, Some(1));
        assert_eq!(layout.open_index, Some(2));
    }

    #[test]
    fn ribbon_with_all_zero_values_draws_a_flat_baseline() {
        let layout = ribbon_layout(&[0, 0, 0], 300.0, 100.0, None);

        assert!(layout.points.iter().all(|point| point.y == 100.0));
    }

    #[test]
    fn ribbon_hover_maps_x_to_the_bucket_under_the_cursor() {
        assert_eq!(bucket_at_x(0.0, 300.0, 3), Some(0));
        assert_eq!(bucket_at_x(99.0, 300.0, 3), Some(0));
        assert_eq!(bucket_at_x(100.0, 300.0, 3), Some(1));
        assert_eq!(bucket_at_x(299.0, 300.0, 3), Some(2));
        assert_eq!(bucket_at_x(-1.0, 300.0, 3), None);
        assert_eq!(bucket_at_x(300.0, 300.0, 3), None);
    }
}
