//! The architecture database's writing side, ported from `architectureDb.ts`, its
//! `populateDb` in `architectureParser.ts`, and `diagrams/common/populateCommonDb.ts`.
//!
//! `architectureParser.ts` parses the whole document first and then populates the database,
//! so a bad edge or `align` line is reported only after the grammar accepted every line. The
//! order of the writes is the one `populateDb` uses: the accessibility fields, then every
//! group, then every service, then every junction, then every edge, then every alignment.
//! That order is observable: a group declared after a service that lives in it is registered
//! before that service, because the groups are written first.
//!
//! Upstream's errors carry no position; here each is reported at the statement's span.
//!
//! One upstream branch is a crash rather than an error: `addEdge` validates that both ends
//! exist in `nodes` or `groups`, then dereferences both as nodes (`this.nodes.get(lhsId)!.in`).
//! An edge whose end is a group therefore throws a `TypeError` from JavaScript; this port
//! returns that message as a [`ParseError`] at the edge's span.

use super::ast::{
    Alignment, Ast, DataStructures, Direction, DirectionPair, Edge, Group, Junction, LayoutHint,
    Node, Service, group_alignment_key,
};
use super::parser::ParsedArchitecture;
use crate::common::sanitize;
use crate::error::{ParseError, Span};

/// `populateDb`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedArchitecture) -> Result<Ast, ParseError> {
    let mut db = Db::new();
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(&value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(&value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_diagram_title(&value);
    }
    for (group, span) in parsed.groups {
        db.add_group(group, span)?;
    }
    for (service, span) in parsed.services {
        db.add_service(service, span)?;
    }
    for (junction, span) in parsed.junctions {
        db.add_junction(junction, span)?;
    }
    for (edge, span) in parsed.edges {
        db.add_edge(edge, span)?;
    }
    for (hint, span) in parsed.alignments {
        db.add_layout_hint(hint, span)?;
    }
    Ok(db.into_ast())
}

/// What `registeredIds` stores for an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Registered {
    Node,
    Group,
}

impl Registered {
    /// The word upstream interpolates into its "already in use" error.
    fn name(self) -> &'static str {
        match self {
            Registered::Node => "node",
            Registered::Group => "group",
        }
    }
}

/// `this.nodes.get(id)!.in` on an endpoint the database only knows as a group.
const GROUP_ENDPOINT_TYPE_ERROR: &str = "Cannot read properties of undefined (reading 'in')";

/// The module state `architectureDb.ts` keeps.
pub(crate) struct Db {
    nodes: Vec<Node>,
    groups: Vec<Group>,
    edges: Vec<Edge>,
    layout_hints: Vec<LayoutHint>,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    pub(crate) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            groups: Vec::new(),
            edges: Vec::new(),
            layout_hints: Vec::new(),
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `setDiagramTitle`: `sanitizeText(txt)`.
    fn set_diagram_title(&mut self, title: &str) {
        self.diagram_title = sanitize::sanitize_text(title);
    }

