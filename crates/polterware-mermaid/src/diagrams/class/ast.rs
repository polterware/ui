//! What upstream's `classDb.ts` holds after parsing, as the shapes `classTypes.ts`
//! declares: `ClassNode`, `ClassMember`, `ClassRelation`, `ClassNote`, `NamespaceNode`,
//! `StyleClass`, and `Interface`. Each public field is the result of the getter of the
//! same name (`getClasses`, `getRelations`, `getNotes`, `getNamespaces`, `getDirection`,
//! `getAccTitle`, `getAccDescription`); the diagram title from the frontmatter is not
//! part of the database and is not here.
//!
//! Two upstream details are engine-level and are left out: the module-global
//! `classCounter`/`namespaceCounter` (here each parse counts from zero, so a diagram's
//! ids do not depend on what was parsed before it) and the DOM side of `setLink` /
//! `setClickEvent` (the `functions` list and its `document.querySelector` closures). The
//! values those methods store on a class (`link`, `link_target`, `have_callback`,
//! `cssClasses`) are kept.
//!
//! `sanitizeText` is a no-op here: upstream runs DOMPurify, which this crate cannot, and
//! the class specs never depend on it.

/// An insertion-ordered map, the shape a JavaScript `Map` has: a later `insert` of an
/// existing key replaces the value without moving the key. Upstream walks its maps in
/// insertion order (`getData`), so the order is part of the data.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedMap<V> {
    entries: Vec<(String, V)>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.entries
            .iter_mut()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.iter().any(|(name, _)| name == key)
    }

    /// A JavaScript `Map.set`: an existing key keeps its position.
    pub fn insert(&mut self, key: impl Into<String>, value: V) {
        let key = key.into();
        if let Some((_, existing)) = self.entries.iter_mut().find(|(name, _)| *name == key) {
            *existing = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(name, _)| name.as_str())
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, value)| value)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().map(|(_, value)| value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
}

/// A member's visibility, upstream's `Visibility` (`''`, `+`, `-`, `#`, `~`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    None,
    Public,
    Private,
    Protected,
    Package,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::None => "",
            Visibility::Public => "+",
            Visibility::Private => "-",
            Visibility::Protected => "#",
            Visibility::Package => "~",
        }
    }

    fn from_char(c: char) -> Option<Self> {
        match c {
            '+' => Some(Visibility::Public),
            '-' => Some(Visibility::Private),
            '#' => Some(Visibility::Protected),
            '~' => Some(Visibility::Package),
            _ => None,
        }
    }
}

/// Upstream's `'method' | 'attribute'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberType {
    Method,
    Attribute,
}

/// A parsed class member (`ClassMember` in `classTypes.ts`).
///
/// The parsing is upstream's `parseMember`: the method regular expression
/// `([#+~-])?(.+)\((.*)\)([\s$*])?(.*)([$*])?` and the attribute path that reads the
/// first and last character. The visible text is `getDisplayDetails`.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassMember {
    pub id: String,
    pub member_type: MemberType,
    pub visibility: Visibility,
    pub text: String,
    pub classifier: String,
    pub parameters: String,
    pub return_type: String,
}

/// The result of upstream's `getDisplayDetails`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayDetails {
    pub display_text: String,
    pub css_style: &'static str,
}

impl ClassMember {
    pub fn new(input: &str, member_type: MemberType) -> Self {
        let mut member = ClassMember {
            id: String::new(),
            member_type,
            visibility: Visibility::None,
            text: String::new(),
            classifier: String::new(),
            parameters: String::new(),
            return_type: String::new(),
        };
        member.parse_member(&sanitize_text(input));
        member
    }

    /// Upstream's `getDisplayDetails`, including the whitespace trim.
    pub fn display_details(&self) -> DisplayDetails {
        let mut display_text = format!(
            "{}{}",
            self.visibility.as_str(),
            parse_generic_types(&self.id)
        );
        if self.member_type == MemberType::Method {
            display_text.push('(');
            display_text.push_str(&parse_generic_types(self.parameters.trim()));
            display_text.push(')');
            if !self.return_type.is_empty() {
                display_text.push_str(" : ");
                display_text.push_str(&parse_generic_types(&self.return_type));
            }
        }
        DisplayDetails {
            display_text: display_text.trim().to_string(),
            css_style: self.css_style(),
        }
    }

