//! A port of [`@upsetjs/venn.js`](https://github.com/upsetjs/venn.js) 2.0.0's layout, the engine
//! `vennRenderer.ts` draws with.
//!
//! Upstream calls two of the package's entry points: `VennDiagram()` (the d3 component, whose
//! `layoutFunction` is `venn()`) and `layout(data, { width, height, padding })`. Both run the
//! same computation — a greedy circle placement optimized by Nelder–Mead against a loss over
//! the requested pairwise areas, normalized so the largest circle sits at the origin, scaled
//! into the viewport, then measured for a text centre per area. [`chart`] is the first and
//! [`layout`] the second; the difference is the `round: 2` that `layout` applies to its paths
//! (the renderer only reads `text` from it).
//!
//! Ported line by line from `src/circleintersection.js`, `src/layout.js` and `src/diagram.js`,
//! with the parts of `fmin` they use (`bisect`, `nelderMead`, and the conjugate gradient that
//! feeds the constrained-MDS initial layout) from the package's bundled `build/venn.esm.js`.
//! Object key order is observable in the result (the text-centre solve samples circles in
//! insertion order), so insertion order is kept in `Vec`s instead of a map.
//!
//! Where the port stops short of upstream: `constrainedMDSLayout` starts its restarts from
//! `Math.random`, so its output is not reproducible run to run. This port keeps the same
//! algorithm but seeds a small deterministic generator, so a diagram with eight or more
//! requested areas gets one fixed answer instead of a random one. Every two- and three-set
//! diagram stays on the greedy layout, which is deterministic in both.

use crate::scene::{Path, Point};

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

/// `SMALL` in `circleintersection.js` and `layout.js`.
const SMALL: f64 = 1e-10;
/// `lossFunction`'s starting best, `1e50`.
const BEST_LOSS: f64 = 1e50;

/// A circle as venn.js carries it: `{x, y, radius}`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Circle {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

impl Circle {
    pub const fn new(x: f64, y: f64, radius: f64) -> Self {
        Self { x, y, radius }
    }
}

/// One requested area: `VennData` without the label the layout never reads.
#[derive(Clone, Debug, PartialEq)]
pub struct VennArea {
    pub sets: Vec<String>,
    pub size: f64,
}

impl VennArea {
    pub fn new(sets: impl IntoIterator<Item = impl Into<String>>, size: f64) -> Self {
        Self {
            sets: sets.into_iter().map(Into::into).collect(),
            size,
        }
    }
}

/// The text centre `computeTextCentres` solves for an area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextCentre {
    pub x: f64,
    pub y: f64,
    /// Set when the area cannot be represented on screen (`disjoint: true`).
    pub disjoint: bool,
}

/// One area of the rendered diagram, with the path venn.js would draw and its text centre.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutArea {
    pub sets: Vec<String>,
    /// The circle of each set, in `sets` order.
    pub circles: Vec<Circle>,
    pub text: TextCentre,
    pub path: Path,
}

/// The whole layout: every set's circle in insertion order, and every requested area.
#[derive(Clone, Debug, PartialEq)]
pub struct VennLayout {
    pub circles: Vec<(String, Circle)>,
    pub areas: Vec<LayoutArea>,
}

impl VennLayout {
    pub fn circle(&self, set: &str) -> Option<&Circle> {
        self.circles
            .iter()
            .find(|(name, _)| name == set)
            .map(|(_, circle)| circle)
    }
}

/// `VennDiagram()`'s layout: `venn()`, `normalizeSolution`, `scaleSolution`, then
/// `computeTextCentres`, with unrounded area paths (`round` is null in the chart).
pub fn chart(areas: &[VennArea], width: f64, height: f64, padding: f64) -> VennLayout {
    compute(areas, width, height, padding, None)
}

/// `layout(data, options)` with its defaults: the same solve, `round: 2` for the paths.
pub fn layout(areas: &[VennArea], width: f64, height: f64, padding: f64) -> VennLayout {
    compute(areas, width, height, padding, Some(2))
}

fn compute(
    areas: &[VennArea],
    width: f64,
    height: f64,
    padding: f64,
    round: Option<i32>,
) -> VennLayout {
    let solution = venn(areas);
    let solution = normalize_solution(&solution, Some(PI / 2.0));
    let circles = scale_solution(&solution, width, height, padding);
    let text_centres = compute_text_centres(&circles, areas);

    let areas = areas
        .iter()
        .map(|area| {
            let area_circles: Vec<Circle> = area
                .sets
                .iter()
                .map(|set| {
                    circles
                        .iter()
                        .find(|(name, _)| name == set)
                        .map(|(_, circle)| *circle)
                        .unwrap_or_default()
                })
                .collect();
            let arcs = intersection_area_arcs(&area_circles);
            let path = arcs_to_path(&arcs, round);
            LayoutArea {
                sets: area.sets.clone(),
                circles: area_circles,
                text: text_centres.get(&area.sets).copied().unwrap_or(TextCentre {
                    x: 0.0,
                    y: 0.0,
                    disjoint: false,
                }),
                path,
            }
        })
        .collect();

    VennLayout { circles, areas }
}

// ---- circleintersection.js ----

/// `distance(p1, p2)`.
pub fn distance(p1: Circle, p2: Circle) -> f64 {
    ((p1.x - p2.x) * (p1.x - p2.x) + (p1.y - p2.y) * (p1.y - p2.y)).sqrt()
}

fn point_distance(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    ((p1.0 - p2.0) * (p1.0 - p2.0) + (p1.1 - p2.1) * (p1.1 - p2.1)).sqrt()
}

/// `circleArea(r, width)`: the circular segment's area.
pub fn circle_area(r: f64, width: f64) -> f64 {
    r * r * (1.0 - width / r).acos() - (r - width) * (width * (2.0 * r - width)).sqrt()
}

/// `circleOverlap(r1, r2, d)`: the overlap area of two circles.
pub fn circle_overlap(r1: f64, r2: f64, d: f64) -> f64 {
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        return PI * r1.min(r2) * r1.min(r2);
    }
    let w1 = r1 - (d * d - r2 * r2 + r1 * r1) / (2.0 * d);
    let w2 = r2 - (d * d - r1 * r1 + r2 * r2) / (2.0 * d);
    circle_area(r1, w1) + circle_area(r2, w2)
}

/// `circleCircleIntersection(p1, p2)`.
pub fn circle_circle_intersection(p1: Circle, p2: Circle) -> Vec<(f64, f64)> {
    let d = distance(p1, p2);
    let r1 = p1.radius;
    let r2 = p2.radius;
    if d >= r1 + r2 || d <= (r1 - r2).abs() {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).sqrt();
    let x0 = p1.x + (a * (p2.x - p1.x)) / d;
    let y0 = p1.y + (a * (p2.y - p1.y)) / d;
    let rx = -(p2.y - p1.y) * (h / d);
    let ry = -(p2.x - p1.x) * (h / d);
    vec![(x0 + rx, y0 - ry), (x0 - rx, y0 + ry)]
}

