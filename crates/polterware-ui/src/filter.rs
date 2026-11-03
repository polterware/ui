//! The filter model of `filter-model.ts`: the fields a list filters by and the clauses that
//! hold the picks. Clauses AND: `is` keeps a record holding any of its values, `is not` one
//! holding none of them. Nothing here draws; `ui::FilterPopover` and the filter bar do.

use super::sort::natural_cmp;

pub const FILTER_IS: &str = "is";
pub const FILTER_IS_NOT: &str = "is_not";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    /// Any number of values, each with a checkbox.
    Multi,
    /// One value, with a radio.
    Single,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterOption {
    pub id: String,
    pub label: String,
    /// Muted trailing text, such as a count.
    pub trailing: Option<String>,
}

impl FilterOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            trailing: None,
        }
    }
}

/// `FilterField` without its icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterSpec {
    pub id: String,
    pub label: String,
    /// The plural noun of the value summary ("3 statuses"); derived from the label when absent.
    pub plural_label: Option<String>,
    pub kind: FilterKind,
    /// Offers `is not`.
    pub negatable: bool,
    /// Each add creates a new pill; when false new values merge into the field's pill.
    pub repeatable: bool,
    pub options: Vec<FilterOption>,
}

impl FilterSpec {
    pub fn multi(
        id: impl Into<String>,
        label: impl Into<String>,
        options: Vec<FilterOption>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            plural_label: None,
            kind: FilterKind::Multi,
            negatable: true,
            repeatable: true,
            options,
        }
    }

    /// `valueFilterField`: one multi field over the distinct values a list has, sorted.
    pub fn values<'a>(
        id: impl Into<String>,
        label: impl Into<String>,
        values: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut seen: Vec<String> = Vec::new();
        for value in values {
            if !value.is_empty() && !seen.iter().any(|item| item == value) {
                seen.push(value.to_string());
            }
        }
        seen.sort_by(|a, b| natural_cmp(a, b).then_with(|| a.cmp(b)));
        let options = seen
            .into_iter()
            .map(|value| FilterOption::new(value.clone(), value))
            .collect();
        Self::multi(id, label, options)
    }

    /// One value at a time, never negated, and always in one pill (`kind: "single"`,
    /// `negatable: false`, `repeatable: false`).
    pub fn single(mut self) -> Self {
        self.kind = FilterKind::Single;
        self.negatable = false;
        self.repeatable = false;
        self
    }

    pub fn plural(mut self, plural: impl Into<String>) -> Self {
        self.plural_label = Some(plural.into());
        self
    }

    pub fn option(&self, id: &str) -> Option<&FilterOption> {
        self.options.iter().find(|option| option.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterClause {
    pub id: String,
    pub field: String,
    pub operator: String,
    pub values: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterOperator {
    pub id: &'static str,
    pub label: &'static str,
}

/// `filterOperators`: is (is any of, for several values of a multi field), then is not.
pub fn operators(spec: &FilterSpec, value_count: usize) -> Vec<FilterOperator> {
    let positive = FilterOperator {
        id: FILTER_IS,
        label: if spec.kind == FilterKind::Multi && value_count > 1 {
            "is any of"
        } else {
            "is"
        },
    };
    if spec.negatable {
        vec![
            positive,
            FilterOperator {
                id: FILTER_IS_NOT,
                label: "is not",
            },
        ]
    } else {
        vec![positive]
    }
}

pub fn operator_label(spec: &FilterSpec, clause: &FilterClause) -> &'static str {
    let operators = operators(spec, clause.values.len());
    operators
        .iter()
        .find(|operator| operator.id == clause.operator)
        .unwrap_or(&operators[0])
        .label
}

/// `pluralize`: Status → statuses, Category → categories, Tag → tags.
pub fn pluralize(word: &str) -> String {
    let lower = word.to_lowercase();
    if lower.ends_with(['s', 'x', 'z']) || lower.ends_with("ch") || lower.ends_with("sh") {
        return format!("{lower}es");
    }
    if let Some(stem) = lower.strip_suffix('y')
        && !stem.ends_with(['a', 'e', 'i', 'o', 'u'])
    {
        return format!("{stem}ies");
    }
    format!("{lower}s")
}

/// `valueSummary`: the one value's label, else how many ("2 agents").
pub fn value_summary(spec: &FilterSpec, clause: &FilterClause) -> String {
    if let [one] = clause.values.as_slice() {
        return spec
            .option(one)
            .map(|option| option.label.clone())
            .unwrap_or_else(|| one.clone());
    }
    let plural = spec
        .plural_label
        .clone()
        .unwrap_or_else(|| pluralize(&spec.label));
    format!("{} {plural}", clause.values.len())
}

/// The clauses of one page, with the ids it hands out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    pub clauses: Vec<FilterClause>,
    next_id: u64,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    pub fn clause(&self, id: &str) -> Option<&FilterClause> {
        self.clauses.iter().find(|clause| clause.id == id)
    }

    /// The field's pill, when the field keeps one.
    pub fn single_clause(&self, spec: &FilterSpec) -> Option<&FilterClause> {
        if spec.repeatable {
            return None;
        }
        self.clauses.iter().find(|clause| clause.field == spec.id)
    }

    /// `addClause`: a new clause, or a merge into the field's clause when the field does not
    /// repeat. Returns the id of the clause that holds the values.
    pub fn add(&mut self, spec: &FilterSpec, values: &[String]) -> String {
        if let Some(existing) = self.single_clause(spec).map(|clause| clause.id.clone()) {
            let merged = match spec.kind {
                FilterKind::Multi => {
                    let mut merged = self.clause(&existing).unwrap().values.clone();
                    for value in values {
                        if !merged.contains(value) {
                            merged.push(value.clone());
                        }
                    }
                    merged
                }
                FilterKind::Single => values.iter().take(1).cloned().collect(),
            };
            self.set_values(&existing, merged);
            return existing;
        }
        self.next_id += 1;
        let id = format!("clause-{}", self.next_id);
        let values = match spec.kind {
            FilterKind::Multi => values.to_vec(),
            FilterKind::Single => values.iter().take(1).cloned().collect(),
        };
        self.clauses.push(FilterClause {
            id: id.clone(),
            field: spec.id.clone(),
            operator: FILTER_IS.to_string(),
            values,
        });
        id
    }

    /// `setClauseValues`: an empty list removes the clause.
    pub fn set_values(&mut self, id: &str, values: Vec<String>) {
        if values.is_empty() {
            self.remove(id);
            return;
        }
        if let Some(clause) = self.clauses.iter_mut().find(|clause| clause.id == id) {
            clause.values = values;
        }
    }

    /// `toggleClauseValue`.
    pub fn toggle(&mut self, id: &str, spec: &FilterSpec, value: &str, checked: bool) {
        let Some(clause) = self.clause(id) else {
            return;
        };
        if spec.kind != FilterKind::Multi {
            let values = if checked {
                vec![value.to_string()]
            } else {
                Vec::new()
            };
            self.set_values(id, values);
            return;
        }
        let mut values = clause.values.clone();
        if checked {
            if !values.iter().any(|item| item == value) {
                values.push(value.to_string());
            }
        } else {
            values.retain(|item| item != value);
        }
        self.set_values(id, values);
    }

    pub fn set_operator(&mut self, id: &str, operator: &str) {
        if let Some(clause) = self.clauses.iter_mut().find(|clause| clause.id == id) {
            clause.operator = operator.to_string();
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.clauses.retain(|clause| clause.id != id);
    }

    pub fn clear(&mut self) {
        self.clauses.clear();
    }

    /// The values of the field's first `is` clause (a page applying one field itself).
    pub fn values_of(&self, field: &str) -> &[String] {
        self.clauses
            .iter()
            .find(|clause| clause.field == field && clause.operator != FILTER_IS_NOT)
            .map(|clause| clause.values.as_slice())
            .unwrap_or(&[])
    }

    /// `matchClauses`: `values(field)` gives a record's values for a field, `None` for a field
    /// the page applies elsewhere (skipped).
    pub fn matches<'a>(&self, values: impl Fn(&str) -> Option<Vec<&'a str>>) -> bool {
        self.clauses.iter().all(|clause| {
            let Some(item) = values(&clause.field) else {
                return true;
            };
            let hit = item
                .iter()
                .any(|value| clause.values.iter().any(|picked| picked == value));
            if clause.operator == FILTER_IS_NOT {
                !hit
            } else {
                hit
            }
        })
    }
}

/// A row of the root menu while its search has text: a field, or a `Field › Value` shortcut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchRow {
    Field(usize),
    Value(usize, usize),
}

