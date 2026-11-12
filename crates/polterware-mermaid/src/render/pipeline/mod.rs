//! Port of upstream `rendering-util/render.ts`: the layout-algorithm registry
//! and unified layout-and-render pipeline.
//!
//! Submodules port the individual pipeline modules:
//! - [`types`][]: `types.ts`
//! - [`create_graph`][]: `createGraph.ts`
//! - [`clone_layout_data_for_measure`][]: `cloneLayoutDataForMeasure.ts`
//! - [`setup_view_port_for_svg`][]: `setupViewPortForSVG.ts`
//! - [`label_transform`][]: `labelTransform.ts`
//! - [`edge_step`][]: `edgeStep.ts`
//! - [`edge_corner_radius`][]: `edgeCornerRadius.ts`
//! - [`dagre`][]: `layout-algorithms/dagre/index.js`
//! - [`cose_bilkent`][]: `layout-algorithms/cose-bilkent/index.js`
//! - [`fcose`]: the `fcose` layout of cytoscape-fcose, run by the architecture renderer
//! - [`grid`]: the grid layout registered from `layout-algorithms/grid`

pub mod clone_layout_data_for_measure;
pub mod cose_bilkent;
pub mod create_graph;
pub mod dagre;
pub mod edge_corner_radius;
pub mod edge_step;
pub mod fcose;
pub mod grid;
pub mod label_transform;
pub mod setup_view_port_for_svg;
pub mod types;

use std::collections::HashMap;
use std::sync::RwLock;

use crate::measure::TextMeasure;
use crate::render::layout_dagre::LayoutError;
use crate::scene::Scene;

pub use clone_layout_data_for_measure::*;
pub use cose_bilkent::*;
pub use create_graph::*;
pub use dagre::*;
pub use edge_corner_radius::*;
pub use edge_step::*;
pub use fcose::*;
pub use grid::*;
pub use label_transform::*;
pub use setup_view_port_for_svg::*;
pub use types::*;

pub const LAST_RESORT_LAYOUT: &str = "dagre";

#[derive(Clone, Debug)]
pub struct LayoutLoaderDefinition {
    pub name: String,
    pub algorithm: Option<String>,
}

static LAYOUT_ALGORITHMS: RwLock<Option<HashMap<String, LayoutLoaderDefinition>>> =
    RwLock::new(None);

fn get_registry() -> HashMap<String, LayoutLoaderDefinition> {
    let read = LAYOUT_ALGORITHMS.read().unwrap();
    if let Some(map) = read.as_ref() {
        return map.clone();
    }
    drop(read);
    let mut write = LAYOUT_ALGORITHMS.write().unwrap();
    if let Some(map) = write.as_ref() {
        return map.clone();
    }
    let mut map = HashMap::new();
    map.insert(
        "dagre".to_string(),
        LayoutLoaderDefinition {
            name: "dagre".to_string(),
            algorithm: None,
        },
    );
    map.insert(
        "cose-bilkent".to_string(),
        LayoutLoaderDefinition {
            name: "cose-bilkent".to_string(),
            algorithm: None,
        },
    );
    map.insert(
        "fcose".to_string(),
        LayoutLoaderDefinition {
            name: "fcose".to_string(),
            algorithm: None,
        },
    );
    map.insert(
        "grid".to_string(),
        LayoutLoaderDefinition {
            name: "grid".to_string(),
            algorithm: None,
        },
    );
    map.insert(
        "swimlane".to_string(),
        LayoutLoaderDefinition {
            name: "swimlane".to_string(),
            algorithm: None,
        },
    );
    *write = Some(map.clone());
    map
}

pub fn register_layout_loaders(loaders: Vec<LayoutLoaderDefinition>) {
    let mut write = LAYOUT_ALGORITHMS.write().unwrap();
    let map = write.get_or_insert_with(|| {
        let mut m = HashMap::new();
        m.insert(
            "dagre".to_string(),
            LayoutLoaderDefinition {
                name: "dagre".to_string(),
                algorithm: None,
            },
        );
        m.insert(
            "cose-bilkent".to_string(),
            LayoutLoaderDefinition {
                name: "cose-bilkent".to_string(),
                algorithm: None,
            },
        );
        m.insert(
            "fcose".to_string(),
            LayoutLoaderDefinition {
                name: "fcose".to_string(),
                algorithm: None,
            },
        );
        m.insert(
            "grid".to_string(),
            LayoutLoaderDefinition {
                name: "grid".to_string(),
                algorithm: None,
            },
        );
        m.insert(
            "swimlane".to_string(),
            LayoutLoaderDefinition {
                name: "swimlane".to_string(),
                algorithm: None,
            },
        );
        m
    });
    for loader in loaders {
        map.insert(loader.name.clone(), loader);
    }
}

