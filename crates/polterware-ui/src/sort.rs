//! How a `DataTable` sorts its rows (`getNextDataTableSort`, `sortDataTableRows`): a click on a
//! header sorts it ascending, then descending, then not at all; missing values go last either
//! way, text compares as `localeCompare` with `numeric` and base sensitivity, and equal values
//! keep their source order.

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// A row's value under the sorted column, as `sortBy` returns it.
#[derive(Clone, Debug, PartialEq)]
pub enum SortValue {
    Missing,
    Number(f64),
    Text(String),
}

/// `getNextDataTableSort`: a new column sorts ascending, then descending, then not at all.
pub fn next_sort<C: PartialEq>(
    current: Option<(C, SortDirection)>,
    column: C,
) -> Option<(C, SortDirection)> {
    match current {
        Some((active, SortDirection::Ascending)) if active == column => {
            Some((column, SortDirection::Descending))
        }
        Some((active, SortDirection::Descending)) if active == column => None,
        _ => Some((column, SortDirection::Ascending)),
    }
}

/// Stable sort. Missing values stay last in both directions. Equal values keep source order.
pub fn sort_rows<T: Clone>(
    rows: &[T],
    direction: SortDirection,
    key: impl Fn(&T) -> SortValue,
) -> Vec<T> {
    let mut indexed: Vec<(usize, T, SortValue)> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (index, row.clone(), normalize_sort(key(row))))
        .collect();
    let descending = direction == SortDirection::Descending;
    indexed.sort_by(|left, right| {
        let left_missing = matches!(left.2, SortValue::Missing);
        let right_missing = matches!(right.2, SortValue::Missing);
        if left_missing != right_missing {
            return if left_missing {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }
        let comparison = compare_sort(&left.2, &right.2);
        if comparison == Ordering::Equal {
            left.0.cmp(&right.0)
        } else if descending {
            comparison.reverse()
        } else {
            comparison
        }
    });
    indexed.into_iter().map(|(_, row, _)| row).collect()
}

fn normalize_sort(value: SortValue) -> SortValue {
    match value {
        SortValue::Number(number) if !number.is_finite() => SortValue::Missing,
        SortValue::Text(text) if text.is_empty() => SortValue::Missing,
        other => other,
    }
}

fn compare_sort(left: &SortValue, right: &SortValue) -> Ordering {
    match (left, right) {
        (SortValue::Number(left), SortValue::Number(right)) => left.total_cmp(right),
        (SortValue::Text(left), SortValue::Text(right)) => natural_cmp(left, right),
        (SortValue::Number(left), SortValue::Text(right)) => natural_cmp(&left.to_string(), right),
        (SortValue::Text(left), SortValue::Number(right)) => natural_cmp(left, &right.to_string()),
        _ => Ordering::Equal,
    }
}

/// Case-insensitive order with numeric chunks, so "Node 2" precedes "Node 10".
pub fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left: Vec<char> = left.to_lowercase().chars().collect();
    let right: Vec<char> = right.to_lowercase().chars().collect();
    let mut i = 0;
    let mut j = 0;
    while i < left.len() && j < right.len() {
        if left[i].is_ascii_digit() && right[j].is_ascii_digit() {
            let i_start = i;
            let j_start = j;
            while i < left.len() && left[i].is_ascii_digit() {
                i += 1;
            }
            while j < right.len() && right[j].is_ascii_digit() {
                j += 1;
            }
            let left_digits = trim_zeros(&left[i_start..i]);
            let right_digits = trim_zeros(&right[j_start..j]);
            match left_digits
                .len()
                .cmp(&right_digits.len())
                .then_with(|| left_digits.cmp(right_digits))
            {
                Ordering::Equal => {}
                other => return other,
            }
        } else {
            match left[i].cmp(&right[j]) {
                Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
                other => return other,
            }
        }
    }
    left.len()
        .saturating_sub(i)
        .cmp(&right.len().saturating_sub(j))
}

fn trim_zeros(digits: &[char]) -> &[char] {
    match digits.iter().position(|ch| *ch != '0') {
        Some(index) => &digits[index..],
        None => &digits[digits.len().saturating_sub(1)..],
    }
}

#[cfg(test)]
mod tests {
    use super::{SortDirection, SortValue, natural_cmp, next_sort, sort_rows};
    use std::cmp::Ordering;

    #[test]
    fn a_header_cycles_ascending_descending_then_none() {
        let first = next_sort(None, 2);
        assert_eq!(first, Some((2, SortDirection::Ascending)));
        let second = next_sort(first, 2);
        assert_eq!(second, Some((2, SortDirection::Descending)));
        assert_eq!(next_sort(second, 2), None);
        assert_eq!(next_sort(second, 0), Some((0, SortDirection::Ascending)));
    }

    #[test]
    fn missing_values_go_last_and_ties_keep_their_order() {
        let rows = [(0, 1.0), (1, f64::NAN), (2, 3.0), (3, 1.0)];
        let key = |row: &(i32, f64)| SortValue::Number(row.1);
        let up: Vec<i32> = sort_rows(&rows, SortDirection::Ascending, key)
            .iter()
            .map(|row| row.0)
            .collect();
        assert_eq!(up, [0, 3, 2, 1]);
        let down: Vec<i32> = sort_rows(&rows, SortDirection::Descending, key)
            .iter()
            .map(|row| row.0)
            .collect();
        assert_eq!(down, [2, 0, 3, 1]);
    }

    #[test]
    fn text_orders_numbers_by_value() {
        assert_eq!(natural_cmp("Node 2", "node 10"), Ordering::Less);
        assert_eq!(natural_cmp("gpt-5", "GPT-5"), Ordering::Equal);
    }
}