/// The root search's results: the fields whose label matches, then every matching value.
pub fn search_rows(specs: &[&FilterSpec], query: &str) -> Vec<SearchRow> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut rows: Vec<SearchRow> = specs
        .iter()
        .enumerate()
        .filter(|(_, spec)| spec.label.to_lowercase().contains(&needle))
        .map(|(index, _)| SearchRow::Field(index))
        .collect();
    for (field, spec) in specs.iter().enumerate() {
        for (option, item) in spec.options.iter().enumerate() {
            if item.label.to_lowercase().contains(&needle) {
                rows.push(SearchRow::Value(field, option));
            }
        }
    }
    rows
}

/// The options a value list shows for `query`, in two sections when `checked_first` keeps
/// the values checked when the list opened on top (Linear).
pub fn option_sections<'a>(
    spec: &'a FilterSpec,
    query: &str,
    pinned: &[String],
    checked_first: bool,
) -> Vec<Vec<&'a FilterOption>> {
    let needle = query.trim().to_lowercase();
    let visible: Vec<&FilterOption> = spec
        .options
        .iter()
        .filter(|option| needle.is_empty() || option.label.to_lowercase().contains(&needle))
        .collect();
    if !checked_first {
        return vec![visible];
    }
    let (top, rest): (Vec<&FilterOption>, Vec<&FilterOption>) = visible
        .into_iter()
        .partition(|option| pinned.contains(&option.id));
    [top, rest]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect()
}

