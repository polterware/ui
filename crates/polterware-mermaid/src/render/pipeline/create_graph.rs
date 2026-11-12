//! Port of upstream `rendering-util/createGraph.ts`.

use std::collections::HashMap;

use crate::layout::dagre::{EdgeLabel, LayeredGraph, NodeLabel};
use crate::layout::graphlib::GraphOptions;
use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::render::shapes::{self, RenderedNode, ShapeError};
use crate::scene::Group;
use crate::theme::ThemeVariables;

use super::types::{CreateGraphOptions, LayoutData, Node, ShapeRenderOptions};

pub struct LayoutElementGroups {
    pub clusters: Group,
    pub edge_paths: Group,
    pub edge_labels: Group,
    pub nodes: Group,
    pub root_groups: Group,
}

pub fn create_layout_element_groups() -> LayoutElementGroups {
    LayoutElementGroups {
        clusters: Group::new().with_class("clusters"),
        edge_paths: Group::new().with_class("edges edgePaths"),
        edge_labels: Group::new().with_class("edgeLabels"),
        nodes: Group::new().with_class("nodes"),
        root_groups: Group::new().with_class("root"),
    }
}

pub fn measure_group_label(
    node: &mut Node,
    measure_width: Option<f64>,
    measure: &dyn TextMeasure,
    font: &Font,
) {
    if let Some(label) = &node.label {
        let width = measure_width.unwrap_or(node.wrapping_width.unwrap_or(f64::INFINITY));
        let block = shape_text(
            label,
            &TextOptions {
                width,
                markdown: node.label_type.as_deref() == Some("markdown"),
                min_width: 0.0,
            },
            font,
            measure,
        );
        if let Ok(block) = block {
            node.label_b_box = Some(super::types::LabelBBox {
                x: Some(0.0),
                y: Some(0.0),
                width: block.size.width,
                height: block.size.height,
            });
            return;
        }
    }
    node.label_b_box = Some(super::types::LabelBBox {
        x: Some(0.0),
        y: Some(0.0),
        width: 0.0,
        height: 0.0,
    });
}

pub fn to_shape_node(node: &Node) -> shapes::Node {
    let mut s = shapes::Node::new(
        node.id.clone(),
        node.label.clone().unwrap_or_default(),
        node.shape.clone().unwrap_or_else(|| "rect".to_string()),
    );
    s.width = node.width;
    s.height = node.height;
    s.description = node.description.clone();
    s.alias = node.alias.clone();
    s.attributes = node
        .attributes
        .iter()
        .map(|attribute| shapes::Attribute {
            type_name: attribute.type_name.clone(),
            name: attribute.name.clone(),
            keys: attribute.keys.clone(),
            comment: attribute.comment.clone(),
        })
        .collect();
    s.padding = node.padding.unwrap_or(0.0);
    s.requirement = node.requirement.clone();
    s.element = node.element.clone();
    s.css_styles = node.css_styles.clone().unwrap_or_default();
    s.rx = node.rx.unwrap_or(0.0);
    s.ry = node.ry.unwrap_or(0.0);
    s.min_width = node.min_width;
    s.wrapping_width = node.wrapping_width;
    s.css_classes = node.css_classes.clone().unwrap_or_default();
    s.look = node.look.clone().unwrap_or_else(|| "classic".to_string());
    s.icon = node.icon.clone();
    s.img = node.img.clone();
    s.asset_width = node.asset_width;
    s.asset_height = node.asset_height;
    s.pos = node.pos.clone();
    s.constraint = node.constraint.clone();
    s.text = node.text.clone();
    s.label_type = node.label_type.clone();
    s.label_font_size = node.label_font_size;
    s.stereotype = node.stereotype.clone();
    s.business = node.business.unwrap_or(false);
    s.json_rows = node
        .json_rows
        .iter()
        .map(|row| shapes::usecase_json_table::JsonRow::new(row.key.clone(), row.value.clone()))
        .collect();
    s.center_label = node.center_label.unwrap_or(false);
    s.class_members = node.members.clone();
    s.class_methods = node.methods.clone();
    s.class_styles = node.styles.clone();
    s.annotations = node.annotations.clone();
    s.class_padding = node.class_padding;
    s.hide_empty_members_box = node.hide_empty_members_box.unwrap_or(false);
    s
}

