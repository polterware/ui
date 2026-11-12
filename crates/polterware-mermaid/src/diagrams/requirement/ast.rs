//! What upstream's `requirementDb.ts` holds after parsing, as `types.ts` declares it.
//!
//! Every public field is the result of the getter of the same name (`getRequirements`,
//! `getElements`, `getRelationships`, `getClasses`, `getDirection`, `getAccTitle`,
//! `getAccDescription`). Upstream stores requirements, elements, and classes in `Map`s keyed
//! by name; insertion order is observable, so they are `Vec`s here with lookup helpers.
//!
//! Two upstream details are left out:
//!
//! * `getData` and the `look`/`config` values it reads are the renderer's projection, and
//!   this crate does no layout or rendering.
//! * The common database's diagram title is never written by this grammar: the lexer has a
//!   `title` rule but no production consumes its token, so `getDiagramTitle` stays empty
//!   upstream too.
//!
//! `risk` and `verifyMethod` start as the empty string upstream (`'' as RiskLevel`,
//! `'' as VerifyType`); [`Option`] carries that state here.

/// The requirement kind (`RequirementType` in `types.ts`), with the display string the
/// database's constant holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RequirementType {
    #[default]
    Requirement,
    FunctionalRequirement,
    InterfaceRequirement,
    PerformanceRequirement,
    PhysicalRequirement,
    DesignConstraint,
}

impl RequirementType {
    /// The value `RequirementDB.RequirementType` maps the keyword to.
    pub fn as_str(self) -> &'static str {
        match self {
            RequirementType::Requirement => "Requirement",
            RequirementType::FunctionalRequirement => "Functional Requirement",
            RequirementType::InterfaceRequirement => "Interface Requirement",
            RequirementType::PerformanceRequirement => "Performance Requirement",
            RequirementType::PhysicalRequirement => "Physical Requirement",
            RequirementType::DesignConstraint => "Design Constraint",
        }
    }
}

/// A risk level (`RiskLevel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    /// The value `RequirementDB.RiskLevel` maps the keyword to.
    pub fn as_str(self) -> &'static str {
        match self {
            RiskLevel::Low => "Low",
            RiskLevel::Medium => "Medium",
            RiskLevel::High => "High",
        }
    }
}

/// A verification method (`VerifyType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyType {
    Analysis,
    Demonstration,
    Inspection,
    Test,
}

impl VerifyType {
    /// The value `RequirementDB.VerifyType` maps the keyword to.
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyType::Analysis => "Analysis",
            VerifyType::Demonstration => "Demonstration",
            VerifyType::Inspection => "Inspection",
            VerifyType::Test => "Test",
        }
    }
}

/// A relationship kind (`RelationshipType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipType {
    Contains,
    Copies,
    Derives,
    Satisfies,
    Verifies,
    Refines,
    Traces,
}

impl RelationshipType {
    /// The value `RequirementDB.Relationships` maps the keyword to.
    pub fn as_str(self) -> &'static str {
        match self {
            RelationshipType::Contains => "contains",
            RelationshipType::Copies => "copies",
            RelationshipType::Derives => "derives",
            RelationshipType::Satisfies => "satisfies",
            RelationshipType::Verifies => "verifies",
            RelationshipType::Refines => "refines",
            RelationshipType::Traces => "traces",
        }
    }
}

/// The layout direction (`getDirection`), `TB` until a `direction` statement says otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Tb,
    Bt,
    Rl,
    Lr,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Tb => "TB",
            Direction::Bt => "BT",
            Direction::Rl => "RL",
            Direction::Lr => "LR",
        }
    }
}

/// One requirement (`Requirement`), as `addRequirement` stores it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Requirement {
    /// The map key and the drawn name.
    pub name: String,
    pub requirement_type: RequirementType,
    /// The `id:` value; empty when the body has none.
    pub requirement_id: String,
    /// The `text:` value; empty when the body has none.
    pub text: String,
    /// `None` is upstream's empty string, the initial value.
    pub risk: Option<RiskLevel>,
    /// `None` is upstream's empty string, the initial value.
    pub verify_method: Option<VerifyType>,
    pub css_styles: Vec<String>,
    /// The class names, starting with `default`.
    pub classes: Vec<String>,
}

