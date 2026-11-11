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
}
