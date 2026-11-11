//! The wardley database's writing side, ported from `wardleyParser.ts` (`populateDb`), the
//! `WardleyBuilder` in `wardleyBuilder.ts`, and `diagrams/common/populateCommonDb.ts`.
//!
//! `wardleyParser.ts` parses the whole document first and then populates the database, so an
//! out-of-range coordinate or a pipeline without a parent is reported only after the grammar
//! accepted every line. The order of the writes is the one `populateDb` uses: the common
//! fields, the size, the evolution axes, then anchors, components, notes, pipelines, links,
//! evolves, the annotations box, each annotation, accelerators, and deaccelerators.
//!
//! `toCoordinates(visibility, evolution, context)` converts both numbers to percentages and
//! names the failing one in its error, so `x` is the evolution value and `y` is the
//! visibility value. Boxes, annotations, accelerators, and deaccelerators pass their `x` as
//! the visibility and their `y` as the evolution, which is why their stored coordinates look
//! swapped.

use super::ast::{
    Accelerator, Annotation, Ast, Axes, Deaccelerator, Flow, Link, Node, Note, Pipeline, Point,
    Size, SourceStrategy, Trend,
};
use super::parser::{Number, ParsedEvolution, ParsedLabel, ParsedWardley};
use crate::common::sanitize;
use crate::error::ParseError;

/// `populateDb`: applies the parsed document to a fresh database.
pub(crate) fn build(parsed: ParsedWardley) -> Result<Ast, ParseError> {
    let mut db = Db::new();

    // populateCommonDb: the common setters skip an empty string, which is falsy upstream.
    if let Some(value) = parsed.acc_description.filter(|value| !value.is_empty()) {
        db.set_acc_description(value);
    }
    if let Some(value) = parsed.acc_title.filter(|value| !value.is_empty()) {
        db.set_acc_title(value);
    }
    if let Some(value) = parsed.title.filter(|value| !value.is_empty()) {
        db.set_diagram_title(value);
    }

    if let Some(size) = parsed.size {
        db.set_size(size.width, size.height);
    }

    if let Some(evolution) = parsed.evolution {
        db.update_axes(&evolution);
    }

    for anchor in parsed.anchors {
        let context = format!("Anchor \"{}\"", anchor.name);
        let coords = to_coordinates(anchor.visibility, anchor.evolution, &context)?;
        db.add_node(NewNode {
            id: anchor.name.clone(),
            label: anchor.name,
            x: coords.x,
            y: coords.y,
            class_name: Some("anchor"),
            label_offset_x: None,
            label_offset_y: None,
            inertia: false,
            source_strategy: None,
        });
    }

    for component in parsed.components {
        let context = format!("Component \"{}\"", component.name);
        let coords = to_coordinates(component.visibility, component.evolution, &context)?;
        let (label_offset_x, label_offset_y) = label_offsets(component.label.as_ref());
        db.add_node(NewNode {
            id: component.name.clone(),
            label: component.name,
            x: coords.x,
            y: coords.y,
            class_name: Some("component"),
            label_offset_x,
            label_offset_y,
            inertia: component.inertia,
            source_strategy: component.strategy,
        });
    }

    for note in parsed.notes {
        let context = format!("Note \"{}\"", note.text);
        let coords = to_coordinates(note.visibility, note.evolution, &context)?;
        db.add_note(note.text, coords.x, coords.y);
    }

    for pipeline in parsed.pipelines {
        let Some(parent) = db.get_node(&pipeline.parent) else {
            return Err(ParseError::at(
                format!(
                    "Pipeline \"{}\" must reference an existing component with coordinates.",
                    pipeline.parent
                ),
                pipeline.parent_span,
            ));
        };
        let parent_y = parent.y;
        db.start_pipeline(&pipeline.parent);
        for component in pipeline.components {
            let component_id = format!("{}_{}", pipeline.parent, component.name);
            let (label_offset_x, label_offset_y) = label_offsets(component.label.as_ref());
            let x = to_percent(
                component.evolution,
                &format!("Pipeline component \"{}\" evolution", component.name),
            )?;
            db.add_node(NewNode {
                id: component_id.clone(),
                label: component.name,
                x,
                y: parent_y,
                class_name: Some("pipeline-component"),
                label_offset_x,
                label_offset_y,
                inertia: false,
                source_strategy: None,
            });
            db.add_pipeline_component(&pipeline.parent, component_id);
        }
    }

    for link in parsed.links {
        let dashed = link
            .arrow
            .as_deref()
            .is_some_and(|arrow| arrow.contains("-.->") || arrow.contains(".-."));
        let mut flow = flow_from_port(link.from_port.as_deref())
            .or_else(|| flow_from_port(link.to_port.as_deref()));
        let (arrow_flow, flow_label) = extract_flow_from_arrow(link.arrow.as_deref());
        if flow.is_none() {
            flow = arrow_flow;
        }
        let label = flow_label.or(link.link_label);
        db.add_link(
            db.resolve_node_id(&link.from),
            db.resolve_node_id(&link.to),
            dashed,
            label,
            flow,
        );
    }

    for evolve in parsed.evolves {
        if let Some(node) = db.get_node(&evolve.component) {
            let target = to_percent(
                evolve.target,
                &format!("Evolve target for \"{}\"", evolve.component),
            )?;
            let target_y = node.y;
            db.add_trend(evolve.component, target, target_y);
        }
    }

    if let Some(candidate) = parsed.annotations_boxes.first() {
        let coords = to_coordinates(candidate.x, candidate.y, "Annotations box")?;
        db.set_annotations_box(coords.x, coords.y);
    }

    for annotation in parsed.annotations {
        let context = format!("Annotation {}", js_number(annotation.number));
        let coords = to_coordinates(annotation.x, annotation.y, &context)?;
        db.add_annotation(annotation.number, coords, annotation.text);
    }

    for accelerator in parsed.accelerators {
        let context = format!("Accelerator \"{}\"", accelerator.name);
        let coords = to_coordinates(accelerator.x, accelerator.y, &context)?;
        db.add_accelerator(accelerator.name, coords.x, coords.y);
    }

    for deaccelerator in parsed.deaccelerators {
        let context = format!("Deaccelerator \"{}\"", deaccelerator.name);
        let coords = to_coordinates(deaccelerator.x, deaccelerator.y, &context)?;
        db.add_deaccelerator(deaccelerator.name, coords.x, coords.y);
    }

    Ok(db.into_ast())
}

