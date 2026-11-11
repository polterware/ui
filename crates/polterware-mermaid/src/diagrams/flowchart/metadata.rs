//! The `@{ ... }` metadata attached to nodes, edges, and subgraphs.
//!
//! Upstream feeds the lexed `SHAPE_DATA` text to js-yaml with `JSON_SCHEMA`, then removes the
//! keys only a pool/lane diagram may set (`flowDb.ts` `INTERNAL_METADATA_KEYS`), and validates
//! `shape` against the shape registry. The YAML reader is [`crate::common::yaml`]; this module
//! holds the shape names and re-exports the plain-text sanitizer the labels go through
//! (`common.ts` `sanitizeText` with `htmlLabels` on).

use serde_json::{Map, Value};

use crate::common::yaml;

/// Keys set by a diagram that models pools and lanes itself, never by the author of a `@{ }`.
const INTERNAL_METADATA_KEYS: [&str; 3] = ["laneRole", "laneIndex", "drawnExtent"];

/// Parses the metadata text the lexer assembled from a `@{ ... }` block.
///
/// Upstream wraps text without a newline in `{ ... }` and parses everything else as a block
/// mapping. The same split happens here so both spellings of the same block agree.
pub fn parse(text: &str) -> Result<Value, String> {
    if text.contains('\n') {
        yaml::parse_document(&format!("{text}\n"))
    } else {
        yaml::parse_flow_mapping(text)
    }
}

/// Drops the keys a `@{ }` block may not set on a node or subgraph.
pub fn without_internal_keys(doc: Value) -> Value {
    match doc {
        Value::Object(mut map) => {
            for key in INTERNAL_METADATA_KEYS {
                map.remove(key);
            }
            Value::Object(map)
        }
        other => other,
    }
}

/// Returns the object a parsed `@{ }` block holds, or `None` when it is not a mapping.
pub fn as_map(doc: &Value) -> Option<&Map<String, Value>> {
    doc.as_object()
}

/// The shape names upstream accepts in `@{ shape: ... }` (the shape registry's short names,
/// aliases, and internal aliases).
pub fn is_valid_shape(shape: &str) -> bool {
    SHAPES.contains(&shape)
}

/// The names and aliases of `rendering-util/rendering-elements/shapes.ts`.
const SHAPES: &[&str] = &[
    "bang",
    "bolt",
    "bow-rect",
    "bow-tie-rectangle",
    "brace",
    "brace-l",
    "brace-r",
    "braces",
    "browser",
    "bucket",
    "card",
    "circ",
    "circle",
    "cloud",
    "collate",
    "com-link",
    "comment",
    "console",
    "cross-circ",
    "crossed-circle",
    "curv-trap",
    "curved-trapezoid",
    "cyl",
    "cylinder",
    "das",
    "data-store",
    "database",
    "datastore",
    "db",
    "dbl-circ",
    "decision",
    "delay",
    "diam",
    "diamond",
    "directory",
    "disk",
    "display",
    "div-proc",
    "div-rect",
    "divided-process",
    "divided-rectangle",
    "doc",
    "docs",
    "document",
    "documents",
    "double-circle",
    "doublecircle",
    "event",
    "extract",
    "f-circ",
    "filled-circle",
    "flag",
    "flip-tri",
    "flipped-triangle",
    "folder",
    "fork",
    "forkJoin",
    "fr-circ",
    "fr-rect",
    "framed-circle",
    "framed-rectangle",
    "h-cyl",
    "half-rounded-rectangle",
    "hex",
    "hexagon",
    "horizontal-cylinder",
    "hourglass",
    "in-out",
    "internal-storage",
    "inv-trapezoid",
    "inv_trapezoid",
    "join",
    "junction",
    "lean-l",
    "lean-left",
    "lean-r",
    "lean-right",
    "lean_left",
    "lean_right",
    "lightning-bolt",
    "lin-cyl",
    "lin-doc",
    "lin-proc",
    "lin-rect",
    "lined-cylinder",
    "lined-document",
    "lined-process",
    "lined-rectangle",
    "loop-limit",
    "manual",
    "manual-file",
    "manual-input",
    "notch-pent",
    "notch-rect",
    "notched-pentagon",
    "notched-rectangle",
    "odd",
    "out-in",
    "paper-tape",
    "person",
    "pill",
    "prepare",
    "priority",
    "proc",
    "process",
    "processes",
    "procs",
    "question",
    "rect",
    "rect_left_inv_arrow",
    "rectangle",
    "rounded",
    "roundedRect",
    "shaded-process",
    "sl-rect",
    "sloped-rectangle",
    "sm-circ",
    "small-circle",
    "squareRect",
    "st-doc",
    "st-rect",
    "stacked-document",
    "stacked-rectangle",
    "stadium",
    "start",
    "stateEnd",
    "stateStart",
    "stop",
    "stored-data",
    "subproc",
    "subprocess",
    "subroutine",
    "summary",
    "tag-doc",
    "tag-proc",
    "tag-rect",
    "tagged-document",
    "tagged-process",
    "tagged-rectangle",
    "terminal",
    "text",
    "trap-b",
    "trap-t",
    "trapezoid",
    "trapezoid-bottom",
    "trapezoid-top",
    "tri",
    "triangle",
    "win-pane",
    "window-pane",
];

/// `common.ts` `sanitizeText` at the default security level: DOMPurify drops `script` and
/// `style` elements, escapes a `<` that opens no tag, and keeps the rest of the markup. The
/// htmlLabels path, which is the one these labels take, keeps a break tag as written.
pub use crate::common::sanitize::sanitize_html_label as sanitize_text;
