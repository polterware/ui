//! Port of `layout-base/src/util/SVD.js`, which layout-base 2.0.1 adds for cose-base
//! 2.x's `ConstraintHandler`.
//!
//! The upstream code is adopted from [JamaJS](https://github.com/dragonfly-ai/JamaJS)
//! (Apache-2.0) and is the one-sided Jacobi SVD of Golub and Van Loan. The port keeps the
//! algorithm, the loop bounds, and the operation order, because the constraint handler's
//! transformation matrix is compared against the JavaScript result; the singular vectors
//! are only determined up to sign and ordering by the matrix itself.
//!
//! Upstream's `svd` mutates its input and returns `{U, V, S}`; here the input is consumed
//! and the result is [`SvdResult`]. Upstream's `SVD.svd` is never called on an empty
//! matrix (`ConstraintHandler` always passes a 2-row matrix), so the port's dimension
//! handling assumes at least one row and one column and says so.
//!
//! The loops below index arrays instead of iterating them so the expressions read like
//! upstream's, including the indices shared by two arrays; clippy's range-loop lint is
//! silenced for that reason.

#![allow(clippy::needless_range_loop)]
//!
//! The loops below index arrays instead of iterating them so the expressions read like
//! upstream's, including the indices shared by two arrays; clippy's range-loop lint is
//! silenced for that reason.

/// The result of [`svd`], upstream's `{U, V, S}`.
#[derive(Clone, Debug, PartialEq)]
pub struct SvdResult {
    /// Left singular vectors, `m` rows by `min(m, n)` columns.
    pub u: Vec<Vec<f64>>,
    /// Right singular vectors, `n` rows by `n` columns.
    pub v: Vec<Vec<f64>>,
    /// Singular values, `min(m + 1, n)` of them.
    pub s: Vec<f64>,
}

/// Upstream `SVD.hypot`: `sqrt(a^2 + b^2)` without underflow or overflow.
pub fn hypot(a: f64, b: f64) -> f64 {
    if a.abs() > b.abs() {
        let r = b / a;
        a.abs() * (1.0 + r * r).sqrt()
    } else if b != 0.0 {
        let r = a / b;
        b.abs() * (1.0 + r * r).sqrt()
    } else {
        0.0
    }
}