/// `toPercent`: a 0-1 value becomes a percentage, a 0-100 value is kept, anything else throws.
fn to_percent(number: Number, context: &str) -> Result<f64, ParseError> {
    let value = number.value;
    let normalized = if value <= 1.0 { value * 100.0 } else { value };
    if !(0.0..=100.0).contains(&normalized) {
        return Err(ParseError::at(
            format!(
                "{context} must be between 0-1 (decimal) or 0-100 (percentage). Received: {}",
                js_number(value)
            ),
            number.span,
        ));
    }
    Ok(normalized)
}

/// `toCoordinates`: `x` is the evolution value, `y` the visibility value.
fn to_coordinates(
    visibility: Number,
    evolution: Number,
    context: &str,
) -> Result<Point, ParseError> {
    let x = to_percent(evolution, &format!("{context} evolution"))?;
    let y = to_percent(visibility, &format!("{context} visibility"))?;
    Ok(Point { x, y })
}

/// `(label.negX ? -1 : 1) * label.offsetX` and the same for `y`.
fn label_offsets(label: Option<&ParsedLabel>) -> (Option<f64>, Option<f64>) {
    match label {
        Some(label) => (
            Some(if label.neg_x {
                -label.offset_x
            } else {
                label.offset_x
            }),
            Some(if label.neg_y {
                -label.offset_y
            } else {
                label.offset_y
            }),
        ),
        None => (None, None),
    }
}

/// `getFlowFromPort`.
fn flow_from_port(port: Option<&str>) -> Option<Flow> {
    match port? {
        "+<>" => Some(Flow::Bidirectional),
        "+<" => Some(Flow::Backward),
        "+>" => Some(Flow::Forward),
        _ => None,
    }
}

