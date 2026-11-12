//! Port of upstream `rendering-util/labelTransform.ts`.

use super::types::LabelBBox;

/// Compute the SVG translate transform needed to center a label element at the origin.
pub fn compute_label_transform(bbox: LabelBBox, use_html_labels: bool) -> String {
    if use_html_labels {
        format!("translate({}, {})", -bbox.width / 2.0, -bbox.height / 2.0)
    } else {
        let x = bbox.x.unwrap_or(0.0);
        let y = bbox.y.unwrap_or(0.0);
        format!(
            "translate({}, {})",
            -(x + bbox.width / 2.0),
            -(y + bbox.height / 2.0)
        )
    }
}
