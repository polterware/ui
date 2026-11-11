//! Reusable scene marker definitions corresponding to upstream `markers.js` and `edgeMarker.ts`.
//! Marker classes retain upstream role names for diagram-specific CSS/theming.
use crate::scene::{
    Color, FillRule, MarkerDef, MarkerOrient, MarkerRef, MarkerUnits, Path, PathShape, PathStyle,
    Point, Rect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Cross,
    Point,
    Barb,
    Circle,
    OpenArrow,
    HollowCircle,
    Hierarchy,
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
    OnlyOne,
    ZeroOrOne,
    OneOrMore,
    ZeroOrMore,
    RequirementArrow,
    RequirementContains,
}
impl MarkerKind {
    pub fn from_arrow_type(value: &str) -> Option<Self> {
        Some(match value {
            "arrow_cross" => Self::Cross,
            "arrow_point" => Self::Point,
            "arrow_barb" | "arrow_barb_neo" => Self::Barb,
            "arrow_circle" => Self::Circle,
            "arrow_open_head" => Self::OpenArrow,
            "arrow_hollow_circle" => Self::HollowCircle,
            "arrow_hierarchy" => Self::Hierarchy,
            "aggregation" => Self::Aggregation,
            "extension" => Self::Extension,
            "composition" => Self::Composition,
            "dependency" => Self::Dependency,
            "lollipop" => Self::Lollipop,
            "only_one" => Self::OnlyOne,
            "zero_or_one" => Self::ZeroOrOne,
            "one_or_more" => Self::OneOrMore,
            "zero_or_more" => Self::ZeroOrMore,
            "requirement_arrow" => Self::RequirementArrow,
            "requirement_contains" => Self::RequirementContains,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::Cross => "cross",
            Self::Point => "point",
            Self::Barb => "barb",
            Self::Circle => "circle",
            Self::OpenArrow => "openArrow",
            Self::HollowCircle => "hollowCircle",
            Self::Hierarchy => "hierarchy",
            Self::Aggregation => "aggregation",
            Self::Extension => "extension",
            Self::Composition => "composition",
            Self::Dependency => "dependency",
            Self::Lollipop => "lollipop",
            Self::OnlyOne => "onlyOne",
            Self::ZeroOrOne => "zeroOrOne",
            Self::OneOrMore => "oneOrMore",
            Self::ZeroOrMore => "zeroOrMore",
            Self::RequirementArrow => "requirement_arrow",
            Self::RequirementContains => "requirement_contains",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerPlacement {
    pub start: Option<MarkerRef>,
    pub end: Option<MarkerRef>,
}

/// Create a marker definition with a consistent scene id, preserving start/end variants.
pub fn marker_definition(
    diagram_id: &str,
    diagram_type: &str,
    kind: MarkerKind,
    start: bool,
    margin: bool,
) -> MarkerDef {
    let name = kind.name();
    let suffix = if start { "Start" } else { "End" };
    let margin_suffix = if margin { "-margin" } else { "" };
    let id = format!("{diagram_id}_{diagram_type}-{name}{suffix}{margin_suffix}");
    let (w, h, reference, commands, fill) = geometry(kind, start, margin);
    let mut def = MarkerDef::new(id, Rect::new(0., 0., w, h))
        .with_reference(reference)
        .with_size(w, h)
        .with_orient(MarkerOrient::Auto)
        .with_class(format!("marker {name} {diagram_type}"));
    let mut style = if fill {
        PathStyle::filled(Color::current())
    } else {
        PathStyle::stroked(Color::current(), 1.0)
    };
    if !fill {
        style.fill = Some(Color::none());
    }
    def.units = MarkerUnits::UserSpaceOnUse;
    def.push(PathShape {
        path: commands,
        style,
        fill_rule: FillRule::default(),
        ..PathShape::default()
    });
    def
}
fn geometry(kind: MarkerKind, start: bool, margin: bool) -> (f64, f64, Point, Path, bool) {
    let mut p = Path::new();
    let (reference, fill) = match kind {
        MarkerKind::Extension
        | MarkerKind::Point
        | MarkerKind::Barb
        | MarkerKind::Composition
        | MarkerKind::Aggregation
        | MarkerKind::Dependency => (
            if start {
                Point::new(18., 7.)
            } else {
                Point::new(1., 7.)
            },
            matches!(
                kind,
                MarkerKind::Point
                    | MarkerKind::Barb
                    | MarkerKind::Composition
                    | MarkerKind::Dependency
            ),
        ),
        _ => (
            if start {
                Point::new(13., 7.)
            } else {
                Point::new(1., 7.)
            },
            false,
        ),
    };
    match kind {
        MarkerKind::Extension | MarkerKind::Point => {
            p.move_to(Point::new(1., 7.))
                .line_to(Point::new(18., 13.))
                .line_to(Point::new(18., 1.))
                .close();
        }
        MarkerKind::Composition | MarkerKind::Aggregation => {
            p.move_to(Point::new(18., 7.))
                .line_to(Point::new(9., 13.))
                .line_to(Point::new(1., 7.))
                .line_to(Point::new(9., 1.))
                .close();
        }
        MarkerKind::Dependency => {
            p.move_to(Point::new(5., 7.))
                .line_to(Point::new(9., 13.))
                .line_to(Point::new(1., 7.))
                .line_to(Point::new(9., 1.))
                .close();
        }
        MarkerKind::Cross => {
            p.move_to(Point::new(3., 3.))
                .line_to(Point::new(11., 11.))
                .move_to(Point::new(11., 3.))
                .line_to(Point::new(3., 11.));
        }
        MarkerKind::Circle | MarkerKind::HollowCircle | MarkerKind::Lollipop => {
            p.move_to(Point::new(13., 7.))
                .arc_to(6., 6., 0., true, true, Point::new(1., 7.))
                .arc_to(6., 6., 0., true, true, Point::new(13., 7.))
                .close();
        }
        MarkerKind::OpenArrow | MarkerKind::Hierarchy | MarkerKind::Barb => {
            p.move_to(Point::new(1., 1.))
                .line_to(Point::new(13., 7.))
                .line_to(Point::new(1., 13.));
        }
        MarkerKind::OnlyOne
        | MarkerKind::ZeroOrOne
        | MarkerKind::OneOrMore
        | MarkerKind::ZeroOrMore => {
            p.move_to(Point::new(1., 7.)).line_to(Point::new(12., 7.));
            if matches!(kind, MarkerKind::ZeroOrOne | MarkerKind::ZeroOrMore) {
                p.move_to(Point::new(12., 7.))
                    .arc_to(5., 5., 0., true, true, Point::new(22., 7.))
                    .arc_to(5., 5., 0., true, true, Point::new(12., 7.));
            }
            if matches!(kind, MarkerKind::OneOrMore | MarkerKind::ZeroOrMore) {
                p.move_to(Point::new(12., 7.))
                    .line_to(Point::new(22., 1.))
                    .move_to(Point::new(12., 7.))
                    .line_to(Point::new(22., 13.));
            }
        }
        MarkerKind::RequirementArrow | MarkerKind::RequirementContains => {
            p.move_to(Point::new(1., 1.))
                .line_to(Point::new(13., 7.))
                .line_to(Point::new(1., 13.));
        }
    }
    let (w, h) = if margin { (20., 14.) } else { (20., 14.) };
    (w, h, reference, p, fill)
}

/// Map arrow names to scene marker references; empty/none are no-ops and unknown names are errors.
pub fn marker_placement(
    start: Option<&str>,
    end: Option<&str>,
    diagram_id: &str,
    diagram_type: &str,
    margin: bool,
) -> Result<MarkerPlacement, String> {
    fn one(
        v: Option<&str>,
        start: bool,
        id: &str,
        dt: &str,
        margin: bool,
    ) -> Result<Option<MarkerRef>, String> {
        match v {
            None | Some("") | Some("none") => Ok(None),
            Some(value) => {
                let kind = MarkerKind::from_arrow_type(value)
                    .ok_or_else(|| format!("Unknown arrow type: {value}"))?;
                let def = marker_definition(id, dt, kind, start, margin);
                Ok(Some(MarkerRef::new(def.id)))
            }
        }
    }
    Ok(MarkerPlacement {
        start: one(start, true, diagram_id, diagram_type, margin)?,
        end: one(end, false, diagram_id, diagram_type, margin)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_every_upstream_arrow_alias() {
        for name in [
            "arrow_cross",
            "arrow_point",
            "arrow_barb",
            "arrow_barb_neo",
            "arrow_circle",
            "arrow_open_head",
            "arrow_hollow_circle",
            "arrow_hierarchy",
            "aggregation",
            "extension",
            "composition",
            "dependency",
            "lollipop",
            "only_one",
            "zero_or_one",
            "one_or_more",
            "zero_or_more",
            "requirement_arrow",
            "requirement_contains",
        ] {
            assert!(MarkerKind::from_arrow_type(name).is_some(), "{name}");
        }
    }
    #[test]
    fn skips_none_and_rejects_unknown() {
        let p = marker_placement(Some("none"), Some("not-real"), "d", "flowchart", false);
        assert!(p.is_err());
        assert!(
            marker_placement(Some(""), None, "d", "flowchart", false)
                .unwrap()
                .start
                .is_none()
        );
    }
    #[test]
    fn definitions_keep_class_and_geometry() {
        let d = marker_definition("d", "class", MarkerKind::Composition, true, false);
        assert!(d.id.contains("compositionStart"));
        assert_eq!(d.class.as_deref(), Some("marker composition class"));
        assert!(!d.shapes[0].path.is_empty());
    }
    #[test]
    fn places_start_and_end_with_stable_ids() {
        let p = marker_placement(
            Some("arrow_point"),
            Some("arrow_open_head"),
            "id",
            "flowchart",
            true,
        )
        .unwrap();
        assert!(p.start.unwrap().def.ends_with("pointStart-margin"));
        assert!(p.end.unwrap().def.ends_with("openArrowEnd-margin"));
    }
}
