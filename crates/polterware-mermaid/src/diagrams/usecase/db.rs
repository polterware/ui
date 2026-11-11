//! The usecase database's writing side, ported from `usecase.visitor.ts`'s drafts,
//! `usecaseModelBuilder.ts` (`UsecaseModelBuilder.finalize` and the collectors it runs),
//! `usecaseAst.ts` (`buildUsecaseGraphAST`), and `usecaseJson.ts` (`parseOrderedJsonObject`).
//!
//! The parser hands over detached drafts; [`build`] resolves them into a complete model and
//! graph AST only after every statement parsed, so a semantic failure publishes nothing.
//! Error messages keep upstream's text; upstream's trailing `at line ..., column ...` moves
//! into the [`ParseError`]'s [`Span`], which is the convention this crate follows.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::ast::{
    Actor, ActorType, Animation, ArrowType, Ast, BoundaryType, ClassDef, Direction, EdgeOccurrence,
    GraphAst, GraphEdge, GraphGroup, GraphHeader, GraphNode, GraphStatement, LabelType,
    Relationship, RelationshipType, StatementKind, SystemBoundary, UseCase, UseCaseShape,
    UsecaseJsonNode, UsecaseNote, UsecaseSymbolKind,
};
use crate::common::sanitize::collapse_description_indent;
use crate::error::{ParseError, Span};

/// A source position as the visitor records it (`DraftLocation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DraftLocation {
    pub(crate) span: Span,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// A label with the span of its text (`DraftLabel`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftLabel {
    pub(crate) text: String,
    pub(crate) type_: LabelType,
    pub(crate) span: Span,
}

/// A metadata value: `string | boolean`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MetadataValue {
    String(String),
    Bool(bool),
}

impl MetadataValue {
    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            MetadataValue::Bool(value) => Some(*value),
            MetadataValue::String(_) => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            MetadataValue::String(value) => Some(value),
            MetadataValue::Bool(_) => None,
        }
    }
}

/// One `@{ key: value }` property (`DraftMetadataProperty`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftMetadataProperty {
    pub(crate) key: String,
    pub(crate) value: MetadataValue,
    pub(crate) span: Span,
    pub(crate) key_span: Span,
    pub(crate) value_span: Span,
    pub(crate) location: DraftLocation,
}

/// A whole `@{ ... }` block (`DraftMetadata`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftMetadata {
    pub(crate) properties: Vec<DraftMetadataProperty>,
    pub(crate) location: DraftLocation,
}

/// An actor or use case declaration (`DraftElement`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftElement {
    pub(crate) id: String,
    pub(crate) kind: ElementKind,
    pub(crate) label: DraftLabel,
    pub(crate) location: DraftLocation,
    pub(crate) generated: bool,
    pub(crate) parent_id: Option<String>,
    pub(crate) parent_location: Option<DraftLocation>,
    pub(crate) shape: Option<UseCaseShape>,
    pub(crate) metadata: Option<DraftMetadata>,
    pub(crate) stereotype: Option<String>,
    pub(crate) classes: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementKind {
    Actor,
    Usecase,
}

impl ElementKind {
    fn as_str(self) -> &'static str {
        match self {
            ElementKind::Actor => "actor",
            ElementKind::Usecase => "usecase",
        }
    }
}

/// A system boundary declaration (`DraftBoundary`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftBoundary {
    pub(crate) id: String,
    pub(crate) label: DraftLabel,
    pub(crate) location: DraftLocation,
    pub(crate) generated: bool,
    pub(crate) classes: Vec<String>,
}

/// A JSON table declaration (`DraftJson`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftJson {
    pub(crate) id: String,
    pub(crate) value: Value,
    pub(crate) property_order: Map<String, Value>,
    pub(crate) location: DraftLocation,
    pub(crate) classes: Vec<String>,
}

/// One end of a relationship (`DraftEndpoint`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftEndpoint {
    pub(crate) id: String,
    pub(crate) label: DraftLabel,
    pub(crate) location: DraftLocation,
    pub(crate) generated: bool,
    pub(crate) declaration: bool,
    pub(crate) classes_on_reference: bool,
}

/// A relationship draft (`DraftRelationship`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftRelationship {
    pub(crate) source: DraftEndpoint,
    pub(crate) target: DraftEndpoint,
    pub(crate) location: DraftLocation,
    pub(crate) explicit_id: Option<String>,
    pub(crate) explicit_id_location: Option<DraftLocation>,
    pub(crate) type_: RelationshipType,
    pub(crate) arrow_type: ArrowType,
    pub(crate) label: Option<DraftLabel>,
    pub(crate) minlen: u32,
}

/// A note draft (`DraftNote`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftNote {
    pub(crate) target: String,
    pub(crate) target_location: DraftLocation,
    pub(crate) label: DraftLabel,
    pub(crate) location: DraftLocation,
}

/// A standalone or inline metadata assignment (`DraftMetadataAssignment`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftMetadataAssignment {
    pub(crate) target: String,
    pub(crate) target_location: DraftLocation,
    pub(crate) metadata: DraftMetadata,
    pub(crate) statement: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftClassAssignment {
    pub(crate) targets: Vec<(String, DraftLocation)>,
    pub(crate) classes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftStyleAssignment {
    pub(crate) target: String,
    pub(crate) location: DraftLocation,
    pub(crate) styles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftClassDefinition {
    pub(crate) ids: Vec<String>,
    pub(crate) styles: Vec<String>,
}

/// One entry of the statement arena: the normalized statement and its child statements.
#[derive(Debug)]
pub(crate) struct StatementNode {
    pub(crate) statement: GraphStatement,
    pub(crate) children: Vec<usize>,
}

/// Everything the visitor collects, plus the graph statements it already normalized.
#[derive(Debug, Default)]
pub(crate) struct ParsedDocument {
    pub(crate) header_span: Span,
    pub(crate) statements: Vec<usize>,
    pub(crate) arena: Vec<StatementNode>,
    pub(crate) acc_title: String,
    pub(crate) acc_description: String,
    pub(crate) elements: Vec<DraftElement>,
    pub(crate) boundaries: Vec<DraftBoundary>,
    pub(crate) json_drafts: Vec<DraftJson>,
    pub(crate) relationship_drafts: Vec<DraftRelationship>,
    pub(crate) note_drafts: Vec<DraftNote>,
    pub(crate) metadata_assignments: Vec<DraftMetadataAssignment>,
    pub(crate) class_assignments: Vec<DraftClassAssignment>,
    pub(crate) style_assignments: Vec<DraftStyleAssignment>,
    pub(crate) class_definitions: Vec<DraftClassDefinition>,
    pub(crate) directions: Vec<Direction>,
}

impl ParsedDocument {
    pub(crate) fn add_statement(
        &mut self,
        statement: GraphStatement,
        children: Vec<usize>,
    ) -> usize {
        let index = self.arena.len();
        self.arena.push(StatementNode {
            statement,
            children,
        });
        index
    }

    pub(crate) fn add_root(&mut self, statement: usize) {
        self.statements.push(statement);
    }
}

/// A `Map` whose iteration order is insertion order, like the JavaScript `Map` upstream uses.
#[derive(Clone, Debug, PartialEq)]
struct OrderedMap<T> {
    entries: Vec<(String, T)>,
}

// Deriving would require `T: Default`, which no entry type needs.
#[allow(clippy::derivable_impls)]
impl<T> Default for OrderedMap<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> OrderedMap<T> {
    fn get(&self, id: &str) -> Option<&T> {
        self.entries
            .iter()
            .find(|(entry_id, _)| entry_id == id)
            .map(|(_, value)| value)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        self.entries
            .iter_mut()
            .find(|(entry_id, _)| entry_id == id)
            .map(|(_, value)| value)
    }

    fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|(entry_id, _)| entry_id == id)
    }

    fn insert(&mut self, id: String, value: T) {
        if let Some(slot) = self.get_mut(&id) {
            *slot = value;
        } else {
            self.entries.push((id, value));
        }
    }

    fn remove(&mut self, id: &str) -> Option<T> {
        let index = self
            .entries
            .iter()
            .position(|(entry_id, _)| entry_id == id)?;
        Some(self.entries.remove(index).1)
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn values(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|(_, value)| value)
    }

    fn into_vec(self) -> Vec<T> {
        self.entries.into_iter().map(|(_, value)| value).collect()
    }
}

