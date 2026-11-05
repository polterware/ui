//! The canvas's drawing library and component silhouettes (`shapes.ts`, `components.ts`,
//! `outlines.ts`): every outline is path data in a w×h box at the origin, where open
//! subpaths are stroke-only details.
//!
//! [`SHAPES`] mirrors `canvas.ShapeKinds` in the Polterware core, in palette order, and
//! [`FAMILIES`] are the silhouettes a component is drawn in: something that stores is a
//! cylinder whatever its type, a client a window with three dots, the edge a hexagon, and
//! what is outside a dashed card.

/// A rectangle filling the box.
pub fn rect(w: f32, h: f32) -> String {
    format!("M0 0H{w}V{h}H0Z")
}

/// An ellipse filling the box.
pub fn ellipse(w: f32, h: f32) -> String {
    format!(
        "M0 {cy}A{rx} {cy} 0 1 0 {w} {cy}A{rx} {cy} 0 1 0 0 {cy}Z",
        cy = h / 2.0,
        rx = w / 2.0
    )
}

/// A diamond touching the middle of each side.
pub fn diamond(w: f32, h: f32) -> String {
    format!(
        "M{cx} 0L{w} {cy}L{cx} {h}L0 {cy}Z",
        cx = w / 2.0,
        cy = h / 2.0
    )
}

/// The rounded rectangle at 12.
pub fn rounded(w: f32, h: f32) -> String {
    rounded_by(12.0, w, h)
}

/// A rounded rectangle at (x, y) with corners of at most `radius`.
pub fn rounded_at(x: f32, y: f32, w: f32, h: f32, radius: f32) -> String {
    let r = radius.min(w / 4.0).min(h / 4.0);
    format!(
        "M{a} {y}H{b}A{r} {r} 0 0 1 {right} {c}V{d}A{r} {r} 0 0 1 {b} {bottom}H{a}A{r} {r} 0 0 1 {x} {d}V{c}A{r} {r} 0 0 1 {a} {y}Z",
        a = x + r,
        b = x + w - r,
        c = y + r,
        d = y + h - r,
        right = x + w,
        bottom = y + h,
    )
}

/// A rounded rectangle filling the box.
pub fn rounded_by(radius: f32, w: f32, h: f32) -> String {
    rounded_at(0.0, 0.0, w, h, radius)
}

/// A pill: half circles at both ends.
pub fn stadium(w: f32, h: f32) -> String {
    let r = (h / 2.0).min(w / 2.0);
    format!(
        "M{r} 0H{a}A{r} {r} 0 0 1 {a} {h}H{r}A{r} {r} 0 0 1 {r} 0Z",
        a = w - r
    )
}

/// A circle about (cx, cy).
pub fn circle(cx: f32, cy: f32, r: f32) -> String {
    format!(
        "M{a} {cy}A{r} {r} 0 1 0 {b} {cy}A{r} {r} 0 1 0 {a} {cy}Z",
        a = cx - r,
        b = cx + r
    )
}

/// Which library group a drawing shape sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeGroup {
    General,
    Flowchart,
}

/// A drawing shape: no meaning, just an outline with a label.
#[derive(Clone, Copy, Debug)]
pub struct ShapeDefinition {
    pub kind: &'static str,
    pub label: &'static str,
    pub group: ShapeGroup,
    /// The outline in a w×h box at the origin; open subpaths are stroke-only details.
    pub path: fn(f32, f32) -> String,
    /// The size a new one starts at, when not the 180×100 default.
    pub geometry: Option<(f32, f32)>,
    /// The label sits under the drawing (an actor).
    pub label_below: bool,
    /// The label reads from the top (a frame).
    pub label_top: bool,
    /// A line drawing: filling would close its open strokes into solid areas.
    pub stroke_only: bool,
}

const fn shape(
    kind: &'static str,
    label: &'static str,
    group: ShapeGroup,
    path: fn(f32, f32) -> String,
) -> ShapeDefinition {
    ShapeDefinition {
        kind,
        label,
        group,
        path,
        geometry: None,
        label_below: false,
        label_top: false,
        stroke_only: false,
    }
}

const fn sized(definition: ShapeDefinition, width: f32, height: f32) -> ShapeDefinition {
    ShapeDefinition {
        geometry: Some((width, height)),
        ..definition
    }
}

