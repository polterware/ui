//! Port of upstream `rendering-util/setupViewPortForSVG.ts`.

use std::collections::HashMap;

use crate::scene::{Rect, Scene};

/// Dimensions and top-left origin expanded by padding on all sides.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaddedDimensions {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
}

/// Calculate the dimensions and position with padding.
pub fn calculate_dimensions_with_padding(bounds: Rect, padding: f64) -> PaddedDimensions {
    PaddedDimensions {
        width: bounds.width + padding * 2.0,
        height: bounds.height + padding * 2.0,
        x: bounds.x,
        y: bounds.y,
    }
}

/// Create the SVG viewBox string or rect.
pub fn create_view_box(x: f64, y: f64, width: f64, height: f64, padding: f64) -> Rect {
    Rect::new(x - padding, y - padding, width, height)
}

/// Create the SVG viewBox string, e.g. "0 0 100 200".
pub fn create_view_box_string(x: f64, y: f64, width: f64, height: f64, padding: f64) -> String {
    format!("{} {} {} {}", x - padding, y - padding, width, height)
}

/// Gives attributes for an SVG's size given arguments.
pub fn calculate_svg_size_attrs(
    height: f64,
    width: f64,
    use_max_width: bool,
) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    if use_max_width {
        attrs.insert("width".to_string(), "100%".to_string());
        attrs.insert("style".to_string(), format!("max-width: {width}px;"));
    } else {
        attrs.insert("height".to_string(), height.to_string());
        attrs.insert("width".to_string(), width.to_string());
    }
    attrs
}

/// Configure the size and aspect ratio of the SVG.
pub fn configure_svg_size(
    _scene: &mut Scene,
    height: f64,
    width: f64,
    use_max_width: bool,
) -> HashMap<String, String> {
    calculate_svg_size_attrs(height, width, use_max_width)
}

/// Port of `setupViewPortForSVG`: updates `scene.view_box` and `scene.bounds`.
pub fn setup_view_port_for_svg(
    scene: &mut Scene,
    padding: f64,
    _css_diagram: &str,
    use_max_width: bool,
) -> Rect {
    let dims = calculate_dimensions_with_padding(scene.bounds, padding);
    let view_box = create_view_box(dims.x, dims.y, dims.width, dims.height, padding);
    scene.view_box = view_box;
    let _ = configure_svg_size(scene, dims.height, dims.width, use_max_width);
    view_box
}