/// One element (`Element`), as `addElement` stores it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    /// The map key and the drawn name.
    pub name: String,
    /// The `type:` value; empty when the body has none.
    pub type_name: String,
    /// The `docref:` value; empty when the body has none.
    pub doc_ref: String,
    pub css_styles: Vec<String>,
    /// The class names, starting with `default`.
    pub classes: Vec<String>,
}

/// One relationship (`Relation`), as `addRelationship` pushes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub relationship_type: RelationshipType,
    pub src: String,
    pub dst: String,
}

/// A reusable class (`RequirementClass`), as `defineClass` stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequirementClass {
    pub id: String,
    pub styles: Vec<String>,
    /// The styles whose text contains `color`, with the first `fill` renamed to `bgFill`.
    pub text_styles: Vec<String>,
}

/// What upstream's requirement database holds after parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct Ast {
    /// `getRequirements()`, in insertion order.
    pub requirements: Vec<Requirement>,
    /// `getElements()`, in insertion order.
    pub elements: Vec<Element>,
    /// `getRelationships()`, in source order.
    pub relationships: Vec<Relation>,
    /// `getClasses()`, in insertion order.
    pub classes: Vec<RequirementClass>,
    /// `getDirection()`.
    pub direction: Direction,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}

impl Default for Ast {
    fn default() -> Self {
        Self {
            requirements: Vec::new(),
            elements: Vec::new(),
            relationships: Vec::new(),
            classes: Vec::new(),
            direction: Direction::Tb,
            acc_title: String::new(),
            acc_description: String::new(),
        }
    }
}

impl Ast {
    /// The value `getRequirements().get(name)` returns.
    pub fn requirement(&self, name: &str) -> Option<&Requirement> {
        self.requirements
            .iter()
            .find(|requirement| requirement.name == name)
    }

    /// The value `getElements().get(name)` returns.
    pub fn element(&self, name: &str) -> Option<&Element> {
        self.elements.iter().find(|element| element.name == name)
    }

    /// The value `getClasses().get(name)` returns.
    pub fn class(&self, name: &str) -> Option<&RequirementClass> {
        self.classes.iter().find(|class| class.id == name)
    }

