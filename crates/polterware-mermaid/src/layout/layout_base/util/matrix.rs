//! Port of `layout-base/src/util/Matrix.js`, which layout-base 2.0.1 adds for
//! cose-base 2.x's `ConstraintHandler` (the orthogonal Procrustes transformation).
//!
//! Upstream's `Matrix` is a namespace object of static functions over 1d and 2d arrays;
//! the port keeps them as free functions in this module. `multGamma` and the products
//! keep upstream's exact operation order, because the constraint handler's transformation
//! is compared against the JavaScript result.
//!
//! The products index their arrays instead of iterating them so the expressions read
//! like upstream's; clippy's range-loop lint is silenced for that reason.

#![allow(clippy::needless_range_loop)]

/// Upstream `Matrix.multMat`: the product of two 2d arrays.
pub fn mult_mat(array1: &[Vec<f64>], array2: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let mut result = Vec::with_capacity(array1.len());

    for i in 0..array1.len() {
        let mut row = Vec::with_capacity(array2[0].len());
        for j in 0..array2[0].len() {
            let mut sum = 0.0;
            for k in 0..array1[0].len() {
                sum += array1[i][k] * array2[k][j];
            }
            row.push(sum);
        }
        result.push(row);
    }

    result
}

/// Upstream `Matrix.transpose`: the transpose of a 2d array.
pub fn transpose(array: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let mut result = Vec::with_capacity(array[0].len());

    for i in 0..array[0].len() {
        let mut row = Vec::with_capacity(array.len());
        for j in 0..array.len() {
            row.push(array[j][i]);
        }
        result.push(row);
    }

    result
}

/// Upstream `Matrix.multCons`: an array scaled by a constant.
pub fn mult_cons(array: &[f64], constant: f64) -> Vec<f64> {
    array.iter().map(|value| value * constant).collect()
}

/// Upstream `Matrix.minusOp`: the element-wise difference of two arrays.
pub fn minus_op(array1: &[f64], array2: &[f64]) -> Vec<f64> {
    array1
        .iter()
        .zip(array2)
        .map(|(lhs, rhs)| lhs - rhs)
        .collect()
}

/// Upstream `Matrix.dotProduct`: the dot product of two arrays.
pub fn dot_product(array1: &[f64], array2: &[f64]) -> f64 {
    let mut product = 0.0;

    for i in 0..array1.len() {
        product += array1[i] * array2[i];
    }

    product
}

/// Upstream `Matrix.mag`: the magnitude of an array.
pub fn mag(array: &[f64]) -> f64 {
    dot_product(array, array).sqrt()
}

/// Upstream `Matrix.normalize`: an array scaled to unit length.
pub fn normalize(array: &[f64]) -> Vec<f64> {
    let magnitude = mag(array);

    array.iter().map(|value| value / magnitude).collect()
}

/// Upstream `Matrix.multGamma`: an array multiplied by the centering matrix, which
/// subtracts its mean.
pub fn mult_gamma(array: &[f64]) -> Vec<f64> {
    let mut sum = 0.0;
    for value in array {
        sum += value;
    }

    sum *= -1.0 / array.len() as f64;

    array.iter().map(|value| sum + value).collect()
}

/// Upstream `Matrix.multL`: `0.5 * C * INV * C^T * array`.
pub fn mult_l(array: &[f64], c: &[Vec<f64>], inv: &[Vec<f64>]) -> Vec<f64> {
    let mut temp1 = Vec::with_capacity(c[0].len());

    // Multiply by C^T.
    for i in 0..c[0].len() {
        let mut sum = 0.0;
        for j in 0..c.len() {
            sum += -0.5 * c[j][i] * array[j];
        }
        temp1.push(sum);
    }

    // Multiply the result by INV.
    let mut temp2 = Vec::with_capacity(inv.len());
    for i in 0..inv.len() {
        let mut sum = 0.0;
        for j in 0..inv.len() {
            sum += inv[i][j] * temp1[j];
        }
        temp2.push(sum);
    }

    // Multiply the result by C.
    let mut result = Vec::with_capacity(c.len());
    for i in 0..c.len() {
        let mut sum = 0.0;
        for j in 0..c[0].len() {
            sum += c[i][j] * temp2[j];
        }
        result.push(sum);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_follow_upstream() {
        let a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];

        assert_eq!(mult_mat(&a, &b), vec![vec![19.0, 22.0], vec![43.0, 50.0]]);
        assert_eq!(transpose(&a), vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
        assert_eq!(dot_product(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]), 32.0);
        assert_eq!(mag(&[3.0, 4.0]), 5.0);
        assert_eq!(mult_cons(&[1.0, -2.0], 2.0), vec![2.0, -4.0]);
        assert_eq!(minus_op(&[1.0, 2.0], &[4.0, 5.0]), vec![-3.0, -3.0]);
        assert_eq!(mult_gamma(&[1.0, 2.0, 3.0]), vec![-1.0, 0.0, 1.0]);
        assert_eq!(normalize(&[3.0, 4.0]), vec![0.6, 0.8]);
    }
}