fn triangle(w: f32, h: f32) -> String {
    format!("M{} 0L{w} {h}H0Z", w / 2.0)
}
fn hexagon(w: f32, h: f32) -> String {
    let s = w * 0.2;
    format!(
        "M{s} 0H{a}L{w} {cy}L{a} {h}H{s}L0 {cy}Z",
        a = w - s,
        cy = h / 2.0
    )
}
fn parallelogram(w: f32, h: f32) -> String {
    let s = w * 0.2;
    format!("M{s} 0H{w}L{} {h}H0Z", w - s)
}
fn trapezoid(w: f32, h: f32) -> String {
    let s = w * 0.2;
    format!("M{s} 0H{}L{w} {h}H0Z", w - s)
}
fn cylinder(w: f32, h: f32) -> String {
    let ry = (h * 0.15).min(16.0);
    format!(
        "M0 {ry}A{rx} {ry} 0 0 1 {w} {ry}V{b}A{rx} {ry} 0 0 1 0 {b}ZM0 {ry}A{rx} {ry} 0 0 0 {w} {ry}",
        rx = w / 2.0,
        b = h - ry
    )
}
fn cloud(w: f32, h: f32) -> String {
    format!(
        "M{} {}A{} {} 0 0 1 {} {}A{} {} 0 0 1 {} {}A{} {} 0 0 1 {} {}A{} {} 0 0 1 {} {}Z",
        w * 0.25,
        h * 0.85,
        w * 0.2,
        h * 0.25,
        w * 0.18,
        h * 0.38,
        w * 0.25,
        h * 0.3,
        w * 0.55,
        h * 0.18,
        w * 0.22,
        h * 0.25,
        w * 0.86,
        h * 0.42,
        w * 0.18,
        h * 0.22,
        w * 0.78,
        h * 0.85
    )
}
fn document(w: f32, h: f32) -> String {
    format!(
        "M0 0H{w}V{y}Q{qx} {qy} {cx} {y}T0 {y}Z",
        y = h * 0.85,
        qx = w * 0.75,
        qy = h * 0.7,
        cx = w / 2.0
    )
}
fn actor(w: f32, h: f32) -> String {
    let r = w.min(h) * 0.18;
    let cx = w / 2.0;
    format!(
        "M{a} {r}A{r} {r} 0 1 0 {b} {r}A{r} {r} 0 1 0 {a} {r}ZM{cx} {d}V{hip}M0 {arms}H{w}M0 {h}L{cx} {hip}L{w} {h}",
        a = cx - r,
        b = cx + r,
        d = 2.0 * r,
        hip = h * 0.65,
        arms = h * 0.4
    )
}
fn callout(w: f32, h: f32) -> String {
    format!(
        "M0 0H{w}V{y}H{a}L{b} {h}L{c} {y}H0Z",
        y = h * 0.75,
        a = w * 0.45,
        b = w * 0.25,
        c = w * 0.3
    )
}
fn predefined(w: f32, h: f32) -> String {
    format!(
        "{}M{a} 0V{h}M{b} 0V{h}",
        rect(w, h),
        a = w * 0.1,
        b = w * 0.9
    )
}
fn manual_input(w: f32, h: f32) -> String {
    format!("M0 {}L{w} 0V{h}H0Z", h * 0.25)
}
fn delay(w: f32, h: f32) -> String {
    format!(
        "M0 0H{a}A{r} {cy} 0 0 1 {a} {h}H0Z",
        a = w * 0.6,
        r = w * 0.4,
        cy = h / 2.0
    )
}
fn display(w: f32, h: f32) -> String {
    format!(
        "M{a} 0H{b}A{r} {cy} 0 0 1 {b} {h}H{a}L0 {cy}Z",
        a = w * 0.15,
        b = w * 0.8,
        r = w * 0.2,
        cy = h / 2.0
    )
}

