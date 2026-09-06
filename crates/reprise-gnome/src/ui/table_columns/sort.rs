use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct SortSpec<K> {
    pub key: K,
    pub direction: SortDirection,
}

impl<K> SortSpec<K> {
    pub(in crate::ui) fn new(key: K, direction: SortDirection) -> Self {
        Self { key, direction }
    }
}

pub(in crate::ui) trait SortKey<R> {
    fn cmp(&self, left: &R, right: &R) -> Ordering;

    fn cmp_descending(&self, left: &R, right: &R) -> Ordering {
        self.cmp(left, right).reverse()
    }
}

pub(in crate::ui) fn compare_text(left: &str, right: &str, direction: SortDirection) -> Ordering {
    match (present(left), present(right)) {
        (Some(left), Some(right)) => apply_direction(
            left.to_lowercase()
                .cmp(&right.to_lowercase())
                .then_with(|| left.cmp(right)),
            direction,
        ),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub(in crate::ui) fn compare_optional<T: PartialOrd>(
    left: Option<T>,
    right: Option<T>,
    direction: SortDirection,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => apply_direction(
            left.partial_cmp(&right).unwrap_or(Ordering::Equal),
            direction,
        ),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn present(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn apply_direction(ordering: Ordering, direction: SortDirection) -> Ordering {
    match direction {
        SortDirection::Ascending => ordering,
        SortDirection::Descending => ordering.reverse(),
    }
}

pub(in crate::ui) fn sort_rows<R, K: SortKey<R>>(rows: &mut [R], spec: &SortSpec<K>) {
    rows.sort_by(|left, right| match spec.direction {
        SortDirection::Ascending => spec.key.cmp(left, right),
        SortDirection::Descending => spec.key.cmp_descending(left, right),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[derive(Clone, Copy)]
    enum TestKey {
        Primary,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Row {
        primary: u8,
        secondary: u8,
        insertion: u8,
    }

    impl SortKey<Row> for TestKey {
        fn cmp(&self, left: &Row, right: &Row) -> Ordering {
            left.primary
                .cmp(&right.primary)
                .then_with(|| left.secondary.cmp(&right.secondary))
        }
    }

    #[test]
    fn sort_is_stable_reverses_direction_and_keeps_declared_ties() {
        let rows = vec![
            Row {
                primary: 2,
                secondary: 1,
                insertion: 0,
            },
            Row {
                primary: 1,
                secondary: 0,
                insertion: 1,
            },
            Row {
                primary: 2,
                secondary: 0,
                insertion: 2,
            },
            Row {
                primary: 2,
                secondary: 0,
                insertion: 3,
            },
        ];
        let mut ascending = rows.clone();
        sort_rows(
            &mut ascending,
            &SortSpec::new(TestKey::Primary, SortDirection::Ascending),
        );
        assert_eq!(
            ascending
                .iter()
                .map(|row| row.insertion)
                .collect::<Vec<_>>(),
            [1, 2, 3, 0]
        );
        let mut descending = rows;
        sort_rows(
            &mut descending,
            &SortSpec::new(TestKey::Primary, SortDirection::Descending),
        );
        assert_eq!(
            descending
                .iter()
                .map(|row| row.insertion)
                .collect::<Vec<_>>(),
            [0, 2, 3, 1]
        );
    }
}
