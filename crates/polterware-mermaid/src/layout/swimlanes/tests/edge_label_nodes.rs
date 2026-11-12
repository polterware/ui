use crate::layout::swimlanes::edge_label_nodes::create_edge_label_nodes;
use crate::render::pipeline::types::{Edge, LayoutData, Node};

fn layout(nodes: Vec<Node>, edges: Vec<Edge>) -> LayoutData {
    LayoutData {
        nodes,
        edges,
        ..Default::default()
    }
}

fn plain(id: &str) -> Node {
    Node {
        id: id.to_string(),
        is_group: Some(false),
        shape: Some("rect".to_string()),
        ..Default::default()
    }
}

fn edge(id: &str, start: &str, end: &str, label: Option<&str>) -> Edge {
    Edge {
        id: id.to_string(),
        start: Some(start.to_string()),
        end: Some(end.to_string()),
        label: label.map(str::to_string),
        r#type: Some("arrow".to_string()),
        ..Default::default()
    }
}

#[test]
fn does_not_modify_edges_without_labels() {
    let mut data = layout(
        vec![plain("A"), plain("B")],
        vec![edge("e1", "A", "B", None)],
    );
    let result = create_edge_label_nodes(&mut data);
    assert_eq!(result.nodes.len(), 2);
    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.edges[0].label_node_id, None);
}

#[test]
fn creates_a_label_node_and_layout_only_edges_for_a_labelled_edge() {
    let mut data = layout(
        vec![
            Node {
                parent_id: Some("lane1".to_string()),
                ..plain("A")
            },
            Node {
                parent_id: Some("lane1".to_string()),
                ..plain("B")
            },
            Node {
                is_group: Some(true),
                ..plain("lane1")
            },
        ],
        vec![edge("e1", "A", "B", Some("my label"))],
    );
    let result = create_edge_label_nodes(&mut data);
    assert_eq!(result.nodes.len(), 4);

    let label_node = result
        .nodes
        .iter()
        .find(|node| node.is_edge_label.unwrap_or(false))
        .expect("label node");
    assert_eq!(label_node.id, "edge-label-A-B-e1");
    assert_eq!(label_node.label.as_deref(), Some("my label"));
    assert_eq!(label_node.edge_start.as_deref(), Some("A"));
    assert_eq!(label_node.edge_end.as_deref(), Some("B"));
    assert_eq!(label_node.shape.as_deref(), Some("labelRect"));
    assert_eq!(label_node.is_dummy, Some(true));
    assert_eq!(label_node.parent_id.as_deref(), Some("lane1"));

    assert_eq!(result.edges.len(), 3);
    let original = result.edges.iter().find(|edge| edge.id == "e1").unwrap();
    assert_eq!(original.label_node_id.as_deref(), Some("edge-label-A-B-e1"));
    assert_eq!(original.is_layout_only, None);
    assert_eq!(original.start.as_deref(), Some("A"));
    assert_eq!(original.end.as_deref(), Some("B"));
    assert_eq!(original.label, None);

    let to_label = result
        .edges
        .iter()
        .find(|edge| edge.id == "e1-to-label")
        .unwrap();
    assert_eq!(to_label.start.as_deref(), Some("A"));
    assert_eq!(to_label.end.as_deref(), Some("edge-label-A-B-e1"));
    assert_eq!(to_label.is_layout_only, Some(true));

    let from_label = result
        .edges
        .iter()
        .find(|edge| edge.id == "e1-from-label")
        .unwrap();
    assert_eq!(from_label.start.as_deref(), Some("edge-label-A-B-e1"));
    assert_eq!(from_label.end.as_deref(), Some("B"));
    assert_eq!(from_label.is_layout_only, Some(true));
}

#[test]
fn creates_label_nodes_for_multiple_labelled_edges() {
    let mut data = layout(
        vec![plain("A"), plain("B"), plain("C")],
        vec![
            edge("e1", "A", "B", Some("label1")),
            edge("e2", "B", "C", Some("label2")),
        ],
    );
    let result = create_edge_label_nodes(&mut data);
    assert_eq!(result.nodes.len(), 5);
    assert_eq!(
        result
            .nodes
            .iter()
            .filter(|node| node.is_edge_label.unwrap_or(false))
            .count(),
        2
    );
    assert_eq!(result.edges.len(), 6);
    assert_eq!(
        result
            .edges
            .iter()
            .filter(|edge| edge.is_layout_only == Some(true))
            .count(),
        4
    );
}

#[test]
fn preserves_the_original_edge_arrow_types_when_stamping_a_label_node_id() {
    let mut data = layout(
        vec![plain("A"), plain("B")],
        vec![Edge {
            arrow_type_start: Some("arrow_circle".to_string()),
            arrow_type_end: Some("arrow_cross".to_string()),
            ..edge("e1", "A", "B", Some("test"))
        }],
    );
    let result = create_edge_label_nodes(&mut data);
    let original = result.edges.iter().find(|edge| edge.id == "e1").unwrap();
    assert_eq!(original.arrow_type_start.as_deref(), Some("arrow_circle"));
    assert_eq!(original.arrow_type_end.as_deref(), Some("arrow_cross"));
    assert_eq!(original.label_node_id.as_deref(), Some("edge-label-A-B-e1"));
}

#[test]
fn skips_edges_that_already_carry_a_label_node_id_stamp() {
    let mut data = layout(
        vec![plain("A"), plain("B")],
        vec![Edge {
            label_node_id: Some("pre-existing-label".to_string()),
            ..edge("e1", "A", "B", Some("test"))
        }],
    );
    let result = create_edge_label_nodes(&mut data);
    assert_eq!(result.nodes.len(), 2);
    assert_eq!(result.edges.len(), 1);
}
