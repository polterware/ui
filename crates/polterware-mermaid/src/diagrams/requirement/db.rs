//! The database the grammar fills: a port of upstream `requirementDb.ts`.
//!
//! Each method mirrors the one the jison actions call (`addRequirement`, `setNewReqId`,
//! `setNewReqText`, `setNewReqRisk`, `setNewReqVerifyMethod`, `addElement`,
//! `setNewElementType`, `setNewElementDocRef`, `addRelationship`, `setCssStyle`, `setClass`,
//! `defineClass`, `setDirection`, `setAccTitle`, `setAccDescription`), and [`Db::into_ast`]
//! exposes the state the getters read.
//!
//! The grammar actions run bottom-up: a requirement's body fields are set before
//! `addRequirement` runs, and `addRequirement` copies the accumulated fields into the map
//! entry, then resets them. A repeated requirement name keeps the first definition's
//! fields, exactly as the `has` check does upstream.

use super::ast::{
    Ast, Direction, Element, Relation, RelationshipType, Requirement, RequirementClass,
    RequirementType, RiskLevel, VerifyType,
};

/// The fields a requirement body has collected so far (`latestRequirement`).
#[derive(Default)]
struct RequirementDraft {
    requirement_id: String,
    text: String,
    risk: Option<RiskLevel>,
    verify_method: Option<VerifyType>,
}

/// The fields an element body has collected so far (`latestElement`).
#[derive(Default)]
struct ElementDraft {
    type_name: String,
    doc_ref: String,
}

/// The mutable database (`RequirementDB`).
#[derive(Default)]
pub(crate) struct Db {
    requirements: Vec<Requirement>,
    elements: Vec<Element>,
    relationships: Vec<Relation>,
    classes: Vec<RequirementClass>,
    direction: Direction,
    acc_title: String,
    acc_description: String,
    latest_requirement: RequirementDraft,
    latest_element: ElementDraft,
}

impl Db {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    pub(crate) fn set_acc_title(&mut self, title: String) {
        self.acc_title = title;
    }

    pub(crate) fn set_acc_description(&mut self, description: String) {
        self.acc_description = description;
    }

    pub(crate) fn set_new_req_id(&mut self, id: String) {
        self.latest_requirement.requirement_id = id;
    }

    pub(crate) fn set_new_req_text(&mut self, text: String) {
        self.latest_requirement.text = text;
    }

    pub(crate) fn set_new_req_risk(&mut self, risk: RiskLevel) {
        self.latest_requirement.risk = Some(risk);
    }

    pub(crate) fn set_new_req_verify_method(&mut self, verify_method: VerifyType) {
        self.latest_requirement.verify_method = Some(verify_method);
    }

    /// `addRequirement(name, type)`: create the entry only when the name is new, then reset
    /// the draft.
    pub(crate) fn add_requirement(&mut self, name: &str, requirement_type: RequirementType) {
        if !self
            .requirements
            .iter()
            .any(|requirement| requirement.name == name)
        {
            let draft = &self.latest_requirement;
            self.requirements.push(Requirement {
                name: name.to_string(),
                requirement_type,
                requirement_id: draft.requirement_id.clone(),
                text: draft.text.clone(),
                risk: draft.risk,
                verify_method: draft.verify_method,
                css_styles: Vec::new(),
                classes: vec!["default".to_string()],
            });
        }
        self.latest_requirement = RequirementDraft::default();
    }

    pub(crate) fn set_new_element_type(&mut self, type_name: String) {
        self.latest_element.type_name = type_name;
    }

    pub(crate) fn set_new_element_doc_ref(&mut self, doc_ref: String) {
        self.latest_element.doc_ref = doc_ref;
    }

    /// `addElement(name)`: create the entry only when the name is new, then reset the draft.
    pub(crate) fn add_element(&mut self, name: &str) {
        if !self.elements.iter().any(|element| element.name == name) {
            let draft = &self.latest_element;
            self.elements.push(Element {
                name: name.to_string(),
                type_name: draft.type_name.clone(),
                doc_ref: draft.doc_ref.clone(),
                css_styles: Vec::new(),
                classes: vec!["default".to_string()],
            });
        }
        self.latest_element = ElementDraft::default();
    }