/// `extractFlowFromArrow`: the flow a `+'label'` arrow declares, and its label.
fn extract_flow_from_arrow(arrow: Option<&str>) -> (Option<Flow>, Option<String>) {
    let Some(arrow) = arrow else {
        return (None, None);
    };
    if !arrow.starts_with('+') {
        return (None, None);
    }
    let label = arrow
        .strip_prefix("+'")
        .and_then(|rest| rest.find('\'').map(|close| rest[..close].to_string()));
    let flow = if arrow.contains("<>") {
        Some(Flow::Bidirectional)
    } else if arrow.contains('<') {
        Some(Flow::Backward)
    } else if arrow.contains('>') {
        Some(Flow::Forward)
    } else {
        None
    };
    (flow, label)
}

/// The fields `wardleyDb.addNode` hands to the builder, before the merge with an existing node.
struct NewNode {
    id: String,
    label: String,
    x: f64,
    y: f64,
    class_name: Option<&'static str>,
    label_offset_x: Option<f64>,
    label_offset_y: Option<f64>,
    inertia: bool,
    source_strategy: Option<SourceStrategy>,
}

/// The module state `WardleyBuilder` keeps.
struct Db {
    nodes: Vec<Node>,
    links: Vec<Link>,
    trends: Vec<Trend>,
    pipelines: Vec<Pipeline>,
    annotations: Vec<Annotation>,
    notes: Vec<Note>,
    accelerators: Vec<Accelerator>,
    deaccelerators: Vec<Deaccelerator>,
    annotations_box: Option<Point>,
    axes: Axes,
    size: Option<Size>,
    diagram_title: String,
    acc_title: String,
    acc_description: String,
}