/// The initial an `OptionTile` shows for a label with no icon: its first letter or digit.
pub fn initial(label: &str) -> String {
    label
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "\u{b7}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> FilterSpec {
        FilterSpec::values("agent", "Agent", ["codex", "claude", "", "claude", "grok"])
    }

    #[test]
    fn value_fields_drop_empties_and_sort() {
        let ids: Vec<_> = agent().options.iter().map(|o| o.id.clone()).collect();
        assert_eq!(ids, ["claude", "codex", "grok"]);
    }

    #[test]
    fn operators_follow_the_value_count_and_negation() {
        let spec = agent();
        assert_eq!(operators(&spec, 1)[0].label, "is");
        assert_eq!(operators(&spec, 2)[0].label, "is any of");
        assert_eq!(operators(&spec, 2)[1].label, "is not");
        let window = FilterSpec::multi("since", "Spend window", vec![]).single();
        assert_eq!(operators(&window, 1).len(), 1);
    }

    #[test]
    fn summaries_name_one_value_or_count_them() {
        let spec = agent();
        let mut filters = Filters::default();
        let id = filters.add(&spec, &["claude".into()]);
        assert_eq!(value_summary(&spec, filters.clause(&id).unwrap()), "claude");
        filters.toggle(&id, &spec, "codex", true);
        assert_eq!(
            value_summary(&spec, filters.clause(&id).unwrap()),
            "2 agents"
        );
        assert_eq!(pluralize("Status"), "statuses");
        assert_eq!(pluralize("Pace"), "paces");
        assert_eq!(pluralize("Policy"), "policies");
    }

    #[test]
    fn unchecking_the_last_value_removes_the_clause() {
        let spec = agent();
        let mut filters = Filters::default();
        let id = filters.add(&spec, &["claude".into()]);
        filters.toggle(&id, &spec, "claude", false);
        assert!(filters.is_empty());
    }

    #[test]
    fn a_single_field_keeps_one_pill_and_one_value() {
        let spec = FilterSpec::multi(
            "since",
            "Spend window",
            vec![
                FilterOption::new("7d", "7 days"),
                FilterOption::new("30d", "30 days"),
            ],
        )
        .single();
        let mut filters = Filters::default();
        let first = filters.add(&spec, &["7d".into()]);
        let second = filters.add(&spec, &["30d".into()]);
        assert_eq!(first, second);
        assert_eq!(filters.values_of("since"), ["30d".to_string()]);
    }

    #[test]
    fn clauses_and_and_is_not_inverts() {
        let spec = agent();
        let mut filters = Filters::default();
        let id = filters.add(&spec, &["claude".into()]);
        assert!(filters.matches(|_| Some(vec!["claude"])));
        assert!(!filters.matches(|_| Some(vec!["codex"])));
        filters.set_operator(&id, FILTER_IS_NOT);
        assert!(filters.matches(|_| Some(vec!["codex"])));
        // A field the page applies elsewhere is skipped.
        assert!(filters.matches(|_| None));
    }

    #[test]
    fn search_lists_fields_then_values() {
        let spec = agent();
        let status = FilterSpec::values("status", "Status", ["completed", "cancelled"]);
        let rows = search_rows(&[&status, &spec], "c");
        assert_eq!(rows[0], SearchRow::Value(0, 0));
        let rows = search_rows(&[&status, &spec], "agent");
        assert_eq!(rows, vec![SearchRow::Field(1)]);
    }

    #[test]
    fn checked_values_come_first() {
        let spec = agent();
        let sections = option_sections(&spec, "", &["grok".into()], true);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0][0].id, "grok");
        assert_eq!(initial("claude"), "C");
        assert_eq!(initial("  "), "\u{b7}");
    }
}