    /// `getData()`: the requirement and element nodes, then the relationship edges.
    ///
    /// Upstream mutates each requirement or element in place with the renderer's node fields.
    /// The projection here keeps the node's own payload (the requirement's type, id, text,
    /// risk and verify method, or the element's type and doc ref) so the requirement box can
    /// draw its body. `look`, `config` and the layout fields are `draw`'s, not the database's,
    /// and [`super::layout_data::layout_data`] adds them.
    ///
    /// Relationship labels stay HTML-escaped (`&lt;&lt;contains&gt;&gt;`) exactly as upstream
    /// returns them; `createText` tokenizes that string and decodes each word when it draws,
    /// so the renderer decodes the label only for the scene text.
    pub fn data(&self) -> Data {
        let mut nodes: Vec<Node> = Vec::new();
        for requirement in &self.requirements {
            nodes.push(Node {
                id: requirement.name.clone(),
                label: requirement.name.clone(),
                requirement: Some(RequirementData {
                    type_name: requirement.requirement_type.as_str().to_string(),
                    requirement_id: requirement.requirement_id.clone(),
                    text: requirement.text.clone(),
                    risk: requirement.risk,
                    verify_method: requirement.verify_method,
                }),
                element: None,
                css_styles: requirement.css_styles.clone(),
                css_classes: requirement.classes.join(" "),
                color_index: nodes.len(),
            });
        }
        for element in &self.elements {
            nodes.push(Node {
                id: element.name.clone(),
                label: element.name.clone(),
                requirement: None,
                element: Some(ElementData {
                    type_name: element.type_name.clone(),
                    doc_ref: element.doc_ref.clone(),
                }),
                css_styles: element.css_styles.clone(),
                css_classes: element.classes.join(" "),
                color_index: nodes.len(),
            });
        }

        let mut edges = Vec::new();
        for (counter, relation) in self.relationships.iter().enumerate() {
            let is_contains = relation.relationship_type == RelationshipType::Contains;
            edges.push(LayoutEdge {
                id: format!("{}-{}-{}", relation.src, relation.dst, counter),
                start: self
                    .requirement(&relation.src)
                    .map(|requirement| requirement.name.clone())
                    .or_else(|| {
                        self.element(&relation.src)
                            .map(|element| element.name.clone())
                    }),
                end: self
                    .requirement(&relation.dst)
                    .map(|requirement| requirement.name.clone())
                    .or_else(|| {
                        self.element(&relation.dst)
                            .map(|element| element.name.clone())
                    }),
                label: format!("&lt;&lt;{}&gt;&gt;", relation.relationship_type.as_str()),
                classes: "relationshipLine".to_string(),
                style: vec![
                    "fill:none".to_string(),
                    if is_contains {
                        String::new()
                    } else {
                        "stroke-dasharray: 10,7".to_string()
                    },
                ],
                labelpos: "c".to_string(),
                thickness: "normal".to_string(),
                type_name: "normal".to_string(),
                pattern: if is_contains { "normal" } else { "dashed" }.to_string(),
                arrow_type_start: if is_contains {
                    "requirement_contains".to_string()
                } else {
                    String::new()
                },
                arrow_type_end: if is_contains {
                    String::new()
                } else {
                    "requirement_arrow".to_string()
                },
            });
        }

        Data {
            nodes,
            edges,
            direction: self.direction,
        }
    }
}

/// `getData()`'s requirement node payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequirementData {
    /// The requirement kind's display string (`Requirement`, `Functional Requirement`, ...).
    pub type_name: String,
    pub requirement_id: String,
    pub text: String,
    pub risk: Option<RiskLevel>,
    pub verify_method: Option<VerifyType>,
}

/// `getData()`'s element node payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementData {
    pub type_name: String,
    pub doc_ref: String,
}

/// One node of the `getData()` projection.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The map key, which upstream also copies to the node's `id`.
    pub id: String,
    /// `node.name`, the drawn name (`requirementBox` reads it, not `label`).
    pub label: String,
    pub requirement: Option<RequirementData>,
    pub element: Option<ElementData>,
    /// `cssStyles`, in the order the grammar appended them.
    pub css_styles: Vec<String>,
    /// `classes.join(' ')`.
    pub css_classes: String,
    /// The index `nodes.length` assigns when the node is pushed.
    pub color_index: usize,
}

/// One edge of the `getData()` projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutEdge {
    /// `<src>-<dst>-<counter>`.
    pub id: String,
    /// The resolved source name; `None` when the relationship names no stored node.
    pub start: Option<String>,
    /// The resolved target name; `None` when the relationship names no stored node.
    pub end: Option<String>,
    /// `<<relationship>>`, HTML-escaped as upstream returns it.
    pub label: String,
    /// Always `relationshipLine`.
    pub classes: String,
    /// `fill:none`, plus the dash declaration non-`contains` relationships carry.
    pub style: Vec<String>,
    /// Always `c`.
    pub labelpos: String,
    /// Always `normal`.
    pub thickness: String,
    /// Always `normal`.
    pub type_name: String,
    /// `normal` for `contains`, `dashed` otherwise.
    pub pattern: String,
    /// `requirement_contains` for `contains`, empty otherwise.
    pub arrow_type_start: String,
    /// `requirement_arrow` unless the relationship is `contains`.
    pub arrow_type_end: String,
}

/// The `getData` result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    pub nodes: Vec<Node>,
    pub edges: Vec<LayoutEdge>,
    pub direction: Direction,
}