pub fn get_layout_loader_definition(name: &str) -> Result<LayoutLoaderDefinition, LayoutError> {
    let map = get_registry();
    map.get(name)
        .cloned()
        .ok_or_else(|| LayoutError::Dagre(format!("Unknown layout algorithm: {name}")))
}

pub fn register_default_layout_loaders() {
    register_layout_loaders(vec![
        LayoutLoaderDefinition {
            name: "dagre".to_string(),
            algorithm: None,
        },
        LayoutLoaderDefinition {
            name: "cose-bilkent".to_string(),
            algorithm: None,
        },
        LayoutLoaderDefinition {
            name: "fcose".to_string(),
            algorithm: None,
        },
        LayoutLoaderDefinition {
            name: "grid".to_string(),
            algorithm: None,
        },
        LayoutLoaderDefinition {
            name: "swimlane".to_string(),
            algorithm: None,
        },
    ]);
}

pub fn get_registered_layout_algorithm(algorithm: &str, fallback: Option<&str>) -> String {
    let map = get_registry();
    if map.contains_key(algorithm) {
        return algorithm.to_string();
    }
    let fb = fallback.unwrap_or(LAST_RESORT_LAYOUT);
    if map.contains_key(fb) {
        return fb.to_string();
    }
    if map.contains_key(LAST_RESORT_LAYOUT) {
        return LAST_RESORT_LAYOUT.to_string();
    }
    LAST_RESORT_LAYOUT.to_string()
}