pub fn insert_measured_node(
    node: &mut Node,
    _render_options: &ShapeRenderOptions,
    measure: &dyn TextMeasure,
    theme: &ThemeVariables,
) -> Result<RenderedNode, ShapeError> {
    let shape_node = to_shape_node(node);
    let rendered = shapes::render_node(shape_node, measure, theme)?;
    // `insertMeasuredNode` sizes the node from the DOM element, not from the intersection
    // box. Most shapes keep both in `bounds`; fork/join and the state end circle report the
    // painted box through `width`/`height` (their `bounds` is the enlarged box an edge stops
    // at). An explicit width or height from the layout data is not a measurement.
    let width = if node.width.is_none() {
        rendered.node.width
    } else {
        None
    };
    let height = if node.height.is_none() {
        rendered.node.height
    } else {
        None
    };
    node.width = Some(width.unwrap_or(rendered.node.bounds.width));
    node.height = Some(height.unwrap_or(rendered.node.bounds.height));
    node.rx = Some(rendered.node.rx);
    node.ry = Some(rendered.node.ry);
    node.bounds = rendered.node.bounds;
    Ok(rendered)
}

pub fn resolve_node_dir(
    node: &Node,
    info_by_id: &HashMap<String, (Option<String>, Option<String>)>,
    diagram_dir: Option<&str>,
) -> Option<String> {
    if let Some(dir) = &node.dir {
        return Some(dir.clone());
    }
    let mut current_parent = node.parent_id.as_deref();
    let mut seen = std::collections::HashSet::new();
    while let Some(parent_id) = current_parent {
        if seen.contains(parent_id) {
            break;
        }
        seen.insert(parent_id);
        if let Some((next_parent, dir)) = info_by_id.get(parent_id) {
            if let Some(d) = dir {
                return Some(d.clone());
            }
            current_parent = next_parent.as_deref();
        } else {
            break;
        }
    }
    diagram_dir.map(String::from)
}

pub struct CreatedGraph {
    pub graph: LayeredGraph,
    pub groups: LayoutElementGroups,
    pub node_elements: HashMap<String, Group>,
}

pub fn create_graph_with_elements(
    data4_layout: &mut LayoutData,
    options: &CreateGraphOptions,
    measure: &dyn TextMeasure,
    theme: &ThemeVariables,
) -> Result<CreatedGraph, ShapeError> {
    let mut graph = LayeredGraph::with_options(GraphOptions {
        multigraph: true,
        compound: true,
        ..GraphOptions::default()
    });

    let groups = create_layout_element_groups();
    let mut node_elements = HashMap::new();
    let font = Font::new("monospace", 16.0);

    let info_by_id: HashMap<String, (Option<String>, Option<String>)> = data4_layout
        .nodes
        .iter()
        .map(|n| (n.id.clone(), (n.parent_id.clone(), n.dir.clone())))
        .collect();
    let diagram_dir = data4_layout.direction.as_deref();

    for node in &mut data4_layout.nodes {
        if node.is_group == Some(true) {
            let unwrap = options.unwrap_group_labels == Some(true)
                && node.label_type.as_deref() != Some("markdown");
            let measure_w = if unwrap {
                Some(f64::INFINITY)
            } else {
                node.wrapping_width
            };
            measure_group_label(node, measure_w, measure, &font);

            let mut nl = NodeLabel::default();
            nl.width = node.width.unwrap_or(0.0);
            nl.height = node.height.unwrap_or(0.0);
            graph.set_node_with(&node.id, nl);
        } else {
            let dir = resolve_node_dir(node, &info_by_id, diagram_dir);
            let rendered = insert_measured_node(
                node,
                &ShapeRenderOptions {
                    config: data4_layout.config.clone(),
                    dir,
                    padding: node.padding,
                },
                measure,
                theme,
            )?;
            node_elements.insert(node.id.clone(), rendered.group);

            let mut nl = NodeLabel::default();
            nl.width = node.width.unwrap_or(0.0);
            nl.height = node.height.unwrap_or(0.0);
            graph.set_node_with(&node.id, nl);
        }
        if let Some(parent_id) = &node.parent_id {
            let _ = graph.set_parent(&node.id, Some(parent_id.as_str()));
        }
    }

    for edge in &mut data4_layout.edges {
        let (label_w, label_h) = if let Some(lbl) = &edge.label {
            if !lbl.is_empty() {
                let block = shape_text(
                    lbl,
                    &TextOptions {
                        width: f64::INFINITY,
                        markdown: edge.label_type.as_deref() == Some("markdown"),
                        min_width: 0.0,
                    },
                    &font,
                    measure,
                );
                if let Ok(block) = block {
                    (block.size.width + 4.0, block.size.height + 4.0)
                } else {
                    (0.0, 0.0)
                }
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };
        edge.width = Some(label_w);
        edge.height = Some(label_h);

        let el = EdgeLabel {
            width: Some(label_w),
            height: Some(label_h),
            labelpos: edge.labelpos.clone(),
            ..Default::default()
        };
        if let (Some(start), Some(end)) = (&edge.start, &edge.end) {
            let _ = graph.set_edge_full(start, end, Some(el), Some(&edge.id));
        }
    }

    Ok(CreatedGraph {
        graph,
        groups,
        node_elements,
    })
}
