//! Differential test against `dagre-d3-es` 7.0.14, the layout library Mermaid
//! 12.3.0 depends on.
//!
//! Every fixture under `testdata/` holds the input graph and the exact coordinates
//! dagre-d3-es produced for it. The inputs cover chains, cycles with both
//! acyclicers, all four `rankdir`s (including lower case), compound and nested
//! clusters, clusters that carry a `rankdir` of their own (which dagre-d3-es
//! ignores), edge labels in every `labelpos` with offsets, multi-edges, self loops,
//! margins, alignments, zero-sized nodes, rankers, and spacing. Coordinates are
//! compared within 1e-6.
//!
//! JSON has no NaN, so a non-finite coordinate is written as `null`, as
//! `JSON.stringify` does; the test requires the port to produce a NaN there too.

use std::fs;

use serde::{Deserialize, Deserializer};

use super::super::graphlib::Edge;
use super::layout::run_layout;
use super::{
    EdgeLabel, GraphLabel, LayeredGraph, NodeLabel, build_layout_graph, update_input_graph,
};

const TOLERANCE: f64 = 1e-6;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    #[serde(default)]
    graph: GraphSpec,
    #[serde(default)]
    nodes: Vec<NodeSpec>,
    #[serde(default)]
    edges: Vec<EdgeSpec>,
    #[serde(default)]
    parents: Vec<ParentSpec>,
    output: Output,
}

#[derive(Deserialize, Default)]
struct GraphSpec {
    rankdir: Option<String>,
    align: Option<String>,
    nodesep: Option<f64>,
    edgesep: Option<f64>,
    ranksep: Option<f64>,
    marginx: Option<f64>,
    marginy: Option<f64>,
    ranker: Option<String>,
    acyclicer: Option<String>,
}

#[derive(Deserialize)]
struct NodeSpec {
    id: String,
    width: Option<f64>,
    height: Option<f64>,
    rank: Option<f64>,
    rankdir: Option<String>,
}

#[derive(Deserialize)]
struct EdgeSpec {
    v: String,
    w: String,
    name: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    minlen: Option<f64>,
    weight: Option<f64>,
    labelpos: Option<String>,
    labeloffset: Option<f64>,
}

#[derive(Deserialize)]
struct ParentSpec {
    v: String,
    parent: String,
}

#[derive(Deserialize)]
struct Output {
    graph: OutputGraph,
    nodes: Vec<OutputNode>,
    edges: Vec<OutputEdge>,
}

#[derive(Deserialize)]
struct OutputGraph {
    width: Option<f64>,
    height: Option<f64>,
}

#[derive(Deserialize)]
struct OutputNode {
    id: String,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    rank: Option<f64>,
    order: Option<usize>,
}

#[derive(Deserialize)]
struct OutputEdge {
    v: String,
    w: String,
    name: Option<String>,
    points: Option<Vec<PointJson>>,
    #[serde(default, deserialize_with = "coordinate")]
    x: Option<Option<f64>>,
    #[serde(default, deserialize_with = "coordinate")]
    y: Option<Option<f64>>,
}

#[derive(Deserialize)]
struct PointJson {
    /// The coordinate, or `null` for a NaN.
    #[serde(default, deserialize_with = "coordinate")]
    x: Option<Option<f64>>,
    /// The coordinate, or `null` for a NaN.
    #[serde(default, deserialize_with = "coordinate")]
    y: Option<Option<f64>>,
}

/// Reads a coordinate, keeping an explicit `null` (a NaN) apart from a missing key.
fn coordinate<'de, D>(deserializer: D) -> Result<Option<Option<f64>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::<f64>::deserialize(deserializer)?))
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/layout/dagre/testdata")
        .join(name)
}

fn fixtures() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(fixture_path(""))
        .expect("the testdata directory exists")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.ends_with(".json").then_some(name)
        })
        .collect();
    names.sort();
    names
}

fn build(fixture: &Fixture) -> LayeredGraph {
    let mut graph: LayeredGraph =
        LayeredGraph::with_options(super::super::graphlib::GraphOptions {
            multigraph: true,
            compound: true,
            ..Default::default()
        });
    graph.set_graph(GraphLabel {
        rankdir: fixture.graph.rankdir.clone(),
        align: fixture.graph.align.clone(),
        nodesep: fixture.graph.nodesep,
        edgesep: fixture.graph.edgesep,
        ranksep: fixture.graph.ranksep,
        marginx: fixture.graph.marginx,
        marginy: fixture.graph.marginy,
        ranker: fixture.graph.ranker.clone(),
        acyclicer: fixture.graph.acyclicer.clone(),
        ..GraphLabel::default()
    });
    for node in &fixture.nodes {
        graph.set_node_with(
            node.id.clone(),
            NodeLabel {
                width: node.width.unwrap_or(0.0),
                height: node.height.unwrap_or(0.0),
                rank: node.rank,
                rankdir: node.rankdir.clone(),
                ..NodeLabel::default()
            },
        );
    }
    for parent in &fixture.parents {
        graph
            .set_parent(&parent.v, Some(&parent.parent))
            .expect("the fixture is a compound graph");
    }
    for edge in &fixture.edges {
        graph
            .set_edge_full(
                &edge.v,
                &edge.w,
                Some(EdgeLabel {
                    width: edge.width,
                    height: edge.height,
                    minlen: edge.minlen,
                    weight: edge.weight,
                    labelpos: edge.labelpos.clone(),
                    labeloffset: edge.labeloffset,
                    ..EdgeLabel::default()
                }),
                edge.name.as_deref(),
            )
            .expect("the fixture is a multigraph");
    }
    graph
}