/// `getCenter(points)`.
fn get_center(points: &[(f64, f64)]) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let mut x = 0.0;
    let mut y = 0.0;
    for point in points {
        x += point.0;
        y += point.1;
    }
    (x / points.len() as f64, y / points.len() as f64)
}

/// An arc of an area's boundary, `stats.arcs[i]`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Arc {
    circle: Circle,
    width: f64,
    p1: (f64, f64),
    p2: (f64, f64),
    large: bool,
    sweep: bool,
}

#[derive(Clone, Copy, Debug)]
struct IntersectionPoint {
    x: f64,
    y: f64,
    parent_index: [usize; 2],
}

#[derive(Clone, Copy, Debug)]
struct InnerPoint {
    x: f64,
    y: f64,
    parent_index: [usize; 2],
    angle: f64,
}

/// `intersectionArea(circles, stats)`; the stats this port needs are the arcs.
fn intersection_area(circles: &[Circle]) -> (f64, Vec<Arc>) {
    if circles.is_empty() {
        return (0.0, Vec::new());
    }
    let intersection_points = get_intersection_points(circles);
    let inner_points: Vec<InnerPoint> = intersection_points
        .iter()
        .filter(|point| contained_in_circles(point.x, point.y, circles))
        .map(|point| InnerPoint {
            x: point.x,
            y: point.y,
            parent_index: point.parent_index,
            angle: 0.0,
        })
        .collect();

    let mut arc_area = 0.0;
    let mut polygon_area = 0.0;
    let mut arcs: Vec<Arc> = Vec::new();

    if inner_points.len() > 1 {
        let center = get_center(
            &inner_points
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );
        let mut inner_points = inner_points;
        for point in &mut inner_points {
            point.angle = (point.x - center.0).atan2(point.y - center.1);
        }
        inner_points.sort_by(|a, b| {
            b.angle
                .partial_cmp(&a.angle)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut p2 = inner_points[inner_points.len() - 1];
        for p1 in &inner_points {
            polygon_area += (p2.x + p1.x) * (p1.y - p2.y);

            let mid_point = ((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
            let mut arc: Option<Arc> = None;

            for index in p1.parent_index {
                if !p2.parent_index.contains(&index) {
                    continue;
                }
                let circle = circles[index];
                let a1 = (p1.x - circle.x).atan2(p1.y - circle.y);
                let a2 = (p2.x - circle.x).atan2(p2.y - circle.y);

                let mut angle_diff = a2 - a1;
                if angle_diff < 0.0 {
                    angle_diff += 2.0 * PI;
                }
                let a = a2 - angle_diff / 2.0;
                let mut width = point_distance(
                    mid_point,
                    (
                        circle.x + circle.radius * a.sin(),
                        circle.y + circle.radius * a.cos(),
                    ),
                );
                if width > circle.radius * 2.0 {
                    width = circle.radius * 2.0;
                }
                if arc.is_none() || arc.is_some_and(|current| current.width > width) {
                    arc = Some(Arc {
                        circle,
                        width,
                        p1: (p1.x, p1.y),
                        p2: (p2.x, p2.y),
                        large: width > circle.radius,
                        sweep: true,
                    });
                }
            }

            if let Some(arc) = arc {
                arcs.push(arc);
                arc_area += circle_area(arc.circle.radius, arc.width);
                p2 = *p1;
            }
        }
    } else {
        let mut smallest = circles[0];
        for circle in &circles[1..] {
            if circle.radius < smallest.radius {
                smallest = *circle;
            }
        }

        let mut disjoint = false;
        for circle in circles {
            if distance(*circle, smallest) > (smallest.radius - circle.radius).abs() {
                disjoint = true;
                break;
            }
        }

        if disjoint {
            arc_area = 0.0;
            polygon_area = 0.0;
        } else {
            arc_area = smallest.radius * smallest.radius * PI;
            arcs.push(Arc {
                circle: smallest,
                p1: (smallest.x, smallest.y + smallest.radius),
                p2: (smallest.x - SMALL, smallest.y + smallest.radius),
                width: smallest.radius * 2.0,
                large: true,
                sweep: true,
            });
        }
    }

    polygon_area /= 2.0;
    (arc_area + polygon_area, arcs)
}

fn contained_in_circles(x: f64, y: f64, circles: &[Circle]) -> bool {
    circles
        .iter()
        .all(|circle| point_distance((x, y), (circle.x, circle.y)) < circle.radius + SMALL)
}

fn get_intersection_points(circles: &[Circle]) -> Vec<IntersectionPoint> {
    let mut points = Vec::new();
    for i in 0..circles.len() {
        for j in (i + 1)..circles.len() {
            for (x, y) in circle_circle_intersection(circles[i], circles[j]) {
                points.push(IntersectionPoint {
                    x,
                    y,
                    parent_index: [i, j],
                });
            }
        }
    }
    points
}

fn intersection_area_arcs(circles: &[Circle]) -> Vec<Arc> {
    if circles.is_empty() {
        return Vec::new();
    }
    intersection_area(circles).1
}

/// `arcsToPath(arcs, round)`.
fn arcs_to_path(arcs: &[Arc], round: Option<i32>) -> Path {
    if arcs.is_empty() {
        let mut path = Path::new();
        path.move_to(Point::new(0.0, 0.0));
        return path;
    }
    let r = |value: f64| match round {
        Some(round) => {
            let factor = 10f64.powi(round);
            js_round(value * factor) / factor
        }
        None => value,
    };
    if arcs.len() == 1 {
        let circle = arcs[0].circle;
        return circle_path(r(circle.x), r(circle.y), r(circle.radius));
    }
    let mut path = Path::new();
    path.move_to(Point::new(r(arcs[0].p2.0), r(arcs[0].p2.1)));
    for arc in arcs {
        let radius = r(arc.circle.radius);
        path.arc_to(
            radius,
            radius,
            0.0,
            arc.large,
            arc.sweep,
            Point::new(r(arc.p1.0), r(arc.p1.1)),
        );
    }
    path
}

/// `circlePath(x, y, r)`: `M x y m -r 0 a r r 0 1 0 2r 0 a r r 0 1 0 -2r 0`, resolved to
/// absolute commands.
pub fn circle_path(x: f64, y: f64, radius: f64) -> Path {
    let mut path = Path::new();
    path.move_to(Point::new(x, y));
    path.move_to(Point::new(x - radius, y));
    path.arc_to(radius, radius, 0.0, true, false, Point::new(x + radius, y));
    path.arc_to(radius, radius, 0.0, true, false, Point::new(x - radius, y));
    path
}

/// JavaScript's `Math.round`: the nearest integer, ties toward positive infinity.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

// ---- fmin (the parts venn.js uses) ----

/// `bisect(f, a, b)`; both ends must have opposite signs, as upstream requires.
fn bisect(f: impl Fn(f64) -> f64, a: f64, b: f64, max_iterations: usize, tolerance: f64) -> f64 {
    let f_a = f(a);
    let f_b = f(b);
    let mut delta = b - a;
    if f_a * f_b > 0.0 {
        // Upstream throws here; the guarded callers never reach it.
        return f64::NAN;
    }
    if f_a == 0.0 {
        return a;
    }
    if f_b == 0.0 {
        return b;
    }
    let mut a = a;
    for _ in 0..max_iterations {
        delta /= 2.0;
        let mid = a + delta;
        let f_mid = f(mid);
        if f_mid * f_a >= 0.0 {
            a = mid;
        }
        if delta.abs() < tolerance || f_mid == 0.0 {
            return mid;
        }
    }
    a + delta
}

fn zeros(count: usize) -> Vec<f64> {
    vec![0.0; count]
}

fn zeros_matrix(rows: usize, columns: usize) -> Vec<Vec<f64>> {
    vec![vec![0.0; columns]; rows]
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut total = 0.0;
    for (left, right) in a.iter().zip(b) {
        total += left * right;
    }
    total
}

fn norm2(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

fn scale(values: &mut [f64], value: &[f64], c: f64) {
    for (slot, item) in values.iter_mut().zip(value) {
        *slot = item * c;
    }
}

fn weighted_sum(ret: &mut [f64], w1: f64, v1: &[f64], w2: f64, v2: &[f64]) {
    for (index, slot) in ret.iter_mut().enumerate() {
        *slot = w1 * v1[index] + w2 * v2[index];
    }
}

/// `nelderMead`'s options; `minTolerance` falls back the way `||` does.
#[derive(Clone, Copy, Debug)]
struct NelderMead {
    max_iterations: usize,
    non_zero_delta: f64,
    zero_delta: f64,
    min_error_delta: f64,
    min_tolerance: f64,
    rho: f64,
    chi: f64,
    psi: f64,
    sigma: f64,
}

impl NelderMead {
    fn new(x_length: usize, max_iterations: Option<usize>, min_error_delta: Option<f64>) -> Self {
        // `const minTolerance = parameters.minErrorDelta || 1e-5;` reads the raw option, so
        // an absent value is 1e-5 even though `minErrorDelta` itself defaults to 1e-6.
        let min_tolerance = min_error_delta
            .filter(|value| *value != 0.0 && !value.is_nan())
            .unwrap_or(1e-5);
        let min_error_delta = min_error_delta.unwrap_or(1e-6);
        Self {
            max_iterations: max_iterations.unwrap_or(x_length * 200),
            non_zero_delta: 1.05,
            zero_delta: 0.001,
            min_error_delta,
            min_tolerance,
            rho: 1.0,
            chi: 2.0,
            psi: -0.5,
            sigma: 0.5,
        }
    }
}

/// `nelderMead(f, x0, parameters)`: the downhill simplex, with the same sort and update
/// order so the returned point matches upstream's.
fn nelder_mead(
    f: &mut impl FnMut(&[f64]) -> f64,
    x0: &[f64],
    options: NelderMead,
) -> (f64, Vec<f64>) {
    let n = x0.len();
    let mut simplex: Vec<Vec<f64>> = Vec::with_capacity(n + 1);
    let mut fx: Vec<f64> = Vec::with_capacity(n + 1);
    simplex.push(x0.to_vec());
    fx.push(f(x0));
    for i in 0..n {
        let mut point = x0.to_vec();
        point[i] = if point[i] != 0.0 {
            point[i] * options.non_zero_delta
        } else {
            options.zero_delta
        };
        simplex.push(point);
        let point = simplex[i + 1].clone();
        fx.push(f(&point));
    }

    let mut centroid = x0.to_vec();
    let mut reflected = x0.to_vec();
    let mut contracted = x0.to_vec();
    let mut expanded = x0.to_vec();

    let sort = |simplex: &mut Vec<Vec<f64>>, fx: &mut Vec<f64>| {
        let mut order: Vec<usize> = (0..simplex.len()).collect();
        order.sort_by(|a, b| {
            fx[*a]
                .partial_cmp(&fx[*b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let points: Vec<Vec<f64>> = order.iter().map(|index| simplex[*index].clone()).collect();
        let values: Vec<f64> = order.iter().map(|index| fx[*index]).collect();
        *simplex = points;
        *fx = values;
    };

    for _ in 0..options.max_iterations {
        sort(&mut simplex, &mut fx);

        let mut max_diff: f64 = 0.0;
        for (best, second) in simplex[0].iter().zip(&simplex[1]).take(n) {
            max_diff = max_diff.max((best - second).abs());
        }
        if (fx[0] - fx[n]).abs() < options.min_error_delta && max_diff < options.min_tolerance {
            break;
        }

        for (i, value) in centroid.iter_mut().enumerate().take(n) {
            *value = 0.0;
            for point in simplex.iter().take(n) {
                *value += point[i];
            }
            *value /= n as f64;
        }

        let worst = simplex[n].clone();
        weighted_sum(
            &mut reflected,
            1.0 + options.rho,
            &centroid,
            -options.rho,
            &worst,
        );
        let reflected_fx = f(&reflected);

        if reflected_fx < fx[0] {
            weighted_sum(
                &mut expanded,
                1.0 + options.chi,
                &centroid,
                -options.chi,
                &worst,
            );
            let expanded_fx = f(&expanded);
            if expanded_fx < reflected_fx {
                simplex[n] = expanded.clone();
                fx[n] = expanded_fx;
            } else {
                simplex[n] = reflected.clone();
                fx[n] = reflected_fx;
            }
        } else if reflected_fx >= fx[n - 1] {
            let mut should_reduce = false;
            if reflected_fx > fx[n] {
                weighted_sum(
                    &mut contracted,
                    1.0 + options.psi,
                    &centroid,
                    -options.psi,
                    &worst,
                );
                let contracted_fx = f(&contracted);
                if contracted_fx < fx[n] {
                    simplex[n] = contracted.clone();
                    fx[n] = contracted_fx;
                } else {
                    should_reduce = true;
                }
            } else {
                weighted_sum(
                    &mut contracted,
                    1.0 - options.psi * options.rho,
                    &centroid,
                    options.psi * options.rho,
                    &worst,
                );
                let contracted_fx = f(&contracted);
                if contracted_fx < reflected_fx {
                    simplex[n] = contracted.clone();
                    fx[n] = contracted_fx;
                } else {
                    should_reduce = true;
                }
            }

            if should_reduce {
                if options.sigma >= 1.0 {
                    break;
                }
                for i in 1..simplex.len() {
                    let mut point = simplex[i].clone();
                    let best = simplex[0].clone();
                    weighted_sum(
                        &mut point,
                        1.0 - options.sigma,
                        &best,
                        options.sigma,
                        &simplex[i],
                    );
                    let value = f(&point);
                    simplex[i] = point;
                    fx[i] = value;
                }
            }
        } else {
            simplex[n] = reflected.clone();
            fx[n] = reflected_fx;
        }
    }

    sort(&mut simplex, &mut fx);
    (fx[0], simplex[0].clone())
}

/// A conjugate-gradient state: `current` in `conjugateGradient`.
#[derive(Clone, Debug)]
struct ConjugateState {
    x: Vec<f64>,
    fx: f64,
    fxprime: Vec<f64>,
}

#[allow(clippy::too_many_arguments)]
fn wolfe_zoom(
    f: &mut impl FnMut(&[f64], &mut [f64]) -> f64,
    pk: &[f64],
    current: &ConjugateState,
    next: &mut ConjugateState,
    a_lo: f64,
    a_high: f64,
    phi_lo: f64,
    phi0: f64,
    phi_prime0: f64,
    c1: f64,
    c2: f64,
) -> f64 {
    let mut a_lo = a_lo;
    let mut a_high = a_high;
    let mut phi_lo = phi_lo;
    for _ in 0..16 {
        let a = (a_lo + a_high) / 2.0;
        weighted_sum(&mut next.x, 1.0, &current.x, a, pk);
        let phi = f(&next.x, &mut next.fxprime);
        next.fx = phi;
        let phi_prime = dot(&next.fxprime, pk);
        if phi > phi0 + c1 * a * phi_prime0 || phi >= phi_lo {
            a_high = a;
        } else {
            if phi_prime.abs() <= -c2 * phi_prime0 {
                return a;
            }
            if phi_prime * (a_high - a_lo) >= 0.0 {
                a_high = a_lo;
            }
            a_lo = a;
            phi_lo = phi;
        }
    }
    0.0
}

fn wolfe_line_search(
    f: &mut impl FnMut(&[f64], &mut [f64]) -> f64,
    pk: &[f64],
    current: &ConjugateState,
    next: &mut ConjugateState,
    mut a: f64,
    c1: f64,
    c2: f64,
) -> f64 {
    let phi0 = current.fx;
    let phi_prime0 = dot(&current.fxprime, pk);
    let mut phi_old = phi0;
    let mut a0 = 0.0;

    for iteration in 0..10 {
        weighted_sum(&mut next.x, 1.0, &current.x, a, pk);
        let phi = f(&next.x, &mut next.fxprime);
        next.fx = phi;
        let phi_prime = dot(&next.fxprime, pk);
        if phi > phi0 + c1 * a * phi_prime0 || (iteration > 0 && phi >= phi_old) {
            return wolfe_zoom(
                f, pk, current, next, a0, a, phi_old, phi0, phi_prime0, c1, c2,
            );
        }
        if phi_prime.abs() <= -c2 * phi_prime0 {
            return a;
        }
        if phi_prime >= 0.0 {
            return wolfe_zoom(f, pk, current, next, a, a0, phi, phi0, phi_prime0, c1, c2);
        }
        phi_old = phi;
        a0 = a;
        a *= 2.0;
    }
    a
}

fn conjugate_gradient(
    f: &mut impl FnMut(&[f64], &mut [f64]) -> f64,
    initial: &[f64],
    max_iterations: usize,
) -> ConjugateState {
    let mut current = ConjugateState {
        x: initial.to_vec(),
        fx: 0.0,
        fxprime: initial.to_vec(),
    };
    let mut next = ConjugateState {
        x: initial.to_vec(),
        fx: 0.0,
        fxprime: initial.to_vec(),
    };
    let mut yk = initial.to_vec();
    let mut a = 1.0;

    current.fx = f(&current.x, &mut current.fxprime);
    let mut pk = current.fxprime.clone();
    scale(&mut pk, &current.fxprime, -1.0);

    for _ in 0..max_iterations {
        a = wolfe_line_search(f, &pk, &current, &mut next, a, 1e-6, 0.1);
        if a == 0.0 {
            scale(&mut pk, &current.fxprime, -1.0);
        } else {
            let next_fxprime = next.fxprime.clone();
            weighted_sum(&mut yk, 1.0, &next_fxprime, -1.0, &current.fxprime);
            let delta_k = dot(&current.fxprime, &current.fxprime);
            let beta_k = (0.0f64).max(dot(&yk, &next_fxprime) / delta_k);
            let pk_old = pk.clone();
            weighted_sum(&mut pk, beta_k, &pk_old, -1.0, &next_fxprime);
            std::mem::swap(&mut current, &mut next);
        }
        if norm2(&current.fxprime) <= 1e-5 {
            break;
        }
    }
    current
}

/// A small deterministic replacement for `Math.random`'s ten restarts (see the module note).
struct Random(u64);

impl Random {
    fn next(&mut self) -> f64 {
        // xorshift64*, mapped to [0, 1).
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let value = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (value >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---- layout.js ----

/// `venn(sets, parameters)`: the initial greedy layout, optimized by Nelder–Mead.
fn venn(areas: &[VennArea]) -> Vec<(String, Circle)> {
    let areas = add_missing_areas(areas);
    let mut circles = best_initial_layout(&areas);

    let setids: Vec<String> = circles.iter().map(|(set, _)| set.clone()).collect();
    let mut initial = Vec::with_capacity(setids.len() * 2);
    for (_, circle) in &circles {
        initial.push(circle.x);
        initial.push(circle.y);
    }

    let (_, positions) = {
        let mut evaluate = |values: &[f64]| -> f64 {
            let current: Vec<(String, Circle)> = setids
                .iter()
                .enumerate()
                .map(|(index, setid)| {
                    (
                        setid.clone(),
                        Circle {
                            x: values[2 * index],
                            y: values[2 * index + 1],
                            radius: circles[index].1.radius,
                        },
                    )
                })
                .collect();
            loss_function(&current, &areas)
        };

        nelder_mead(
            &mut evaluate,
            &initial,
            NelderMead::new(initial.len(), Some(500), None),
        )
    };
    for (index, (_, circle)) in circles.iter_mut().enumerate() {
        circle.x = positions[2 * index];
        circle.y = positions[2 * index + 1];
    }
    circles
}

/// `distanceFromIntersectArea(r1, r2, overlap)`.
fn distance_from_intersect_area(r1: f64, r2: f64, overlap: f64) -> f64 {
    if r1.min(r2) * r1.min(r2) * PI <= overlap + SMALL {
        return (r1 - r2).abs();
    }
    bisect(
        |distance| circle_overlap(r1, r2, distance) - overlap,
        0.0,
        r1 + r2,
        100,
        1e-10,
    )
}

/// `addMissingAreas(areas)`: every pair of single sets gets an area, `size: 0` when the source
/// did not name one.
fn add_missing_areas(areas: &[VennArea]) -> Vec<VennArea> {
    let mut result: Vec<VennArea> = areas.to_vec();

    let mut ids: Vec<String> = Vec::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for area in &result {
        if area.sets.len() == 1 {
            ids.push(area.sets[0].clone());
        } else if area.sets.len() == 2 {
            pairs.push((area.sets[0].clone(), area.sets[1].clone()));
            pairs.push((area.sets[1].clone(), area.sets[0].clone()));
        }
    }

    ids.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = &ids[i];
            let b = &ids[j];
            if !pairs.iter().any(|(left, right)| left == a && right == b) {
                result.push(VennArea::new([a.clone(), b.clone()], 0.0));
            }
        }
    }
    result
}

fn distance_matrices(
    areas: &[VennArea],
    sets: &[&VennArea],
    setids: &HashMap<String, usize>,
) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let mut distances = zeros_matrix(sets.len(), sets.len());
    let mut constraints = zeros_matrix(sets.len(), sets.len());

    for current in areas.iter().filter(|area| area.sets.len() == 2) {
        let left = setids[&current.sets[0]];
        let right = setids[&current.sets[1]];
        let r1 = (sets[left].size / PI).sqrt();
        let r2 = (sets[right].size / PI).sqrt();
        let distance = distance_from_intersect_area(r1, r2, current.size);
        distances[left][right] = distance;
        distances[right][left] = distance;

        let mut c = 0.0;
        if current.size + 1e-10 >= sets[left].size.min(sets[right].size) {
            c = 1.0;
        } else if current.size <= 1e-10 {
            c = -1.0;
        }
        constraints[left][right] = c;
        constraints[right][left] = c;
    }
    (distances, constraints)
}

fn constrained_mds_gradient(
    x: &[f64],
    fxprime: &mut [f64],
    distances: &[Vec<f64>],
    constraints: &[Vec<f64>],
) -> f64 {
    fxprime.fill(0.0);
    let mut loss = 0.0;
    for i in 0..distances.len() {
        let xi = x[2 * i];
        let yi = x[2 * i + 1];
        for j in (i + 1)..distances.len() {
            let xj = x[2 * j];
            let yj = x[2 * j + 1];
            let dij = distances[i][j];
            let constraint = constraints[i][j];
            let squared_distance = (xj - xi) * (xj - xi) + (yj - yi) * (yj - yi);
            let distance = squared_distance.sqrt();
            let delta = squared_distance - dij * dij;
            if (constraint > 0.0 && distance <= dij) || (constraint < 0.0 && distance >= dij) {
                continue;
            }
            loss += 2.0 * delta * delta;
            fxprime[2 * i] += 4.0 * delta * (xi - xj);
            fxprime[2 * i + 1] += 4.0 * delta * (yi - yj);
            fxprime[2 * j] += 4.0 * delta * (xj - xi);
            fxprime[2 * j + 1] += 4.0 * delta * (yj - yi);
        }
    }
    loss
}

/// `constrainedMDSLayout(areas)`: the alternative initial layout for eight or more areas.
fn constrained_mds_layout(areas: &[VennArea]) -> Vec<(String, Circle)> {
    let mut sets: Vec<&VennArea> = Vec::new();
    let mut setids: HashMap<String, usize> = HashMap::new();
    for area in areas {
        if area.sets.len() == 1 {
            setids.insert(area.sets[0].clone(), sets.len());
            sets.push(area);
        }
    }

    let (mut distances, constraints) = distance_matrices(areas, &sets, &setids);
    let norm = if distances.is_empty() {
        0.0
    } else {
        distances
            .iter()
            .map(|row| norm2(row))
            .collect::<Vec<_>>()
            .iter()
            .copied()
            .sum::<f64>()
            / distances.len() as f64
    };
    if norm != 0.0 {
        for row in &mut distances {
            for value in row {
                *value /= norm;
            }
        }
    }

    let mut best: Option<ConjugateState> = None;
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    for _ in 0..10 {
        let initial: Vec<f64> = zeros(distances.len() * 2)
            .iter()
            .map(|_| random.next())
            .collect();
        let mut objective = |x: &[f64], fxprime: &mut [f64]| {
            constrained_mds_gradient(x, fxprime, &distances, &constraints)
        };
        let current = conjugate_gradient(&mut objective, &initial, 500);
        if best.as_ref().is_none_or(|best| current.fx < best.fx) {
            best = Some(current);
        }
    }
    let Some(best) = best else {
        return Vec::new();
    };
    let positions = best.x;

    let mut circles = Vec::with_capacity(sets.len());
    for (i, set) in sets.iter().enumerate() {
        circles.push((
            set.sets[0].clone(),
            Circle {
                x: positions[2 * i] * norm,
                y: positions[2 * i + 1] * norm,
                radius: (set.size / PI).sqrt(),
            },
        ));
    }
    circles
}

/// `bestInitialLayout(areas)`: greedy, unless constrained MDS beats it on eight or more areas.
fn best_initial_layout(areas: &[VennArea]) -> Vec<(String, Circle)> {
    let mut initial = greedy_layout(areas);
    if areas.len() >= 8 {
        let constrained = constrained_mds_layout(areas);
        if !constrained.is_empty() {
            let constrained_loss = loss_function(&constrained, areas);
            let greedy_loss = loss_function(&initial, areas);
            if constrained_loss + 1e-8 < greedy_loss {
                initial = constrained;
            }
        }
    }
    initial
}

/// `greedyLayout(areas)`: places one circle at a time, picking the candidate position with the
/// lowest loss against the circles already placed.
fn greedy_layout(areas: &[VennArea]) -> Vec<(String, Circle)> {
    let mut circles: Vec<(String, Circle)> = Vec::new();
    let mut set_sizes: HashMap<String, f64> = HashMap::new();
    let mut set_overlaps: Vec<(String, Vec<Overlap>)> = Vec::new();

    for area in areas.iter().filter(|area| area.sets.len() == 1) {
        let set = area.sets[0].clone();
        set_sizes.insert(set.clone(), area.size);
        circles.push((
            set.clone(),
            Circle {
                x: 1e10,
                y: 1e10,
                radius: (area.size / PI).sqrt(),
            },
        ));
        set_overlaps.push((set, Vec::new()));
    }

    let pair_areas: Vec<VennArea> = areas
        .iter()
        .filter(|area| area.sets.len() == 2)
        .cloned()
        .collect();

    for current in &pair_areas {
        let mut weight = 1.0;
        let left = &current.sets[0];
        let right = &current.sets[1];
        if current.size + SMALL >= set_sizes[left].min(set_sizes[right]) {
            weight = 0.0;
        }
        if let Some((_, overlaps)) = set_overlaps.iter_mut().find(|(set, _)| set == left) {
            overlaps.push(Overlap {
                set: right.clone(),
                size: current.size,
                weight,
            });
        }
        if let Some((_, overlaps)) = set_overlaps.iter_mut().find(|(set, _)| set == right) {
            overlaps.push(Overlap {
                set: left.clone(),
                size: current.size,
                weight,
            });
        }
    }

    let mut most_overlapped: Vec<(String, f64)> = set_overlaps
        .iter()
        .map(|(set, overlaps)| {
            let mut size = 0.0;
            for overlap in overlaps {
                size += overlap.size * overlap.weight;
            }
            (set.clone(), size)
        })
        .collect();
    most_overlapped.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if most_overlapped.is_empty() {
        return circles;
    }

    let mut positioned: Vec<String> = Vec::new();
    position_set(
        &mut circles,
        &most_overlapped[0].0,
        (0.0, 0.0),
        &mut positioned,
    );

    for (set_index, _) in most_overlapped.iter().enumerate().skip(1) {
        let set_index = &most_overlapped[set_index].0;
        let mut overlap: Vec<Overlap> = set_overlaps
            .iter()
            .find(|(set, _)| set == set_index)
            .map(|(_, overlaps)| {
                overlaps
                    .iter()
                    .filter(|overlap| positioned.contains(&overlap.set))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        overlap.sort_by(|a, b| {
            b.size
                .partial_cmp(&a.size)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if overlap.is_empty() {
            // `addMissingAreas` makes this unreachable for parsed diagrams.
            continue;
        }

        let set = circles
            .iter()
            .find(|(set, _)| set == set_index)
            .map(|(_, circle)| *circle)
            .unwrap_or_default();

        let mut points: Vec<(f64, f64)> = Vec::new();
        for j in 0..overlap.len() {
            let p1 = circles
                .iter()
                .find(|(set, _)| set == &overlap[j].set)
                .map(|(_, circle)| *circle)
                .unwrap_or_default();
            let d1 = distance_from_intersect_area(set.radius, p1.radius, overlap[j].size);
            points.push((p1.x + d1, p1.y));
            points.push((p1.x - d1, p1.y));
            points.push((p1.x, p1.y + d1));
            points.push((p1.x, p1.y - d1));

            for other in overlap.iter().skip(j + 1) {
                let p2 = circles
                    .iter()
                    .find(|(set, _)| set == &other.set)
                    .map(|(_, circle)| *circle)
                    .unwrap_or_default();
                let d2 = distance_from_intersect_area(set.radius, p2.radius, other.size);
                points.extend(circle_circle_intersection(
                    Circle::new(p1.x, p1.y, d1),
                    Circle::new(p2.x, p2.y, d2),
                ));
            }
        }

        let mut best_loss = BEST_LOSS;
        let mut best_point = points[0];
        for point in &points {
            if let Some((_, circle)) = circles.iter_mut().find(|(set, _)| set == set_index) {
                circle.x = point.0;
                circle.y = point.1;
            }
            let local_loss = loss_function(&circles, &pair_areas);
            if local_loss < best_loss {
                best_loss = local_loss;
                best_point = *point;
            }
        }
        position_set(&mut circles, set_index, best_point, &mut positioned);
    }

    circles
}

#[derive(Clone, Debug)]
struct Overlap {
    set: String,
    size: f64,
    weight: f64,
}

fn position_set(
    circles: &mut [(String, Circle)],
    set: &str,
    point: (f64, f64),
    positioned: &mut Vec<String>,
) {
    if let Some((_, circle)) = circles.iter_mut().find(|(name, _)| name == set) {
        circle.x = point.0;
        circle.y = point.1;
    }
    if !positioned.iter().any(|name| name == set) {
        positioned.push(set.to_string());
    }
}

/// `lossFunction(circles, overlaps)`.
fn loss_function(circles: &[(String, Circle)], overlaps: &[VennArea]) -> f64 {
    let mut output = 0.0;
    for area in overlaps {
        if area.sets.len() == 1 {
            continue;
        }
        let overlap = if area.sets.len() == 2 {
            let left = circle_lookup(circles, &area.sets[0]);
            let right = circle_lookup(circles, &area.sets[1]);
            circle_overlap(left.radius, right.radius, distance(left, right))
        } else {
            let area_circles: Vec<Circle> = area
                .sets
                .iter()
                .map(|set| circle_lookup(circles, set))
                .collect();
            intersection_area(&area_circles).0
        };
        let weight = 1.0;
        output += weight * (overlap - area.size) * (overlap - area.size);
    }
    output
}

fn circle_lookup(circles: &[(String, Circle)], set: &str) -> Circle {
    circles
        .iter()
        .find(|(name, _)| name == set)
        .map(|(_, circle)| *circle)
        .unwrap_or_default()
}

/// `normalizeSolution(solution, orientation, orientationOrder)`.
fn normalize_solution(
    solution: &[(String, Circle)],
    orientation: Option<f64>,
) -> Vec<(String, Circle)> {
    let orientation = orientation.unwrap_or(PI / 2.0);
    let mut circles: Vec<(String, Circle)> = solution.to_vec();

    let clusters = disjoint_cluster(&circles);
    let mut positioned_clusters: Vec<Cluster> = clusters
        .into_iter()
        .map(|mut cluster| {
            orientate_circles(&mut cluster.circles, orientation);
            let bounds = get_located_bounding_box(&cluster.circles);
            let size = (bounds.x.max - bounds.x.min) * (bounds.y.max - bounds.y.min);
            Cluster {
                circles: cluster.circles,
                bounds,
                size,
            }
        })
        .collect();
    positioned_clusters.sort_by(|a, b| {
        b.size
            .partial_cmp(&a.size)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if positioned_clusters.is_empty() {
        return Vec::new();
    }

    circles = positioned_clusters[0]
        .circles
        .iter()
        .map(|circle| (circle.set.clone(), circle.circle))
        .collect();
    let mut return_bounds = positioned_clusters[0].bounds;
    let spacing = (return_bounds.x.max - return_bounds.x.min) / 50.0;

    fn add_cluster(
        cluster: Option<&Cluster>,
        right: bool,
        bottom: bool,
        circles: &mut Vec<(String, Circle)>,
        return_bounds: Bounds,
        spacing: f64,
    ) {
        let Some(cluster) = cluster else {
            return;
        };
        let bounds = cluster.bounds;
        let mut x_offset;
        let mut y_offset;
        if right {
            x_offset = return_bounds.x.max - bounds.x.min + spacing;
        } else {
            x_offset = return_bounds.x.max - bounds.x.max;
            let centering = (bounds.x.max - bounds.x.min) / 2.0
                - (return_bounds.x.max - return_bounds.x.min) / 2.0;
            if centering < 0.0 {
                x_offset += centering;
            }
        }
        if bottom {
            y_offset = return_bounds.y.max - bounds.y.min + spacing;
        } else {
            y_offset = return_bounds.y.max - bounds.y.max;
            let centering = (bounds.y.max - bounds.y.min) / 2.0
                - (return_bounds.y.max - return_bounds.y.min) / 2.0;
            if centering < 0.0 {
                y_offset += centering;
            }
        }
        for circle in &cluster.circles {
            circles.push((
                circle.set.clone(),
                Circle {
                    x: circle.circle.x + x_offset,
                    y: circle.circle.y + y_offset,
                    radius: circle.circle.radius,
                },
            ));
        }
    }

    let mut index = 1;
    while index < positioned_clusters.len() {
        add_cluster(
            positioned_clusters.get(index),
            true,
            false,
            &mut circles,
            return_bounds,
            spacing,
        );
        add_cluster(
            positioned_clusters.get(index + 1),
            false,
            true,
            &mut circles,
            return_bounds,
            spacing,
        );
        add_cluster(
            positioned_clusters.get(index + 2),
            true,
            true,
            &mut circles,
            return_bounds,
            spacing,
        );
        index += 3;
        return_bounds = get_bounding_box(&circles);
    }

    circles
}

#[derive(Clone, Copy, Debug)]
struct Range {
    max: f64,
    min: f64,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    x: Range,
    y: Range,
}

#[derive(Clone, Debug)]
struct LocatedCircle {
    set: String,
    circle: Circle,
}

#[derive(Clone, Debug)]
struct Cluster {
    circles: Vec<LocatedCircle>,
    bounds: Bounds,
    size: f64,
}

fn get_bounding_box(circles: &[(String, Circle)]) -> Bounds {
    let min_max = |coordinate: fn(&Circle) -> f64| Range {
        max: circles.iter().fold(f64::NEG_INFINITY, |acc, (_, circle)| {
            acc.max(coordinate(circle) + circle.radius)
        }),
        min: circles.iter().fold(f64::INFINITY, |acc, (_, circle)| {
            acc.min(coordinate(circle) - circle.radius)
        }),
    };
    Bounds {
        x: min_max(|circle| circle.x),
        y: min_max(|circle| circle.y),
    }
}

fn get_located_bounding_box(circles: &[LocatedCircle]) -> Bounds {
    let min_max = |coordinate: fn(&Circle) -> f64| Range {
        max: circles.iter().fold(f64::NEG_INFINITY, |acc, circle| {
            acc.max(coordinate(&circle.circle) + circle.circle.radius)
        }),
        min: circles.iter().fold(f64::INFINITY, |acc, circle| {
            acc.min(coordinate(&circle.circle) - circle.circle.radius)
        }),
    };
    Bounds {
        x: min_max(|circle| circle.x),
        y: min_max(|circle| circle.y),
    }
}

/// `disjointCluster(circles)`: union-find over touching circles, grouped in input order.
fn disjoint_cluster(circles: &[(String, Circle)]) -> Vec<Cluster> {
    let mut parent: Vec<usize> = (0..circles.len()).collect();
    fn find(parent: &mut Vec<usize>, index: usize) -> usize {
        if parent[index] != index {
            let root = find(parent, parent[index]);
            parent[index] = root;
        }
        parent[index]
    }

    for i in 0..circles.len() {
        for j in (i + 1)..circles.len() {
            let max_distance = circles[i].1.radius + circles[j].1.radius;
            if distance(circles[i].1, circles[j].1) + 1e-10 < max_distance {
                let x_root = find(&mut parent, j);
                let y_root = find(&mut parent, i);
                parent[x_root] = y_root;
            }
        }
    }

    let mut roots: Vec<usize> = Vec::new();
    let mut groups: Vec<Vec<LocatedCircle>> = Vec::new();
    for (i, (set, circle)) in circles.iter().enumerate() {
        let root = find(&mut parent, i);
        let group = match roots.iter().position(|candidate| *candidate == root) {
            Some(index) => index,
            None => {
                roots.push(root);
                groups.push(Vec::new());
                groups.len() - 1
            }
        };
        groups[group].push(LocatedCircle {
            set: set.clone(),
            circle: *circle,
        });
    }

    groups
        .into_iter()
        .map(|circles| {
            let bounds = get_located_bounding_box(&circles);
            Cluster {
                circles,
                bounds,
                size: 0.0,
            }
        })
        .collect()
}

/// `orientateCircles(circles, orientation, orientationOrder)` with no `orientationOrder`.
fn orientate_circles(circles: &mut [LocatedCircle], orientation: f64) {
    circles.sort_by(|a, b| {
        b.circle
            .radius
            .partial_cmp(&a.circle.radius)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(first) = circles.first() {
        let (largest_x, largest_y) = (first.circle.x, first.circle.y);
        for circle in circles.iter_mut() {
            circle.circle.x -= largest_x;
            circle.circle.y -= largest_y;
        }
    }

    if circles.len() == 2 {
        let dist = distance(circles[0].circle, circles[1].circle);
        if dist < (circles[1].circle.radius - circles[0].circle.radius).abs() {
            circles[1].circle.x =
                circles[0].circle.x + circles[0].circle.radius - circles[1].circle.radius - 1e-10;
            circles[1].circle.y = circles[0].circle.y;
        }
    }

    if circles.len() > 1 {
        let rotation = circles[1].circle.x.atan2(circles[1].circle.y) - orientation;
        let c = rotation.cos();
        let s = rotation.sin();
        for circle in circles.iter_mut() {
            let x = circle.circle.x;
            let y = circle.circle.y;
            circle.circle.x = c * x - s * y;
            circle.circle.y = s * x + c * y;
        }
    }

    if circles.len() > 2 {
        let mut angle = circles[2].circle.x.atan2(circles[2].circle.y) - orientation;
        while angle < 0.0 {
            angle += TAU;
        }
        while angle > TAU {
            angle -= TAU;
        }
        if angle > PI {
            let slope = circles[1].circle.y / (1e-10 + circles[1].circle.x);
            for circle in circles.iter_mut() {
                let d = (circle.circle.x + slope * circle.circle.y) / (1.0 + slope * slope);
                circle.circle.x = 2.0 * d - circle.circle.x;
                circle.circle.y = 2.0 * d * slope - circle.circle.y;
            }
        }
    }
}

/// `scaleSolution(solution, width, height, padding, scaleToFit)` with `scaleToFit` unset.
fn scale_solution(
    solution: &[(String, Circle)],
    width: f64,
    height: f64,
    padding: f64,
) -> Vec<(String, Circle)> {
    let width = width - 2.0 * padding;
    let height = height - 2.0 * padding;
    let bounds = get_bounding_box(solution);
    if bounds.x.max == bounds.x.min || bounds.y.max == bounds.y.min {
        return solution.to_vec();
    }

    let x_scaling = width / (bounds.x.max - bounds.x.min);
    let y_scaling = height / (bounds.y.max - bounds.y.min);
    let scaling = y_scaling.min(x_scaling);
    let x_offset = (width - (bounds.x.max - bounds.x.min) * scaling) / 2.0;
    let y_offset = (height - (bounds.y.max - bounds.y.min) * scaling) / 2.0;

    solution
        .iter()
        .map(|(set, circle)| {
            (
                set.clone(),
                Circle {
                    radius: scaling * circle.radius,
                    x: padding + x_offset + (circle.x - bounds.x.min) * scaling,
                    y: padding + y_offset + (circle.y - bounds.y.min) * scaling,
                },
            )
        })
        .collect()
}

// ---- computeTextCentres (diagram.js) ----

/// `circleMargin(current, interior, exterior)`.
fn circle_margin(current: (f64, f64), interior: &[Circle], exterior: &[Circle]) -> f64 {
    let mut margin = interior[0].radius - point_distance((interior[0].x, interior[0].y), current);
    for circle in &interior[1..] {
        let m = circle.radius - point_distance((circle.x, circle.y), current);
        if m <= margin {
            margin = m;
        }
    }
    for circle in exterior {
        let m = point_distance((circle.x, circle.y), current) - circle.radius;
        if m <= margin {
            margin = m;
        }
    }
    margin
}

/// `computeTextCentre(interior, exterior, symmetricalTextCentre)`.
fn compute_text_centre(
    interior: &[Circle],
    exterior: &[Circle],
    symmetrical_text_centre: bool,
) -> TextCentre {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for circle in interior {
        points.push((circle.x, circle.y));
        points.push((circle.x + circle.radius / 2.0, circle.y));
        points.push((circle.x - circle.radius / 2.0, circle.y));
        points.push((circle.x, circle.y + circle.radius / 2.0));
        points.push((circle.x, circle.y - circle.radius / 2.0));
    }

    if points.is_empty() {
        return TextCentre {
            x: 0.0,
            y: 0.0,
            disjoint: false,
        };
    }

    let mut initial = points[0];
    let mut margin = circle_margin(points[0], interior, exterior);
    for point in &points[1..] {
        let m = circle_margin(*point, interior, exterior);
        if m >= margin {
            initial = *point;
            margin = m;
        }
    }

    let mut evaluate = |p: &[f64]| -circle_margin((p[0], p[1]), interior, exterior);
    let (_, solution) = nelder_mead(
        &mut evaluate,
        &[initial.0, initial.1],
        NelderMead::new(2, Some(500), Some(1e-10)),
    );

    let ret = TextCentre {
        x: if symmetrical_text_centre {
            0.0
        } else {
            solution[0]
        },
        y: solution[1],
        disjoint: false,
    };

    let mut valid = true;
    for circle in interior {
        if point_distance((ret.x, ret.y), (circle.x, circle.y)) > circle.radius {
            valid = false;
            break;
        }
    }
    for circle in exterior {
        if point_distance((ret.x, ret.y), (circle.x, circle.y)) < circle.radius {
            valid = false;
            break;
        }
    }
    if valid {
        return ret;
    }

    if interior.len() == 1 {
        return TextCentre {
            x: interior[0].x,
            y: interior[0].y,
            disjoint: false,
        };
    }
    let (_, arcs) = intersection_area(interior);
    if arcs.is_empty() {
        return TextCentre {
            x: 0.0,
            y: -1000.0,
            disjoint: true,
        };
    }
    if arcs.len() == 1 {
        return TextCentre {
            x: arcs[0].circle.x,
            y: arcs[0].circle.y,
            disjoint: false,
        };
    }
    if !exterior.is_empty() {
        return compute_text_centre(interior, &[], symmetrical_text_centre);
    }
    let (x, y) = get_center(&arcs.iter().map(|arc| arc.p1).collect::<Vec<_>>());
    TextCentre {
        x,
        y,
        disjoint: false,
    }
}

/// `getOverlappingCircles(circles)`.
fn get_overlapping_circles(circles: &[(String, Circle)]) -> HashMap<String, Vec<String>> {
    let mut overlapping: HashMap<String, Vec<String>> = HashMap::new();
    for (set, _) in circles {
        overlapping.insert(set.clone(), Vec::new());
    }
    for (i, (ci, a)) in circles.iter().enumerate() {
        for (cj, b) in &circles[(i + 1)..] {
            let d = distance(*a, *b);
            if d + b.radius <= a.radius + 1e-10 {
                overlapping.entry(cj.clone()).or_default().push(ci.clone());
            } else if d + a.radius <= b.radius + 1e-10 {
                overlapping.entry(ci.clone()).or_default().push(cj.clone());
            }
        }
    }
    overlapping
}

/// `computeTextCentres(circles, areas, symmetricalTextCentre)`.
fn compute_text_centres(
    circles: &[(String, Circle)],
    areas: &[VennArea],
) -> HashMap<Vec<String>, TextCentre> {
    let overlapping = get_overlapping_circles(circles);
    let mut centres = HashMap::new();
    for area in areas {
        let mut area_ids: Vec<String> = Vec::new();
        let mut exclude: Vec<String> = Vec::new();
        for set in &area.sets {
            area_ids.push(set.clone());
            if let Some(overlaps) = overlapping.get(set) {
                for overlap in overlaps {
                    if !exclude.contains(overlap) {
                        exclude.push(overlap.clone());
                    }
                }
            }
        }

        let mut interior = Vec::new();
        let mut exterior = Vec::new();
        for (set, circle) in circles {
            if area_ids.contains(set) {
                interior.push(*circle);
            } else if !exclude.contains(set) {
                exterior.push(*circle);
            }
        }
        let centre = compute_text_centre(&interior, &exterior, false);
        centres.insert(area.sets.clone(), centre);
    }
    centres
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_circles_overlap_at_the_requested_distance() {
        let overlap = circle_overlap(10.0, 10.0, 10.0);
        let again = circle_overlap(10.0, 10.0, 10.0);
        assert_eq!(overlap, again);
        assert!((0.0..PI * 100.0).contains(&overlap));
    }

    #[test]
    fn a_single_set_layout_centers_its_circle() {
        let layout = chart(&[VennArea::new(["A"], 10.0)], 800.0, 450.0, 8.0);
        assert_eq!(layout.circles.len(), 1);
        let circle = layout.circle("A").expect("circle A");
        assert!((circle.x - 400.0).abs() < 0.5);
        assert!((circle.y - 225.0).abs() < 0.5);
        assert!(!layout.areas[0].path.is_empty());
    }

    #[test]
    fn pairwise_areas_are_added_for_three_sets() {
        let areas = add_missing_areas(&[
            VennArea::new(["A"], 10.0),
            VennArea::new(["B"], 10.0),
            VennArea::new(["C"], 10.0),
        ]);
        assert_eq!(areas.len(), 6);
        assert!(areas.iter().any(|area| area.sets == ["A", "C"]));
    }
}