/// Upstream `SVD.svd`: the singular value decomposition of `a`, consuming the matrix
/// because the algorithm works in place.
pub fn svd(mut a: Vec<Vec<f64>>) -> SvdResult {
    let m = a.len();
    let n = a[0].len();
    let nu = m.min(n);

    let mut s = vec![0.0; (m + 1).min(n)];
    let mut u = vec![vec![0.0; nu]; m];
    let mut v = vec![vec![0.0; n]; n];
    let mut e = vec![0.0; n];
    let mut work = vec![0.0; m];

    let wantu = true;
    let wantv = true;

    let nct = (m as isize - 1).min(n as isize).max(0) as usize;
    let nrt = ((n as isize - 2).min(m as isize)).max(0) as usize;

    for k in 0..nct.max(nrt) {
        if k < nct {
            // Compute the transformation for the k-th column and place the k-th
            // diagonal in s[k].
            s[k] = 0.0;
            for i in k..m {
                s[k] = hypot(s[k], a[i][k]);
            }

            if s[k] != 0.0 {
                if a[k][k] < 0.0 {
                    s[k] = -s[k];
                }
                for i in k..m {
                    a[i][k] /= s[k];
                }
                a[k][k] += 1.0;
            }
            s[k] = -s[k];
        }

        for j in k + 1..n {
            if k < nct && s[k] != 0.0 {
                // Apply the transformation.
                let mut t = 0.0;
                for i in k..m {
                    t += a[i][k] * a[i][j];
                }
                t = -t / a[k][k];
                for i in k..m {
                    a[i][j] += t * a[i][k];
                }
            }

            // Place the k-th row of A into e for the subsequent calculation of the row
            // transformation.
            e[j] = a[k][j];
        }

        // Place the transformation in U for subsequent back multiplication.
        if wantu && k < nct {
            for i in k..m {
                u[i][k] = a[i][k];
            }
        }

        if k < nrt {
            // Compute the k-th row transformation and place the k-th super-diagonal in
            // e[k].
            e[k] = 0.0;
            for i in k + 1..n {
                e[k] = hypot(e[k], e[i]);
            }

            if e[k] != 0.0 {
                if e[k + 1] < 0.0 {
                    e[k] = -e[k];
                }
                for i in k + 1..n {
                    e[i] /= e[k];
                }
                e[k + 1] += 1.0;
            }
            e[k] = -e[k];

            // Apply the transformation.
            if k + 1 < m && e[k] != 0.0 {
                for i in k + 1..m {
                    work[i] = 0.0;
                }

                for j in k + 1..n {
                    for i in k + 1..m {
                        work[i] += e[j] * a[i][j];
                    }
                }

                for j in k + 1..n {
                    let t = -e[j] / e[k + 1];
                    for i in k + 1..m {
                        a[i][j] += t * work[i];
                    }
                }
            }

            // Place the transformation in V for subsequent back multiplication.
            if wantv {
                for i in k + 1..n {
                    v[i][k] = e[i];
                }
            }
        }
    }

    // Set up the final bidiagonal matrix or order p.
    let mut p = n.min(m + 1);
    if nct < n {
        s[nct] = a[nct][nct];
    }
    if m < p {
        s[p - 1] = 0.0;
    }
    if nrt + 1 < p {
        e[nrt] = a[nrt][p - 1];
    }
    e[p - 1] = 0.0;

    // If required, generate U.
    if wantu {
        for j in nct..nu {
            for i in 0..m {
                u[i][j] = 0.0;
            }
            u[j][j] = 1.0;
        }

        for k in (0..nct).rev() {
            if s[k] != 0.0 {
                for j in k + 1..nu {
                    let mut t = 0.0;
                    for i in k..m {
                        t += u[i][k] * u[i][j];
                    }
                    t = -t / u[k][k];
                    for i in k..m {
                        u[i][j] += t * u[i][k];
                    }
                }

                for i in k..m {
                    u[i][k] = -u[i][k];
                }
                u[k][k] += 1.0;
                for i in 0..k.saturating_sub(1) {
                    u[i][k] = 0.0;
                }
            } else {
                for i in 0..m {
                    u[i][k] = 0.0;
                }
                u[k][k] = 1.0;
            }
        }
    }

    // If required, generate V.
    if wantv {
        for k in (0..n).rev() {
            if k < nrt && e[k] != 0.0 {
                for j in k + 1..nu {
                    let mut t = 0.0;
                    for i in k + 1..n {
                        t += v[i][k] * v[i][j];
                    }
                    t = -t / v[k + 1][k];
                    for i in k + 1..n {
                        v[i][j] += t * v[i][k];
                    }
                }
            }

            for i in 0..n {
                v[i][k] = 0.0;
            }
            v[k][k] = 1.0;
        }
    }

    // Main iteration loop for the singular values.
    let pp = p - 1;
    let eps = 2.0f64.powi(-52);
    let tiny = 2.0f64.powi(-966);

    while p > 0 {
        // Here is where a test for too many iterations would go.
        //
        // This section of the program inspects for negligible elements in the
        // s and e arrays. On completion the variables kase and k are set as follows:
        //
        // kase = 1     if s(p) and e[k-1] are negligible and k < p
        // kase = 2     if s(k) is negligible and k < p
        // kase = 3     if e[k-1] is negligible, k < p, and s(k), ..., s(p) are not
        //              negligible (QR step)
        // kase = 4     if e(p-1) is negligible (convergence)
        let mut k: isize = p as isize - 2;
        while k >= -1 {
            if k == -1 {
                break;
            }
            let ku = k as usize;
            if e[ku].abs() <= tiny + eps * (s[ku].abs() + s[ku + 1].abs()) {
                e[ku] = 0.0;
                break;
            }
            k -= 1;
        }

        let kase: u8;
        if k == p as isize - 2 {
            kase = 4;
        } else {
            let mut ks: isize = p as isize - 1;
            while ks >= k {
                if ks == k {
                    break;
                }
                let ksu = ks as usize;
                let t = (if ks != p as isize { e[ksu].abs() } else { 0.0 })
                    + (if ks != k + 1 { e[ksu - 1].abs() } else { 0.0 });
                if s[ksu].abs() <= tiny + eps * t {
                    s[ksu] = 0.0;
                    break;
                }
                ks -= 1;
            }

            if ks == k {
                kase = 3;
            } else if ks == p as isize - 1 {
                kase = 1;
            } else {
                kase = 2;
                k = ks;
            }
        }
        k += 1;

        let ku = k as usize;

        match kase {
            1 => {
                // Deflate negligible s(p).
                let mut f = e[p - 2];
                e[p - 2] = 0.0;
                for j in (ku..=p - 2).rev() {
                    let mut t = hypot(s[j], f);
                    let cs = s[j] / t;
                    let sn = f / t;
                    s[j] = t;
                    if j != ku {
                        f = -sn * e[j - 1];
                        e[j - 1] *= cs;
                    }
                    if wantv {
                        for i in 0..n {
                            t = cs * v[i][j] + sn * v[i][p - 1];
                            v[i][p - 1] = -sn * v[i][j] + cs * v[i][p - 1];
                            v[i][j] = t;
                        }
                    }
                }
            }
            2 => {
                // Transpose negligible s(k).
                let mut f = e[ku - 1];
                e[ku - 1] = 0.0;
                for j in ku..p {
                    let mut t = hypot(s[j], f);
                    let cs = s[j] / t;
                    let sn = f / t;
                    s[j] = t;
                    f = -sn * e[j];
                    e[j] *= cs;
                    if wantu {
                        for i in 0..m {
                            t = cs * u[i][j] + sn * u[i][ku - 1];
                            u[i][ku - 1] = -sn * u[i][j] + cs * u[i][ku - 1];
                            u[i][j] = t;
                        }
                    }
                }
            }
            3 => {
                // Chase a bulge.
                let scale = s[p - 1]
                    .abs()
                    .max(s[p - 2].abs())
                    .max(e[p - 2].abs())
                    .max(s[ku].abs())
                    .max(e[ku].abs());
                let sp = s[p - 1] / scale;
                let spm1 = s[p - 2] / scale;
                let epm1 = e[p - 2] / scale;
                let sk = s[ku] / scale;
                let ek = e[ku] / scale;
                let b = ((spm1 + sp) * (spm1 - sp) + epm1 * epm1) / 2.0;
                let c = (sp * epm1) * (sp * epm1);
                let mut shift = 0.0;
                if b != 0.0 || c != 0.0 {
                    shift = (b * b + c).sqrt();
                    if b < 0.0 {
                        shift = -shift;
                    }
                    shift = c / (b + shift);
                }
                let mut f = (sk + sp) * (sk - sp) + shift;
                let mut g = sk * ek;
                for j in ku..p - 1 {
                    let mut t = hypot(f, g);
                    let mut cs = f / t;
                    let mut sn = g / t;
                    if j != ku {
                        e[j - 1] = t;
                    }
                    f = cs * s[j] + sn * e[j];
                    e[j] = cs * e[j] - sn * s[j];
                    g = sn * s[j + 1];
                    s[j + 1] *= cs;
                    if wantv {
                        for i in 0..n {
                            t = cs * v[i][j] + sn * v[i][j + 1];
                            v[i][j + 1] = -sn * v[i][j] + cs * v[i][j + 1];
                            v[i][j] = t;
                        }
                    }
                    t = hypot(f, g);
                    cs = f / t;
                    sn = g / t;
                    s[j] = t;
                    f = cs * e[j] + sn * s[j + 1];
                    s[j + 1] = -sn * e[j] + cs * s[j + 1];
                    g = sn * e[j + 1];
                    e[j + 1] *= cs;
                    if wantu && j < m - 1 {
                        for i in 0..m {
                            t = cs * u[i][j] + sn * u[i][j + 1];
                            u[i][j + 1] = -sn * u[i][j] + cs * u[i][j + 1];
                            u[i][j] = t;
                        }
                    }
                }
                e[p - 2] = f;
            }
            _ => {
                // Convergence.
                if s[ku] <= 0.0 {
                    s[ku] = if s[ku] < 0.0 { -s[ku] } else { 0.0 };
                    if wantv {
                        for i in 0..=pp {
                            v[i][ku] = -v[i][ku];
                        }
                    }
                }

                let mut kk = ku;
                while kk < pp {
                    if s[kk] >= s[kk + 1] {
                        break;
                    }
                    s.swap(kk, kk + 1);
                    if wantv && kk < n - 1 {
                        for row in v.iter_mut() {
                            row.swap(kk, kk + 1);
                        }
                    }
                    if wantu && kk < m - 1 {
                        for row in u.iter_mut() {
                            row.swap(kk, kk + 1);
                        }
                    }
                    kk += 1;
                }

                p -= 1;
            }
        }
    }

    SvdResult { u, v, s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::util::matrix;

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn hypot_avoids_overflow() {
        assert_close(hypot(3.0, 4.0), 5.0);
        assert_close(hypot(-3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert!(hypot(1e200, 1e200).is_finite());
    }

    #[test]
    fn decomposition_reconstructs_the_matrix() {
        let a = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
        let result = svd(a.clone());

        // A = U * S * V'
        let mut us = vec![vec![0.0; 2]; 3];
        for i in 0..3 {
            for j in 0..2 {
                us[i][j] = result.u[i][j] * result.s[j];
            }
        }
        let reconstructed = matrix::mult_mat(&us, &matrix::transpose(&result.v));

        for i in 0..3 {
            for j in 0..2 {
                assert!(
                    (reconstructed[i][j] - a[i][j]).abs() < 1e-9,
                    "{} != {}",
                    reconstructed[i][j],
                    a[i][j]
                );
            }
        }
    }

    #[test]
    fn decomposition_orders_singular_values() {
        let result = svd(vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]]);

        assert!(result.s[0] >= result.s[1]);
        assert!(result.s[0] > 0.0);
    }
}
