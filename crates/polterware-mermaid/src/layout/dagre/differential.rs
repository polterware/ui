//! Differential test against upstream `@dagrejs/dagre`.
//!
//! Every fixture under `testdata/` holds the input graph and the exact coordinates
//! the upstream build produced (dagre 3.1.3, `dist/dagre.esm.js`). The inputs cover
//! chains, cycles with both acyclicers, all four `rankdir`s (including lower case),
//! compound and nested clusters, clusters with their own `rankdir`, edge labels in
//! every `labelpos` with offsets, multi-edges, self loops, margins, alignments,
//! rankers, and spacing. Coordinates are compared within 1e-6.

use std::fs;

use serde::Deserialize;

use super::super::graphlib::Edge;
use super::{EdgeLabel, GraphLabel, LayeredGraph, NodeLabel, layout};

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
    rankalign: Option<String>,
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
    width: f64,
    height: f64,
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
    x: Option<f64>,
    y: Option<f64>,
}

#[derive(Deserialize)]
struct PointJson {
    x: f64,
    y: f64,
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
        rankalign: fixture.graph.rankalign.clone(),
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

#[test]
fn matches_upstream_dagre_coordinates() {
    let names = fixtures();
    assert!(names.len() >= 30, "the fixtures cover at least 30 graphs");
    for name in names {
        let text = fs::read_to_string(fixture_path(&name)).expect("the fixture is readable");
        let fixture: Fixture = serde_json::from_str(&text).expect("the fixture parses");
        let context = fixture.name.clone();

        let mut graph = build(&fixture);
        layout(&mut graph).unwrap_or_else(|error| panic!("{context}: layout failed: {error}"));

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
            if let Some(rank) = expected.rank {
                close(
                    node.rank.unwrap_or(f64::NAN),
                    rank,
                    &format!("{context}: node {} rank", expected.id),
                );
            }
            if let Some(order) = expected.order {
                assert_eq!(
                    node.order,
                    Some(order),
                    "{context}: node {} order",
                    expected.id
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
                        close(
                            actual.x,
                            expected.x,
                            &format!("{context}: edge {edge:?} point {index} x"),
                        );
                        close(
                            actual.y,
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
            if let Some(x) = expected.x {
                close(
                    label.x.unwrap_or(f64::NAN),
                    x,
                    &format!("{context}: edge {edge:?} x"),
                );
            }
            if let Some(y) = expected.y {
                close(
                    label.y.unwrap_or(f64::NAN),
                    y,
                    &format!("{context}: edge {edge:?} y"),
                );
            }
        }

        let graph_label = graph.graph().expect("the layout graph keeps a label");
        close(
            graph_label.width.unwrap_or(f64::NAN),
            fixture.output.graph.width,
            &format!("{context}: graph width"),
        );
        close(
            graph_label.height.unwrap_or(f64::NAN),
            fixture.output.graph.height,
            &format!("{context}: graph height"),
        );
    }
}