/// A declared id and where it came from (`SymbolOrigin`).
#[derive(Clone, Debug, PartialEq)]
struct SymbolOrigin {
    kind: UsecaseSymbolKind,
    location: DraftLocation,
    generated: bool,
    label: Option<String>,
}

/// The resolved state of an actor or use case (`ElementState`).
#[derive(Clone, Debug, PartialEq)]
struct ElementState {
    kind: ElementKind,
    id: String,
    label: DraftLabel,
    location: DraftLocation,
    generated: bool,
    parent_id: Option<String>,
    parent_location: Option<DraftLocation>,
    shape: Option<UseCaseShape>,
    stereotype: Option<String>,
    stereotype_location: Option<DraftLocation>,
    classes: Vec<String>,
    actor_type: Option<ActorType>,
    icon: Option<String>,
    business: Option<bool>,
}

/// The resolved state of a system boundary (`BoundaryState`).
#[derive(Clone, Debug, PartialEq)]
struct BoundaryState {
    id: String,
    label: DraftLabel,
    location: DraftLocation,
    generated: bool,
    type_: Option<BoundaryType>,
    classes: Vec<String>,
    styles: Vec<String>,
    members: Vec<String>,
}

struct EdgeState {
    relationship_index: usize,
}

fn location_text(location: &DraftLocation) -> String {
    format!(
        "line {}, column {} [{},{})",
        location.line, location.column, location.span.start, location.span.end
    )
}

fn label_suffix(label: Option<&str>) -> String {
    label.map_or_else(String::new, |label| format!(" (label \"{label}\")"))
}

fn generated_from(origin: &SymbolOrigin) -> Option<&str> {
    if origin.generated {
        origin.label.as_deref()
    } else {
        None
    }
}

/// `commonDb.setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
fn set_acc_title(value: &str) -> String {
    value
        .trim_start_matches(crate::preprocess::is_js_whitespace)
        .to_string()
}

/// `commonDb.setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
fn set_acc_description(value: &str) -> String {
    collapse_description_indent(value)
}