/// The drawing library, in palette order.
pub const SHAPES: [ShapeDefinition; 22] = [
    shape("rectangle", "Rectangle", ShapeGroup::General, rect),
    shape("rounded", "Rounded rectangle", ShapeGroup::General, rounded),
    sized(
        shape("square", "Square", ShapeGroup::General, rect),
        120.0,
        120.0,
    ),
    shape("ellipse", "Ellipse", ShapeGroup::General, ellipse),
    sized(
        shape("circle", "Circle", ShapeGroup::General, ellipse),
        120.0,
        120.0,
    ),
    sized(
        shape("diamond", "Diamond", ShapeGroup::General, diamond),
        160.0,
        120.0,
    ),
    sized(
        shape("triangle", "Triangle", ShapeGroup::General, triangle),
        140.0,
        120.0,
    ),
    shape("hexagon", "Hexagon", ShapeGroup::General, hexagon),
    shape(
        "parallelogram",
        "Parallelogram",
        ShapeGroup::General,
        parallelogram,
    ),
    shape("trapezoid", "Trapezoid", ShapeGroup::General, trapezoid),
    sized(
        shape("cylinder", "Cylinder", ShapeGroup::General, cylinder),
        120.0,
        140.0,
    ),
    shape("cloud", "Cloud", ShapeGroup::General, cloud),
    shape("document", "Document", ShapeGroup::General, document),
    ShapeDefinition {
        label_below: true,
        stroke_only: true,
        ..sized(
            shape("actor", "Actor", ShapeGroup::General, actor),
            60.0,
            110.0,
        )
    },
    shape("callout", "Callout", ShapeGroup::General, callout),
    ShapeDefinition {
        label_top: true,
        ..sized(
            shape("frame", "Frame", ShapeGroup::General, rounded),
            440.0,
            300.0,
        )
    },
    shape("process", "Process", ShapeGroup::Flowchart, rect),
    sized(
        shape("terminator", "Terminator", ShapeGroup::Flowchart, stadium),
        180.0,
        72.0,
    ),
    shape(
        "predefined",
        "Predefined process",
        ShapeGroup::Flowchart,
        predefined,
    ),
    shape(
        "manual-input",
        "Manual input",
        ShapeGroup::Flowchart,
        manual_input,
    ),
    shape("delay", "Delay", ShapeGroup::Flowchart, delay),
    shape("display", "Display", ShapeGroup::Flowchart, display),
];

/// The drawing shape of `kind`.
pub fn shape_of(kind: &str) -> Option<&'static ShapeDefinition> {
    SHAPES.iter().find(|shape| shape.kind == kind)
}

/// Where a semantic card's content sits clear of its outline's details.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inset {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// A component family: what a component is, drawn as its silhouette.
#[derive(Clone, Copy, Debug)]
pub struct Family {
    pub id: &'static str,
    pub label: &'static str,
    /// The type a new component of this family starts as.
    pub kind: &'static str,
    pub geometry: (f32, f32),
    /// The outline in a w×h box at the origin; open subpaths are interior details.
    pub path: fn(f32, f32) -> String,
    /// Where the content sits clear of the outline's details.
    pub inset: fn(f32, f32) -> Inset,
    /// Things outside the project are drawn with a dashed frame.
    pub dashed: bool,
    /// People get a round icon tile, like an avatar.
    pub round: bool,
    /// Small solid marks painted in the line color, such as a window's three dots.
    pub marks: Option<fn(f32, f32) -> String>,
}

fn none(_: f32, _: f32) -> Inset {
    Inset::default()
}
fn card(w: f32, h: f32) -> String {
    rounded_by(6.0, w, h)
}
fn bar(h: f32) -> f32 {
    13.0_f32.min(h / 5.0)
}
fn cap(h: f32) -> f32 {
    14.0_f32.min(h * 0.14)
}
fn end(w: f32) -> f32 {
    8.0_f32.min(w / 14.0)
}
fn side(w: f32, h: f32) -> f32 {
    (w / 5.0).min(h * 0.4)
}
fn pin(h: f32) -> f32 {
    6.0_f32.min(h / 10.0)
}
fn point_(h: f32) -> f32 {
    22.0_f32.min(h * 0.28)
}
fn notch(h: f32) -> f32 {
    18.0_f32.min(h * 0.35)
}
fn wave(h: f32) -> f32 {
    10.0_f32.min(h * 0.12)
}

