//! Port of `layout-base/src/util/IGeometry.js`.
//!
//! The static geometry helpers: the separation amount of two overlapping rectangles,
//! the clipping points of the segment between two rectangle centers, line and segment
//! intersections, and the pre-calculated angle constants.
//!
//! Upstream's `getIntersection` is overloaded: with three arguments it returns the
//! boolean of `getIntersection2`, with four it returns a `Point`. Only the three-argument
//! form has a caller (`LEdge.updateLength` and `fd/FDLayout`), so the port keeps
//! [`get_intersection2`] for it and [`get_intersection`] for the four-argument form.

use super::super::LayoutError;
use super::point::Point;
use super::rectangle_d::RectangleD;

/// Half of pi, upstream's `IGeometry.HALF_PI`.
pub const HALF_PI: f64 = 0.5 * std::f64::consts::PI;
/// One and a half pi, upstream's `IGeometry.ONE_AND_HALF_PI`.
pub const ONE_AND_HALF_PI: f64 = 1.5 * std::f64::consts::PI;
/// Two pi, upstream's `IGeometry.TWO_PI`.
pub const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
/// Three pi, upstream's `IGeometry.THREE_PI`.
pub const THREE_PI: f64 = 3.0 * std::f64::consts::PI;

/// Upstream `IGeometry.calcSeparationAmount`: half the amount each rectangle must move
/// in `x` and `y` to stop overlapping, plus the buffer. Upstream throws when the
/// rectangles do not intersect, which is the only failure.
pub fn calc_separation_amount(
    rect_a: &RectangleD,
    rect_b: &RectangleD,
    overlap_amount: &mut [f64; 2],
    separation_buffer: f64,
) -> Result<(), LayoutError> {
    if !rect_a.intersects(rect_b) {
        return Err(LayoutError("assert failed"));
    }

    let mut directions = [0i32; 2];

    decide_directions_for_overlapping_nodes(rect_a, rect_b, &mut directions);

    overlap_amount[0] = rect_a.get_right().min(rect_b.get_right()) - rect_a.x.max(rect_b.x);
    overlap_amount[1] = rect_a.get_bottom().min(rect_b.get_bottom()) - rect_a.y.max(rect_b.y);

    // Update the overlapping amounts for the cases where one rectangle contains the
    // other in one direction.
    if (rect_a.get_x() <= rect_b.get_x()) && (rect_a.get_right() >= rect_b.get_right()) {
        overlap_amount[0] +=
            (rect_b.get_x() - rect_a.get_x()).min(rect_a.get_right() - rect_b.get_right());
    } else if (rect_b.get_x() <= rect_a.get_x()) && (rect_b.get_right() >= rect_a.get_right()) {
        overlap_amount[0] +=
            (rect_a.get_x() - rect_b.get_x()).min(rect_b.get_right() - rect_a.get_right());
    }
    if (rect_a.get_y() <= rect_b.get_y()) && (rect_a.get_bottom() >= rect_b.get_bottom()) {
        overlap_amount[1] +=
            (rect_b.get_y() - rect_a.get_y()).min(rect_a.get_bottom() - rect_b.get_bottom());
    } else if (rect_b.get_y() <= rect_a.get_y()) && (rect_b.get_bottom() >= rect_a.get_bottom()) {
        overlap_amount[1] +=
            (rect_a.get_y() - rect_b.get_y()).min(rect_b.get_bottom() - rect_a.get_bottom());
    }

    // Slope of the line that passes through the two centers.
    let mut slope = ((rect_b.get_center_y() - rect_a.get_center_y())
        / (rect_b.get_center_x() - rect_a.get_center_x()))
    .abs();
    // If the centers are the same, assume 45 degrees.
    if (rect_b.get_center_y() == rect_a.get_center_y())
        && (rect_b.get_center_x() == rect_a.get_center_x())
    {
        slope = 1.0;
    }

    let mut move_by_y = slope * overlap_amount[0];
    let mut move_by_x = overlap_amount[1] / slope;
    if overlap_amount[0] < move_by_x {
        move_by_x = overlap_amount[0];
    } else {
        move_by_y = overlap_amount[1];
    }
    // Half the amount, so that moving each rectangle the same amount in opposite
    // directions resolves the overlap.
    overlap_amount[0] = -(directions[0] as f64) * ((move_by_x / 2.0) + separation_buffer);
    overlap_amount[1] = -(directions[1] as f64) * ((move_by_y / 2.0) + separation_buffer);

    Ok(())
}

