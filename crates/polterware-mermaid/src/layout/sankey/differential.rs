//! Differential test against `d3-sankey` 0.12.3, the layout Mermaid 12.3.0's
//! sankey renderer calls.
//!
//! Every fixture under `testdata/` holds the input graph, the generator options,
//! and the exact result d3-sankey produced for it: node `x0`/`x1`/`y0`/`y1`,
//! `value`, `depth`, `height`, and `layer`, link `y0`/`y1` and `width`, and the
//! `sankeyLinkHorizontal` path. The inputs cover a chain in all four alignments, a
//! fan-out, a fan-in, a diamond, a `fixedValue` node, a single node, and Mermaid's
//! own sankey example data (`energy.csv`).
//!
//! JSON has no NaN, so `JSON.stringify` writes it as `null`; a `null` coordinate
//! means the port must produce a NaN there too, as upstream does for a graph with a
//! single column. Coordinates are compared within `1e-6`; paths are compared
//! exactly, because they are the strings d3-path builds.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer};

use super::{Sankey, SankeyAlignment, SankeyLinkInput, SankeyNodeInput};

const TOLERANCE: f64 = 1e-6;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    nodes: Vec<NodeSpec>,
    links: Vec<LinkSpec>,
    #[serde(rename = "nodeWidth")]
    node_width: f64,
    #[serde(rename = "nodePadding")]
    node_padding: f64,
    extent: [[f64; 2]; 2],
    alignment: String,
    iterations: usize,
    output: Output,
}

#[derive(Deserialize)]
struct NodeSpec {
    id: String,
    #[serde(rename = "fixedValue", default)]
    fixed_value: Option<f64>,
}

#[derive(Deserialize)]
struct LinkSpec {
    source: String,
    target: String,
    #[serde(deserialize_with = "number")]
    value: f64,
}

#[derive(Deserialize)]
struct Output {
    nodes: Vec<OutputNode>,
    links: Vec<OutputLink>,
}

#[derive(Deserialize)]
struct OutputNode {
    id: String,
    index: usize,
    depth: usize,
    height: usize,
    layer: usize,
    #[serde(deserialize_with = "number")]
    value: f64,
    #[serde(deserialize_with = "number")]
    x0: f64,
    #[serde(deserialize_with = "number")]
    x1: f64,
    #[serde(deserialize_with = "number")]
    y0: f64,
    #[serde(deserialize_with = "number")]
    y1: f64,
}

#[derive(Deserialize)]
struct OutputLink {
    index: usize,
    source: usize,
    target: usize,
    #[serde(deserialize_with = "number")]
    value: f64,
    #[serde(deserialize_with = "number")]
    width: f64,
    #[serde(deserialize_with = "number")]
    y0: f64,
    #[serde(deserialize_with = "number")]
    y1: f64,
    path: String,
}

/// Reads a number, turning an explicit `null` (a NaN in JSON, which has no NaN)
/// into a NaN.
fn number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::NAN))
}

fn testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("layout")
        .join("sankey")
        .join("testdata")
}

fn fixtures() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(testdata())
        .expect("the testdata directory is readable")
        .filter_map(|entry| {
            let path = entry.expect("a readable directory entry").path();
            (path.extension()? == "json").then(|| {
                path.file_stem()
                    .expect("a file stem")
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .collect();
    names.sort();
    names
}

fn close(actual: f64, expected: f64, context: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{context}: expected NaN, got {actual}");
        return;
    }
    assert!(
        (actual - expected).abs() <= TOLERANCE,
        "{context}: expected {expected}, got {actual}"
    );
}

#[test]
fn matches_d3_sankey_layout() {
    let names = fixtures();
    assert!(names.len() >= 5, "the fixtures cover at least 5 graphs");
    for name in names {
        let text = fs::read_to_string(testdata().join(format!("{name}.json")))
            .expect("the fixture is readable");
        let fixture: Fixture = serde_json::from_str(&text).expect("the fixture parses");
        let context = fixture.name.clone();
        assert_eq!(context, name, "the fixture name matches its file");

        let alignment = SankeyAlignment::from_name(&fixture.alignment)
            .unwrap_or_else(|| panic!("{context}: unknown alignment {}", fixture.alignment));
        let graph = Sankey {
            extent: fixture.extent,
            node_width: fixture.node_width,
            node_padding: fixture.node_padding,
            alignment,
            iterations: fixture.iterations,
        }
        .layout(
            &fixture
                .nodes
                .iter()
                .map(|node| SankeyNodeInput {
                    id: node.id.clone(),
                    fixed_value: node.fixed_value,
                })
                .collect::<Vec<_>>(),
            &fixture
                .links
                .iter()
                .map(|link| SankeyLinkInput {
                    source: link.source.clone(),
                    target: link.target.clone(),
                    value: link.value,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|error| panic!("{context}: layout failed: {error}"));

        assert_eq!(
            graph.nodes.len(),
            fixture.output.nodes.len(),
            "{context}: nodes"
        );
        assert_eq!(
            graph.links.len(),
            fixture.output.links.len(),
            "{context}: links"
        );

        for expected in &fixture.output.nodes {
            let node = &graph.nodes[expected.index];
            assert_eq!(node.id, expected.id, "{context}: node index");
            assert_eq!(
                node.depth, expected.depth,
                "{context}: {} depth",
                expected.id
            );
            assert_eq!(
                node.height, expected.height,
                "{context}: {} height",
                expected.id
            );
            assert_eq!(
                node.layer, expected.layer,
                "{context}: {} layer",
                expected.id
            );
            close(
                node.value,
                expected.value,
                &format!("{context}: {} value", expected.id),
            );
            close(
                node.x0,
                expected.x0,
                &format!("{context}: {} x0", expected.id),
            );
            close(
                node.x1,
                expected.x1,
                &format!("{context}: {} x1", expected.id),
            );
            close(
                node.y0,
                expected.y0,
                &format!("{context}: {} y0", expected.id),
            );
            close(
                node.y1,
                expected.y1,
                &format!("{context}: {} y1", expected.id),
            );
        }

        for expected in &fixture.output.links {
            let link = &graph.links[expected.index];
            assert_eq!(link.source, expected.source, "{context}: link source");
            assert_eq!(link.target, expected.target, "{context}: link target");
            close(
                link.value,
                expected.value,
                &format!("{context}: link {} value", expected.index),
            );
            close(
                link.width,
                expected.width,
                &format!("{context}: link {} width", expected.index),
            );
            close(
                link.y0,
                expected.y0,
                &format!("{context}: link {} y0", expected.index),
            );
            close(
                link.y1,
                expected.y1,
                &format!("{context}: link {} y1", expected.index),
            );
            assert_eq!(
                graph.link_path(link),
                expected.path,
                "{context}: link {} path",
                expected.index
            );
        }
    }
}

#[test]
fn reads_alignments_by_name() {
    assert_eq!(
        SankeyAlignment::from_name("left"),
        Some(SankeyAlignment::Left)
    );
    assert_eq!(
        SankeyAlignment::from_name("right"),
        Some(SankeyAlignment::Right)
    );
    assert_eq!(
        SankeyAlignment::from_name("center"),
        Some(SankeyAlignment::Center)
    );
    assert_eq!(
        SankeyAlignment::from_name("justify"),
        Some(SankeyAlignment::Justify)
    );
    assert_eq!(SankeyAlignment::from_name("nowhere"), None);
}