/// Unified render pipeline entry point.
///
/// Returns a [`Scene`] containing positioned node shapes, cluster boxes, edge paths with markers,
/// and labels, plus the fitted `viewBox`.
pub fn render(
    data4_layout: &mut LayoutData,
    measure: &dyn TextMeasure,
) -> Result<Scene, LayoutError> {
    // Prefix all node domIds with diagramId to ensure uniqueness
    if let Some(diagram_id) = &data4_layout.diagram_id {
        for node in &mut data4_layout.nodes {
            let original_dom_id = node.dom_id.clone().unwrap_or_else(|| node.id.clone());
            node.dom_id = Some(format!("{diagram_id}-{original_dom_id}"));
        }
    }

    let algo = data4_layout
        .layout_algorithm
        .as_deref()
        .unwrap_or(LAST_RESORT_LAYOUT);
    let resolved_algo = get_registered_layout_algorithm(algo, Some(LAST_RESORT_LAYOUT));

    match resolved_algo.as_str() {
        "cose-bilkent" => cose_bilkent_render(data4_layout, measure),
        "fcose" => fcose_render(data4_layout, measure),
        "grid" => grid_render(data4_layout, measure),
        _ => dagre_render(data4_layout, measure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;

    #[test]
    fn test_pipeline_fixtures() {
        let testdata_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/render/testdata/pipeline");

        let entries = std::fs::read_dir(&testdata_dir).expect("read testdata/pipeline");
        let mut count = 0;

        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let file_content = std::fs::read_to_string(&path).expect("read fixture file");
            let fixture: serde_json::Value =
                serde_json::from_str(&file_content).expect("parse fixture json");

            let mut input: LayoutData =
                serde_json::from_value(fixture["input"].clone()).expect("parse input LayoutData");
            let expected = &fixture["expected"];

            let scene = render(&mut input, &FallbackMeasure).expect("render pipeline");

            // Compare nodes
            let expected_nodes = expected["nodes"].as_array().expect("expected nodes array");
            for exp_node in expected_nodes {
                let id = exp_node["id"].as_str().unwrap();
                let actual_node = input
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .unwrap_or_else(|| panic!("node {id} in actual"));

                let exp_x = exp_node["x"].as_f64().unwrap();
                let exp_y = exp_node["y"].as_f64().unwrap();
                let exp_w = exp_node["width"].as_f64().unwrap();
                let exp_h = exp_node["height"].as_f64().unwrap();

                let act_x = actual_node.x.expect("actual node x");
                let act_y = actual_node.y.expect("actual node y");
                let act_w = actual_node.width.expect("actual node width");
                let act_h = actual_node.height.expect("actual node height");

                assert!(
                    (act_x - exp_x).abs() <= 0.01,
                    "{}: node {} x mismatch: actual {} vs expected {}",
                    path.display(),
                    id,
                    act_x,
                    exp_x
                );
                assert!(
                    (act_y - exp_y).abs() <= 0.01,
                    "{}: node {} y mismatch: actual {} vs expected {}",
                    path.display(),
                    id,
                    act_y,
                    exp_y
                );
                assert!(
                    (act_w - exp_w).abs() <= 0.01,
                    "{}: node {} width mismatch: actual {} vs expected {}",
                    path.display(),
                    id,
                    act_w,
                    exp_w
                );
                assert!(
                    (act_h - exp_h).abs() <= 0.01,
                    "{}: node {} height mismatch: actual {} vs expected {}",
                    path.display(),
                    id,
                    act_h,
                    exp_h
                );
            }

            // Compare clusters
            if let Some(expected_clusters) = expected["clusters"].as_array() {
                for exp_cluster in expected_clusters {
                    let id = exp_cluster["id"].as_str().unwrap();
                    let actual_cluster = input
                        .nodes
                        .iter()
                        .find(|n| n.id == id)
                        .unwrap_or_else(|| panic!("cluster {id} in actual"));

                    let exp_x = exp_cluster["x"].as_f64().unwrap();
                    let exp_y = exp_cluster["y"].as_f64().unwrap();
                    let exp_w = exp_cluster["width"].as_f64().unwrap();
                    let exp_h = exp_cluster["height"].as_f64().unwrap();

                    let act_x = actual_cluster.x.expect("actual cluster x");
                    let act_y = actual_cluster.y.expect("actual cluster y");
                    let act_w = actual_cluster.width.expect("actual cluster width");
                    let act_h = actual_cluster.height.expect("actual cluster height");

                    assert!(
                        (act_x - exp_x).abs() <= 0.01,
                        "{}: cluster {} x mismatch: actual {} vs expected {}",
                        path.display(),
                        id,
                        act_x,
                        exp_x
                    );
                    assert!(
                        (act_y - exp_y).abs() <= 0.01,
                        "{}: cluster {} y mismatch: actual {} vs expected {}",
                        path.display(),
                        id,
                        act_y,
                        exp_y
                    );
                    assert!(
                        (act_w - exp_w).abs() <= 0.01,
                        "{}: cluster {} width mismatch: actual {} vs expected {}",
                        path.display(),
                        id,
                        act_w,
                        exp_w
                    );
                    assert!(
                        (act_h - exp_h).abs() <= 0.01,
                        "{}: cluster {} height mismatch: actual {} vs expected {}",
                        path.display(),
                        id,
                        act_h,
                        exp_h
                    );
                }
            }

            // Compare edges
            let expected_edges = expected["edges"].as_array().expect("expected edges array");
            for exp_edge in expected_edges {
                let id = exp_edge["id"].as_str().unwrap();
                let actual_edge = input
                    .edges
                    .iter()
                    .find(|e| e.id == id)
                    .unwrap_or_else(|| panic!("edge {id} in actual"));

                let exp_points = exp_edge["points"].as_array().expect("points array");
                let act_points = actual_edge.points.as_ref().expect("actual points");

                assert_eq!(
                    act_points.len(),
                    exp_points.len(),
                    "{}: edge {} points count mismatch",
                    path.display(),
                    id
                );
                for (i, (act_pt, exp_pt)) in act_points.iter().zip(exp_points.iter()).enumerate() {
                    let ep_x = exp_pt["x"].as_f64().unwrap();
                    let ep_y = exp_pt["y"].as_f64().unwrap();
                    assert!(
                        (act_pt.x - ep_x).abs() <= 0.01,
                        "{}: edge {} point {} x mismatch: actual {} vs expected {}",
                        path.display(),
                        id,
                        i,
                        act_pt.x,
                        ep_x
                    );
                    assert!(
                        (act_pt.y - ep_y).abs() <= 0.01,
                        "{}: edge {} point {} y mismatch: actual {} vs expected {}",
                        path.display(),
                        id,
                        i,
                        act_pt.y,
                        ep_y
                    );
                }
            }

            // Compare viewBox
            let exp_vb = &expected["viewBox"];
            let exp_vx = exp_vb["x"].as_f64().unwrap();
            let exp_vy = exp_vb["y"].as_f64().unwrap();
            let exp_vw = exp_vb["width"].as_f64().unwrap();
            let exp_vh = exp_vb["height"].as_f64().unwrap();

            assert!(
                (scene.view_box.x - exp_vx).abs() <= 0.01,
                "{}: viewBox x mismatch: actual {} vs expected {}",
                path.display(),
                scene.view_box.x,
                exp_vx
            );
            assert!(
                (scene.view_box.y - exp_vy).abs() <= 0.01,
                "{}: viewBox y mismatch: actual {} vs expected {}",
                path.display(),
                scene.view_box.y,
                exp_vy
            );
            assert!(
                (scene.view_box.width - exp_vw).abs() <= 0.01,
                "{}: viewBox width mismatch: actual {} vs expected {}",
                path.display(),
                scene.view_box.width,
                exp_vw
            );
            assert!(
                (scene.view_box.height - exp_vh).abs() <= 0.01,
                "{}: viewBox height mismatch: actual {} vs expected {}",
                path.display(),
                scene.view_box.height,
                exp_vh
            );

            count += 1;
        }

        assert!(count >= 8, "Expected at least 8 fixtures, got {count}");
    }
}