/// Upstream `IGeometry.decideDirectionsForOverlappingNodes`: `directions[0]` is `-1`
/// when `rect_a` moves left, `directions[1]` is `-1` when it moves up.
pub fn decide_directions_for_overlapping_nodes(
    rect_a: &RectangleD,
    rect_b: &RectangleD,
    directions: &mut [i32; 2],
) {
    if rect_a.get_center_x() < rect_b.get_center_x() {
        directions[0] = -1;
    } else {
        directions[0] = 1;
    }

    if rect_a.get_center_y() < rect_b.get_center_y() {
        directions[1] = -1;
    } else {
        directions[1] = 1;
    }
}

/// Upstream `IGeometry.getIntersection2`: the clipping points of the segment between the
/// centers of the two rectangles, in `result[0..2]` for `rect_a` and `result[2..4]` for
/// `rect_b`, and whether the rectangles intersect (in which case the points are the
/// centers). The caller supplies the four slots, as upstream's array is also mutated in
/// place; a branch can return without writing all of them, as upstream can.
pub fn get_intersection2(rect_a: &RectangleD, rect_b: &RectangleD, result: &mut [f64; 4]) -> bool {
    let p1x = rect_a.get_center_x();
    let p1y = rect_a.get_center_y();
    let p2x = rect_b.get_center_x();
    let p2y = rect_b.get_center_y();

    // If the two rectangles intersect, the clipping points are the centers.
    if rect_a.intersects(rect_b) {
        result[0] = p1x;
        result[1] = p1y;
        result[2] = p2x;
        result[3] = p2y;
        return true;
    }

    // Variables for rect_a.
    let top_left_ax = rect_a.get_x();
    let top_left_ay = rect_a.get_y();
    let top_right_ax = rect_a.get_right();
    let bottom_left_ax = rect_a.get_x();
    let bottom_left_ay = rect_a.get_bottom();
    let bottom_right_ax = rect_a.get_right();
    let half_width_a = rect_a.get_width_half();
    let half_height_a = rect_a.get_height_half();
    // Variables for rect_b.
    let top_left_bx = rect_b.get_x();
    let top_left_by = rect_b.get_y();
    let top_right_bx = rect_b.get_right();
    let bottom_left_bx = rect_b.get_x();
    let bottom_left_by = rect_b.get_bottom();
    let bottom_right_bx = rect_b.get_right();
    let half_width_b = rect_b.get_width_half();
    let half_height_b = rect_b.get_height_half();

    // Whether the clipping points are found.
    let mut clip_point_a_found = false;
    let mut clip_point_b_found = false;

    // The line is vertical.
    if p1x == p2x {
        if p1y > p2y {
            result[0] = p1x;
            result[1] = top_left_ay;
            result[2] = p2x;
            result[3] = bottom_left_by;
            return false;
        } else if p1y < p2y {
            result[0] = p1x;
            result[1] = bottom_left_ay;
            result[2] = p2x;
            result[3] = top_left_by;
            return false;
        }
        // Otherwise the centers are the same point, which is not a line.
    }
    // The line is horizontal.
    else if p1y == p2y {
        if p1x > p2x {
            result[0] = top_left_ax;
            result[1] = p1y;
            result[2] = top_right_bx;
            result[3] = p2y;
            return false;
        } else if p1x < p2x {
            result[0] = top_right_ax;
            result[1] = p1y;
            result[2] = top_left_bx;
            result[3] = p2y;
            return false;
        }
        // Otherwise the centers are the same point, which is not a line.
    } else {
        // Slopes of the rectangles' diagonals.
        let slope_a = rect_a.height / rect_a.width;
        let slope_b = rect_b.height / rect_b.width;

        // Slope of the line between the centers.
        let slope_prime = (p2y - p1y) / (p2x - p1x);
        let cardinal_direction_a;
        let cardinal_direction_b;

        // Whether the clipping point is the corner of node A.
        if (-slope_a) == slope_prime {
            if p1x > p2x {
                result[0] = bottom_left_ax;
                result[1] = bottom_left_ay;
                clip_point_a_found = true;
            } else {
                result[0] = top_right_ax;
                result[1] = top_left_ay;
                clip_point_a_found = true;
            }
        } else if slope_a == slope_prime {
            if p1x > p2x {
                result[0] = top_left_ax;
                result[1] = top_left_ay;
                clip_point_a_found = true;
            } else {
                result[0] = bottom_right_ax;
                result[1] = bottom_left_ay;
                clip_point_a_found = true;
            }
        }

        // Whether the clipping point is the corner of node B.
        if (-slope_b) == slope_prime {
            if p2x > p1x {
                result[2] = bottom_left_bx;
                result[3] = bottom_left_by;
                clip_point_b_found = true;
            } else {
                result[2] = top_right_bx;
                result[3] = top_left_by;
                clip_point_b_found = true;
            }
        } else if slope_b == slope_prime {
            if p2x > p1x {
                result[2] = top_left_bx;
                result[3] = top_left_by;
                clip_point_b_found = true;
            } else {
                result[2] = bottom_right_bx;
                result[3] = bottom_left_by;
                clip_point_b_found = true;
            }
        }

        // If both clipping points are corners, the segment is along a diagonal.
        if clip_point_a_found && clip_point_b_found {
            return false;
        }

        // Cardinal directions of the rectangles.
        if p1x > p2x {
            if p1y > p2y {
                cardinal_direction_a = get_cardinal_direction(slope_a, slope_prime, 4);
                cardinal_direction_b = get_cardinal_direction(slope_b, slope_prime, 2);
            } else {
                cardinal_direction_a = get_cardinal_direction(-slope_a, slope_prime, 3);
                cardinal_direction_b = get_cardinal_direction(-slope_b, slope_prime, 1);
            }
        } else if p1y > p2y {
            cardinal_direction_a = get_cardinal_direction(-slope_a, slope_prime, 1);
            cardinal_direction_b = get_cardinal_direction(-slope_b, slope_prime, 3);
        } else {
            cardinal_direction_a = get_cardinal_direction(slope_a, slope_prime, 2);
            cardinal_direction_b = get_cardinal_direction(slope_b, slope_prime, 4);
        }

        // Calculate the clipping point of A if it was not found before.
        if !clip_point_a_found {
            match cardinal_direction_a {
                1 => {
                    result[1] = top_left_ay;
                    result[0] = p1x + (-half_height_a) / slope_prime;
                }
                2 => {
                    result[0] = bottom_right_ax;
                    result[1] = p1y + half_width_a * slope_prime;
                }
                3 => {
                    result[1] = bottom_left_ay;
                    result[0] = p1x + half_height_a / slope_prime;
                }
                4 => {
                    result[0] = bottom_left_ax;
                    result[1] = p1y + (-half_width_a) * slope_prime;
                }
                _ => {}
            }
        }
        // Calculate the clipping point of B if it was not found before.
        if !clip_point_b_found {
            match cardinal_direction_b {
                1 => {
                    result[3] = top_left_by;
                    result[2] = p2x + (-half_height_b) / slope_prime;
                }
                2 => {
                    result[2] = bottom_right_bx;
                    result[3] = p2y + half_width_b * slope_prime;
                }
                3 => {
                    result[3] = bottom_left_by;
                    result[2] = p2x + half_height_b / slope_prime;
                }
                4 => {
                    result[2] = bottom_left_bx;
                    result[3] = p2y + (-half_width_b) * slope_prime;
                }
                _ => {}
            }
        }
    }

    false
}

