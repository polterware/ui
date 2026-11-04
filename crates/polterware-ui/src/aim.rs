//! Menu aim, the safe triangle every native menu keeps: while the pointer travels from the
//! row that opened a submenu toward that submenu, the rows it crosses on the way neither
//! close the submenu nor open their own. A row it rests on still does, a moment later.
//!
//! The pointer aims at the submenu when its newest position lies in the triangle from the
//! position before it to the two corners of the submenu's near edge (widened by
//! [`SLACK`]). A pointer that stopped, or turned away, no longer aims.
//!
//! A menu feeds it every pointer move while its submenu is open ([`MenuAim::moved`]) and the
//! submenu's bounds ([`MenuAim::set_target`]); on a hover that would switch or close the
//! submenu it asks [`MenuAim::aiming`] and, while that holds, waits [`AIM_DELAY`] and asks
//! again, with [`MenuAim::moves`] telling whether the pointer moved at all since.

use std::time::Duration;

use gpui::{Bounds, Pixels, Point, point};

/// How long a crossed row waits before it may take over, asked again while the pointer aims.
pub const AIM_DELAY: Duration = Duration::from_millis(220);

/// How far the triangle's base runs past the submenu's top and bottom corners.
pub const SLACK: f32 = 24.0;

#[derive(Clone, Debug, Default)]
pub struct MenuAim {
    previous: Option<Point<Pixels>>,
    current: Option<Point<Pixels>>,
    target: Option<Bounds<Pixels>>,
    moves: u64,
}

impl MenuAim {
    /// The open submenu's bounds, or None once it closes.
    pub fn set_target(&mut self, target: Option<Bounds<Pixels>>) {
        self.target = target;
    }

    /// The pointer moved to `position`.
    pub fn moved(&mut self, position: Point<Pixels>) {
        if self.current == Some(position) {
            return;
        }
        self.previous = self.current;
        self.current = Some(position);
        self.moves = self.moves.wrapping_add(1);
    }

    /// How many moves it has seen: unchanged between two asks means the pointer rested.
    pub fn moves(&self) -> u64 {
        self.moves
    }

    /// Forgets the pointer's path, as when the menu closes.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Whether the pointer's last move heads into the submenu.
    pub fn aiming(&self) -> bool {
        let (Some(from), Some(to), Some(target)) = (self.previous, self.current, self.target)
        else {
            return false;
        };
        if target.contains(&to) {
            return true;
        }
        let x = |p: Point<Pixels>| f32::from(p.x);
        let y = |p: Point<Pixels>| f32::from(p.y);
        let (left, right) = (f32::from(target.left()), f32::from(target.right()));
        // The edge nearer to where the pointer came from.
        let edge = if x(from) <= left {
            left
        } else if x(from) >= right {
            right
        } else {
            return false;
        };
        let top = point(edge, f32::from(target.top()) - SLACK);
        let bottom = point(edge, f32::from(target.bottom()) + SLACK);
        in_triangle(point(x(to), y(to)), point(x(from), y(from)), top, bottom)
    }
}

/// Whether `p` lies in the triangle `a b c`, edges included.
fn in_triangle(p: Point<f32>, a: Point<f32>, b: Point<f32>, c: Point<f32>) -> bool {
    let side = |p: Point<f32>, q: Point<f32>, r: Point<f32>| {
        (p.x - r.x) * (q.y - r.y) - (q.x - r.x) * (p.y - r.y)
    };
    let d1 = side(p, a, b);
    let d2 = side(p, b, c);
    let d3 = side(p, c, a);
    let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(negative && positive)
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};

    use super::MenuAim;

    /// A submenu 200 wide and 160 tall to the right of x = 300, from y = 100.
    fn aimed(from: (f32, f32), to: (f32, f32)) -> bool {
        let mut aim = MenuAim::default();
        aim.set_target(Some(Bounds::new(
            point(px(300.0), px(100.0)),
            size(px(200.0), px(160.0)),
        )));
        aim.moved(point(px(from.0), px(from.1)));
        aim.moved(point(px(to.0), px(to.1)));
        aim.aiming()
    }

    #[test]
    fn a_diagonal_toward_the_submenu_aims_at_it() {
        // From a row at y 110, down and right toward the submenu's lower rows.
        assert!(aimed((250.0, 110.0), (262.0, 122.0)));
        // Straight across.
        assert!(aimed((250.0, 110.0), (270.0, 110.0)));
    }

    #[test]
    fn a_move_away_or_along_the_menu_does_not() {
        // Back to the left, away from the submenu.
        assert!(!aimed((250.0, 110.0), (240.0, 112.0)));
        // Straight down the menu, past the triangle.
        assert!(!aimed((250.0, 110.0), (250.0, 140.0)));
    }

    #[test]
    fn a_pointer_that_rests_or_has_no_submenu_does_not_aim() {
        let mut aim = MenuAim::default();
        aim.moved(point(px(250.0), px(110.0)));
        aim.moved(point(px(262.0), px(122.0)));
        assert!(!aim.aiming(), "no submenu");
        aim.set_target(Some(Bounds::new(
            point(px(300.0), px(100.0)),
            size(px(200.0), px(160.0)),
        )));
        let moves = aim.moves();
        // The same position again is no move.
        aim.moved(point(px(262.0), px(122.0)));
        assert_eq!(aim.moves(), moves);
    }

    #[test]
    fn a_submenu_on_the_left_is_aimed_at_from_the_right() {
        let mut aim = MenuAim::default();
        aim.set_target(Some(Bounds::new(
            point(px(0.0), px(100.0)),
            size(px(200.0), px(160.0)),
        )));
        aim.moved(point(px(260.0), px(110.0)));
        aim.moved(point(px(240.0), px(130.0)));
        assert!(aim.aiming());
    }
}