    /// Upstream's `parseClassifier`.
    pub fn css_style(&self) -> &'static str {
        match self.classifier.as_str() {
            "*" => "font-style:italic;",
            "$" => "text-decoration:underline;",
            _ => "",
        }
    }

    fn parse_member(&mut self, input: &str) {
        let mut potential_classifier = String::new();

        if self.member_type == MemberType::Method {
            if let Some(parts) = match_method(input) {
                self.visibility = parts.visibility;
                self.id = parts.id;
                self.parameters = parts.parameters.trim().to_string();
                potential_classifier = parts.classifier.trim().to_string();
                self.return_type = parts.return_type.trim().to_string();

                if potential_classifier.is_empty()
                    && let Some(last) = self.return_type.chars().last()
                    && (last == '$' || last == '*')
                {
                    potential_classifier = last.to_string();
                    let keep = self.return_type.len() - last.len_utf8();
                    self.return_type.truncate(keep);
                }
            }
        } else {
            let length = input.chars().count();
            let first_char = input.chars().next();
            let last_char = input.chars().last();

            if let Some(first) = first_char {
                self.visibility = Visibility::from_char(first).unwrap_or(Visibility::None);
            }

            if let Some(last) = last_char
                && (last == '$' || last == '*')
            {
                potential_classifier = last.to_string();
            }

            let start = if self.visibility == Visibility::None {
                0
            } else {
                1
            };
            let end = if potential_classifier.is_empty() {
                length
            } else {
                length.saturating_sub(1)
            };
            self.id = input
                .chars()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect();
        }

        self.classifier = potential_classifier;
        // Upstream keeps one leading space: `id.startsWith(' ') ? ' ' + id.trim() : id.trim()`.
        let trimmed = self.id.trim().to_string();
        self.id = if self.id.starts_with(' ') {
            format!(" {trimmed}")
        } else {
            trimmed
        };

        let mut text = String::new();
        if self.visibility != Visibility::None {
            text.push('\\');
            text.push_str(self.visibility.as_str());
        }
        text.push_str(&parse_generic_types(&self.id));
        if self.member_type == MemberType::Method {
            text.push('(');
            text.push_str(&parse_generic_types(&self.parameters));
            text.push(')');
            if !self.return_type.is_empty() {
                text.push_str(" : ");
                text.push_str(&parse_generic_types(&self.return_type));
            }
        }
        // Upstream escapes with `replaceAll` and restores a leading `\&lt;` to `~`.
        let escaped = text.replace('<', "&lt;").replace('>', "&gt;");
        self.text = match escaped.strip_prefix("\\&lt;") {
            Some(rest) => format!("~{rest}"),
            None => escaped,
        };
    }
}

/// The parts the method regular expression captures.
struct MethodParts {
    visibility: Visibility,
    id: String,
    parameters: String,
    classifier: String,
    return_type: String,
}

/// Searches `input` like `RegExp.exec` does: the leftmost position that matches wins.
fn match_method(input: &str) -> Option<MethodParts> {
    for (start, _) in input.char_indices() {
        if let Some(parts) = match_method_at(input, start) {
            return Some(parts);
        }
    }
    None
}

fn match_method_at(input: &str, start: usize) -> Option<MethodParts> {
    let first = input[start..].chars().next()?;
    if let Some(visibility) = Visibility::from_char(first)
        && let Some(parts) = match_method_from(input, start + first.len_utf8(), visibility)
    {
        return Some(parts);
    }
    match_method_from(input, start, Visibility::None)
}