/// Upstream `IGeometry.getCardinalDirection`: `1` north, `2` east, `3` south, `4` west.
pub fn get_cardinal_direction(slope: f64, slope_prime: f64, line: i32) -> i32 {
    if slope > slope_prime {
        line
    } else {
        1 + line % 4
    }
}

/// Upstream `IGeometry.getIntersection(s1, s2, f1, f2)`: the intersection of the lines
/// through the point pairs, or `None` when they are parallel.
pub fn get_intersection(s1: &Point, s2: &Point, f1: &Point, f2: &Point) -> Option<Point> {
    let x1 = s1.x;
    let y1 = s1.y;
    let x2 = s2.x;
    let y2 = s2.y;
    let x3 = f1.x;
    let y3 = f1.y;
    let x4 = f2.x;
    let y4 = f2.y;

    // Coefficients of the line equations.
    let a1 = y2 - y1;
    let b1 = x1 - x2;
    let c1 = x2 * y1 - x1 * y2; // a1*x + b1*y + c1 = 0 is line 1

    let a2 = y4 - y3;
    let b2 = x3 - x4;
    let c2 = x4 * y3 - x3 * y4; // a2*x + b2*y + c2 = 0 is line 2

    let denom = a1 * b2 - a2 * b1;

    if denom == 0.0 {
        return None;
    }

    let x = (b1 * c2 - b2 * c1) / denom;
    let y = (a2 * c1 - a1 * c2) / denom;

    Some(Point::new(x, y))
}