fn push_unique(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn conflict(
    message: impl Into<String>,
    current: &DraftLocation,
    previous: &DraftLocation,
    current_label: Option<&str>,
    previous_label: Option<&str>,
) -> ParseError {
    ParseError::at(
        format!(
            "{} at {}{}; previous declaration at {}{}",
            message.into(),
            location_text(current),
            label_suffix(current_label),
            location_text(previous),
            label_suffix(previous_label)
        ),
        current.span,
    )
}

/// Applies the parsed document to fresh database state, like `UsecaseModelBuilder.finalize`
/// followed by `db.commit`.
pub(crate) fn build(document: ParsedDocument, source: &str) -> Result<Ast, ParseError> {
    let ParsedDocument {
        header_span,
        statements,
        mut arena,
        acc_title,
        acc_description,
        elements: element_drafts,
        boundaries: boundary_drafts,
        json_drafts,
        relationship_drafts,
        note_drafts,
        metadata_assignments,
        class_assignments,
        style_assignments,
        class_definitions,
        directions,
    } = document;

    let mut symbols: OrderedMap<SymbolOrigin> = OrderedMap::default();
    let mut element_states: OrderedMap<ElementState> = OrderedMap::default();
    let mut boundary_states: OrderedMap<BoundaryState> = OrderedMap::default();
    let mut first: HashMap<String, usize> = HashMap::new();

    for relation in &relationship_drafts {
        record_first(
            &mut first,
            &relation.source.id,
            relation.source.location.span.start,
        );
        record_first(
            &mut first,
            &relation.target.id,
            relation.target.location.span.start,
        );
    }
    for draft in &element_drafts {
        record_first(&mut first, &draft.id, draft.location.span.start);
    }
    for draft in &boundary_drafts {
        record_first(&mut first, &draft.id, draft.location.span.start);
    }
    for draft in &json_drafts {
        record_first(&mut first, &draft.id, draft.location.span.start);
    }

    enum Declaration<'a> {
        Element(&'a DraftElement),
        Boundary(&'a DraftBoundary),
        Json(&'a DraftJson),
        Edge(&'a DraftRelationship),
    }

    let mut declarations: Vec<(usize, Declaration<'_>)> = Vec::new();
    declarations.extend(
        element_drafts
            .iter()
            .map(|value| (value.location.span.start, Declaration::Element(value))),
    );
    declarations.extend(
        boundary_drafts
            .iter()
            .map(|value| (value.location.span.start, Declaration::Boundary(value))),
    );
    declarations.extend(
        json_drafts
            .iter()
            .map(|value| (value.location.span.start, Declaration::Json(value))),
    );
    declarations.extend(relationship_drafts.iter().filter_map(|value| {
        value.explicit_id.as_ref().map(|_| {
            (
                value
                    .explicit_id_location
                    .expect("explicit id has a location")
                    .span
                    .start,
                Declaration::Edge(value),
            )
        })
    }));
    declarations.sort_by_key(|(offset, _)| *offset);

    for (_, declaration) in declarations {
        match declaration {
            Declaration::Element(draft) => {
                collect_element(draft, &mut symbols, &mut element_states)?;
            }
            Declaration::Boundary(draft) => {
                collect_boundary(draft, &mut symbols, &mut boundary_states)?;
            }
            Declaration::Json(draft) => register_unique(
                &mut symbols,
                &draft.id,
                UsecaseSymbolKind::Json,
                &draft.location,
                false,
            )?,
            Declaration::Edge(draft) => register_unique(
                &mut symbols,
                draft.explicit_id.as_deref().expect("explicit edge id"),
                UsecaseSymbolKind::Edge,
                draft
                    .explicit_id_location
                    .as_ref()
                    .expect("explicit id has a location"),
                false,
            )?,
        }
    }

    let mut actors: OrderedMap<Actor> = OrderedMap::default();
    let mut use_cases: OrderedMap<UseCase> = OrderedMap::default();
    let mut boundaries: OrderedMap<SystemBoundary> = OrderedMap::default();
    let mut json_nodes: OrderedMap<UsecaseJsonNode> = OrderedMap::default();
    let mut class_defs: OrderedMap<ClassDef> = OrderedMap::default();
    let mut relationships: Vec<Relationship> = Vec::new();
    let mut edges: OrderedMap<EdgeState> = OrderedMap::default();
    let mut notes: OrderedMap<UsecaseNote> = OrderedMap::default();
    let mut note_counter = 0;
    let mut relationship_counter = 0;

    materialize_elements(&element_states, &first, &mut actors, &mut use_cases);
    materialize_boundaries(&boundary_states, &first, &mut boundaries);
    materialize_json(&json_drafts, &first, &mut json_nodes);
    materialize_relationships(
        &relationship_drafts,
        &mut symbols,
        &mut element_states,
        &mut first,
        &mut relationships,
        &mut edges,
        &mut relationship_counter,
    )?;
    reorder_elements(&element_states, &first, &mut actors, &mut use_cases);
    apply_metadata_assignments(
        &metadata_assignments,
        &symbols,
        &mut element_states,
        &mut boundary_states,
        &edges,
        &mut relationships,
        &mut arena,
    )?;
    validate_and_refresh_elements(&element_states, &mut actors, &mut use_cases)?;
    refresh_boundaries(
        &element_drafts,
        &element_states,
        &mut boundary_states,
        &mut boundaries,
    )?;
    materialize_notes(&note_drafts, &symbols, &mut notes, &mut note_counter)?;
    apply_class_definitions(&class_definitions, &mut class_defs);
    apply_classes_and_styles(
        &class_assignments,
        &style_assignments,
        &symbols,
        &mut actors,
        &mut use_cases,
        &mut boundaries,
        &mut json_nodes,
        &edges,
        &mut relationships,
    )?;

    let direction = directions.last().copied().unwrap_or_default();

    let acc_title_raw = acc_title.clone();
    let acc_description_raw = acc_description.clone();
    let acc_title = set_acc_title(&acc_title);
    let acc_description = set_acc_description(&acc_description);

    let graph = build_graph(
        &actors,
        &use_cases,
        &boundaries,
        &notes,
        &json_nodes,
        &class_defs,
        &relationships,
        source,
        header_span,
        &acc_title_raw,
        &acc_description_raw,
        direction,
        &arena,
        &statements,
    );

    let symbols = symbols
        .entries
        .into_iter()
        .map(|(id, origin)| (id, origin.kind))
        .collect();

    Ok(Ast {
        actors: actors.into_vec(),
        use_cases: use_cases.into_vec(),
        system_boundaries: boundaries.into_vec(),
        relationships,
        notes: notes.into_vec(),
        json_nodes: json_nodes.into_vec(),
        class_defs: class_defs.into_vec(),
        symbols,
        direction,
        relationship_counter,
        note_counter,
        acc_title,
        acc_description,
        graph,
    })
}

fn record_first(first: &mut HashMap<String, usize>, id: &str, offset: usize) {
    match first.get(id) {
        Some(previous) if *previous <= offset => {}
        _ => {
            first.insert(id.to_string(), offset);
        }
    }
}

fn collect_element(
    draft: &DraftElement,
    symbols: &mut OrderedMap<SymbolOrigin>,
    states: &mut OrderedMap<ElementState>,
) -> Result<(), ParseError> {
    let origin = symbols.get(&draft.id).cloned();
    let draft_label = draft.generated.then_some(draft.label.text.as_str());
    if let Some(origin) = &origin {
        if origin.kind != symbol_kind(draft.kind) {
            return Err(conflict(
                format!(
                    "ID '{}' is declared as both {} and {}",
                    draft.id,
                    origin.kind.as_str(),
                    draft.kind.as_str()
                ),
                &draft.location,
                &origin.location,
                draft_label,
                generated_from(origin),
            ));
        }
        if origin.generated || draft.generated {
            return Err(conflict(
                format!(
                    "Generated ID '{}' collides with another declaration",
                    draft.id
                ),
                &draft.location,
                &origin.location,
                draft_label,
                generated_from(origin),
            ));
        }
    }

    let Some(existing) = states.get(&draft.id).cloned() else {
        let mut state = ElementState {
            kind: draft.kind,
            id: draft.id.clone(),
            label: draft.label.clone(),
            location: draft.location,
            generated: draft.generated,
            parent_id: draft.parent_id.clone(),
            parent_location: draft.parent_location,
            shape: draft.shape,
            stereotype: draft.stereotype.clone(),
            stereotype_location: draft.stereotype.as_ref().map(|_| draft.location),
            classes: draft.classes.clone(),
            actor_type: None,
            icon: None,
            business: None,
        };
        apply_declaration_metadata(&mut state, draft.metadata.as_ref())?;
        states.insert(draft.id.clone(), state);
        symbols.insert(
            draft.id.clone(),
            SymbolOrigin {
                kind: symbol_kind(draft.kind),
                location: draft.location,
                generated: draft.generated,
                label: Some(draft.label.text.clone()),
            },
        );
        return Ok(());
    };

    if existing.label.text != draft.label.text || existing.label.type_ != draft.label.type_ {
        return Err(conflict(
            format!("ID '{}' has conflicting labels", draft.id),
            &draft.location,
            &existing.location,
            None,
            None,
        ));
    }
    if let Some(shape) = draft.shape
        && let Some(existing_shape) = existing.shape
        && shape != existing_shape
    {
        return Err(conflict(
            format!("Use case '{}' has conflicting shapes", draft.id),
            &draft.location,
            &existing.location,
            None,
            None,
        ));
    }
    if let (Some(parent), Some(existing_parent)) = (&draft.parent_id, &existing.parent_id)
        && parent != existing_parent
    {
        return Err(conflict(
            format!(
                "Element '{}' belongs to more than one system boundary",
                draft.id
            ),
            draft.parent_location.as_ref().unwrap_or(&draft.location),
            existing
                .parent_location
                .as_ref()
                .unwrap_or(&existing.location),
            None,
            None,
        ));
    }
    if let (Some(stereotype), Some(existing_stereotype)) = (&draft.stereotype, &existing.stereotype)
        && stereotype != existing_stereotype
    {
        return Err(conflict(
            format!("Element '{}' has conflicting stereotypes", draft.id),
            &draft.location,
            existing
                .stereotype_location
                .as_ref()
                .unwrap_or(&existing.location),
            None,
            None,
        ));
    }

    let mut state = existing;
    state.shape = state.shape.or(draft.shape);
    state.parent_id = state.parent_id.or_else(|| draft.parent_id.clone());
    state.parent_location = state.parent_location.or(draft.parent_location);
    state.stereotype = state.stereotype.or_else(|| draft.stereotype.clone());
    state.stereotype_location = state.stereotype_location.or(if draft.stereotype.is_some() {
        Some(draft.location)
    } else {
        None
    });
    push_unique(&mut state.classes, &draft.classes);
    apply_declaration_metadata(&mut state, draft.metadata.as_ref())?;
    states.insert(draft.id.clone(), state);
    Ok(())
}

fn symbol_kind(kind: ElementKind) -> UsecaseSymbolKind {
    match kind {
        ElementKind::Actor => UsecaseSymbolKind::Actor,
        ElementKind::Usecase => UsecaseSymbolKind::Usecase,
    }
}

fn collect_boundary(
    draft: &DraftBoundary,
    symbols: &mut OrderedMap<SymbolOrigin>,
    states: &mut OrderedMap<BoundaryState>,
) -> Result<(), ParseError> {
    let origin = symbols.get(&draft.id).cloned();
    let draft_label = draft.generated.then_some(draft.label.text.as_str());
    if let Some(origin) = &origin {
        if origin.kind != UsecaseSymbolKind::Boundary {
            return Err(conflict(
                format!(
                    "ID '{}' is declared as both {} and boundary",
                    draft.id,
                    origin.kind.as_str()
                ),
                &draft.location,
                &origin.location,
                draft_label,
                generated_from(origin),
            ));
        }
        if origin.generated || draft.generated {
            return Err(conflict(
                format!(
                    "Generated ID '{}' collides with another declaration",
                    draft.id
                ),
                &draft.location,
                &origin.location,
                draft_label,
                generated_from(origin),
            ));
        }
    }

    if let Some(existing) = states.get(&draft.id).cloned() {
        if existing.label.text != draft.label.text || existing.label.type_ != draft.label.type_ {
            return Err(conflict(
                format!("Boundary '{}' has conflicting titles", draft.id),
                &draft.location,
                &existing.location,
                None,
                None,
            ));
        }
        let mut existing = existing;
        push_unique(&mut existing.classes, &draft.classes);
        states.insert(draft.id.clone(), existing);
        return Ok(());
    }

    states.insert(
        draft.id.clone(),
        BoundaryState {
            id: draft.id.clone(),
            label: draft.label.clone(),
            location: draft.location,
            generated: draft.generated,
            type_: None,
            classes: draft.classes.clone(),
            styles: Vec::new(),
            members: Vec::new(),
        },
    );
    symbols.insert(
        draft.id.clone(),
        SymbolOrigin {
            kind: UsecaseSymbolKind::Boundary,
            location: draft.location,
            generated: draft.generated,
            label: Some(draft.label.text.clone()),
        },
    );
    Ok(())
}

fn register_unique(
    symbols: &mut OrderedMap<SymbolOrigin>,
    id: &str,
    kind: UsecaseSymbolKind,
    location: &DraftLocation,
    generated: bool,
) -> Result<(), ParseError> {
    if let Some(previous) = symbols.get(id).cloned() {
        return Err(conflict(
            format!(
                "ID '{id}' is declared more than once ({} and {})",
                previous.kind.as_str(),
                kind.as_str()
            ),
            location,
            &previous.location,
            None,
            generated_from(&previous),
        ));
    }
    symbols.insert(
        id.to_string(),
        SymbolOrigin {
            kind,
            location: *location,
            generated,
            label: None,
        },
    );
    Ok(())
}

fn sorted_states<'a, T>(
    states: &'a OrderedMap<T>,
    first: &HashMap<String, usize>,
    id: impl Fn(&T) -> &str,
) -> Vec<&'a T> {
    let mut values: Vec<&T> = states.values().collect();
    values.sort_by_key(|state| first.get(id(state)).copied().unwrap_or(0));
    values
}

fn materialize_elements(
    states: &OrderedMap<ElementState>,
    first: &HashMap<String, usize>,
    actors: &mut OrderedMap<Actor>,
    use_cases: &mut OrderedMap<UseCase>,
) {
    for state in sorted_states(states, first, |state| &state.id) {
        set_element_model(state, actors, use_cases);
    }
}

fn materialize_boundaries(
    states: &OrderedMap<BoundaryState>,
    first: &HashMap<String, usize>,
    boundaries: &mut OrderedMap<SystemBoundary>,
) {
    for state in sorted_states(states, first, |state| &state.id) {
        boundaries.insert(
            state.id.clone(),
            SystemBoundary {
                id: state.id.clone(),
                label: state.label.text.clone(),
                label_type: state.label.type_,
                type_: state.type_.unwrap_or_default(),
                members: Vec::new(),
                classes: state.classes.clone(),
                styles: state.styles.clone(),
            },
        );
    }
}

fn materialize_json(
    drafts: &[DraftJson],
    first: &HashMap<String, usize>,
    json_nodes: &mut OrderedMap<UsecaseJsonNode>,
) {
    let mut drafts: Vec<&DraftJson> = drafts.iter().collect();
    drafts.sort_by_key(|draft| first.get(&draft.id).copied().unwrap_or(0));
    for draft in drafts {
        json_nodes.insert(
            draft.id.clone(),
            UsecaseJsonNode {
                id: draft.id.clone(),
                value: draft.value.clone(),
                property_order: draft.property_order.clone(),
                classes: draft.classes.clone(),
                styles: Vec::new(),
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn materialize_relationships(
    drafts: &[DraftRelationship],
    symbols: &mut OrderedMap<SymbolOrigin>,
    states: &mut OrderedMap<ElementState>,
    first: &mut HashMap<String, usize>,
    relationships: &mut Vec<Relationship>,
    edges: &mut OrderedMap<EdgeState>,
    anonymous: &mut usize,
) -> Result<(), ParseError> {
    for draft in drafts {
        let source_kind = resolve_endpoint(&draft.source, symbols, states, first)?;
        let target_kind = resolve_endpoint(&draft.target, symbols, states, first)?;
        validate_relationship(draft, source_kind, target_kind, symbols)?;
        let id = draft
            .explicit_id
            .clone()
            .unwrap_or_else(|| format!("edge-{anonymous}"));
        if draft.explicit_id.is_none() {
            *anonymous += 1;
        }
        let relationship = Relationship {
            id: id.clone(),
            explicit_id: draft.explicit_id.is_some(),
            source: draft.source.id.clone(),
            target: draft.target.id.clone(),
            type_: draft.type_,
            arrow_type: draft.arrow_type,
            label: draft.label.as_ref().map(|label| label.text.clone()),
            label_type: draft.label.as_ref().map(|label| label.type_),
            minlen: draft.minlen,
            classes: Vec::new(),
            styles: Vec::new(),
            animate: false,
            animation: None,
        };
        relationships.push(relationship);
        edges.insert(
            id,
            EdgeState {
                relationship_index: relationships.len() - 1,
            },
        );
    }
    Ok(())
}

fn resolve_endpoint(
    endpoint: &DraftEndpoint,
    symbols: &mut OrderedMap<SymbolOrigin>,
    states: &mut OrderedMap<ElementState>,
    first: &mut HashMap<String, usize>,
) -> Result<UsecaseSymbolKind, ParseError> {
    if endpoint.classes_on_reference && !endpoint.declaration {
        return Err(ParseError::at(
            format!(
                "Relationship endpoint '{}' uses ::: without declaring the node at {}",
                endpoint.id,
                location_text(&endpoint.location)
            ),
            endpoint.location.span,
        ));
    }
    if let Some(origin) = symbols.get(&endpoint.id) {
        return Ok(origin.kind);
    }
    states.insert(
        endpoint.id.clone(),
        ElementState {
            kind: ElementKind::Usecase,
            id: endpoint.id.clone(),
            label: endpoint.label.clone(),
            location: endpoint.location,
            generated: endpoint.generated,
            parent_id: None,
            parent_location: None,
            shape: Some(UseCaseShape::Ellipse),
            stereotype: None,
            stereotype_location: None,
            classes: Vec::new(),
            actor_type: None,
            icon: None,
            business: None,
        },
    );
    symbols.insert(
        endpoint.id.clone(),
        SymbolOrigin {
            kind: UsecaseSymbolKind::Usecase,
            location: endpoint.location,
            generated: endpoint.generated,
            label: Some(endpoint.label.text.clone()),
        },
    );
    record_first(first, &endpoint.id, endpoint.location.span.start);
    Ok(UsecaseSymbolKind::Usecase)
}

fn reorder_elements(
    states: &OrderedMap<ElementState>,
    first: &HashMap<String, usize>,
    actors: &mut OrderedMap<Actor>,
    use_cases: &mut OrderedMap<UseCase>,
) {
    let previous_actors = actors.clone();
    let previous_use_cases = use_cases.clone();
    actors.clear();
    use_cases.clear();
    for state in sorted_states(states, first, |state| &state.id) {
        if !previous_actors.contains(&state.id) && !previous_use_cases.contains(&state.id) {
            set_element_model(state, actors, use_cases);
        }
        let actor = previous_actors
            .get(&state.id)
            .cloned()
            .or_else(|| actors.get(&state.id).cloned());
        if let Some(actor) = actor {
            actors.insert(state.id.clone(), actor);
            continue;
        }
        let use_case = previous_use_cases
            .get(&state.id)
            .cloned()
            .or_else(|| use_cases.get(&state.id).cloned());
        if let Some(use_case) = use_case {
            use_cases.insert(state.id.clone(), use_case);
        }
    }
}

fn apply_metadata_assignments(
    assignments: &[DraftMetadataAssignment],
    symbols: &OrderedMap<SymbolOrigin>,
    elements: &mut OrderedMap<ElementState>,
    boundaries: &mut OrderedMap<BoundaryState>,
    edges: &OrderedMap<EdgeState>,
    relationships: &mut [Relationship],
    arena: &mut [StatementNode],
) -> Result<(), ParseError> {
    for assignment in assignments {
        let Some(origin) = symbols.get(&assignment.target).cloned() else {
            let inferred = infer_metadata_kind(&assignment.metadata);
            let suffix = inferred
                .map(|kind| format!(" (metadata implies {kind})"))
                .unwrap_or_default();
            return Err(ParseError::at(
                format!(
                    "Metadata target '{}' is unresolved{} at {}",
                    assignment.target,
                    suffix,
                    location_text(&assignment.target_location)
                ),
                assignment.target_location.span,
            ));
        };
        match origin.kind {
            UsecaseSymbolKind::Actor | UsecaseSymbolKind::Usecase => {
                let state = elements
                    .get_mut(&assignment.target)
                    .expect("a resolved element has state");
                apply_standalone_element_metadata(state, &assignment.metadata)?;
            }
            UsecaseSymbolKind::Boundary => {
                let boundary = boundaries
                    .get_mut(&assignment.target)
                    .expect("a resolved boundary has state");
                for property in &assignment.metadata.properties {
                    if property.key != "type"
                        || !matches!(property.value.as_str(), Some("rect") | Some("package"))
                    {
                        return Err(invalid_metadata(&assignment.target, origin.kind, property));
                    }
                    boundary.type_ = Some(if property.value.as_str() == Some("package") {
                        BoundaryType::Package
                    } else {
                        BoundaryType::Rect
                    });
                }
            }
            UsecaseSymbolKind::Edge => {
                let Some(edge) = edges.get(&assignment.target) else {
                    return Err(ParseError::at(
                        format!(
                            "Metadata target '{}' is not an explicit edge at {}",
                            assignment.target,
                            location_text(&assignment.target_location)
                        ),
                        assignment.target_location.span,
                    ));
                };
                let relationship = &mut relationships[edge.relationship_index];
                let statement = &mut arena[assignment.statement].statement;
                statement.kind = StatementKind::EdgeMetadata;
                let metadata = statement.metadata.clone();
                statement.edges = vec![EdgeOccurrence {
                    id: assignment.target.clone(),
                    span: statement.span,
                    id_span: Some(assignment.target_location.span),
                    metadata,
                    ..EdgeOccurrence::default()
                }];
                statement.nodes.clear();
                for property in &assignment.metadata.properties {
                    if property.key == "animate" {
                        if let Some(value) = property.value.as_bool() {
                            relationship.animate = value;
                            continue;
                        }
                    } else if property.key == "animation"
                        && let Some(value) = property.value.as_str()
                        && (value == "fast" || value == "slow")
                    {
                        relationship.animation = Some(if value == "fast" {
                            Animation::Fast
                        } else {
                            Animation::Slow
                        });
                        relationship.animate = true;
                        continue;
                    }
                    return Err(invalid_metadata(&assignment.target, origin.kind, property));
                }
            }
            UsecaseSymbolKind::Json => {
                if let Some(property) = assignment.metadata.properties.first() {
                    return Err(invalid_metadata(&assignment.target, origin.kind, property));
                }
            }
        }
    }
    for relationship in relationships.iter_mut() {
        if relationship.animation.is_some() {
            relationship.animate = true;
        }
    }
    Ok(())
}

fn apply_standalone_element_metadata(
    state: &mut ElementState,
    metadata: &DraftMetadata,
) -> Result<(), ParseError> {
    for property in &metadata.properties {
        if state.kind == ElementKind::Actor {
            apply_actor_property(state, property, true)?;
        } else if property.key == "business" {
            if let Some(value) = property.value.as_bool() {
                state.business = Some(value);
            } else {
                return Err(invalid_metadata(
                    &state.id,
                    symbol_kind(state.kind),
                    property,
                ));
            }
        } else {
            return Err(invalid_metadata(
                &state.id,
                symbol_kind(state.kind),
                property,
            ));
        }
    }
    Ok(())
}

fn apply_declaration_metadata(
    state: &mut ElementState,
    metadata: Option<&DraftMetadata>,
) -> Result<(), ParseError> {
    let Some(metadata) = metadata else {
        return Ok(());
    };
    for property in &metadata.properties {
        if state.kind == ElementKind::Actor {
            apply_actor_property(state, property, false)?;
        } else if property.key == "business" {
            if let Some(value) = property.value.as_bool() {
                if state.business.is_some() && state.business != Some(value) {
                    return Err(conflict(
                        format!("Use case '{}' has conflicting business metadata", state.id),
                        &property.location,
                        &state.location,
                        None,
                        None,
                    ));
                }
                state.business = Some(value);
            } else {
                return Err(invalid_metadata(
                    &state.id,
                    symbol_kind(state.kind),
                    property,
                ));
            }
        } else {
            return Err(invalid_metadata(
                &state.id,
                symbol_kind(state.kind),
                property,
            ));
        }
    }
    Ok(())
}

fn apply_actor_property(
    state: &mut ElementState,
    property: &DraftMetadataProperty,
    replace: bool,
) -> Result<(), ParseError> {
    if property.key == "type" {
        if let Some(value) = property.value.as_str() {
            let actor_type = match value {
                "normal" => Some(ActorType::Normal),
                "hollow" => Some(ActorType::Hollow),
                "awesome" => Some(ActorType::Awesome),
                _ => None,
            };
            if let Some(actor_type) = actor_type {
                if !replace && state.actor_type.is_some() && state.actor_type != Some(actor_type) {
                    return Err(conflict(
                        format!("Actor '{}' has conflicting type metadata", state.id),
                        &property.location,
                        &state.location,
                        None,
                        None,
                    ));
                }
                state.actor_type = Some(actor_type);
                return Ok(());
            }
        }
    } else if property.key == "icon" {
        if let Some(value) = property.value.as_str() {
            if !replace && state.icon.is_some() && state.icon.as_deref() != Some(value) {
                return Err(conflict(
                    format!("Actor '{}' has conflicting icon metadata", state.id),
                    &property.location,
                    &state.location,
                    None,
                    None,
                ));
            }
            state.icon = Some(value.to_string());
            return Ok(());
        }
    } else if property.key == "business"
        && let Some(value) = property.value.as_bool()
    {
        if !replace && state.business.is_some() && state.business != Some(value) {
            return Err(conflict(
                format!("Actor '{}' has conflicting business metadata", state.id),
                &property.location,
                &state.location,
                None,
                None,
            ));
        }
        state.business = Some(value);
        return Ok(());
    }
    Err(invalid_metadata(
        &state.id,
        UsecaseSymbolKind::Actor,
        property,
    ))
}

fn invalid_metadata(
    id: &str,
    kind: UsecaseSymbolKind,
    property: &DraftMetadataProperty,
) -> ParseError {
    ParseError::at(
        format!(
            "Metadata property '{}' is invalid for {} '{}' at {}",
            property.key,
            kind.as_str(),
            id,
            location_text(&property.location)
        ),
        property.location.span,
    )
}

fn infer_metadata_kind(metadata: &DraftMetadata) -> Option<&'static str> {
    let mut actor = true;
    let mut usecase = true;
    let mut boundary = true;
    let mut edge = true;
    for property in &metadata.properties {
        if property.key == "icon" {
            usecase = false;
            boundary = false;
            edge = false;
        } else if property.key == "animate" || property.key == "animation" {
            actor = false;
            usecase = false;
            boundary = false;
        } else if property.key == "type" {
            actor = false;
            usecase = false;
            boundary = false;
            match property.value.as_str() {
                Some("rect") | Some("package") => boundary = true,
                Some("normal") | Some("hollow") | Some("awesome") => actor = true,
                _ => {}
            }
        } else if property.key == "business" {
            boundary = false;
            edge = false;
        } else {
            return None;
        }
    }
    let possible = [
        (actor, "actor"),
        (usecase, "usecase"),
        (boundary, "boundary"),
        (edge, "edge"),
    ];
    let matches: Vec<&str> = possible
        .iter()
        .filter(|(possible, _)| *possible)
        .map(|(_, kind)| *kind)
        .collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

fn validate_and_refresh_elements(
    states: &OrderedMap<ElementState>,
    actors: &mut OrderedMap<Actor>,
    use_cases: &mut OrderedMap<UseCase>,
) -> Result<(), ParseError> {
    for state in states.values() {
        let type_ = if state.icon.is_some() {
            ActorType::Icon
        } else {
            state.actor_type.unwrap_or_default()
        };
        if state.kind == ElementKind::Actor {
            if state.icon.is_some()
                && state.actor_type.is_some()
                && state.actor_type != Some(ActorType::Normal)
            {
                return Err(ParseError::at(
                    format!(
                        "Actor '{}' cannot combine icon with type '{}' at {}",
                        state.id,
                        state.actor_type.expect("checked above").as_str(),
                        location_text(&state.location)
                    ),
                    state.location.span,
                ));
            }
            if state.business == Some(true)
                && (type_ == ActorType::Icon || type_ == ActorType::Awesome)
            {
                return Err(ParseError::at(
                    format!(
                        "Business actor '{}' must use normal or hollow geometry at {}",
                        state.id,
                        location_text(&state.location)
                    ),
                    state.location.span,
                ));
            }
        } else if state.shape.unwrap_or_default() == UseCaseShape::Rect
            && state.business == Some(true)
        {
            return Err(ParseError::at(
                format!(
                    "Rectangular use case '{}' cannot be a business use case at {}",
                    state.id,
                    location_text(&state.location)
                ),
                state.location.span,
            ));
        }
        set_element_model(state, actors, use_cases);
    }
    Ok(())
}

fn refresh_boundaries(
    element_drafts: &[DraftElement],
    elements: &OrderedMap<ElementState>,
    states: &mut OrderedMap<BoundaryState>,
    boundaries: &mut OrderedMap<SystemBoundary>,
) -> Result<(), ParseError> {
    for boundary in states.entries.iter_mut() {
        boundary.1.members.clear();
    }
    let mut drafts: Vec<&DraftElement> = element_drafts.iter().collect();
    drafts.sort_by_key(|draft| draft.location.span.start);
    for draft in drafts {
        let Some(parent_id) = &draft.parent_id else {
            continue;
        };
        let Some(boundary) = states.get_mut(parent_id) else {
            return Err(ParseError::at(
                format!(
                    "Parent boundary '{}' for '{}' is unresolved at {}",
                    parent_id,
                    draft.id,
                    location_text(draft.parent_location.as_ref().unwrap_or(&draft.location))
                ),
                draft
                    .parent_location
                    .as_ref()
                    .unwrap_or(&draft.location)
                    .span,
            ));
        };
        if !boundary.members.contains(&draft.id) {
            boundary.members.push(draft.id.clone());
        }
    }
    for state in elements.values() {
        if let Some(parent_id) = &state.parent_id
            && !states.contains(parent_id)
        {
            return Err(ParseError::at(
                format!(
                    "Parent boundary '{}' for '{}' is unresolved at {}",
                    parent_id,
                    state.id,
                    location_text(state.parent_location.as_ref().unwrap_or(&state.location))
                ),
                state
                    .parent_location
                    .as_ref()
                    .unwrap_or(&state.location)
                    .span,
            ));
        }
    }
    for state in states.values() {
        let model = boundaries
            .get_mut(&state.id)
            .expect("every boundary state has a model entry");
        model.type_ = state.type_.unwrap_or_default();
        model.members = state.members.clone();
        model.classes = state.classes.clone();
        model.styles = state.styles.clone();
    }
    Ok(())
}

fn materialize_notes(
    drafts: &[DraftNote],
    symbols: &OrderedMap<SymbolOrigin>,
    notes: &mut OrderedMap<UsecaseNote>,
    counter: &mut usize,
) -> Result<(), ParseError> {
    for draft in drafts {
        let Some(origin) = symbols.get(&draft.target) else {
            return Err(ParseError::at(
                format!(
                    "Note target '{}' is unresolved at {}",
                    draft.target,
                    location_text(&draft.target_location)
                ),
                draft.target_location.span,
            ));
        };
        if origin.kind != UsecaseSymbolKind::Actor && origin.kind != UsecaseSymbolKind::Usecase {
            return Err(conflict(
                format!(
                    "Note target '{}' must be an actor or use case, not {}",
                    draft.target,
                    origin.kind.as_str()
                ),
                &draft.target_location,
                &origin.location,
                None,
                None,
            ));
        }
        let id = format!("note-{counter}");
        *counter += 1;
        notes.insert(
            id.clone(),
            UsecaseNote {
                id,
                target: draft.target.clone(),
                label: draft.label.text.clone(),
                label_type: draft.label.type_,
            },
        );
    }
    Ok(())
}

fn apply_class_definitions(
    definitions: &[DraftClassDefinition],
    class_defs: &mut OrderedMap<ClassDef>,
) {
    for definition in definitions {
        for id in &definition.ids {
            class_defs.insert(
                id.clone(),
                ClassDef {
                    id: id.clone(),
                    styles: definition.styles.clone(),
                },
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_classes_and_styles(
    class_assignments: &[DraftClassAssignment],
    style_assignments: &[DraftStyleAssignment],
    symbols: &OrderedMap<SymbolOrigin>,
    actors: &mut OrderedMap<Actor>,
    use_cases: &mut OrderedMap<UseCase>,
    boundaries: &mut OrderedMap<SystemBoundary>,
    json_nodes: &mut OrderedMap<UsecaseJsonNode>,
    edges: &OrderedMap<EdgeState>,
    relationships: &mut [Relationship],
) -> Result<(), ParseError> {
    for assignment in class_assignments {
        for (id, location) in &assignment.targets {
            let mut target = get_stylable(
                id,
                symbols,
                actors,
                use_cases,
                boundaries,
                json_nodes,
                edges,
                relationships,
                location,
            )?;
            push_unique(target.classes(), &assignment.classes);
        }
    }
    for assignment in style_assignments {
        let mut target = get_stylable(
            &assignment.target,
            symbols,
            actors,
            use_cases,
            boundaries,
            json_nodes,
            edges,
            relationships,
            &assignment.location,
        )?;
        target.styles().extend(assignment.styles.clone());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn get_stylable<'a>(
    id: &str,
    symbols: &OrderedMap<SymbolOrigin>,
    actors: &'a mut OrderedMap<Actor>,
    use_cases: &'a mut OrderedMap<UseCase>,
    boundaries: &'a mut OrderedMap<SystemBoundary>,
    json_nodes: &'a mut OrderedMap<UsecaseJsonNode>,
    edges: &OrderedMap<EdgeState>,
    relationships: &'a mut [Relationship],
    location: &DraftLocation,
) -> Result<Stylable<'a>, ParseError> {
    let kind = symbols.get(id).map(|origin| origin.kind);
    let target = match kind {
        Some(UsecaseSymbolKind::Actor) => actors.get_mut(id).map(Stylable::Actor),
        Some(UsecaseSymbolKind::Usecase) => use_cases.get_mut(id).map(Stylable::UseCase),
        Some(UsecaseSymbolKind::Boundary) => boundaries.get_mut(id).map(Stylable::Boundary),
        Some(UsecaseSymbolKind::Json) => json_nodes.get_mut(id).map(Stylable::Json),
        Some(UsecaseSymbolKind::Edge) => edges
            .get(id)
            .map(|edge| Stylable::Relationship(&mut relationships[edge.relationship_index])),
        None => None,
    };
    target.ok_or_else(|| {
        ParseError::at(
            format!(
                "Class/style target '{}' is unresolved or anonymous at {}",
                id,
                location_text(location)
            ),
            location.span,
        )
    })
}

/// The four model shapes a class or style assignment can reach.
enum Stylable<'a> {
    Actor(&'a mut Actor),
    UseCase(&'a mut UseCase),
    Boundary(&'a mut SystemBoundary),
    Json(&'a mut UsecaseJsonNode),
    Relationship(&'a mut Relationship),
}

impl Stylable<'_> {
    fn classes(&mut self) -> &mut Vec<String> {
        match self {
            Stylable::Actor(actor) => &mut actor.classes,
            Stylable::UseCase(use_case) => &mut use_case.classes,
            Stylable::Boundary(boundary) => &mut boundary.classes,
            Stylable::Json(json) => &mut json.classes,
            Stylable::Relationship(relationship) => &mut relationship.classes,
        }
    }

    fn styles(&mut self) -> &mut Vec<String> {
        match self {
            Stylable::Actor(actor) => &mut actor.styles,
            Stylable::UseCase(use_case) => &mut use_case.styles,
            Stylable::Boundary(boundary) => &mut boundary.styles,
            Stylable::Json(json) => &mut json.styles,
            Stylable::Relationship(relationship) => &mut relationship.styles,
        }
    }
}

fn validate_relationship(
    draft: &DraftRelationship,
    source_kind: UsecaseSymbolKind,
    target_kind: UsecaseSymbolKind,
    symbols: &OrderedMap<SymbolOrigin>,
) -> Result<(), ParseError> {
    let allowed = |kind: UsecaseSymbolKind| {
        matches!(
            kind,
            UsecaseSymbolKind::Actor | UsecaseSymbolKind::Usecase | UsecaseSymbolKind::Json
        )
    };
    if !allowed(source_kind) {
        return Err(conflict(
            format!(
                "Relationship source '{}' cannot be {}",
                draft.source.id,
                source_kind.as_str()
            ),
            &draft.source.location,
            &symbols
                .get(&draft.source.id)
                .expect("the source resolved")
                .location,
            None,
            None,
        ));
    }
    if !allowed(target_kind) {
        return Err(conflict(
            format!(
                "Relationship target '{}' cannot be {}",
                draft.target.id,
                target_kind.as_str()
            ),
            &draft.target.location,
            &symbols
                .get(&draft.target.id)
                .expect("the target resolved")
                .location,
            None,
            None,
        ));
    }
    if matches!(
        draft.type_,
        RelationshipType::Include | RelationshipType::Extend
    ) && (source_kind != UsecaseSymbolKind::Usecase || target_kind != UsecaseSymbolKind::Usecase)
    {
        return Err(ParseError::at(
            format!(
                "{} relationship requires use-case endpoints at {}",
                draft.type_.as_str(),
                location_text(&draft.location)
            ),
            draft.location.span,
        ));
    }
    if draft.type_ == RelationshipType::Generalization
        && ((source_kind != UsecaseSymbolKind::Actor && source_kind != UsecaseSymbolKind::Usecase)
            || source_kind != target_kind)
    {
        return Err(ParseError::at(
            format!(
                "Generalization requires actor-to-actor or use-case-to-use-case endpoints at {}",
                location_text(&draft.location)
            ),
            draft.location.span,
        ));
    }
    if draft.type_ == RelationshipType::Association
        && (source_kind == UsecaseSymbolKind::Json || target_kind == UsecaseSymbolKind::Json)
        && !matches!(
            draft.arrow_type,
            ArrowType::SolidArrow | ArrowType::BackArrow | ArrowType::LineSolid
        )
    {
        return Err(ParseError::at(
            format!(
                "JSON relationship '{}' to '{}' permits only point, reversed-point, or markerless solid association at {}",
                draft.source.id,
                draft.target.id,
                location_text(&draft.location)
            ),
            draft.location.span,
        ));
    }
    Ok(())
}

fn set_element_model(
    state: &ElementState,
    actors: &mut OrderedMap<Actor>,
    use_cases: &mut OrderedMap<UseCase>,
) {
    if state.kind == ElementKind::Actor {
        let type_ = if state.icon.is_some() {
            ActorType::Icon
        } else {
            state.actor_type.unwrap_or_default()
        };
        use_cases.remove(&state.id);
        let styles = actors
            .get(&state.id)
            .map(|actor| actor.styles.clone())
            .unwrap_or_default();
        actors.insert(
            state.id.clone(),
            Actor {
                id: state.id.clone(),
                label: state.label.text.clone(),
                label_type: state.label.type_,
                type_,
                icon: state.icon.clone(),
                business: state.business.unwrap_or(false),
                stereotype: state.stereotype.clone(),
                parent_id: state.parent_id.clone(),
                classes: state.classes.clone(),
                styles,
            },
        );
    } else {
        actors.remove(&state.id);
        let styles = use_cases
            .get(&state.id)
            .map(|use_case| use_case.styles.clone())
            .unwrap_or_default();
        use_cases.insert(
            state.id.clone(),
            UseCase {
                id: state.id.clone(),
                label: state.label.text.clone(),
                label_type: state.label.type_,
                shape: state.shape.unwrap_or_default(),
                business: state.business.unwrap_or(false),
                stereotype: state.stereotype.clone(),
                parent_id: state.parent_id.clone(),
                classes: state.classes.clone(),
                styles,
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn build_graph(
    actors: &OrderedMap<Actor>,
    use_cases: &OrderedMap<UseCase>,
    boundaries: &OrderedMap<SystemBoundary>,
    notes: &OrderedMap<UsecaseNote>,
    json_nodes: &OrderedMap<UsecaseJsonNode>,
    class_defs: &OrderedMap<ClassDef>,
    relationships: &[Relationship],
    source: &str,
    header_span: Span,
    acc_title: &str,
    acc_description: &str,
    direction: Direction,
    arena: &[StatementNode],
    statements: &[usize],
) -> GraphAst {
    let mut nodes: Vec<(String, GraphNode)> = Vec::new();
    for actor in actors.values() {
        nodes.push((actor.id.clone(), actor_node(actor)));
    }
    for use_case in use_cases.values() {
        nodes.push((use_case.id.clone(), use_case_node(use_case)));
    }
    for note in notes.values() {
        nodes.push((note.id.clone(), note_node(note)));
    }
    for json in json_nodes.values() {
        nodes.push((json.id.clone(), json_node(json)));
    }

    let mut groups: Vec<(String, GraphGroup)> = Vec::new();
    for boundary in boundaries.values() {
        let mut attrs = Map::new();
        attrs.insert("kind".into(), Value::String("systemBoundary".into()));
        attrs.insert(
            "boundaryType".into(),
            Value::String(boundary.type_.as_str().into()),
        );
        attrs.insert(
            "labelType".into(),
            Value::String(boundary.label_type.as_str().into()),
        );
        groups.push((
            boundary.id.clone(),
            GraphGroup {
                title: (boundary.label != boundary.id).then(|| boundary.label.clone()),
                nodes: boundary.members.clone(),
                classes: boundary.classes.clone(),
                styles: boundary.styles.clone(),
                attrs: Value::Object(attrs),
            },
        ));
    }

    let mut edges: Vec<GraphEdge> = relationships.iter().map(relationship_edge).collect();
    for note in notes.values() {
        edges.push(note_edge(note));
    }

    let final_statements: Vec<GraphStatement> = statements
        .iter()
        .map(|index| materialize_statement(arena, *index))
        .collect();

    GraphAst {
        version: 1,
        diagram_type: "usecase".into(),
        source: source.to_string(),
        header: GraphHeader {
            keyword: "usecase".into(),
            direction,
            span: header_span,
        },
        acc_title: (!acc_title.is_empty()).then(|| acc_title.to_string()),
        acc_descr: (!acc_description.is_empty()).then(|| acc_description.to_string()),
        nodes,
        edges,
        groups,
        class_defs: class_defs.values().cloned().collect(),
        statements: final_statements,
    }
}

fn materialize_statement(arena: &[StatementNode], index: usize) -> GraphStatement {
    let node = &arena[index];
    let mut statement = node.statement.clone();
    statement.children = node
        .children
        .iter()
        .map(|child| materialize_statement(arena, *child))
        .collect();
    statement
}

fn actor_node(actor: &Actor) -> GraphNode {
    let shape = match actor.type_ {
        ActorType::Normal => "actor",
        ActorType::Hollow => "actor-hollow",
        ActorType::Awesome => "actor-awesome",
        ActorType::Icon => "actor-icon",
    };
    let mut attrs = Map::new();
    attrs.insert("kind".into(), Value::String("actor".into()));
    attrs.insert(
        "actorType".into(),
        Value::String(actor.type_.as_str().into()),
    );
    attrs.insert("business".into(), Value::Bool(actor.business));
    attrs.insert(
        "labelType".into(),
        Value::String(actor.label_type.as_str().into()),
    );
    if let Some(icon) = &actor.icon {
        attrs.insert("icon".into(), Value::String(icon.clone()));
    }
    if let Some(stereotype) = &actor.stereotype {
        attrs.insert("stereotype".into(), Value::String(stereotype.clone()));
    }
    if let Some(parent_id) = &actor.parent_id {
        attrs.insert("parentId".into(), Value::String(parent_id.clone()));
    }
    GraphNode {
        label: (actor.label != actor.id).then(|| actor.label.clone()),
        shape: shape.into(),
        classes: actor.classes.clone(),
        styles: actor.styles.clone(),
        attrs: Value::Object(attrs),
    }
}

fn use_case_node(use_case: &UseCase) -> GraphNode {
    let mut attrs = Map::new();
    attrs.insert("kind".into(), Value::String("usecase".into()));
    attrs.insert(
        "useCaseShape".into(),
        Value::String(use_case.shape.as_str().into()),
    );
    attrs.insert("business".into(), Value::Bool(use_case.business));
    attrs.insert(
        "labelType".into(),
        Value::String(use_case.label_type.as_str().into()),
    );
    if let Some(stereotype) = &use_case.stereotype {
        attrs.insert("stereotype".into(), Value::String(stereotype.clone()));
    }
    if let Some(parent_id) = &use_case.parent_id {
        attrs.insert("parentId".into(), Value::String(parent_id.clone()));
    }
    GraphNode {
        label: (use_case.label != use_case.id).then(|| use_case.label.clone()),
        shape: use_case.shape.as_str().into(),
        classes: use_case.classes.clone(),
        styles: use_case.styles.clone(),
        attrs: Value::Object(attrs),
    }
}

fn note_node(note: &UsecaseNote) -> GraphNode {
    let mut attrs = Map::new();
    attrs.insert("kind".into(), Value::String("note".into()));
    attrs.insert("target".into(), Value::String(note.target.clone()));
    attrs.insert(
        "labelType".into(),
        Value::String(note.label_type.as_str().into()),
    );
    GraphNode {
        label: Some(note.label.clone()),
        shape: "note".into(),
        classes: Vec::new(),
        styles: Vec::new(),
        attrs: Value::Object(attrs),
    }
}

fn json_node(json: &UsecaseJsonNode) -> GraphNode {
    let mut attrs = Map::new();
    attrs.insert("kind".into(), Value::String("json".into()));
    attrs.insert("value".into(), json.value.clone());
    attrs.insert(
        "propertyOrder".into(),
        Value::Object(json.property_order.clone()),
    );
    attrs.insert("labelType".into(), Value::String("text".into()));
    GraphNode {
        label: Some(json.id.clone()),
        shape: "json-table".into(),
        classes: json.classes.clone(),
        styles: json.styles.clone(),
        attrs: Value::Object(attrs),
    }
}

fn relationship_edge(relationship: &Relationship) -> GraphEdge {
    let mut attrs = Map::new();
    attrs.insert(
        "relationshipType".into(),
        Value::String(relationship.type_.as_str().into()),
    );
    attrs.insert(
        "arrowType".into(),
        Value::Number(relationship.arrow_type.code().into()),
    );
    attrs.insert("minlen".into(), Value::Number(relationship.minlen.into()));
    attrs.insert("explicitId".into(), Value::Bool(relationship.explicit_id));
    attrs.insert("animate".into(), Value::Bool(relationship.animate));
    if let Some(animation) = relationship.animation {
        attrs.insert("animation".into(), Value::String(animation.as_str().into()));
    }
    if let Some(label_type) = relationship.label_type {
        attrs.insert(
            "labelType".into(),
            Value::String(label_type.as_str().into()),
        );
    }
    GraphEdge {
        id: relationship.id.clone(),
        source: relationship.source.clone(),
        target: relationship.target.clone(),
        label: relationship.label.clone(),
        classes: relationship.classes.clone(),
        styles: relationship.styles.clone(),
        attrs: Value::Object(attrs),
    }
}

fn note_edge(note: &UsecaseNote) -> GraphEdge {
    let mut attrs = Map::new();
    attrs.insert("relationshipType".into(), Value::String("note".into()));
    attrs.insert(
        "arrowType".into(),
        Value::Number(ArrowType::LineSolid.code().into()),
    );
    attrs.insert("pattern".into(), Value::String("dotted".into()));
    attrs.insert("minlen".into(), Value::Number(1.into()));
    attrs.insert("explicitId".into(), Value::Bool(false));
    attrs.insert("animate".into(), Value::Bool(false));
    attrs.insert("internal".into(), Value::Bool(true));
    GraphEdge {
        id: format!("{}-edge", note.id),
        source: note.id.clone(),
        target: note.target.clone(),
        label: None,
        classes: Vec::new(),
        styles: Vec::new(),
        attrs: Value::Object(attrs),
    }
}