fn client(w: f32, h: f32) -> String {
    // A window: square corners, a title bar and its three dots.
    format!("{}M0 {b}H{w}", rounded_by(3.0, w, h), b = bar(h))
}
fn client_inset(_: f32, h: f32) -> Inset {
    Inset {
        top: bar(h),
        ..Inset::default()
    }
}
fn client_marks(_: f32, h: f32) -> String {
    (0..3)
        .map(|i| {
            circle(
                bar(h) * (0.65 + i as f32 * 0.5),
                bar(h) / 2.0,
                bar(h) * 0.13,
            )
        })
        .collect()
}
fn workflow(w: f32, h: f32) -> String {
    // A chevron: work that runs in steps or on a trigger.
    let n = notch(h);
    format!(
        "M0 0H{a}L{w} {cy}L{a} {h}H0L{n} {cy}Z",
        a = w - n,
        cy = h / 2.0
    )
}
fn workflow_inset(_: f32, h: f32) -> Inset {
    Inset {
        left: notch(h),
        right: notch(h),
        ..Inset::default()
    }
}
fn storage(w: f32, h: f32) -> String {
    let r = cap(h);
    format!(
        "M0 {r}V{b}A{rx} {r} 0 0 0 {w} {b}V{r}A{rx} {r} 0 0 0 0 {r}ZM0 {r}A{rx} {r} 0 0 0 {w} {r}",
        b = h - r,
        rx = w / 2.0
    )
}
fn storage_inset(_: f32, h: f32) -> Inset {
    Inset {
        top: cap(h) * 2.0,
        bottom: cap(h) / 2.0,
        ..Inset::default()
    }
}
fn messaging(w: f32, h: f32) -> String {
    let r = end(w);
    format!(
        "M{r} 0H{a}A{r} {cy} 0 0 1 {a} {h}H{r}A{r} {cy} 0 0 1 {r} 0ZM{a} 0A{r} {cy} 0 0 0 {a} {h}",
        a = w - r,
        cy = h / 2.0
    )
}
fn messaging_inset(w: f32, _: f32) -> Inset {
    Inset {
        left: end(w),
        right: end(w) * 2.0 + 4.0,
        ..Inset::default()
    }
}
fn ai(w: f32, h: f32) -> String {
    // A chip: the body with pins along the top and bottom edges.
    let p = pin(h);
    let pins: String = [0.2, 0.4, 0.6, 0.8]
        .iter()
        .map(|x| {
            let x = w * x;
            format!("M{x} 0V{p}M{x} {}V{h}", h - p)
        })
        .collect();
    format!("{}{pins}", rounded_at(0.0, p, w, h - p * 2.0, 6.0))
}
fn ai_inset(_: f32, h: f32) -> Inset {
    Inset {
        top: pin(h),
        bottom: pin(h),
        ..Inset::default()
    }
}
fn edge(w: f32, h: f32) -> String {
    let s = side(w, h);
    format!(
        "M{s} 0H{a}L{w} {cy}L{a} {h}H{s}L0 {cy}Z",
        a = w - s,
        cy = h / 2.0
    )
}
fn edge_inset(w: f32, h: f32) -> Inset {
    Inset {
        left: side(w, h) * 0.6,
        right: side(w, h) * 0.6,
        ..Inset::default()
    }
}
fn security(w: f32, h: f32) -> String {
    // A badge: square shoulders and a pointed base, like a shield.
    let r = 6.0;
    let t = point_(h);
    format!(
        "M{r} 0H{a}A{r} {r} 0 0 1 {w} {r}V{b}L{cx} {h}L0 {b}V{r}A{r} {r} 0 0 1 {r} 0Z",
        a = w - r,
        b = h - t,
        cx = w / 2.0
    )
}
fn security_inset(_: f32, h: f32) -> Inset {
    Inset {
        bottom: point_(h),
        ..Inset::default()
    }
}
fn platform(w: f32, h: f32) -> String {
    // Platform holds things: two slots along the bottom half.
    let pad = 10.0;
    let top = h * 0.5;
    let width = (w - pad * 3.0) / 2.0;
    format!(
        "{}{}{}",
        card(w, h),
        rounded_at(pad, top, width, h - top - pad, 4.0),
        rounded_at(pad * 2.0 + width, top, width, h - top - pad, 4.0)
    )
}
fn platform_inset(_: f32, h: f32) -> Inset {
    Inset {
        bottom: h * 0.5,
        ..Inset::default()
    }
}
fn observability(w: f32, h: f32) -> String {
    // A card with a pulse line along the bottom.
    let y = h - 12.0;
    format!(
        "{}M10 {y}H{a}L{b} {c}L{d} {e}L{f} {g}L{i} {y}H{j}",
        card(w, h),
        a = w * 0.36,
        b = w * 0.41,
        c = y - 8.0,
        d = w * 0.47,
        e = y + 6.0,
        f = w * 0.52,
        g = y - 3.0,
        i = w * 0.55,
        j = w - 10.0
    )
}
fn observability_inset(_: f32, _: f32) -> Inset {
    Inset {
        bottom: 20.0,
        ..Inset::default()
    }
}
fn people_inset(_: f32, h: f32) -> Inset {
    Inset {
        left: h * 0.12,
        right: h * 0.2,
        ..Inset::default()
    }
}
fn document_family(w: f32, h: f32) -> String {
    // A page with a wavy bottom edge: written artifacts such as specs, config and data files.
    let r = 6.0;
    let d = wave(h);
    format!(
        "M{r} 0H{a}A{r} {r} 0 0 1 {w} {r}V{b}Q{qx} {qy} {cx} {b}T0 {b}V{r}A{r} {r} 0 0 1 {r} 0Z",
        a = w - r,
        b = h - d,
        qx = w * 0.75,
        qy = h - d * 2.4,
        cx = w / 2.0
    )
}
fn document_inset(_: f32, h: f32) -> Inset {
    Inset {
        bottom: wave(h) * 1.5,
        ..Inset::default()
    }
}

