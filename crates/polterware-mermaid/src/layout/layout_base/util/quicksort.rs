//! Port of `layout-base/src/util/Quicksort.js`.
//!
//! Hoare's partition, sorting in place, with upstream's exact loop shapes. Upstream's
//! `compareFunction` branch always installs the default comparison (its condition tests
//! `!== null || !== undefined`, which is always true), so a custom comparison is never
//! used and the port only offers the default: ascending, `b > a`.
//!
//! Upstream holds the pivot object by reference; the port clones it, which matters only
//! for a `PartialOrd` that looks at identity.
//!
//! The `LinkedList` branch is not ported: it indexes the 1-based `get_object_at` with
//! 0-based indices, and `get_object_at` itself throws (it calls `this.length()`, which
//! is a number), so upstream's linked-list path cannot run. No layout-base or cose-base
//! caller sorts a list.

/// Sorts `values` in place, upstream's `new Quicksort(values, compareFunction)`.
pub fn sort<T: Clone + PartialOrd>(values: &mut [T]) {
    let length = values.len() as isize;
    quicksort(values, 0, length - 1);
}

/// Upstream's `_quicksort`.
fn quicksort<T: Clone + PartialOrd>(values: &mut [T], p: isize, r: isize) {
    if p < r {
        let q = partition(values, p, r);
        quicksort(values, p, q);
        quicksort(values, q + 1, r);
    }
}

/// Upstream's `_partition`.
fn partition<T: Clone + PartialOrd>(values: &mut [T], p: isize, r: isize) -> isize {
    let pivot = values[p as usize].clone();
    let mut i = p;
    let mut j = r;
    loop {
        while values[j as usize] > pivot {
            j -= 1;
        }
        while values[i as usize] < pivot {
            i += 1;
        }

        if i < j {
            values.swap(i as usize, j as usize);
            i += 1;
            j -= 1;
        } else {
            return j;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_in_place() {
        let mut values = [5, 3, 9, 1, 5, 7, 2];
        sort(&mut values);
        assert_eq!(values, [1, 2, 3, 5, 5, 7, 9]);
    }

    #[test]
    fn keeps_upstream_order_for_duplicates() {
        // node -e 'const Q = require(".../util/Quicksort.js");
        //          const a = [3, 1, 2, 1, 3, 2, 1]; new Q(a); console.log(a)'
        // -> [1, 1, 1, 2, 2, 3, 3]
        let mut values = [3, 1, 2, 1, 3, 2, 1];
        sort(&mut values);
        assert_eq!(values, [1, 1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn handles_empty_and_single() {
        let mut empty: [i32; 0] = [];
        sort(&mut empty);
        assert!(empty.is_empty());

        let mut single = [42];
        sort(&mut single);
        assert_eq!(single, [42]);
    }
}