    /// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
    fn set_acc_title(&mut self, title: &str) {
        self.acc_title = sanitize::set_acc_title(title);
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    fn set_acc_description(&mut self, description: &str) {
        self.acc_description = sanitize::set_acc_description(description);
    }

    /// `registeredIds.get(id)`.
    fn registered(&self, id: &str) -> Option<Registered> {
        if self.groups.iter().any(|group| group.id == id) {
            Some(Registered::Group)
        } else if self.nodes.iter().any(|node| node.id() == id) {
            Some(Registered::Node)
        } else {
            None
        }
    }

    /// `addGroup`.
    pub(crate) fn add_group(&mut self, group: Group, span: Span) -> Result<(), ParseError> {
        if let Some(kind) = self.registered(&group.id) {
            return Err(ParseError::at(
                format!(
                    "The group id [{}] is already in use by another {}",
                    group.id,
                    kind.name()
                ),
                span,
            ));
        }
        if let Some(parent) = &group.container {
            if *parent == group.id {
                return Err(ParseError::at(
                    format!("The group [{}] cannot be placed within itself", group.id),
                    span,
                ));
            }
            match self.registered(parent) {
                None => {
                    return Err(ParseError::at(
                        format!(
                            "The group [{}]'s parent does not exist. Please make sure the parent is created before this group",
                            group.id
                        ),
                        span,
                    ));
                }
                Some(Registered::Node) => {
                    return Err(ParseError::at(
                        format!("The group [{}]'s parent is not a group", group.id),
                        span,
                    ));
                }
                Some(Registered::Group) => {}
            }
        }
        self.groups.push(group);
        Ok(())
    }

    /// `addService`.
    pub(crate) fn add_service(&mut self, service: Service, span: Span) -> Result<(), ParseError> {
        if let Some(kind) = self.registered(&service.id) {
            return Err(ParseError::at(
                format!(
                    "The service id [{}] is already in use by another {}",
                    service.id,
                    kind.name()
                ),
                span,
            ));
        }
        if let Some(parent) = &service.container {
            if *parent == service.id {
                return Err(ParseError::at(
                    format!(
                        "The service [{}] cannot be placed within itself",
                        service.id
                    ),
                    span,
                ));
            }
            match self.registered(parent) {
                None => {
                    return Err(ParseError::at(
                        format!(
                            "The service [{}]'s parent does not exist. Please make sure the parent is created before this service",
                            service.id
                        ),
                        span,
                    ));
                }
                Some(Registered::Node) => {
                    return Err(ParseError::at(
                        format!("The service [{}]'s parent is not a group", service.id),
                        span,
                    ));
                }
                Some(Registered::Group) => {}
            }
        }
        self.nodes.push(Node::Service(service));
        Ok(())
    }

    /// `addJunction`.
    pub(crate) fn add_junction(
        &mut self,
        junction: Junction,
        span: Span,
    ) -> Result<(), ParseError> {
        if let Some(kind) = self.registered(&junction.id) {
            return Err(ParseError::at(
                format!(
                    "The junction id [{}] is already in use by another {}",
                    junction.id,
                    kind.name()
                ),
                span,
            ));
        }
        if let Some(parent) = &junction.container {
            if *parent == junction.id {
                return Err(ParseError::at(
                    format!(
                        "The junction [{}] cannot be placed within itself",
                        junction.id
                    ),
                    span,
                ));
            }
            match self.registered(parent) {
                None => {
                    return Err(ParseError::at(
                        format!(
                            "The junction [{}]'s parent does not exist. Please make sure the parent is created before this junction",
                            junction.id
                        ),
                        span,
                    ));
                }
                Some(Registered::Node) => {
                    return Err(ParseError::at(
                        format!("The junction [{}]'s parent is not a group", junction.id),
                        span,
                    ));
                }
                Some(Registered::Group) => {}
            }
        }
        self.nodes.push(Node::Junction(junction));
        Ok(())
    }

    /// `addEdge`. The grammar only produces `L`, `R`, `T`, and `B`, so the direction check
    /// upstream performs first can never fire from parsed text; it is kept in the record's
    /// type instead, which has exactly those four values.
    pub(crate) fn add_edge(&mut self, edge: Edge, span: Span) -> Result<(), ParseError> {
        if !self.endpoint_exists(&edge.lhs_id) {
            return Err(ParseError::at(
                format!(
                    "The left-hand id [{}] does not yet exist. Please create the service/group before declaring an edge to it.",
                    edge.lhs_id
                ),
                span,
            ));
        }
        if !self.endpoint_exists(&edge.rhs_id) {
            return Err(ParseError::at(
                format!(
                    "The right-hand id [{}] does not yet exist. Please create the service/group before declaring an edge to it.",
                    edge.rhs_id
                ),
                span,
            ));
        }

        let Some(lhs_node) = self.nodes.iter().find(|node| node.id() == edge.lhs_id) else {
            return Err(ParseError::at(GROUP_ENDPOINT_TYPE_ERROR, span));
        };
        let lhs_group = lhs_node.container().map(str::to_string);
        let Some(rhs_node) = self.nodes.iter().find(|node| node.id() == edge.rhs_id) else {
            return Err(ParseError::at(GROUP_ENDPOINT_TYPE_ERROR, span));
        };
        let rhs_group = rhs_node.container().map(str::to_string);

        if edge.lhs_group
            && let (Some(lhs_parent), Some(rhs_parent)) = (&lhs_group, &rhs_group)
            && lhs_parent == rhs_parent
        {
            return Err(ParseError::at(
                format!(
                    "The left-hand id [{}] is modified to traverse the group boundary, but the edge does not pass through two groups.",
                    edge.lhs_id
                ),
                span,
            ));
        }
        if edge.rhs_group
            && let (Some(lhs_parent), Some(rhs_parent)) = (&lhs_group, &rhs_group)
            && lhs_parent == rhs_parent
        {
            return Err(ParseError::at(
                format!(
                    "The right-hand id [{}] is modified to traverse the group boundary, but the edge does not pass through two groups.",
                    edge.rhs_id
                ),
                span,
            ));
        }

        let lhs_index = self.nodes.iter().position(|node| node.id() == edge.lhs_id);
        let rhs_index = self.nodes.iter().position(|node| node.id() == edge.rhs_id);
        self.edges.push(edge.clone());
        if let (Some(lhs), Some(rhs)) = (lhs_index, rhs_index) {
            self.nodes[lhs].push_edge(edge.clone());
            self.nodes[rhs].push_edge(edge);
        }
        Ok(())
    }

    /// `addLayoutHint`.
    pub(crate) fn add_layout_hint(
        &mut self,
        hint: LayoutHint,
        span: Span,
    ) -> Result<(), ParseError> {
        if hint.members.len() < 2 {
            return Err(ParseError::at(
                format!(
                    "An align directive requires at least two members; got {}",
                    hint.members.len()
                ),
                span,
            ));
        }
        let direction = hint.direction.as_str();
        let mut seen: Vec<&str> = Vec::new();
        for id in &hint.members {
            if self.registered(id) != Some(Registered::Node) {
                return Err(ParseError::at(
                    format!(
                        "align {direction} references [{id}], which is not a service or junction"
                    ),
                    span,
                ));
            }
            if seen.contains(&id.as_str()) {
                return Err(ParseError::at(
                    format!("align {direction} lists [{id}] more than once"),
                    span,
                ));
            }
            seen.push(id);
        }
        self.layout_hints.push(hint);
        Ok(())
    }

    fn endpoint_exists(&self, id: &str) -> bool {
        self.registered(id).is_some()
    }

    pub(crate) fn into_ast(self) -> Ast {
        Ast {
            groups: self.groups,
            nodes: self.nodes,
            edges: self.edges,
            layout_hints: self.layout_hints,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}

/// `getDataStructures`: the adjacency list, the spatial maps of its connected components,
/// and the alignments between groups.
pub(crate) fn data_structures(ast: &Ast) -> DataStructures {
    let mut group_alignments: Vec<(String, Alignment)> = Vec::new();
    let mut adjacency: Vec<(String, Vec<(DirectionPair, String)>)> = Vec::new();

    for node in &ast.nodes {
        let id = node.id();
        let mut directions: Vec<(DirectionPair, String)> = Vec::new();
        for edge in node.edges() {
            let lhs_container = ast.node(&edge.lhs_id).and_then(Node::container);
            let rhs_container = ast.node(&edge.rhs_id).and_then(Node::container);
            if let (Some(lhs_group), Some(rhs_group)) = (lhs_container, rhs_container)
                && lhs_group != rhs_group
            {
                let alignment = Alignment::for_directions(edge.lhs_dir, edge.rhs_dir);
                if alignment != Alignment::Bend {
                    set_group_alignment(
                        &mut group_alignments,
                        group_alignment_key(lhs_group, rhs_group),
                        alignment,
                    );
                }
            }

            let pair = if edge.lhs_id == id {
                DirectionPair::new(edge.lhs_dir, edge.rhs_dir)
                    .map(|pair| (pair, edge.rhs_id.clone()))
            } else {
                DirectionPair::new(edge.rhs_dir, edge.lhs_dir)
                    .map(|pair| (pair, edge.lhs_id.clone()))
            };
            if let Some((pair, neighbor)) = pair {
                set_direction(&mut directions, pair, neighbor);
            }
        }
        adjacency.push((id.to_string(), directions));
    }

    let mut visited: Vec<String> = Vec::new();
    let mut not_visited: Vec<String> = adjacency.iter().map(|(id, _)| id.clone()).collect();
    let mut spatial_maps = Vec::new();
    while let Some(start) = not_visited.first().cloned() {
        spatial_maps.push(breadth_first(
            start,
            &adjacency,
            &mut visited,
            &mut not_visited,
        ));
    }

    DataStructures {
        adjacency,
        spatial_maps,
        group_alignments,
    }
}

/// `BFS` in `getDataStructures`. The frontier is the same queue; a neighbour that is queued
/// but not yet visited can be reached twice, and the later position wins, as upstream has it.
fn breadth_first(
    start: String,
    adjacency: &[(String, Vec<(DirectionPair, String)>)],
    visited: &mut Vec<String>,
    not_visited: &mut Vec<String>,
) -> Vec<(String, (i32, i32))> {
    let mut spatial_map: Vec<(String, (i32, i32))> = vec![(start.clone(), (0, 0))];
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(id) = queue.pop_front() {
        if !visited.contains(&id) {
            visited.push(id.clone());
        }
        not_visited.retain(|other| *other != id);
        let directions = adjacency
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .map(|(_, directions)| directions)
            .expect("every node is an adjacency list key");
        let position = spatial_map
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .map(|(_, position)| *position)
            .expect("the queue only holds positioned nodes");
        for (pair, neighbor) in directions {
            if !visited.contains(neighbor) {
                let next = shift_position(position, *pair);
                match spatial_map
                    .iter_mut()
                    .find(|(node_id, _)| node_id == neighbor)
                {
                    Some(entry) => entry.1 = next,
                    None => spatial_map.push((neighbor.clone(), next)),
                }
                queue.push_back(neighbor.clone());
            }
        }
    }
    spatial_map
}

/// `shiftPositionByArchitectureDirectionPair`.
fn shift_position((x, y): (i32, i32), pair: DirectionPair) -> (i32, i32) {
    let DirectionPair { source, target } = pair;
    if source.is_horizontal() {
        let x_shift = if source == Direction::Left { -1 } else { 1 };
        if target.is_vertical() {
            let y_shift = if target == Direction::Top { 1 } else { -1 };
            (x + x_shift, y + y_shift)
        } else {
            (x + x_shift, y)
        }
    } else if target.is_horizontal() {
        let x_shift = if target == Direction::Left { 1 } else { -1 };
        let y_shift = if source == Direction::Top { 1 } else { -1 };
        (x + x_shift, y + y_shift)
    } else {
        let y_shift = if source == Direction::Top { 1 } else { -1 };
        (x, y + y_shift)
    }
}

/// `Map.set` on an existing pair keeps its position and takes the new neighbour.
fn set_direction(
    directions: &mut Vec<(DirectionPair, String)>,
    pair: DirectionPair,
    neighbor: String,
) {
    match directions
        .iter_mut()
        .find(|(existing, _)| *existing == pair)
    {
        Some(entry) => entry.1 = neighbor,
        None => directions.push((pair, neighbor)),
    }
}

/// `Map.set` on an existing group pair keeps its position and takes the new alignment.
fn set_group_alignment(
    alignments: &mut Vec<(String, Alignment)>,
    key: String,
    alignment: Alignment,
) {
    match alignments.iter_mut().find(|(existing, _)| *existing == key) {
        Some(entry) => entry.1 = alignment,
        None => alignments.push((key, alignment)),
    }
}