impl Db {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            links: Vec::new(),
            trends: Vec::new(),
            pipelines: Vec::new(),
            annotations: Vec::new(),
            notes: Vec::new(),
            accelerators: Vec::new(),
            deaccelerators: Vec::new(),
            annotations_box: None,
            axes: Axes::default(),
            size: None,
            diagram_title: String::new(),
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }

    /// `addNode`: the spread updates every field, except the class and label offsets, which
    /// keep the existing value when the new node has none. A merge also clears
    /// `inPipeline` and `isPipelineParent`, as the spread does upstream.
    fn add_node(&mut self, node: NewNode) {
        if let Some(existing) = self
            .nodes
            .iter_mut()
            .find(|existing| existing.id == node.id)
        {
            existing.label = node.label;
            existing.x = node.x;
            existing.y = node.y;
            if let Some(class_name) = node.class_name {
                existing.class_name = Some(class_name.to_string());
            }
            if node.label_offset_x.is_some() {
                existing.label_offset_x = node.label_offset_x;
            }
            if node.label_offset_y.is_some() {
                existing.label_offset_y = node.label_offset_y;
            }
            existing.in_pipeline = false;
            existing.is_pipeline_parent = false;
            existing.inertia = node.inertia;
            existing.source_strategy = node.source_strategy;
            return;
        }
        self.nodes.push(Node {
            id: node.id,
            label: node.label,
            x: node.x,
            y: node.y,
            class_name: node.class_name.map(str::to_string),
            label_offset_x: node.label_offset_x,
            label_offset_y: node.label_offset_y,
            in_pipeline: false,
            is_pipeline_parent: false,
            inertia: node.inertia,
            source_strategy: node.source_strategy,
        });
    }

    fn add_link(
        &mut self,
        source: String,
        target: String,
        dashed: bool,
        label: Option<String>,
        flow: Option<Flow>,
    ) {
        self.links.push(Link {
            source,
            target,
            dashed,
            label,
            flow,
        });
    }

    /// `addTrend`: a `Map` keyed by node id keeps the first insertion's position.
    fn add_trend(&mut self, node_id: String, target_x: f64, target_y: f64) {
        if let Some(trend) = self
            .trends
            .iter_mut()
            .find(|trend| trend.node_id == node_id)
        {
            trend.target_x = target_x;
            trend.target_y = target_y;
            return;
        }
        self.trends.push(Trend {
            node_id,
            target_x,
            target_y,
        });
    }

    fn add_annotation(&mut self, number: f64, coordinates: Point, text: String) {
        self.annotations.push(Annotation {
            number,
            coordinates: vec![coordinates],
            text: Some(text),
        });
    }

    fn add_note(&mut self, text: String, x: f64, y: f64) {
        self.notes.push(Note { text, x, y });
    }

    fn add_accelerator(&mut self, name: String, x: f64, y: f64) {
        self.accelerators.push(Accelerator { name, x, y });
    }

    fn add_deaccelerator(&mut self, name: String, x: f64, y: f64) {
        self.deaccelerators.push(Deaccelerator { name, x, y });
    }

    fn set_annotations_box(&mut self, x: f64, y: f64) {
        self.annotations_box = Some(Point { x, y });
    }

    /// `startPipeline`: reset the pipeline at its first insertion position and mark the
    /// parent node.
    fn start_pipeline(&mut self, node_id: &str) {
        if let Some(pipeline) = self
            .pipelines
            .iter_mut()
            .find(|pipeline| pipeline.node_id == node_id)
        {
            pipeline.component_ids.clear();
        } else {
            self.pipelines.push(Pipeline {
                node_id: node_id.to_string(),
                component_ids: Vec::new(),
            });
        }
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == node_id) {
            node.is_pipeline_parent = true;
        }
    }

    /// `addPipelineComponent`: append the id and mark the component as being in a pipeline.
    fn add_pipeline_component(&mut self, pipeline_node_id: &str, component_id: String) {
        if let Some(pipeline) = self
            .pipelines
            .iter_mut()
            .find(|pipeline| pipeline.node_id == pipeline_node_id)
        {
            pipeline.component_ids.push(component_id.clone());
        }
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == component_id) {
            node.in_pipeline = true;
        }
    }

    fn get_node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// `resolveNodeId`.
    fn resolve_node_id(&self, name: &str) -> String {
        if let Some(node) = self.nodes.iter().find(|node| node.id == name) {
            return node.id.clone();
        }
        if let Some(node) = self.nodes.iter().find(|node| node.label == name) {
            return node.id.clone();
        }
        name.to_string()
    }

    /// `setSize`.
    fn set_size(&mut self, width: f64, height: f64) {
        self.size = Some(Size { width, height });
    }

    /// `updateAxes({ stages, stageBoundaries })`, merged into the empty initial config.
    fn update_axes(&mut self, evolution: &ParsedEvolution) {
        let stages = evolution
            .stages
            .iter()
            .map(|stage| match &stage.second_name {
                Some(second_name) => format!("{} / {}", stage.name.trim(), second_name.trim()),
                None => stage.name.trim().to_string(),
            })
            .collect();
        let boundaries = evolution
            .stages
            .iter()
            .filter_map(|stage| stage.boundary)
            .collect();
        self.axes.stages = Some(stages);
        self.axes.stage_boundaries = Some(boundaries);
    }

    /// `setDiagramTitle`: `sanitizeText(txt)`.
    fn set_diagram_title(&mut self, title: String) {
        self.diagram_title = sanitize::sanitize_text(&title);
    }

    /// `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
    fn set_acc_title(&mut self, title: String) {
        self.acc_title = sanitize::set_acc_title(&title);
    }

    /// `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
    fn set_acc_description(&mut self, description: String) {
        self.acc_description = sanitize::set_acc_description(&description);
    }

    /// `WardleyBuilder.build()`: every node must carry coordinates, which every `addNode`
    /// call in `populateDb` does.
    fn into_ast(self) -> Ast {
        Ast {
            nodes: self.nodes,
            links: self.links,
            trends: self.trends,
            pipelines: self.pipelines,
            annotations: self.annotations,
            notes: self.notes,
            accelerators: self.accelerators,
            deaccelerators: self.deaccelerators,
            annotations_box: self.annotations_box,
            axes: self.axes,
            size: self.size,
            diagram_title: self.diagram_title,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }
}

/// `${value}` in an error or context message: JavaScript's `Number::toString`, so ordinary
/// decimals print the same and the scientific form only differs in the exponent's sign.
fn js_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs();
    if !(1e-6..1e21).contains(&magnitude) {
        let scientific = format!("{value:e}");
        if let Some((mantissa, exponent)) = scientific.split_once('e')
            && !exponent.starts_with('-')
        {
            return format!("{mantissa}e+{exponent}");
        }
        return scientific;
    }
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
