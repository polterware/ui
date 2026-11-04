//! The lit material of `base.css` (`--material-*`), drawn with GPUI's quads.
//!
//! CSS draws the edge of a raised object as inset box shadows: a 0.5px ring and a 0.5px top
//! band that follows the radius and fades along the corners. GPUI has no inset shadow, so the
//! edge is an overlay of two borders on the same radius: a full 0.5px ring and a 0.5px top
//! border, whose rounded corners taper the band the same way. Drops are outer box shadows;
//! CSS blur radii are twice GPUI's (GPUI takes the Gaussian sigma).

use gpui::{
    Background, BoxShadow, Div, Rgba, Styled, div, linear_color_stop, linear_gradient, point,
    prelude::*, px,
};

use crate::theme::Theme;

/// A CSS `box-shadow: 0 <y>px <blur>px <spread>px <color>`.
pub fn drop(y: f32, blur: f32, color: Rgba) -> BoxShadow {
    BoxShadow {
        color: color.into(),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur / 2.0),
        spread_radius: px(0.0),
    }
}

/// The drop of `--material-control`: `0 1px 2px` at `--material-drop-strong`.
pub fn control_drop(theme: &Theme) -> Vec<BoxShadow> {
    vec![drop(1.0, 2.0, theme.material_drop_strong)]
}

/// The drops of `--material-raised`: `0 2px 4px` and `0 1px 2px` at `--material-drop`.
pub fn raised_drop(theme: &Theme) -> Vec<BoxShadow> {
    vec![
        drop(2.0, 4.0, theme.material_drop),
        drop(1.0, 2.0, theme.material_drop),
    ]
}

/// The drops of `--material-float`: `0 8px 16px` and `0 4px 8px` at `--material-drop`.
pub fn float_drop(theme: &Theme) -> Vec<BoxShadow> {
    vec![
        drop(8.0, 16.0, theme.material_drop),
        drop(4.0, 8.0, theme.material_drop),
    ]
}

/// A top-to-bottom face, `linear-gradient(to bottom, top, bottom)`.
pub fn face(top: Rgba, bottom: Rgba) -> Background {
    linear_gradient(
        180.0,
        linear_color_stop(top, 0.0),
        linear_color_stop(bottom, 1.0),
    )
}

/// Which lit edge an object carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// `--material-edge`: controls (rim .2, ring .1).
    Strong,
    /// `--material-edge-quiet`: panels, cards, and floating surfaces (rim .16, ring .06).
    Quiet,
    /// `--material-hairline`: groups nested in a raised host (ring border-default, top .06).
    Hairline,
    /// `--material-well`: an inset 0.5px ring alone (white .1).
    Well,
}

/// The edge overlay for a box of `radius`, drawn inside it. Add it as the box's last child;
/// the box must be `relative()`.
pub fn edge(kind: Edge, radius: f32, theme: &Theme) -> Div {
    let (top, ring) = match kind {
        Edge::Strong => (
            Some(theme.material_rim_strong),
            theme.material_highlight_strong,
        ),
        Edge::Quiet => (Some(theme.material_rim), theme.material_highlight),
        Edge::Hairline => (Some(theme.material_highlight), theme.color_border_default),
        Edge::Well => (None, theme.material_highlight_strong),
    };
    let overlay = div().absolute().inset_0().rounded(px(radius)).child(
        div()
            .absolute()
            .inset_0()
            .rounded(px(radius))
            .border(px(0.5))
            .border_color(ring),
    );
    match top {
        Some(top) => overlay.child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius))
                .border_t(px(0.5))
                .border_color(top),
        ),
        None => overlay,
    }
}

/// `--material-rim-fade`: a 0.5px ring just outside a pill or a large radius, brightest on
/// top (.2) and fading down (.06). GPUI borders take one color, so the fade is a .06 ring
/// with a .15 top border over it, which composites to .2 on top.
pub fn rim_fade(radius: f32, theme: &Theme) -> Div {
    div()
        .absolute()
        .top(px(-0.5))
        .left(px(-0.5))
        .right(px(-0.5))
        .bottom(px(-0.5))
        .rounded(px(radius + 0.5))
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius + 0.5))
                .border(px(0.5))
                .border_color(theme.material_highlight),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius + 0.5))
                .border_t(px(0.5))
                .border_color(crate::theme::white(0.15)),
        )
}

/// A raised host panel (`.surface-panel`, `.card-panel`): an opaque face, the quiet edge,
/// and the raised drop.
pub fn raised_panel<E: Styled + ParentElement>(element: E, radius: f32, theme: &Theme) -> E {
    element
        .rounded(px(radius))
        .shadow(raised_drop(theme))
        .child(edge(Edge::Quiet, radius, theme))
}

/// A floating surface (menus, popovers, dialogs, tooltips): the quiet edge and the float drop.
/// It takes the pointer over its whole face, so a press on it never reaches what it floats
/// over, wherever it sits: a submenu or a picker drawn past its owner's bounds blocks the
/// page under it as much as a panel in place does.
pub fn floating_panel<E: Styled + ParentElement + InteractiveElement>(
    element: E,
    radius: f32,
    theme: &Theme,
) -> E {
    element
        .occlude()
        .rounded(px(radius))
        .shadow(float_drop(theme))
        .child(edge(Edge::Quiet, radius, theme))
}