/// Matches the rest of `([#+~-])?(.+)\((.*)\)([\s$*])?(.*)([$*])?` after the optional
/// visibility character. The trailing `([$*])?` never captures: the greedy `(.*)` before
/// it consumes the rest of the line, which is why upstream strips the trailing classifier
/// from the return type afterwards.
fn match_method_from(input: &str, from: usize, visibility: Visibility) -> Option<MethodParts> {
    let opens: Vec<usize> = input[from..]
        .match_indices('(')
        .map(|(offset, _)| from + offset)
        .collect();
    // The greedy `(.+)` takes the last `(` that still has a `)` after it.
    for &open in opens.iter().rev() {
        if open == from {
            continue;
        }
        let Some(close_offset) = input[open + 1..].rfind(')') else {
            continue;
        };
        let close = open + 1 + close_offset;
        let id = &input[from..open];
        let parameters = &input[open + 1..close];
        let rest = &input[close + 1..];
        let mut classifier = String::new();
        let mut after = rest;
        if let Some(c) = rest.chars().next()
            && (c == '$' || c == '*' || c.is_whitespace())
        {
            classifier = c.to_string();
            after = &rest[c.len_utf8()..];
        }
        return Some(MethodParts {
            visibility,
            id: id.to_string(),
            parameters: parameters.to_string(),
            classifier,
            return_type: after.to_string(),
        });
    }
    None
}

/// `parseGenericTypes` from `diagrams/common/common.ts`: turns `~T~` into `<T>`,
/// including upstream's comma-rejoining rule for `~K, V~`.
pub(super) fn parse_generic_types(input: &str) -> String {
    let mut pieces: Vec<String> = Vec::new();
    for (index, part) in input.split(',').enumerate() {
        if index > 0 {
            pieces.push(",".to_string());
        }
        pieces.push(part.to_string());
    }

    let mut output: Vec<String> = Vec::new();
    let mut index = 0;
    while index < pieces.len() {
        let mut this_set = pieces[index].clone();
        if this_set == "," && index > 0 && index + 1 < pieces.len() {
            let previous = &pieces[index - 1];
            let next = &pieces[index + 1];
            if count_occurrence(previous, "~") == 1 && count_occurrence(next, "~") == 1 {
                this_set = format!("{previous},{next}");
                index += 1;
                output.pop();
            }
        }
        output.push(process_set(&this_set));
        index += 1;
    }
    output.concat()
}

fn count_occurrence(text: &str, substring: &str) -> usize {
    text.matches(substring).count()
}

fn process_set(input: &str) -> String {
    let tilde_count = count_occurrence(input, "~");
    if tilde_count <= 1 {
        return input.to_string();
    }

    let mut text = input.to_string();
    let mut had_starting_tilde = false;
    if !tilde_count.is_multiple_of(2) && text.starts_with('~') {
        text.remove(0);
        had_starting_tilde = true;
    }

    let mut chars: Vec<char> = text.chars().collect();
    loop {
        let first = chars.iter().position(|c| *c == '~');
        let last = chars.iter().rposition(|c| *c == '~');
        match (first, last) {
            (Some(first), Some(last)) if first != last => {
                chars[first] = '<';
                chars[last] = '>';
            }
            _ => break,
        }
    }

    if had_starting_tilde {
        chars.insert(0, '~');
    }
    chars.into_iter().collect()
}

/// A class box (`ClassNode`).
#[derive(Clone, Debug, PartialEq)]
pub struct ClassNode {
    pub id: String,
    /// Upstream's `type`: the generic parameter parsed out of `Class~T~`.
    pub type_name: String,
    pub label: String,
    pub text: String,
    pub css_classes: String,
    pub methods: Vec<ClassMember>,
    pub members: Vec<ClassMember>,
    pub annotations: Vec<String>,
    pub dom_id: String,
    pub styles: Vec<String>,
    pub parent: Option<String>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub have_callback: bool,
    pub tooltip: Option<String>,
}

/// Upstream's numeric `relationType`: `none` is the string `'none'` there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Endpoint {
    #[default]
    None,
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
}

/// Upstream's `lineType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineType {
    #[default]
    Line,
    DottedLine,
}

/// The relation itself: `type1`, `type2`, and `lineType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Relation {
    pub type1: Endpoint,
    pub type2: Endpoint,
    pub line_type: LineType,
}

/// A relation between two classes (`ClassRelation`).
#[derive(Clone, Debug, PartialEq)]
pub struct ClassRelation {
    pub id1: String,
    pub id2: String,
    pub relation_title1: String,
    pub relation_title2: String,
    /// Undefined upstream when the statement has no `: label`.
    pub title: Option<String>,
    pub style: Vec<String>,
    pub relation: Relation,
}

/// A note (`ClassNote`).
#[derive(Clone, Debug, PartialEq)]
pub struct ClassNote {
    pub id: String,
    /// The class the note is about, or `None` for a plain `note`.
    pub class: Option<String>,
    pub text: String,
    pub index: usize,
    pub parent: Option<String>,
}