fn close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= TOLERANCE,
        "{context}: expected {expected}, got {actual}"
    );
}

/// Compares a coordinate: a missing key is not compared, an explicit `null` (a NaN
/// in JSON, which has no NaN) requires a NaN.
fn close_optional(actual: Option<f64>, expected: Option<Option<f64>>, context: &str) {
    match expected {
        None => {}
        Some(Some(expected)) => close(actual.unwrap_or(f64::NAN), expected, context),
        Some(None) => assert!(
            actual.is_some_and(f64::is_nan),
            "{context}: expected NaN, got {actual:?}"
        ),
    }
}

#[test]
fn matches_dagre_d3_es_coordinates() {
    let names = fixtures();
    assert!(names.len() >= 50, "the fixtures cover at least 50 graphs");
    for name in names {
        let text = fs::read_to_string(fixture_path(&name)).expect("the fixture is readable");
        let fixture: Fixture = serde_json::from_str(&text).expect("the fixture parses");
        let context = fixture.name.clone();

        let mut graph = build(&fixture);
        let mut layout_graph = build_layout_graph(&graph);
        run_layout(&mut layout_graph)
            .unwrap_or_else(|error| panic!("{context}: layout failed: {error}"));
        update_input_graph(&mut graph, &layout_graph);

        assert_eq!(
            graph.node_count(),
            fixture.output.nodes.len(),
            "{context}: node count"
        );
        assert_eq!(
            graph.edge_count(),
            fixture.output.edges.len(),
            "{context}: edge count"
        );

        for expected in &fixture.output.nodes {
            let node = graph
                .node(&expected.id)
                .unwrap_or_else(|| panic!("{context}: missing node {}", expected.id));
            if let Some(x) = expected.x {
                close(
                    node.x.unwrap_or(f64::NAN),
                    x,
                    &format!("{context}: node {} x", expected.id),
                );
            }
            if let Some(y) = expected.y {
                close(
                    node.y.unwrap_or(f64::NAN),
                    y,
                    &format!("{context}: node {} y", expected.id),
                );
            }
            // Upstream only overwrites a cluster's width and height.
            if !graph.children(&expected.id).is_empty() {
                if let Some(width) = expected.width {
                    close(
                        node.width,
                        width,
                        &format!("{context}: node {} width", expected.id),
                    );
                }
                if let Some(height) = expected.height {
                    close(
                        node.height,
                        height,
                        &format!("{context}: node {} height", expected.id),
                    );
                }
            }
            // dagre-d3-es does not copy these back; the layout graph keeps them.
            let layout_node = layout_graph
                .node(&expected.id)
                .unwrap_or_else(|| panic!("{context}: missing layout node {}", expected.id));
            if let Some(rank) = expected.rank {
                close(
                    layout_node.rank.unwrap_or(f64::NAN),
                    rank,
                    &format!("{context}: node {} rank", expected.id),
                );
            }
            if let Some(order) = expected.order {
                assert_eq!(
                    layout_node.order,
                    Some(order),
                    "{context}: node {} order",
                    expected.id
                );
            }
        }

        for expected in &fixture.output.edges {
            let edge = Edge {
                v: expected.v.clone(),
                w: expected.w.clone(),
                name: expected.name.clone(),
            };
            let label = graph
                .edge_object(&edge)
                .unwrap_or_else(|| panic!("{context}: missing edge {edge:?}"));
            match &expected.points {
                Some(points) => {
                    let actual = label
                        .points
                        .as_ref()
                        .unwrap_or_else(|| panic!("{context}: edge {edge:?} has no points"));
                    assert_eq!(
                        actual.len(),
                        points.len(),
                        "{context}: edge {edge:?} point count"
                    );
                    for (index, (actual, expected)) in actual.iter().zip(points).enumerate() {
                        close_optional(
                            Some(actual.x),
                            expected.x,
                            &format!("{context}: edge {edge:?} point {index} x"),
                        );
                        close_optional(
                            Some(actual.y),
                            expected.y,
                            &format!("{context}: edge {edge:?} point {index} y"),
                        );
                    }
                }
                None => assert!(
                    label.points.is_none(),
                    "{context}: edge {edge:?} unexpectedly has points"
                ),
            }
            close_optional(label.x, expected.x, &format!("{context}: edge {edge:?} x"));
            close_optional(label.y, expected.y, &format!("{context}: edge {edge:?} y"));
        }

        let graph_label = graph.graph().expect("the layout graph keeps a label");
        close_optional(
            graph_label.width,
            fixture.output.graph.width.map(Some),
            &format!("{context}: graph width"),
        );
        close_optional(
            graph_label.height,
            fixture.output.graph.height.map(Some),
            &format!("{context}: graph height"),
        );
    }
}