    pub(crate) fn add_relationship(
        &mut self,
        relationship_type: RelationshipType,
        src: &str,
        dst: &str,
    ) {
        self.relationships.push(Relation {
            relationship_type,
            src: src.to_string(),
            dst: dst.to_string(),
        });
    }

    /// `setCssStyle(ids, styles)`: applies the declarations to each requirement or element.
    ///
    /// Upstream returns from the whole method on the first id that names nothing (or on an
    /// empty style list), so later ids are left unstyled; that is kept.
    pub(crate) fn set_css_style(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            if styles.is_empty() {
                return;
            }
            let Some(styles_target) = self.node_styles_mut(id) else {
                return;
            };
            for style in styles {
                if style.contains(',') {
                    styles_target.extend(style.split(',').map(str::to_string));
                } else {
                    styles_target.push(style.clone());
                }
            }
        }
    }

    /// `setClass(ids, classNames)`: appends the class names and the styles each class
    /// already defines. Missing ids and undefined classes are skipped.
    pub(crate) fn set_class(&mut self, ids: &[String], class_names: &[String]) {
        for id in ids {
            for class_name in class_names {
                let class_styles = self
                    .classes
                    .iter()
                    .find(|class| class.id == *class_name)
                    .map(|class| class.styles.clone());
                let Some(mut node) = self.node_mut(id) else {
                    continue;
                };
                node.classes().push(class_name.clone());
                if let Some(class_styles) = class_styles {
                    node.css_styles().extend(class_styles);
                }
            }
        }
    }

    /// `defineClass(ids, style)`: merges the declarations into each class and appends them to
    /// every requirement and element that already carries the class name.
    pub(crate) fn define_class(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            if !self.classes.iter().any(|class| class.id == *id) {
                self.classes.push(RequirementClass {
                    id: id.clone(),
                    styles: Vec::new(),
                    text_styles: Vec::new(),
                });
            }
            let text_styles: Vec<String> = styles
                .iter()
                .filter(|style| style.contains("color"))
                .map(|style| style.replacen("fill", "bgFill", 1))
                .collect();
            if let Some(class) = self.classes.iter_mut().find(|class| class.id == *id) {
                class.text_styles.extend(text_styles);
                class.styles.extend(styles.iter().cloned());
            }

            let flattened: Vec<String> = styles
                .iter()
                .flat_map(|style| style.split(',').map(str::to_string))
                .collect();
            for requirement in &mut self.requirements {
                if requirement.classes.contains(id) {
                    requirement.css_styles.extend(flattened.iter().cloned());
                }
            }
            for element in &mut self.elements {
                if element.classes.contains(id) {
                    element.css_styles.extend(flattened.iter().cloned());
                }
            }
        }
    }

    /// The state the getters return.
    pub(crate) fn into_ast(self) -> Ast {
        Ast {
            requirements: self.requirements,
            elements: self.elements,
            relationships: self.relationships,
            classes: self.classes,
            direction: self.direction,
            acc_title: self.acc_title,
            acc_description: self.acc_description,
        }
    }

    /// The requirement or element of `id`, requirement first, like `requirements.get(id) ??
    /// elements.get(id)`.
    fn node_mut(&mut self, id: &str) -> Option<NodeRef<'_>> {
        if let Some(requirement) = self
            .requirements
            .iter_mut()
            .find(|requirement| requirement.name == id)
        {
            return Some(NodeRef::Requirement(requirement));
        }
        self.elements
            .iter_mut()
            .find(|element| element.name == id)
            .map(NodeRef::Element)
    }

    /// The style list of the requirement or element named `id`.
    fn node_styles_mut(&mut self, id: &str) -> Option<&mut Vec<String>> {
        match self.node_mut(id)? {
            NodeRef::Requirement(requirement) => Some(&mut requirement.css_styles),
            NodeRef::Element(element) => Some(&mut element.css_styles),
        }
    }
}

enum NodeRef<'a> {
    Requirement(&'a mut Requirement),
    Element(&'a mut Element),
}

impl NodeRef<'_> {
    fn css_styles(&mut self) -> &mut Vec<String> {
        match self {
            NodeRef::Requirement(requirement) => &mut requirement.css_styles,
            NodeRef::Element(element) => &mut element.css_styles,
        }
    }

    fn classes(&mut self) -> &mut Vec<String> {
        match self {
            NodeRef::Requirement(requirement) => &mut requirement.classes,
            NodeRef::Element(element) => &mut element.classes,
        }
    }
}