/// A namespace (`NamespaceNode`). `classes` and `notes` hold the ids of the nodes
/// declared directly inside it, where upstream holds the same objects in its maps; the
/// nodes themselves stay in [`Ast::classes`] and [`Ast::notes`] here.
#[derive(Clone, Debug, PartialEq)]
pub struct NamespaceNode {
    pub id: String,
    pub label: String,
    pub dom_id: String,
    pub classes: Vec<String>,
    pub notes: Vec<String>,
    pub children: Vec<String>,
    pub parent: Option<String>,
    /// True for a namespace the user declared, false for an intermediate ancestor
    /// created by a dotted name.
    pub explicit: bool,
}

/// A `classDef` style definition (`StyleClass`).
#[derive(Clone, Debug, PartialEq)]
pub struct StyleClass {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// A lollipop relation's hidden interface node (`Interface`).
#[derive(Clone, Debug, PartialEq)]
pub struct Interface {
    pub id: String,
    pub label: String,
    pub class_id: String,
}

/// The diagram's direction, upstream's `getDirection`.
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

/// The parsed diagram.
#[derive(Clone, Debug, PartialEq)]
pub struct Ast {
    pub classes: OrderedMap<ClassNode>,
    pub relations: Vec<ClassRelation>,
    pub notes: OrderedMap<ClassNote>,
    pub namespaces: OrderedMap<NamespaceNode>,
    pub style_classes: OrderedMap<StyleClass>,
    pub interfaces: Vec<Interface>,
    pub direction: Direction,
    pub acc_title: Option<String>,
    pub acc_description: Option<String>,
}

impl Default for Ast {
    fn default() -> Self {
        Self {
            classes: OrderedMap::new(),
            relations: Vec::new(),
            notes: OrderedMap::new(),
            namespaces: OrderedMap::new(),
            style_classes: OrderedMap::new(),
            interfaces: Vec::new(),
            direction: Direction::Tb,
            acc_title: None,
            acc_description: None,
        }
    }
}

impl Ast {
    /// Upstream's `getClass`.
    pub fn get_class(&self, id: &str) -> Option<&ClassNode> {
        self.classes.get(id)
    }

    /// Upstream's `getClasses`.
    pub fn get_classes(&self) -> &OrderedMap<ClassNode> {
        &self.classes
    }

    /// Upstream's `getRelations`.
    pub fn get_relations(&self) -> &[ClassRelation] {
        &self.relations
    }

    /// Upstream's `getNote(id)` with its numeric form.
    pub fn get_note(&self, index: usize) -> Option<&ClassNote> {
        self.get_note_by_id(&format!("note{index}"))
    }

    /// Upstream's `getNote(id)` with a string id.
    pub fn get_note_by_id(&self, id: &str) -> Option<&ClassNote> {
        self.notes.get(id)
    }

    /// Upstream's `getNotes`.
    pub fn get_notes(&self) -> &OrderedMap<ClassNote> {
        &self.notes
    }

    /// Upstream's `getNamespace`.
    pub fn get_namespace(&self, name: &str) -> Option<&NamespaceNode> {
        self.namespaces.get(name)
    }

    /// Upstream's `getNamespaces`.
    pub fn get_namespaces(&self) -> &OrderedMap<NamespaceNode> {
        &self.namespaces
    }

    /// Upstream's `getDirection`.
    pub fn get_direction(&self) -> Direction {
        self.direction
    }
}

/// `splitClassNameAndType` from `classDb.ts`.
pub(super) fn split_class_name_and_type(id: &str) -> (String, String) {
    let id = sanitize_text(id);
    let mut class_name = id.clone();
    let mut generic_type = String::new();
    if let Some(index) = id.find('~')
        && index > 0
    {
        let mut parts = id.split('~');
        class_name = sanitize_text(parts.next().unwrap_or_default());
        generic_type = sanitize_text(parts.next().unwrap_or_default());
    }
    (class_name, generic_type)
}

/// `common.sanitizeText` is DOMPurify upstream. This crate has no DOM, and the class
/// specs do not depend on sanitization, so text is kept as written.
pub(super) fn sanitize_text(text: &str) -> String {
    text.to_string()
}