/// Upstream `IGeometry.angleOfVector`: the angle of the vector from the positive x-axis,
/// clockwise (the Java coordinate system).
pub fn angle_of_vector(cx: f64, cy: f64, nx: f64, ny: f64) -> f64 {
    let mut c_angle;

    if cx != nx {
        c_angle = ((ny - cy) / (nx - cx)).atan();

        if nx < cx {
            c_angle += std::f64::consts::PI;
        } else if ny < cy {
            c_angle += TWO_PI;
        }
    } else if ny < cy {
        c_angle = ONE_AND_HALF_PI; // 270 degrees
    } else {
        c_angle = HALF_PI; // 90 degrees
    }

    c_angle
}

/// Upstream `IGeometry.doIntersect`: whether the two segments cross at a point other
/// than their endpoints.
pub fn do_intersect(p1: &Point, p2: &Point, p3: &Point, p4: &Point) -> bool {
    let a = p1.x;
    let b = p1.y;
    let c = p2.x;
    let d = p2.y;
    let p = p3.x;
    let q = p3.y;
    let r = p4.x;
    let s = p4.y;
    let det = (c - a) * (s - q) - (r - p) * (d - b);

    if det == 0.0 {
        false
    } else {
        let lambda = ((s - q) * (r - a) + (p - r) * (s - b)) / det;
        let gamma = ((b - d) * (r - a) + (c - a) * (s - b)) / det;
        (0.0 < lambda && lambda < 1.0) && (0.0 < gamma && gamma < 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `getIntersection2` with four unset slots, as upstream's `new Array(4)`.
    fn clip(rect_a: &RectangleD, rect_b: &RectangleD) -> (bool, [f64; 4]) {
        let mut result = [f64::NAN; 4];
        let intersects = get_intersection2(rect_a, rect_b, &mut result);
        (intersects, result)
    }

    // The expected values in these tests were printed by upstream's JavaScript:
    // node -e 'const I = require(".../util/IGeometry.js");
    //          const R = require(".../util/RectangleD.js");
    //          const r = new Array(4);
    //          console.log(I.getIntersection2(new R(...), new R(...), r), r)'

    #[test]
    fn diagonal_corner_clipping_points() {
        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(200.0, 100.0, 80.0, 40.0),
        );
        assert!(!intersects);
        assert_eq!(result, [100.0, 50.0, 200.0, 100.0]);

        let (intersects, result) = clip(
            &RectangleD::new(200.0, 100.0, 80.0, 40.0),
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
        );
        assert!(!intersects);
        assert_eq!(result, [200.0, 100.0, 100.0, 50.0]);
    }

    #[test]
    fn vertical_and_horizontal_clipping_points() {
        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(0.0, 200.0, 100.0, 50.0),
        );
        assert!(!intersects);
        assert_eq!(result, [50.0, 50.0, 50.0, 200.0]);

        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 40.0, 200.0),
            &RectangleD::new(0.0, -300.0, 40.0, 200.0),
        );
        assert!(!intersects);
        assert_eq!(result, [20.0, 0.0, 20.0, -100.0]);

        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(300.0, 0.0, 100.0, 50.0),
        );
        assert!(!intersects);
        assert_eq!(result, [100.0, 25.0, 300.0, 25.0]);

        let (intersects, result) = clip(
            &RectangleD::new(300.0, 0.0, 100.0, 50.0),
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
        );
        assert!(!intersects);
        assert_eq!(result, [300.0, 25.0, 100.0, 25.0]);
    }

    #[test]
    fn cardinal_direction_clipping_points() {
        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(300.0, 50.0, 100.0, 100.0),
        );
        assert!(!intersects);
        assert_eq!(result, [100.0, 37.5, 300.0, 87.5]);

        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(-300.0, 50.0, 100.0, 100.0),
        );
        assert!(!intersects);
        assert_eq!(result, [0.0, 37.5, -200.0, 87.5]);

        let (intersects, result) = clip(
            &RectangleD::new(0.0, 100.0, 100.0, 50.0),
            &RectangleD::new(300.0, 0.0, 100.0, 100.0),
        );
        assert!(!intersects);
        assert_eq!(result, [100.0, 112.5, 300.0, 62.5]);
    }

    #[test]
    fn overlapping_rectangles_clip_at_their_centers() {
        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(40.0, 20.0, 100.0, 50.0),
        );
        assert!(intersects);
        assert_eq!(result, [50.0, 25.0, 90.0, 45.0]);

        let (intersects, result) = clip(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
        );
        assert!(intersects);
        assert_eq!(result, [50.0, 25.0, 50.0, 25.0]);
    }

    #[test]
    fn separation_amounts() {
        // node -e 'const I = require(".../util/IGeometry.js"); ...'
        let mut overlap = [0.0; 2];
        calc_separation_amount(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(40.0, 20.0, 100.0, 50.0),
            &mut overlap,
            5.0,
        )
        .unwrap();
        assert_eq!(overlap, [35.0, 20.0]);

        let mut overlap = [0.0; 2];
        calc_separation_amount(
            &RectangleD::new(0.0, 0.0, 100.0, 100.0),
            &RectangleD::new(10.0, 10.0, 100.0, 100.0),
            &mut overlap,
            0.0,
        )
        .unwrap();
        assert_eq!(overlap, [45.0, 45.0]);

        let mut overlap = [0.0; 2];
        calc_separation_amount(
            &RectangleD::new(0.0, 0.0, 100.0, 100.0),
            &RectangleD::new(90.0, 90.0, 100.0, 100.0),
            &mut overlap,
            10.0,
        )
        .unwrap();
        assert_eq!(overlap, [15.0, 15.0]);

        let mut overlap = [0.0; 2];
        calc_separation_amount(
            &RectangleD::new(0.0, 0.0, 100.0, 100.0),
            &RectangleD::new(-90.0, -90.0, 100.0, 100.0),
            &mut overlap,
            10.0,
        )
        .unwrap();
        assert_eq!(overlap, [-15.0, -15.0]);

        let mut overlap = [0.0; 2];
        calc_separation_amount(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(0.0, 40.0, 100.0, 50.0),
            &mut overlap,
            0.0,
        )
        .unwrap();
        assert_eq!(overlap, [0.0, 5.0]);

        let mut overlap = [0.0; 2];
        calc_separation_amount(
            &RectangleD::new(0.0, 0.0, 100.0, 50.0),
            &RectangleD::new(90.0, 0.0, 100.0, 50.0),
            &mut overlap,
            0.0,
        )
        .unwrap();
        assert_eq!(overlap, [5.0, 0.0]);
    }

    #[test]
    fn separation_needs_intersecting_rectangles() {
        let mut overlap = [0.0; 2];
        let error = calc_separation_amount(
            &RectangleD::new(0.0, 0.0, 10.0, 10.0),
            &RectangleD::new(100.0, 100.0, 10.0, 10.0),
            &mut overlap,
            0.0,
        )
        .unwrap_err();
        assert_eq!(error, LayoutError("assert failed"));
    }

    #[test]
    fn line_intersections() {
        let intersection = get_intersection(
            &Point::new(0.0, 0.0),
            &Point::new(10.0, 10.0),
            &Point::new(0.0, 10.0),
            &Point::new(10.0, 0.0),
        )
        .unwrap();
        assert_eq!(intersection, Point::new(5.0, 5.0));

        assert_eq!(
            get_intersection(
                &Point::new(0.0, 0.0),
                &Point::new(10.0, 10.0),
                &Point::new(0.0, 5.0),
                &Point::new(10.0, 15.0),
            ),
            None
        );

        assert!(do_intersect(
            &Point::new(0.0, 0.0),
            &Point::new(10.0, 10.0),
            &Point::new(0.0, 10.0),
            &Point::new(10.0, 0.0),
        ));
        assert!(!do_intersect(
            &Point::new(0.0, 0.0),
            &Point::new(10.0, 10.0),
            &Point::new(0.0, 5.0),
            &Point::new(10.0, 15.0),
        ));
    }

    #[test]
    fn angles_and_cardinal_directions() {
        assert_eq!(
            angle_of_vector(0.0, 0.0, 10.0, 10.0),
            std::f64::consts::FRAC_PI_4
        );
        assert_eq!(angle_of_vector(0.0, 0.0, -10.0, 0.0), std::f64::consts::PI);
        assert_eq!(angle_of_vector(0.0, 10.0, 0.0, 0.0), ONE_AND_HALF_PI);
        assert_eq!(angle_of_vector(0.0, 0.0, 0.0, 10.0), HALF_PI);

        assert_eq!(get_cardinal_direction(0.5, 0.2, 4), 4);
        assert_eq!(get_cardinal_direction(0.1, 0.2, 4), 1);
        assert_eq!(get_cardinal_direction(1.0, 0.2, 1), 1);
    }
}