const fn family(
    id: &'static str,
    label: &'static str,
    kind: &'static str,
    geometry: (f32, f32),
    path: fn(f32, f32) -> String,
    inset: fn(f32, f32) -> Inset,
) -> Family {
    Family {
        id,
        label,
        kind,
        geometry,
        path,
        inset,
        dashed: false,
        round: false,
        marks: None,
    }
}

/// The silhouettes, in palette order.
pub const FAMILIES: [Family; 13] = [
    Family {
        marks: Some(client_marks),
        ..family(
            "client",
            "Client",
            "frontend",
            (220.0, 76.0),
            client,
            client_inset,
        )
    },
    family("compute", "Compute", "service", (220.0, 64.0), card, none),
    family(
        "workflow",
        "Workflow",
        "pipeline",
        (240.0, 64.0),
        workflow,
        workflow_inset,
    ),
    family(
        "storage",
        "Storage",
        "database",
        (200.0, 96.0),
        storage,
        storage_inset,
    ),
    family(
        "messaging",
        "Messaging",
        "queue",
        (240.0, 64.0),
        messaging,
        messaging_inset,
    ),
    family("ai", "AI", "llm", (220.0, 76.0), ai, ai_inset),
    family("edge", "Edge", "gateway", (240.0, 64.0), edge, edge_inset),
    family(
        "security",
        "Security",
        "auth",
        (220.0, 84.0),
        security,
        security_inset,
    ),
    family(
        "platform",
        "Platform",
        "cluster",
        (220.0, 112.0),
        platform,
        platform_inset,
    ),
    family(
        "observability",
        "Observability",
        "monitoring",
        (220.0, 80.0),
        observability,
        observability_inset,
    ),
    Family {
        dashed: true,
        ..family(
            "external",
            "External",
            "external",
            (220.0, 64.0),
            card,
            none,
        )
    },
    Family {
        round: true,
        ..family(
            "people",
            "People",
            "user",
            (200.0, 56.0),
            stadium,
            people_inset,
        )
    },
    family(
        "document",
        "Document",
        "spec",
        (200.0, 84.0),
        document_family,
        document_inset,
    ),
];

/// The family of `id`.
pub fn family_of(id: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|family| family.id == id)
}

/// The box a family outline is drawn in at icon size, where the card proportions would
/// flatten it.
pub const GLYPH: (f32, f32) = (40.0, 30.0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::path::{Segment, closed_only, parse};

    #[test]
    fn every_outline_parses_closed() {
        for shape in SHAPES {
            let (w, h) = shape.geometry.unwrap_or((180.0, 100.0));
            let path = parse(&(shape.path)(w, h));
            assert!(
                path.contains(&Segment::Close),
                "{} has no closed outline",
                shape.kind
            );
        }
        for family in FAMILIES {
            let (w, h) = family.geometry;
            assert!(
                !closed_only(&parse(&(family.path)(w, h))).is_empty(),
                "{}",
                family.id
            );
        }
    }

    #[test]
    fn rounded_corners_never_pass_a_quarter_of_the_side() {
        assert_eq!(
            rounded_at(0.0, 0.0, 20.0, 100.0, 12.0),
            "M5 0H15A5 5 0 0 1 20 5V95A5 5 0 0 1 15 100H5A5 5 0 0 1 0 95V5A5 5 0 0 1 5 0Z"
        );
    }
}
