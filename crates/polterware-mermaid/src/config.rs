//! Mermaid's configuration: the defaults every diagram section starts from, and the merge
//! that puts frontmatter and `%%{init: …}%%` directives on top of them.
//!
//! Ported from `schemas/config.schema.yaml` (the JSON defaults), `defaultConfig.ts` (the
//! values that are not JSON, such as the sequence and c4 font builders and the class and
//! railroad sections it rebuilds), `config.ts` (secure keys, sanitizing, and the order a
//! directive merges in) and `assignWithDepth.ts` (the merge itself). The sections and their
//! defaults are typed; a directive is merged as JSON, so an unknown key is ignored, as
//! upstream's `sanitizeDirective` ignores it, and a value of the wrong type is an error
//! instead of an unusable string reaching a renderer later.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::theme::Theme;

/// A number, or a string such as a font weight (`"normal"`) or a CSS length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(f64),
    Text(String),
}

impl Default for NumberOrString {
    fn default() -> Self {
        Self::Number(0.0)
    }
}

impl NumberOrString {
    /// The value as text, for a font size or weight written either way.
    pub fn as_text(&self) -> String {
        match self {
            Self::Number(number) => number.to_string(),
            Self::Text(text) => text.clone(),
        }
    }
}

/// Elk's edge-crossing rendering: on, off, or one of its two shapes. `lineHops` accepts a
/// boolean or `"arc"` / `"gap"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LineHops {
    Enabled(bool),
    Kind(LineHopsKind),
}

impl Default for LineHops {
    fn default() -> Self {
        Self::Enabled(true)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineHopsKind {
    Arc,
    Gap,
}

/// The three font settings a diagram section carries (upstream's `FontConfig`), which the
/// section's `…Font()` builders return.
#[derive(Clone, Debug, PartialEq)]
pub struct FontConfig {
    pub font_family: String,
    pub font_size: NumberOrString,
    pub font_weight: NumberOrString,
}

/// A merge or type error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

impl From<crate::theme::ThemeError> for ConfigError {
    fn from(error: crate::theme::ThemeError) -> Self {
        Self::new(error.to_string())
    }
}

/// `assignWithDepth.ts`: merges `src` into `dst`, objects at most `depth` levels deep.
///
/// Unlike `Object.assign`, a value never replaces a value of another shape: an object does
/// not clobber a scalar and a scalar does not clobber an object. `null` and arrays follow
/// upstream's rules (null values are skipped; array members are appended when missing).
pub fn assign_with_depth(dst: &mut Value, src: &Value, depth: i32) {
    if let Value::Array(items) = src {
        match dst {
            Value::Array(dst_items) => {
                for item in items {
                    if !dst_items.contains(item) {
                        dst_items.push(item.clone());
                    }
                }
            }
            _ => {
                for item in items {
                    assign_with_depth(dst, item, depth);
                }
            }
        }
        return;
    }

    if depth <= 0 || dst.is_null() {
        if !dst.is_null() && dst.is_object() && src.is_object() {
            if let (Value::Object(destination), Value::Object(source)) = (&mut *dst, src) {
                for (key, value) in source {
                    destination.insert(key.clone(), value.clone());
                }
            }
        } else {
            *dst = src.clone();
        }
        return;
    }

    if !dst.is_object() || !src.is_object() {
        return;
    }
    let (Value::Object(destination), Value::Object(source)) = (&mut *dst, src) else {
        return;
    };

    for (key, source_value) in source {
        // `typeof null === "object"`, and assignWithDepth returns without setting it.
        if source_value.is_null() {
            continue;
        }
        if source_value.is_object() || source_value.is_array() {
            match destination.get_mut(key) {
                None => {
                    let mut fresh = if source_value.is_array() {
                        Value::Array(Vec::new())
                    } else {
                        Value::Object(Map::new())
                    };
                    assign_with_depth(&mut fresh, source_value, depth - 1);
                    destination.insert(key.clone(), fresh);
                }
                Some(slot) if slot.is_object() || slot.is_array() || slot.is_null() => {
                    assign_with_depth(slot, source_value, depth - 1);
                }
                // A scalar destination is not replaced by an object, as upstream does not.
                Some(_) => {}
            }
        } else {
            match destination.get_mut(key) {
                // `typeof null === "object"`: a scalar does not replace null either.
                Some(slot) if !slot.is_object() && !slot.is_array() && !slot.is_null() => {
                    *slot = source_value.clone();
                }
                Some(_) => {}
                None => {
                    destination.insert(key.clone(), source_value.clone());
                }
            }
        }
    }
}

/// The `sanitize` in `config.ts`: drops secure keys (at every level), `__`-prefixed keys,
/// and strings that could inject markup or a data URL.
fn sanitize(options: &mut Value, secure: &[String]) {
    match options {
        Value::Object(map) => {
            for key in secure {
                map.remove(key);
            }
            let keys: Vec<String> = map
                .keys()
                .filter(|key| key.starts_with("__"))
                .cloned()
                .collect();
            for key in keys {
                map.remove(&key);
            }
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let dangerous = match map.get(&key) {
                    Some(Value::String(text)) => {
                        text.contains('<') || text.contains('>') || text.contains("url(data:")
                    }
                    _ => false,
                };
                if dangerous {
                    map.remove(&key);
                    continue;
                }
                if let Some(value) = map.get_mut(&key)
                    && (value.is_object() || value.is_array())
                {
                    sanitize(value, secure);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if let Value::String(text) = item
                    && (text.contains('<') || text.contains('>') || text.contains("url(data:"))
                {
                    // Deleting an array member leaves a hole upstream, which JSON writes as null.
                    *item = Value::Null;
                } else if item.is_object() || item.is_array() {
                    sanitize(item, secure);
                }
            }
        }
        _ => {}
    }
}

/// The whole `MermaidConfig`, every section typed and defaulted.
///
/// Only the five themes in [`crate::theme`] are implemented; a `theme` naming another
/// upstream theme is kept as text and does not rebuild `theme_variables`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// The theme's variables. Built for [`Config::theme`] when it changes.
    pub theme_variables: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "themeCSS")]
    pub theme_css: Option<String>,
    /// Which main look to use.
    pub look: String,
    pub hand_drawn_seed: f64,
    /// Layout engine. Defaults to `elk`.
    pub layout: String,
    /// The grid layout's configuration block, read by `readGridConfig` when `layout` is `grid`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<serde_json::Value>,
    pub max_text_size: f64,
    pub max_edges: f64,
    pub elk: ElkConfig,
    pub dark_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_labels: Option<bool>,
    pub font_family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_font_family: Option<String>,
    pub log_level: NumberOrString,
    pub security_level: String,
    pub start_on_load: bool,
    pub arrow_marker_absolute: bool,
    /// Keys a directive may not change.
    pub secure: Vec<String>,
    #[serde(rename = "legacyMathML")]
    pub legacy_math_ml: bool,
    #[serde(rename = "forceLegacyMathML")]
    pub force_legacy_math_ml: bool,
    pub deterministic_ids: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "deterministicIDSeed")]
    pub deterministic_id_seed: Option<String>,
    pub flowchart: FlowchartConfig,
    pub swimlane: SwimlaneConfig,
    pub agentflow: AgentflowConfig,
    pub sequence: SequenceConfig,
    pub gantt: GanttConfig,
    pub journey: JourneyConfig,
    pub timeline: TimelineConfig,
    pub class: ClassConfig,
    pub state: StateConfig,
    pub er: ErConfig,
    pub pie: PieConfig,
    pub quadrant_chart: QuadrantChartConfig,
    pub xy_chart: XYChartConfig,
    pub requirement: RequirementConfig,
    pub architecture: ArchitectureConfig,
    pub mindmap: MindmapConfig,
    pub ishikawa: IshikawaConfig,
    pub kanban: KanbanConfig,
    pub git_graph: GitGraphConfig,
    pub c4: C4Config,
    pub sankey: SankeyConfig,
    pub packet: PacketConfig,
    pub block: BlockConfig,
    pub eventmodeling: EventmodelingConfig,
    pub tree_view: TreeViewConfig,
    pub radar: RadarConfig,
    pub usecase: UsecaseConfig,
    pub venn: VennConfig,
    /// Upstream's key is `wardley-beta`.
    #[serde(rename = "wardley-beta")]
    pub wardley_beta: WardleyConfig,
    pub cynefin: CynefinConfig,
    pub railroad: RailroadConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dompurify_config: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    pub font_size: f64,
    pub markdown_auto_wrap: bool,
    pub suppress_error_rendering: bool,
    /// Treemap has no schema section; `defaultConfig.ts` adds it whole.
    pub treemap: TreemapConfig,
    /// themeVariables exactly as the user wrote them, so a later theme change can rebuild
    /// `theme_variables` without feeding the generated ones back in.
    #[serde(skip)]
    theme_variables_override: Map<String, Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            theme_variables: Theme::Default
                .variables(&Map::new())
                .unwrap_or_default()
                .into_map(),
            theme_css: None,
            look: "classic".to_string(),
            hand_drawn_seed: 0.0,
            layout: "elk".to_string(),
            grid: None,
            max_text_size: 50000.0,
            max_edges: 500.0,
            elk: ElkConfig::default(),
            dark_mode: false,
            html_labels: None,
            font_family: "\"trebuchet ms\", verdana, arial, sans-serif;".to_string(),
            alt_font_family: None,
            log_level: NumberOrString::Number(5.0),
            security_level: "strict".to_string(),
            start_on_load: true,
            arrow_marker_absolute: false,
            secure: vec![
                "secure".to_string(),
                "securityLevel".to_string(),
                "startOnLoad".to_string(),
                "maxTextSize".to_string(),
                "suppressErrorRendering".to_string(),
                "maxEdges".to_string(),
            ],
            legacy_math_ml: false,
            force_legacy_math_ml: false,
            deterministic_ids: false,
            deterministic_id_seed: None,
            flowchart: FlowchartConfig::default(),
            swimlane: SwimlaneConfig::default(),
            agentflow: AgentflowConfig::default(),
            sequence: SequenceConfig::default(),
            gantt: GanttConfig::default(),
            journey: JourneyConfig::default(),
            timeline: TimelineConfig::default(),
            class: ClassConfig::default(),
            state: StateConfig::default(),
            er: ErConfig::default(),
            pie: PieConfig::default(),
            quadrant_chart: QuadrantChartConfig::default(),
            xy_chart: XYChartConfig::default(),
            requirement: RequirementConfig::default(),
            architecture: ArchitectureConfig::default(),
            mindmap: MindmapConfig::default(),
            ishikawa: IshikawaConfig::default(),
            kanban: KanbanConfig::default(),
            git_graph: GitGraphConfig::default(),
            c4: C4Config::default(),
            sankey: SankeyConfig::default(),
            packet: PacketConfig::default(),
            block: BlockConfig::default(),
            eventmodeling: EventmodelingConfig::default(),
            tree_view: TreeViewConfig::default(),
            radar: RadarConfig::default(),
            usecase: UsecaseConfig::default(),
            venn: VennConfig::default(),
            wardley_beta: WardleyConfig::default(),
            cynefin: CynefinConfig::default(),
            railroad: RailroadConfig::default(),
            dompurify_config: None,
            wrap: None,
            font_size: 16.0,
            markdown_auto_wrap: true,
            suppress_error_rendering: false,
            treemap: TreemapConfig::default(),
            theme_variables_override: Map::new(),
        }
    }
}

impl Config {
    /// The defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// The configuration for a preprocessed source's `config` (`frontmatter` plus every
    /// directive, already merged by [`crate::preprocess`]).
    pub fn from_source_config(source: &Map<String, Value>) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        config.apply(source)?;
        Ok(config)
    }

    /// Merges one directive over this configuration, as `updateCurrentConfig` does after
    /// `sanitize`: secure keys and markup-bearing strings are dropped, then
    /// [`assign_with_depth`] runs at upstream's depth of two.
    ///
    /// A theme named by the directive rebuilds [`Config::theme_variables`] from the
    /// `themeVariables` the user has written so far. Later directives win.
    pub fn apply(&mut self, directive: &Map<String, Value>) -> Result<(), ConfigError> {
        let mut value = Value::Object(directive.clone());
        let mut secure = vec!["secure".to_string()];
        secure.extend(self.secure.iter().cloned());
        sanitize(&mut value, &secure);
        if !value.is_object() {
            return Ok(());
        }

        let mut merged = self.to_json();
        assign_with_depth(&mut merged, &value, 2);
        let mut next: Config = serde_json::from_value(merged)
            .map_err(|error| ConfigError::new(format!("invalid configuration: {error}")))?;
        // `to_json` leaves the user's theme originals out, so carry them over by hand.
        next.theme_variables_override = self.theme_variables_override.clone();

        if let Some(Value::Object(theme_variables)) = value.get("themeVariables") {
            let mut overrides = Value::Object(std::mem::take(&mut next.theme_variables_override));
            assign_with_depth(&mut overrides, &Value::Object(theme_variables.clone()), 2);
            if let Value::Object(overrides) = overrides {
                next.theme_variables_override = overrides;
            }
        }

        if let Some(theme_name) = value.get("theme").and_then(Value::as_str)
            && !theme_name.is_empty()
            && let Some(theme) = Theme::from_name(theme_name)
        {
            next.theme_variables = theme
                .variables(&next.theme_variables_override)
                .map_err(ConfigError::from)?
                .into_map();
        }

        *self = next;
        Ok(())
    }

    /// Applies directives in order, each on top of the previous one.
    pub fn with_directives(
        directives: impl IntoIterator<Item = Map<String, Value>>,
    ) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        for directive in directives {
            config.apply(&directive)?;
        }
        Ok(config)
    }

    /// The configuration as JSON, as `getConfig()` would return it (without the function
    /// valued sequence and c4 font builders, which are methods here).
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// `getEffectiveHtmlLabels`: the root `htmlLabels`, then `flowchart.htmlLabels`, then
    /// true.
    pub fn effective_html_labels(&self) -> bool {
        self.html_labels
            .or(self.flowchart.html_labels)
            .unwrap_or(true)
    }
}

// ---- nested ----

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SubGraphTitleMargin {
    pub top: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GitGraphNodeLabel {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
}

impl Default for GitGraphNodeLabel {
    fn default() -> Self {
        Self {
            width: 75.0,
            height: 100.0,
            x: -25.0,
            y: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct XYChartAxisConfig {
    pub show_label: bool,
    pub label_font_size: f64,
    pub label_padding: f64,
    pub show_title: bool,
    pub title_font_size: f64,
    pub title_padding: f64,
    pub show_tick: bool,
    pub tick_length: f64,
    pub tick_width: f64,
    pub show_axis_line: bool,
    pub axis_line_width: f64,
    pub label_rotation: f64,
}

impl Default for XYChartAxisConfig {
    fn default() -> Self {
        Self {
            show_label: true,
            label_font_size: 14.0,
            label_padding: 5.0,
            show_title: true,
            title_font_size: 16.0,
            title_padding: 5.0,
            show_tick: true,
            tick_length: 5.0,
            tick_width: 2.0,
            show_axis_line: true,
            axis_line_width: 2.0,
            label_rotation: 0.0,
        }
    }
}

// ---- elk ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ElkConfig {
    /// Elk specific option that allows edges to share path where it convenient.
    pub merge_edges: bool,
    /// Elk specific option affecting how nodes are placed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_placement_strategy: Option<String>,
    /// Elk specific option affecting Brandes-Koepf node placement alignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_placement_alignment: Option<String>,
    /// Named combination of layering, node placement, placement alignment
    pub preset: String,
    /// Straightens an edge that leaves or enters a node with a tiny step.
    pub straighten_edges: bool,
    /// Renders edge crossings as small arcs ("hops") or visible gaps, so that
    pub line_hops: LineHops,
    /// Elk specific option deciding which layer each node is assigned to — the
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layering_strategy: Option<String>,
    /// Elk specific option capping how many nodes COFFMAN_GRAHAM will put in
    pub layering_layer_bound: f64,
    /// This strategy decides how to find cycles in the graph and deciding which edges need adjustment to break loops.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_breaking_strategy: Option<String>,
    /// The node order given by the model does not change to produce a better layout.
    pub force_node_model_order: bool,
    /// Preserves the order of nodes and edges in the model file if this does not lead to additional edge crossings.
    pub consider_model_order: String,
    /// Elk specific option that keeps the entry node of a recursive flow at the top of the layout.
    pub keep_entry_node_on_top: bool,
    /// Routes an edge that re-enters a subgraph downstream instead of around the subgraph.
    pub orient_feedback_edges: bool,
}

impl Default for ElkConfig {
    fn default() -> Self {
        Self {
            merge_edges: false,
            node_placement_strategy: None,
            node_placement_alignment: None,
            preset: "default".to_string(),
            straighten_edges: true,
            line_hops: LineHops::Enabled(true),
            layering_strategy: None,
            layering_layer_bound: 4.0,
            cycle_breaking_strategy: None,
            force_node_model_order: false,
            consider_model_order: "NODES_AND_EDGES".to_string(),
            keep_entry_node_on_top: false,
            orient_feedback_edges: true,
        }
    }
}

// ---- flowchart ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FlowchartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin top for the text over the diagram
    pub title_top_margin: f64,
    /// Defines a top/bottom margin for subgraph titles
    pub sub_graph_title_margin: SubGraphTitleMargin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_marker_absolute: Option<bool>,
    /// The amount of padding around the diagram as a whole so that embedded
    pub diagram_padding: f64,
    /// **DEPRECATED: Use global `htmlLabels` instead.**
    pub html_labels: Option<bool>,
    /// Defines the spacing between nodes on the same level
    pub node_spacing: f64,
    /// Defines the spacing between nodes on different levels
    pub rank_spacing: f64,
    /// Defines how mermaid renders curves for flowcharts.
    pub curve: String,
    /// Represents the padding between the labels and the shape
    pub padding: f64,
    /// Width of nodes where text is wrapped.
    pub wrapping_width: f64,
    /// Minimum width of the label area of a node.
    pub min_node_width: f64,
    /// Seconds the travelling dot spends on each step of the step-by-step
    pub step_duration: f64,
    /// If true, each edge with a `step` number shows that number in a small
    pub step_numbers: bool,
    /// If true, subgraphs without explicit direction will inherit the global graph direction
    pub inherit_dir: bool,
}

impl Default for FlowchartConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            title_top_margin: 25.0,
            sub_graph_title_margin: SubGraphTitleMargin::default(),
            arrow_marker_absolute: None,
            diagram_padding: 8.0,
            html_labels: None,
            node_spacing: 50.0,
            rank_spacing: 50.0,
            curve: "basis".to_string(),
            padding: 15.0,
            wrapping_width: 120.0,
            min_node_width: 120.0,
            step_duration: 1.0,
            step_numbers: false,
            inherit_dir: false,
        }
    }
}

// ---- swimlane ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SwimlaneConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    pub layout: String,
    /// Renders edge crossings as small arcs ("hops") or visible gaps so that
    pub line_hops: LineHops,
    /// Ignores edges that cross swimlane boundaries during swimlane layer
    pub ignore_cross_lane_edges: bool,
    /// Enables a crossing-aware rank optimization pass for swimlane layouts.
    pub optimize_ranks_by_crossings: bool,
    /// Automatically reorders top-level swimlanes with a deterministic
    pub automatic_lane_ordering: bool,
}

impl Default for SwimlaneConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: "swimlane".to_string(),
            line_hops: LineHops::Kind(LineHopsKind::Arc),
            ignore_cross_lane_edges: true,
            optimize_ranks_by_crossings: true,
            automatic_lane_ordering: false,
        }
    }
}

// ---- agentflow ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentflowConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin top for the text over the diagram
    pub title_top_margin: f64,
    /// The amount of padding around the diagram as a whole so that embedded
    pub diagram_padding: f64,
    /// Defines the spacing between nodes on the same level.
    pub node_spacing: f64,
    /// Defines the spacing between nodes on different levels.
    pub rank_spacing: f64,
    /// Width of nodes where text is wrapped.
    pub wrapping_width: f64,
    /// Minimum width of the label area of a node.
    pub min_node_width: f64,
}

impl Default for AgentflowConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            title_top_margin: 25.0,
            diagram_padding: 8.0,
            node_spacing: 50.0,
            rank_spacing: 50.0,
            wrapping_width: 120.0,
            min_node_width: 120.0,
        }
    }
}

// ---- sequence ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SequenceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_marker_absolute: Option<bool>,
    pub hide_unused_participants: bool,
    /// Width of the activation rect
    pub activation_width: f64,
    /// Margin to the right and left of the sequence diagram
    pub diagram_margin_x: f64,
    /// Margin to the over and under the sequence diagram
    pub diagram_margin_y: f64,
    /// Margin between actors
    pub actor_margin: f64,
    /// Width of actor boxes
    pub width: f64,
    /// Height of actor boxes
    pub height: f64,
    /// Margin around loop boxes
    pub box_margin: f64,
    /// Margin around the text in loop/alt/opt boxes
    pub box_text_margin: f64,
    /// Margin around notes
    pub note_margin: f64,
    /// Space between messages.
    pub message_margin: f64,
    /// Multiline message alignment
    pub message_align: String,
    /// Mirror actors under diagram
    pub mirror_actors: bool,
    /// forces actor popup menus to always be visible (to support E2E testing).
    pub force_menus: bool,
    /// Prolongs the edge of the diagram downwards.
    pub bottom_margin_adj: f64,
    /// Curved Arrows become Right Angles
    pub right_angles: bool,
    /// This will show the node numbers
    pub show_sequence_numbers: bool,
    /// This sets the font size of the actor's description
    pub actor_font_size: NumberOrString,
    /// This sets the font family of the actor's description
    pub actor_font_family: String,
    /// This sets the font weight of the actor's description
    pub actor_font_weight: NumberOrString,
    /// This sets the font size of actor-attached notes
    pub note_font_size: NumberOrString,
    /// This sets the font family of actor-attached notes
    pub note_font_family: String,
    /// This sets the font weight of actor-attached notes
    pub note_font_weight: NumberOrString,
    /// This sets the text alignment of actor-attached notes
    pub note_align: String,
    /// This sets the font size of actor messages
    pub message_font_size: NumberOrString,
    /// This sets the font family of actor messages
    pub message_font_family: String,
    /// This sets the font weight of actor messages
    pub message_font_weight: NumberOrString,
    /// This sets the auto-wrap state for the diagram
    pub wrap: bool,
    /// This sets the auto-wrap padding for the diagram (sides only)
    pub wrap_padding: f64,
    /// This sets the width of the loop-box (loop, alt, opt, par)
    pub label_box_width: f64,
    /// This sets the height of the loop-box (loop, alt, opt, par)
    pub label_box_height: f64,
}

impl Default for SequenceConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            arrow_marker_absolute: None,
            hide_unused_participants: false,
            activation_width: 10.0,
            diagram_margin_x: 50.0,
            diagram_margin_y: 10.0,
            actor_margin: 50.0,
            width: 150.0,
            height: 65.0,
            box_margin: 10.0,
            box_text_margin: 5.0,
            note_margin: 10.0,
            message_margin: 35.0,
            message_align: "center".to_string(),
            mirror_actors: true,
            force_menus: false,
            bottom_margin_adj: 1.0,
            right_angles: false,
            show_sequence_numbers: false,
            actor_font_size: NumberOrString::Number(14.0),
            actor_font_family: "\"Open Sans\", sans-serif".to_string(),
            actor_font_weight: NumberOrString::Number(400.0),
            note_font_size: NumberOrString::Number(14.0),
            note_font_family: "\"trebuchet ms\", verdana, arial, sans-serif".to_string(),
            note_font_weight: NumberOrString::Number(400.0),
            note_align: "center".to_string(),
            message_font_size: NumberOrString::Number(16.0),
            message_font_family: "\"trebuchet ms\", verdana, arial, sans-serif".to_string(),
            message_font_weight: NumberOrString::Number(400.0),
            wrap: false,
            wrap_padding: 10.0,
            label_box_width: 50.0,
            label_box_height: 20.0,
        }
    }
}

// ---- gantt ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GanttConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin top for the text over the diagram
    pub title_top_margin: f64,
    /// The height of the bars in the graph
    pub bar_height: f64,
    /// The margin between the different activities in the gantt diagram
    pub bar_gap: f64,
    /// Margin between title and gantt diagram and between axis and gantt diagram.
    pub top_padding: f64,
    /// The space allocated for the section name to the right of the activities
    pub right_padding: f64,
    /// The space allocated for the section name to the left of the activities
    pub left_padding: f64,
    /// Vertical starting position of the grid lines
    pub grid_line_start_padding: f64,
    /// Font size
    pub font_size: f64,
    /// Font size for sections
    pub section_font_size: f64,
    /// The number of alternating section styles
    pub number_section_styles: f64,
    /// Date/time format of the axis
    pub axis_format: String,
    /// axis ticks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_interval: Option<String>,
    /// When this flag is set, date labels will be added to the top of the chart
    pub top_axis: bool,
    /// Controls the display mode.
    pub display_mode: String,
    /// On which day a week-based interval should start
    pub weekday: String,
}

impl Default for GanttConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            title_top_margin: 25.0,
            bar_height: 20.0,
            bar_gap: 4.0,
            top_padding: 50.0,
            right_padding: 75.0,
            left_padding: 75.0,
            grid_line_start_padding: 35.0,
            font_size: 11.0,
            section_font_size: 11.0,
            number_section_styles: 4.0,
            axis_format: "%Y-%m-%d".to_string(),
            tick_interval: None,
            top_axis: false,
            display_mode: "".to_string(),
            weekday: "sunday".to_string(),
        }
    }
}

// ---- journey ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JourneyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin to the right and left of the c4 diagram, must be a positive value.
    pub diagram_margin_x: f64,
    /// Margin to the over and under the c4 diagram, must be a positive value.
    pub diagram_margin_y: f64,
    /// Margin between actors
    pub left_margin: f64,
    /// Maximum width of actor labels
    pub max_label_width: f64,
    /// Width of actor boxes
    pub width: f64,
    /// Height of actor boxes
    pub height: f64,
    /// Margin around loop boxes
    pub box_margin: f64,
    /// Margin around the text in loop/alt/opt boxes
    pub box_text_margin: f64,
    /// Margin around notes
    pub note_margin: f64,
    /// Space between messages.
    pub message_margin: f64,
    /// Multiline message alignment
    pub message_align: String,
    /// Prolongs the edge of the diagram downwards.
    pub bottom_margin_adj: f64,
    /// Curved Arrows become Right Angles
    pub right_angles: bool,
    pub task_font_size: NumberOrString,
    pub task_font_family: String,
    pub task_margin: f64,
    /// Width of activation box
    pub activation_width: f64,
    /// text placement as: tspan | fo | old only text as before
    pub text_placement: String,
    pub actor_colours: Vec<String>,
    pub section_fills: Vec<String>,
    pub section_colours: Vec<String>,
    /// Color of the title text in Journey Diagrams
    pub title_color: String,
    /// Font family to be used for the title text in Journey Diagrams
    pub title_font_family: String,
    /// Font size to be used for the title text in Journey Diagrams
    pub title_font_size: String,
}

impl Default for JourneyConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            diagram_margin_x: 50.0,
            diagram_margin_y: 10.0,
            left_margin: 150.0,
            max_label_width: 360.0,
            width: 150.0,
            height: 50.0,
            box_margin: 10.0,
            box_text_margin: 5.0,
            note_margin: 10.0,
            message_margin: 35.0,
            message_align: "center".to_string(),
            bottom_margin_adj: 1.0,
            right_angles: false,
            task_font_size: NumberOrString::Number(14.0),
            task_font_family: "\"Open Sans\", sans-serif".to_string(),
            task_margin: 50.0,
            activation_width: 10.0,
            text_placement: "fo".to_string(),
            actor_colours: vec![
                "#8FBC8F".to_string(),
                "#7CFC00".to_string(),
                "#00FFFF".to_string(),
                "#20B2AA".to_string(),
                "#B0E0E6".to_string(),
                "#FFFFE0".to_string(),
            ],
            section_fills: vec![
                "#191970".to_string(),
                "#8B008B".to_string(),
                "#4B0082".to_string(),
                "#2F4F4F".to_string(),
                "#800000".to_string(),
                "#8B4513".to_string(),
                "#00008B".to_string(),
            ],
            section_colours: vec!["#fff".to_string()],
            title_color: "".to_string(),
            title_font_family: "\"trebuchet ms\", verdana, arial, sans-serif".to_string(),
            title_font_size: "4ex".to_string(),
        }
    }
}

// ---- timeline ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimelineConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin to the right and left of the c4 diagram, must be a positive value.
    pub diagram_margin_x: f64,
    /// Margin to the over and under the c4 diagram, must be a positive value.
    pub diagram_margin_y: f64,
    /// Margin between actors
    pub left_margin: f64,
    /// Width of actor boxes
    pub width: f64,
    /// Height of actor boxes
    pub height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
    /// Margin around loop boxes
    pub box_margin: f64,
    /// Margin around the text in loop/alt/opt boxes
    pub box_text_margin: f64,
    /// Margin around notes
    pub note_margin: f64,
    /// Space between messages.
    pub message_margin: f64,
    /// Multiline message alignment
    pub message_align: String,
    /// Prolongs the edge of the diagram downwards.
    pub bottom_margin_adj: f64,
    /// Curved Arrows become Right Angles
    pub right_angles: bool,
    pub task_font_size: NumberOrString,
    pub task_font_family: String,
    pub task_margin: f64,
    /// Width of activation box
    pub activation_width: f64,
    /// text placement as: tspan | fo | old only text as before
    pub text_placement: String,
    pub actor_colours: Vec<String>,
    pub section_fills: Vec<String>,
    pub section_colours: Vec<String>,
    pub disable_multicolor: bool,
}

impl Default for TimelineConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            diagram_margin_x: 50.0,
            diagram_margin_y: 10.0,
            left_margin: 150.0,
            width: 150.0,
            height: 50.0,
            padding: None,
            box_margin: 10.0,
            box_text_margin: 5.0,
            note_margin: 10.0,
            message_margin: 35.0,
            message_align: "center".to_string(),
            bottom_margin_adj: 1.0,
            right_angles: false,
            task_font_size: NumberOrString::Number(14.0),
            task_font_family: "\"Open Sans\", sans-serif".to_string(),
            task_margin: 50.0,
            activation_width: 10.0,
            text_placement: "fo".to_string(),
            actor_colours: vec![
                "#8FBC8F".to_string(),
                "#7CFC00".to_string(),
                "#00FFFF".to_string(),
                "#20B2AA".to_string(),
                "#B0E0E6".to_string(),
                "#FFFFE0".to_string(),
            ],
            section_fills: vec![
                "#191970".to_string(),
                "#8B008B".to_string(),
                "#4B0082".to_string(),
                "#2F4F4F".to_string(),
                "#800000".to_string(),
                "#8B4513".to_string(),
                "#00008B".to_string(),
            ],
            section_colours: vec!["#fff".to_string()],
            disable_multicolor: false,
        }
    }
}

// ---- class ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClassConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_max_width: Option<bool>,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin top for the text over the diagram
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_top_margin: Option<f64>,
    /// Controls whether or arrow markers in html code are absolute paths or anchors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_marker_absolute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divider_margin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_spacing: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_spacing: Option<f64>,
    /// The amount of padding around the diagram as a whole so that embedded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_padding: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_labels: Option<bool>,
    pub hide_empty_members_box: bool,
    /// When true (default), nested namespaces render as hierarchical clusters,
    pub hierarchical_namespaces: bool,
}

impl Default for ClassConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: None,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            title_top_margin: None,
            arrow_marker_absolute: None,
            divider_margin: None,
            padding: None,
            text_height: None,
            node_spacing: None,
            rank_spacing: None,
            diagram_padding: None,
            html_labels: None,
            hide_empty_members_box: false,
            hierarchical_namespaces: true,
        }
    }
}

// ---- state ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StateConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    pub layout: String,
    /// Width of nodes where text is wrapped.
    pub wrapping_width: f64,
    /// Minimum width of the label area of a node.
    pub min_node_width: f64,
    /// Margin top for the text over the diagram
    pub title_top_margin: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_marker_absolute: Option<bool>,
    pub divider_margin: f64,
    pub size_unit: f64,
    pub padding: f64,
    pub text_height: f64,
    pub title_shift: f64,
    pub note_margin: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_spacing: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_spacing: Option<f64>,
    pub fork_width: f64,
    pub fork_height: f64,
    pub mini_padding: f64,
    /// Font size factor, this is used to guess the width of the edges labels
    pub font_size_factor: f64,
    pub font_size: f64,
    pub label_height: f64,
    pub edge_length_factor: String,
    pub composit_title_size: f64,
    pub radius: f64,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: "elk".to_string(),
            wrapping_width: 120.0,
            min_node_width: 120.0,
            title_top_margin: 25.0,
            arrow_marker_absolute: None,
            divider_margin: 10.0,
            size_unit: 5.0,
            padding: 8.0,
            text_height: 10.0,
            title_shift: -15.0,
            note_margin: 10.0,
            node_spacing: None,
            rank_spacing: None,
            fork_width: 70.0,
            fork_height: 7.0,
            mini_padding: 2.0,
            font_size_factor: 5.02,
            font_size: 24.0,
            label_height: 16.0,
            edge_length_factor: "20".to_string(),
            composit_title_size: 35.0,
            radius: 5.0,
        }
    }
}

// ---- er ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ErConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin top for the text over the diagram
    pub title_top_margin: f64,
    /// The amount of padding around the diagram as a whole so that embedded
    pub diagram_padding: f64,
    /// Directional bias for layout of entities
    pub layout_direction: String,
    /// The minimum width of an entity box.
    pub min_entity_width: f64,
    /// The minimum height of an entity box.
    pub min_entity_height: f64,
    /// The minimum internal padding between text in an entity box and the enclosing box borders.
    pub entity_padding: f64,
    pub node_spacing: f64,
    pub rank_spacing: f64,
    /// Stroke color of box edges and lines.
    pub stroke: String,
    /// Fill color of entity boxes
    pub fill: String,
    /// Font size (expressed as an integer representing a number of pixels)
    pub font_size: f64,
}

impl Default for ErConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            title_top_margin: 25.0,
            diagram_padding: 20.0,
            layout_direction: "TB".to_string(),
            min_entity_width: 100.0,
            min_entity_height: 75.0,
            entity_padding: 15.0,
            node_spacing: 140.0,
            rank_spacing: 80.0,
            stroke: "gray".to_string(),
            fill: "honeydew".to_string(),
            font_size: 12.0,
        }
    }
}

// ---- pie ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PieConfig {
    pub use_width: f64,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Axial position of slice's label from zero at the center to 1 at the outside edges.
    pub text_position: f64,
    /// Donut hole ratio.
    pub donut_hole: f64,
    /// Legend's position relative to the chart.
    pub legend_position: String,
    /// Highlight specific slice with matching label.
    pub highlight_slice: String,
}

impl Default for PieConfig {
    fn default() -> Self {
        Self {
            use_width: 984.0,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            text_position: 0.75,
            donut_hole: 0.0,
            legend_position: "right".to_string(),
            highlight_slice: "".to_string(),
        }
    }
}

// ---- quadrantChart ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QuadrantChartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Width of the chart
    pub chart_width: f64,
    /// Height of the chart
    pub chart_height: f64,
    /// Chart title top and bottom padding
    pub title_font_size: f64,
    /// Padding around the quadrant square
    pub title_padding: f64,
    /// quadrant title padding from top if the quadrant is rendered on top
    pub quadrant_padding: f64,
    /// Padding around x-axis labels
    pub x_axis_label_padding: f64,
    /// Padding around y-axis labels
    pub y_axis_label_padding: f64,
    /// x-axis label font size
    pub x_axis_label_font_size: f64,
    /// y-axis label font size
    pub y_axis_label_font_size: f64,
    /// quadrant title font size
    pub quadrant_label_font_size: f64,
    /// quadrant title padding from top if the quadrant is rendered on top
    pub quadrant_text_top_padding: f64,
    /// padding between point and point label
    pub point_text_padding: f64,
    /// point title font size
    pub point_label_font_size: f64,
    /// radius of the point to be drawn
    pub point_radius: f64,
    /// position of x-axis labels
    pub x_axis_position: String,
    /// position of y-axis labels
    pub y_axis_position: String,
    /// stroke width of edges of the box that are inside the quadrant
    pub quadrant_internal_border_stroke_width: f64,
    /// stroke width of edges of the box that are outside the quadrant
    pub quadrant_external_border_stroke_width: f64,
}

impl Default for QuadrantChartConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            chart_width: 500.0,
            chart_height: 500.0,
            title_font_size: 20.0,
            title_padding: 10.0,
            quadrant_padding: 5.0,
            x_axis_label_padding: 5.0,
            y_axis_label_padding: 5.0,
            x_axis_label_font_size: 16.0,
            y_axis_label_font_size: 16.0,
            quadrant_label_font_size: 16.0,
            quadrant_text_top_padding: 5.0,
            point_text_padding: 5.0,
            point_label_font_size: 12.0,
            point_radius: 5.0,
            x_axis_position: "top".to_string(),
            y_axis_position: "left".to_string(),
            quadrant_internal_border_stroke_width: 1.0,
            quadrant_external_border_stroke_width: 2.0,
        }
    }
}

// ---- xyChart ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct XYChartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// width of the chart
    pub width: f64,
    /// height of the chart
    pub height: f64,
    /// Font size of the chart title
    pub title_font_size: f64,
    /// Top and bottom space from the chart title
    pub title_padding: f64,
    /// Should show the value corresponding to the bar within the bar
    pub show_data_label: bool,
    /// If showing data label then show it outside the bar
    pub show_data_label_outside_bar: bool,
    /// Should show the chart title
    pub show_title: bool,
    /// Should show a legend for named plots
    pub show_legend: bool,
    /// Font size of the legend text
    pub legend_font_size: f64,
    /// Padding around the legend
    pub legend_padding: f64,
    /// This object contains configuration for XYChart axis config
    pub x_axis: XYChartAxisConfig,
    /// This object contains configuration for XYChart axis config
    pub y_axis: XYChartAxisConfig,
    /// How to plot will be drawn horizontal or vertical
    pub chart_orientation: String,
    /// Minimum percent of space plots of the chart will take
    pub plot_reserved_space_percent: f64,
}

impl Default for XYChartConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            width: 700.0,
            height: 500.0,
            title_font_size: 20.0,
            title_padding: 10.0,
            show_data_label: false,
            show_data_label_outside_bar: false,
            show_title: true,
            show_legend: true,
            legend_font_size: 14.0,
            legend_padding: 10.0,
            x_axis: XYChartAxisConfig::default(),
            y_axis: XYChartAxisConfig::default(),
            chart_orientation: "vertical".to_string(),
            plot_reserved_space_percent: 50.0,
        }
    }
}

// ---- requirement ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RequirementConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(rename = "rect_fill")]
    pub rect_fill: String,
    #[serde(rename = "text_color")]
    pub text_color: String,
    #[serde(rename = "rect_border_size")]
    pub rect_border_size: String,
    #[serde(rename = "rect_border_color")]
    pub rect_border_color: String,
    #[serde(rename = "rect_min_width")]
    pub rect_min_width: f64,
    #[serde(rename = "rect_min_height")]
    pub rect_min_height: f64,
    pub font_size: f64,
    #[serde(rename = "rect_padding")]
    pub rect_padding: f64,
    #[serde(rename = "line_height")]
    pub line_height: f64,
}

impl Default for RequirementConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            rect_fill: "#f9f9f9".to_string(),
            text_color: "#333".to_string(),
            rect_border_size: "0.5px".to_string(),
            rect_border_color: "#bbb".to_string(),
            rect_min_width: 200.0,
            rect_min_height: 200.0,
            font_size: 14.0,
            rect_padding: 10.0,
            line_height: 20.0,
        }
    }
}

// ---- architecture ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ArchitectureConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    pub padding: f64,
    pub icon_size: f64,
    pub font_size: f64,
    /// Whether to randomize initial node positions before running the layout algorithm.
    pub randomize: bool,
    /// Minimum separation (in pixels) between sibling nodes in the same group, passed through to the
    pub node_separation: f64,
    /// Multiplier applied to `iconSize` to compute the ideal length of edges between nodes within the
    pub ideal_edge_length_multiplier: f64,
    /// Spring elasticity (0–1) applied to edges between nodes within the same group, passed through to
    pub edge_elasticity: f64,
    /// Maximum number of iterations the fcose layout algorithm runs before stopping.
    pub num_iter: f64,
    /// Deterministic seed for the fcose layout.
    pub seed: f64,
}

impl Default for ArchitectureConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            padding: 40.0,
            icon_size: 80.0,
            font_size: 16.0,
            randomize: false,
            node_separation: 75.0,
            ideal_edge_length_multiplier: 1.5,
            edge_elasticity: 0.45,
            num_iter: 2500.0,
            seed: 1.0,
        }
    }
}

// ---- mindmap ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MindmapConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Color nodes by their top-level branch or by their depth in the tree.
    pub color_by: String,
    pub padding: f64,
    pub max_node_width: f64,
    /// Layout algorithm to use for positioning mindmap nodes
    pub layout_algorithm: String,
}

impl Default for MindmapConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            color_by: "branch".to_string(),
            padding: 10.0,
            max_node_width: 200.0,
            layout_algorithm: "cose-bilkent".to_string(),
        }
    }
}

// ---- ishikawa ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct IshikawaConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The amount of padding around the diagram as a whole so that embedded
    pub diagram_padding: f64,
}

impl Default for IshikawaConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: false,
            theme: None,
            look: None,
            layout: None,
            diagram_padding: 20.0,
        }
    }
}

// ---- kanban ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KanbanConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    pub padding: f64,
    pub section_width: f64,
    pub ticket_base_url: String,
}

impl Default for KanbanConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            padding: 8.0,
            section_width: 200.0,
            ticket_base_url: "".to_string(),
        }
    }
}

// ---- gitGraph ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GitGraphConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin top for the text over the diagram
    pub title_top_margin: f64,
    pub diagram_padding: f64,
    pub node_label: GitGraphNodeLabel,
    pub main_branch_name: String,
    pub main_branch_order: f64,
    pub show_commit_label: bool,
    pub show_branches: bool,
    pub rotate_commit_label: bool,
    pub parallel_commits: bool,
    /// Controls whether or arrow markers in html code are absolute paths or anchors.
    pub arrow_marker_absolute: bool,
}

impl Default for GitGraphConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            title_top_margin: 25.0,
            diagram_padding: 8.0,
            node_label: GitGraphNodeLabel::default(),
            main_branch_name: "main".to_string(),
            main_branch_order: 0.0,
            show_commit_label: true,
            show_branches: true,
            rotate_commit_label: true,
            parallel_commits: false,
            arrow_marker_absolute: false,
        }
    }
}

// ---- c4 ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct C4Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Margin to the right and left of the c4 diagram, must be a positive value.
    pub diagram_margin_x: f64,
    /// Margin to the over and under the c4 diagram, must be a positive value.
    pub diagram_margin_y: f64,
    /// Margin between shapes
    pub c4_shape_margin: f64,
    /// Padding between shapes
    pub c4_shape_padding: f64,
    /// Width of person boxes
    pub width: f64,
    /// Height of person boxes
    pub height: f64,
    /// Margin around boxes
    pub box_margin: f64,
    /// How many shapes to place in each row.
    pub c4_shape_in_row: f64,
    pub next_line_padding_x: f64,
    /// How many boundaries to place in each row.
    pub c4_boundary_in_row: f64,
    /// This sets the font size of Person shape for the diagram
    pub person_font_size: NumberOrString,
    /// This sets the font weight of Person shape for the diagram
    pub person_font_family: String,
    /// This sets the font weight of Person shape for the diagram
    pub person_font_weight: NumberOrString,
    /// This sets the font size of External Person shape for the diagram
    #[serde(rename = "external_personFontSize")]
    pub external_person_font_size: NumberOrString,
    /// This sets the font family of External Person shape for the diagram
    #[serde(rename = "external_personFontFamily")]
    pub external_person_font_family: String,
    /// This sets the font weight of External Person shape for the diagram
    #[serde(rename = "external_personFontWeight")]
    pub external_person_font_weight: NumberOrString,
    /// This sets the font size of System shape for the diagram
    pub system_font_size: NumberOrString,
    /// This sets the font family of System shape for the diagram
    pub system_font_family: String,
    /// This sets the font weight of System shape for the diagram
    pub system_font_weight: NumberOrString,
    /// This sets the font size of External System shape for the diagram
    #[serde(rename = "external_systemFontSize")]
    pub external_system_font_size: NumberOrString,
    /// This sets the font family of External System shape for the diagram
    #[serde(rename = "external_systemFontFamily")]
    pub external_system_font_family: String,
    /// This sets the font weight of External System shape for the diagram
    #[serde(rename = "external_systemFontWeight")]
    pub external_system_font_weight: NumberOrString,
    /// This sets the font size of System DB shape for the diagram
    #[serde(rename = "system_dbFontSize")]
    pub system_db_font_size: NumberOrString,
    /// This sets the font family of System DB shape for the diagram
    #[serde(rename = "system_dbFontFamily")]
    pub system_db_font_family: String,
    /// This sets the font weight of System DB shape for the diagram
    #[serde(rename = "system_dbFontWeight")]
    pub system_db_font_weight: NumberOrString,
    /// This sets the font size of External System DB shape for the diagram
    #[serde(rename = "external_system_dbFontSize")]
    pub external_system_db_font_size: NumberOrString,
    /// This sets the font family of External System DB shape for the diagram
    #[serde(rename = "external_system_dbFontFamily")]
    pub external_system_db_font_family: String,
    /// This sets the font weight of External System DB shape for the diagram
    #[serde(rename = "external_system_dbFontWeight")]
    pub external_system_db_font_weight: NumberOrString,
    /// This sets the font size of System Queue shape for the diagram
    #[serde(rename = "system_queueFontSize")]
    pub system_queue_font_size: NumberOrString,
    /// This sets the font family of System Queue shape for the diagram
    #[serde(rename = "system_queueFontFamily")]
    pub system_queue_font_family: String,
    /// This sets the font weight of System Queue shape for the diagram
    #[serde(rename = "system_queueFontWeight")]
    pub system_queue_font_weight: NumberOrString,
    /// This sets the font size of External System Queue shape for the diagram
    #[serde(rename = "external_system_queueFontSize")]
    pub external_system_queue_font_size: NumberOrString,
    /// This sets the font family of External System Queue shape for the diagram
    #[serde(rename = "external_system_queueFontFamily")]
    pub external_system_queue_font_family: String,
    /// This sets the font weight of External System Queue shape for the diagram
    #[serde(rename = "external_system_queueFontWeight")]
    pub external_system_queue_font_weight: NumberOrString,
    /// This sets the font size of Boundary shape for the diagram
    pub boundary_font_size: NumberOrString,
    /// This sets the font family of Boundary shape for the diagram
    pub boundary_font_family: String,
    /// This sets the font weight of Boundary shape for the diagram
    pub boundary_font_weight: NumberOrString,
    /// This sets the font size of Message shape for the diagram
    pub message_font_size: NumberOrString,
    /// This sets the font family of Message shape for the diagram
    pub message_font_family: String,
    /// This sets the font weight of Message shape for the diagram
    pub message_font_weight: NumberOrString,
    /// This sets the font size of Container shape for the diagram
    pub container_font_size: NumberOrString,
    /// This sets the font family of Container shape for the diagram
    pub container_font_family: String,
    /// This sets the font weight of Container shape for the diagram
    pub container_font_weight: NumberOrString,
    /// This sets the font size of External Container shape for the diagram
    #[serde(rename = "external_containerFontSize")]
    pub external_container_font_size: NumberOrString,
    /// This sets the font family of External Container shape for the diagram
    #[serde(rename = "external_containerFontFamily")]
    pub external_container_font_family: String,
    /// This sets the font weight of External Container shape for the diagram
    #[serde(rename = "external_containerFontWeight")]
    pub external_container_font_weight: NumberOrString,
    /// This sets the font size of Container DB shape for the diagram
    #[serde(rename = "container_dbFontSize")]
    pub container_db_font_size: NumberOrString,
    /// This sets the font family of Container DB shape for the diagram
    #[serde(rename = "container_dbFontFamily")]
    pub container_db_font_family: String,
    /// This sets the font weight of Container DB shape for the diagram
    #[serde(rename = "container_dbFontWeight")]
    pub container_db_font_weight: NumberOrString,
    /// This sets the font size of External Container DB shape for the diagram
    #[serde(rename = "external_container_dbFontSize")]
    pub external_container_db_font_size: NumberOrString,
    /// This sets the font family of External Container DB shape for the diagram
    #[serde(rename = "external_container_dbFontFamily")]
    pub external_container_db_font_family: String,
    /// This sets the font weight of External Container DB shape for the diagram
    #[serde(rename = "external_container_dbFontWeight")]
    pub external_container_db_font_weight: NumberOrString,
    /// This sets the font size of Container Queue shape for the diagram
    #[serde(rename = "container_queueFontSize")]
    pub container_queue_font_size: NumberOrString,
    /// This sets the font family of Container Queue shape for the diagram
    #[serde(rename = "container_queueFontFamily")]
    pub container_queue_font_family: String,
    /// This sets the font weight of Container Queue shape for the diagram
    #[serde(rename = "container_queueFontWeight")]
    pub container_queue_font_weight: NumberOrString,
    /// This sets the font size of External Container Queue shape for the diagram
    #[serde(rename = "external_container_queueFontSize")]
    pub external_container_queue_font_size: NumberOrString,
    /// This sets the font family of External Container Queue shape for the diagram
    #[serde(rename = "external_container_queueFontFamily")]
    pub external_container_queue_font_family: String,
    /// This sets the font weight of External Container Queue shape for the diagram
    #[serde(rename = "external_container_queueFontWeight")]
    pub external_container_queue_font_weight: NumberOrString,
    /// This sets the font size of Component shape for the diagram
    pub component_font_size: NumberOrString,
    /// This sets the font family of Component shape for the diagram
    pub component_font_family: String,
    /// This sets the font weight of Component shape for the diagram
    pub component_font_weight: NumberOrString,
    /// This sets the font size of External Component shape for the diagram
    #[serde(rename = "external_componentFontSize")]
    pub external_component_font_size: NumberOrString,
    /// This sets the font family of External Component shape for the diagram
    #[serde(rename = "external_componentFontFamily")]
    pub external_component_font_family: String,
    /// This sets the font weight of External Component shape for the diagram
    #[serde(rename = "external_componentFontWeight")]
    pub external_component_font_weight: NumberOrString,
    /// This sets the font size of Component DB shape for the diagram
    #[serde(rename = "component_dbFontSize")]
    pub component_db_font_size: NumberOrString,
    /// This sets the font family of Component DB shape for the diagram
    #[serde(rename = "component_dbFontFamily")]
    pub component_db_font_family: String,
    /// This sets the font weight of Component DB shape for the diagram
    #[serde(rename = "component_dbFontWeight")]
    pub component_db_font_weight: NumberOrString,
    /// This sets the font size of External Component DB shape for the diagram
    #[serde(rename = "external_component_dbFontSize")]
    pub external_component_db_font_size: NumberOrString,
    /// This sets the font family of External Component DB shape for the diagram
    #[serde(rename = "external_component_dbFontFamily")]
    pub external_component_db_font_family: String,
    /// This sets the font weight of External Component DB shape for the diagram
    #[serde(rename = "external_component_dbFontWeight")]
    pub external_component_db_font_weight: NumberOrString,
    /// This sets the font size of Component Queue shape for the diagram
    #[serde(rename = "component_queueFontSize")]
    pub component_queue_font_size: NumberOrString,
    /// This sets the font family of Component Queue shape for the diagram
    #[serde(rename = "component_queueFontFamily")]
    pub component_queue_font_family: String,
    /// This sets the font weight of Component Queue shape for the diagram
    #[serde(rename = "component_queueFontWeight")]
    pub component_queue_font_weight: NumberOrString,
    /// This sets the font size of External Component Queue shape for the diagram
    #[serde(rename = "external_component_queueFontSize")]
    pub external_component_queue_font_size: NumberOrString,
    /// This sets the font family of External Component Queue shape for the diagram
    #[serde(rename = "external_component_queueFontFamily")]
    pub external_component_queue_font_family: String,
    /// This sets the font weight of External Component Queue shape for the diagram
    #[serde(rename = "external_component_queueFontWeight")]
    pub external_component_queue_font_weight: NumberOrString,
    /// This sets the auto-wrap state for the diagram
    pub wrap: bool,
    /// This sets the auto-wrap padding for the diagram (sides only)
    pub wrap_padding: f64,
    #[serde(rename = "person_bg_color")]
    pub person_bg_color: String,
    #[serde(rename = "person_border_color")]
    pub person_border_color: String,
    #[serde(rename = "external_person_bg_color")]
    pub external_person_bg_color: String,
    #[serde(rename = "external_person_border_color")]
    pub external_person_border_color: String,
    #[serde(rename = "system_bg_color")]
    pub system_bg_color: String,
    #[serde(rename = "system_border_color")]
    pub system_border_color: String,
    #[serde(rename = "system_db_bg_color")]
    pub system_db_bg_color: String,
    #[serde(rename = "system_db_border_color")]
    pub system_db_border_color: String,
    #[serde(rename = "system_queue_bg_color")]
    pub system_queue_bg_color: String,
    #[serde(rename = "system_queue_border_color")]
    pub system_queue_border_color: String,
    #[serde(rename = "external_system_bg_color")]
    pub external_system_bg_color: String,
    #[serde(rename = "external_system_border_color")]
    pub external_system_border_color: String,
    #[serde(rename = "external_system_db_bg_color")]
    pub external_system_db_bg_color: String,
    #[serde(rename = "external_system_db_border_color")]
    pub external_system_db_border_color: String,
    #[serde(rename = "external_system_queue_bg_color")]
    pub external_system_queue_bg_color: String,
    #[serde(rename = "external_system_queue_border_color")]
    pub external_system_queue_border_color: String,
    #[serde(rename = "container_bg_color")]
    pub container_bg_color: String,
    #[serde(rename = "container_border_color")]
    pub container_border_color: String,
    #[serde(rename = "container_db_bg_color")]
    pub container_db_bg_color: String,
    #[serde(rename = "container_db_border_color")]
    pub container_db_border_color: String,
    #[serde(rename = "container_queue_bg_color")]
    pub container_queue_bg_color: String,
    #[serde(rename = "container_queue_border_color")]
    pub container_queue_border_color: String,
    #[serde(rename = "external_container_bg_color")]
    pub external_container_bg_color: String,
    #[serde(rename = "external_container_border_color")]
    pub external_container_border_color: String,
    #[serde(rename = "external_container_db_bg_color")]
    pub external_container_db_bg_color: String,
    #[serde(rename = "external_container_db_border_color")]
    pub external_container_db_border_color: String,
    #[serde(rename = "external_container_queue_bg_color")]
    pub external_container_queue_bg_color: String,
    #[serde(rename = "external_container_queue_border_color")]
    pub external_container_queue_border_color: String,
    #[serde(rename = "component_bg_color")]
    pub component_bg_color: String,
    #[serde(rename = "component_border_color")]
    pub component_border_color: String,
    #[serde(rename = "component_db_bg_color")]
    pub component_db_bg_color: String,
    #[serde(rename = "component_db_border_color")]
    pub component_db_border_color: String,
    #[serde(rename = "component_queue_bg_color")]
    pub component_queue_bg_color: String,
    #[serde(rename = "component_queue_border_color")]
    pub component_queue_border_color: String,
    #[serde(rename = "external_component_bg_color")]
    pub external_component_bg_color: String,
    #[serde(rename = "external_component_border_color")]
    pub external_component_border_color: String,
    #[serde(rename = "external_component_db_bg_color")]
    pub external_component_db_bg_color: String,
    #[serde(rename = "external_component_db_border_color")]
    pub external_component_db_border_color: String,
    #[serde(rename = "external_component_queue_bg_color")]
    pub external_component_queue_bg_color: String,
    #[serde(rename = "external_component_queue_border_color")]
    pub external_component_queue_border_color: String,
    /// defaultConfig.ts gives c4 its own flowchart defaults with inheritDir false.
    pub flowchart: FlowchartConfig,
}

impl Default for C4Config {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            diagram_margin_x: 50.0,
            diagram_margin_y: 10.0,
            c4_shape_margin: 50.0,
            c4_shape_padding: 20.0,
            width: 216.0,
            height: 60.0,
            box_margin: 10.0,
            c4_shape_in_row: 4.0,
            next_line_padding_x: 0.0,
            c4_boundary_in_row: 2.0,
            person_font_size: NumberOrString::Number(14.0),
            person_font_family: "\"Open Sans\", sans-serif".to_string(),
            person_font_weight: NumberOrString::Text("normal".to_string()),
            external_person_font_size: NumberOrString::Number(14.0),
            external_person_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_person_font_weight: NumberOrString::Text("normal".to_string()),
            system_font_size: NumberOrString::Number(14.0),
            system_font_family: "\"Open Sans\", sans-serif".to_string(),
            system_font_weight: NumberOrString::Text("normal".to_string()),
            external_system_font_size: NumberOrString::Number(14.0),
            external_system_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_system_font_weight: NumberOrString::Text("normal".to_string()),
            system_db_font_size: NumberOrString::Number(14.0),
            system_db_font_family: "\"Open Sans\", sans-serif".to_string(),
            system_db_font_weight: NumberOrString::Text("normal".to_string()),
            external_system_db_font_size: NumberOrString::Number(14.0),
            external_system_db_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_system_db_font_weight: NumberOrString::Text("normal".to_string()),
            system_queue_font_size: NumberOrString::Number(14.0),
            system_queue_font_family: "\"Open Sans\", sans-serif".to_string(),
            system_queue_font_weight: NumberOrString::Text("normal".to_string()),
            external_system_queue_font_size: NumberOrString::Number(14.0),
            external_system_queue_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_system_queue_font_weight: NumberOrString::Text("normal".to_string()),
            boundary_font_size: NumberOrString::Number(14.0),
            boundary_font_family: "\"Open Sans\", sans-serif".to_string(),
            boundary_font_weight: NumberOrString::Text("normal".to_string()),
            message_font_size: NumberOrString::Number(12.0),
            message_font_family: "\"Open Sans\", sans-serif".to_string(),
            message_font_weight: NumberOrString::Text("normal".to_string()),
            container_font_size: NumberOrString::Number(14.0),
            container_font_family: "\"Open Sans\", sans-serif".to_string(),
            container_font_weight: NumberOrString::Text("normal".to_string()),
            external_container_font_size: NumberOrString::Number(14.0),
            external_container_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_container_font_weight: NumberOrString::Text("normal".to_string()),
            container_db_font_size: NumberOrString::Number(14.0),
            container_db_font_family: "\"Open Sans\", sans-serif".to_string(),
            container_db_font_weight: NumberOrString::Text("normal".to_string()),
            external_container_db_font_size: NumberOrString::Number(14.0),
            external_container_db_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_container_db_font_weight: NumberOrString::Text("normal".to_string()),
            container_queue_font_size: NumberOrString::Number(14.0),
            container_queue_font_family: "\"Open Sans\", sans-serif".to_string(),
            container_queue_font_weight: NumberOrString::Text("normal".to_string()),
            external_container_queue_font_size: NumberOrString::Number(14.0),
            external_container_queue_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_container_queue_font_weight: NumberOrString::Text("normal".to_string()),
            component_font_size: NumberOrString::Number(14.0),
            component_font_family: "\"Open Sans\", sans-serif".to_string(),
            component_font_weight: NumberOrString::Text("normal".to_string()),
            external_component_font_size: NumberOrString::Number(14.0),
            external_component_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_component_font_weight: NumberOrString::Text("normal".to_string()),
            component_db_font_size: NumberOrString::Number(14.0),
            component_db_font_family: "\"Open Sans\", sans-serif".to_string(),
            component_db_font_weight: NumberOrString::Text("normal".to_string()),
            external_component_db_font_size: NumberOrString::Number(14.0),
            external_component_db_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_component_db_font_weight: NumberOrString::Text("normal".to_string()),
            component_queue_font_size: NumberOrString::Number(14.0),
            component_queue_font_family: "\"Open Sans\", sans-serif".to_string(),
            component_queue_font_weight: NumberOrString::Text("normal".to_string()),
            external_component_queue_font_size: NumberOrString::Number(14.0),
            external_component_queue_font_family: "\"Open Sans\", sans-serif".to_string(),
            external_component_queue_font_weight: NumberOrString::Text("normal".to_string()),
            wrap: true,
            wrap_padding: 10.0,
            person_bg_color: "#08427B".to_string(),
            person_border_color: "#073B6F".to_string(),
            external_person_bg_color: "#686868".to_string(),
            external_person_border_color: "#8A8A8A".to_string(),
            system_bg_color: "#1168BD".to_string(),
            system_border_color: "#3C7FC0".to_string(),
            system_db_bg_color: "#1168BD".to_string(),
            system_db_border_color: "#3C7FC0".to_string(),
            system_queue_bg_color: "#1168BD".to_string(),
            system_queue_border_color: "#3C7FC0".to_string(),
            external_system_bg_color: "#999999".to_string(),
            external_system_border_color: "#8A8A8A".to_string(),
            external_system_db_bg_color: "#999999".to_string(),
            external_system_db_border_color: "#8A8A8A".to_string(),
            external_system_queue_bg_color: "#999999".to_string(),
            external_system_queue_border_color: "#8A8A8A".to_string(),
            container_bg_color: "#438DD5".to_string(),
            container_border_color: "#3C7FC0".to_string(),
            container_db_bg_color: "#438DD5".to_string(),
            container_db_border_color: "#3C7FC0".to_string(),
            container_queue_bg_color: "#438DD5".to_string(),
            container_queue_border_color: "#3C7FC0".to_string(),
            external_container_bg_color: "#B3B3B3".to_string(),
            external_container_border_color: "#A6A6A6".to_string(),
            external_container_db_bg_color: "#B3B3B3".to_string(),
            external_container_db_border_color: "#A6A6A6".to_string(),
            external_container_queue_bg_color: "#B3B3B3".to_string(),
            external_container_queue_border_color: "#A6A6A6".to_string(),
            component_bg_color: "#85BBF0".to_string(),
            component_border_color: "#78A8D8".to_string(),
            component_db_bg_color: "#85BBF0".to_string(),
            component_db_border_color: "#78A8D8".to_string(),
            component_queue_bg_color: "#85BBF0".to_string(),
            component_queue_border_color: "#78A8D8".to_string(),
            external_component_bg_color: "#CCCCCC".to_string(),
            external_component_border_color: "#BFBFBF".to_string(),
            external_component_db_bg_color: "#CCCCCC".to_string(),
            external_component_db_border_color: "#BFBFBF".to_string(),
            external_component_queue_bg_color: "#CCCCCC".to_string(),
            external_component_queue_border_color: "#BFBFBF".to_string(),
            flowchart: FlowchartConfig {
                inherit_dir: false,
                ..FlowchartConfig::default()
            },
        }
    }
}

// ---- sankey ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SankeyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    pub width: f64,
    pub height: f64,
    /// The color of the links in the sankey diagram.
    pub link_color: String,
    /// Controls the alignment of the Sankey diagrams.
    pub node_alignment: String,
    /// Toggle to display or hide values along with title.
    pub show_values: bool,
    /// The prefix to use for values
    pub prefix: String,
    /// The suffix to use for values
    pub suffix: String,
    /// The width of the nodes in the sankey diagram.
    pub node_width: f64,
    /// The padding between nodes in the sankey diagram.
    pub node_padding: f64,
    /// The style of labels in the sankey diagram.
    pub label_style: String,
    /// A mapping of node IDs to their colors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_colors: Option<Map<String, Value>>,
}

impl Default for SankeyConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: false,
            theme: None,
            look: None,
            layout: None,
            width: 600.0,
            height: 400.0,
            link_color: "gradient".to_string(),
            node_alignment: "justify".to_string(),
            show_values: true,
            prefix: "".to_string(),
            suffix: "".to_string(),
            node_width: 10.0,
            node_padding: 12.0,
            label_style: "legacy".to_string(),
            node_colors: None,
        }
    }
}

// ---- packet ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PacketConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The height of each row in the packet diagram.
    pub row_height: f64,
    /// The width of each bit in the packet diagram.
    pub bit_width: f64,
    /// The number of bits to display per row.
    pub bits_per_row: f64,
    /// Toggle to display or hide bit numbers.
    pub show_bits: bool,
    /// The direction each row is numbered in.
    pub bit_order: String,
    /// The horizontal padding between the blocks in a row.
    pub padding_x: f64,
    /// The vertical padding between the rows.
    pub padding_y: f64,
}

impl Default for PacketConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            row_height: 32.0,
            bit_width: 32.0,
            bits_per_row: 32.0,
            show_bits: true,
            bit_order: "ascending".to_string(),
            padding_x: 5.0,
            padding_y: 5.0,
        }
    }
}

// ---- block ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BlockConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    pub padding: f64,
}

impl Default for BlockConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            padding: 8.0,
        }
    }
}

// ---- eventmodeling ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EventmodelingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The padding around the Event Modeling diagram.
    pub padding: f64,
    /// The height of each row in the Event Modeling diagram.
    pub row_height: f64,
}

impl Default for EventmodelingConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            padding: 30.0,
            row_height: 32.0,
        }
    }
}

// ---- treeView ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TreeViewConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Horizontal distance between rows differing by one level
    pub row_indent: f64,
    /// Horizontal padding of label
    pub padding_x: f64,
    /// Vertical padding of label
    pub padding_y: f64,
    /// Thickness of the line
    pub line_thickness: f64,
    /// Whether to show the default file/folder icons next to labels.
    pub show_icons: bool,
    /// Name of a registered iconify pack used to resolve unprefixed icon
    pub default_icon_pack: String,
    /// Exact-filename → icon map used to pick a file's icon when
    pub filename_icons: Map<String, Value>,
    /// File-extension → icon map used to pick a file's icon when
    pub extension_icons: Map<String, Value>,
}

impl Default for TreeViewConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            row_indent: 10.0,
            padding_x: 5.0,
            padding_y: 5.0,
            line_thickness: 1.0,
            show_icons: false,
            default_icon_pack: "".to_string(),
            filename_icons: Map::new(),
            extension_icons: Map::new(),
        }
    }
}

// ---- radar ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RadarConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The size of the radar diagram.
    pub width: f64,
    /// The size of the radar diagram.
    pub height: f64,
    /// The margin from the top of the radar diagram.
    pub margin_top: f64,
    /// The margin from the right of the radar diagram.
    pub margin_right: f64,
    /// The margin from the bottom of the radar diagram.
    pub margin_bottom: f64,
    /// The margin from the left of the radar diagram.
    pub margin_left: f64,
    /// The scale factor of the axis.
    pub axis_scale_factor: f64,
    /// The scale factor of the axis label.
    pub axis_label_factor: f64,
    /// The tension factor for the Catmull-Rom spline conversion to cubic Bézier curves.
    pub curve_tension: f64,
}

impl Default for RadarConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            width: 600.0,
            height: 600.0,
            margin_top: 50.0,
            margin_right: 50.0,
            margin_bottom: 50.0,
            margin_left: 50.0,
            axis_scale_factor: 1.0,
            axis_label_factor: 1.05,
            curve_tension: 0.17,
        }
    }
}

// ---- usecase ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UsecaseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Width of nodes where text is wrapped.
    pub wrapping_width: f64,
    /// Minimum width of the label area of a node.
    pub min_node_width: f64,
    /// Font size for actor labels
    pub actor_font_size: f64,
    /// Font family for actor labels.
    pub actor_font_family: String,
    /// Font weight for actor labels.
    pub actor_font_weight: String,
    /// Font size for usecase labels
    pub usecase_font_size: f64,
    /// Font family for usecase labels.
    pub usecase_font_family: String,
    /// Font weight for usecase labels.
    pub usecase_font_weight: String,
    /// Spacing between nodes on the same level
    pub node_spacing: f64,
    /// Spacing between nodes on different levels
    pub rank_spacing: f64,
    /// Padding around the entire diagram
    pub diagram_padding: f64,
    /// How a use case diagram takes its colours from the active theme.
    pub color_scheme: String,
}

impl Default for UsecaseConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            wrapping_width: 120.0,
            min_node_width: 120.0,
            actor_font_size: 14.0,
            actor_font_family: "\"Open Sans\", sans-serif".to_string(),
            actor_font_weight: "normal".to_string(),
            usecase_font_size: 12.0,
            usecase_font_family: "\"Open Sans\", sans-serif".to_string(),
            usecase_font_weight: "normal".to_string(),
            node_spacing: 50.0,
            rank_spacing: 50.0,
            diagram_padding: 20.0,
            color_scheme: "role".to_string(),
        }
    }
}

// ---- venn ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VennConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    pub theme: String,
    /// Defines which main look to use for the diagram.
    pub look: String,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The width of the Venn diagram.
    pub width: f64,
    /// The height of the Venn diagram.
    pub height: f64,
    pub padding: f64,
    pub use_debug_layout: bool,
}

impl Default for VennConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: "redux-color".to_string(),
            look: "neo".to_string(),
            layout: None,
            width: 800.0,
            height: 450.0,
            padding: 8.0,
            use_debug_layout: false,
        }
    }
}

// ---- wardley-beta ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WardleyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The width of the Wardley diagram canvas.
    pub width: f64,
    /// The height of the Wardley diagram canvas.
    pub height: f64,
    /// The padding around the Wardley diagram.
    pub padding: f64,
    /// The radius of component nodes.
    pub node_radius: f64,
    /// The offset distance for node labels.
    pub node_label_offset: f64,
    /// The font size for axis labels.
    pub axis_font_size: f64,
    /// The font size for component labels.
    pub label_font_size: f64,
    /// Whether to display a background grid.
    pub show_grid: bool,
}

impl Default for WardleyConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            width: 900.0,
            height: 600.0,
            padding: 48.0,
            node_radius: 6.0,
            node_label_offset: 8.0,
            axis_font_size: 12.0,
            label_font_size: 10.0,
            show_grid: false,
        }
    }
}

// ---- cynefin ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CynefinConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The width of the Cynefin diagram.
    pub width: f64,
    /// The height of the Cynefin diagram.
    pub height: f64,
    /// Padding around the diagram.
    pub padding: f64,
    /// Show decision model and practice type labels.
    pub show_domain_descriptions: bool,
    /// Waviness amplitude of domain boundaries (0 for straight).
    pub boundary_amplitude: f64,
    /// Deterministic seed for boundary waviness.
    pub seed: f64,
}

impl Default for CynefinConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            width: 800.0,
            height: 600.0,
            padding: 40.0,
            show_domain_descriptions: true,
            boundary_amplitude: 8.0,
            seed: 0.0,
        }
    }
}

// ---- railroad ----
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RailroadConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_width: Option<f64>,
    /// When this flag is set to `true`, the height and width is set to 100%
    pub use_max_width: bool,
    /// Theme, the CSS style sheet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Defines which main look to use for the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// Defines which layout algorithm to use for rendering the diagram.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Use compact layout mode
    pub compact_mode: bool,
    /// Padding around elements
    pub padding: f64,
    /// Vertical separation between elements
    pub vertical_separation: f64,
    /// Horizontal separation between elements
    pub horizontal_separation: f64,
    /// Radius for curved paths
    pub arc_radius: f64,
    /// Font size for text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Font family for text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Fill color for terminal elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_fill: Option<String>,
    /// Stroke color for terminal elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_stroke: Option<String>,
    /// Text color for terminal elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_text_color: Option<String>,
    /// Fill color for non-terminal elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_terminal_fill: Option<String>,
    /// Stroke color for non-terminal elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_terminal_stroke: Option<String>,
    /// Text color for non-terminal elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_terminal_text_color: Option<String>,
    /// Color for connection lines
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_color: Option<String>,
    /// Width of strokes
    pub stroke_width: f64,
    /// Fill color for start/end markers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_fill: Option<String>,
    /// Fill color for comments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_fill: Option<String>,
    /// Stroke color for comments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_stroke: Option<String>,
    /// Text color for comments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_text_color: Option<String>,
    /// Fill color for special sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_fill: Option<String>,
    /// Stroke color for special sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_stroke: Option<String>,
    /// Color for rule names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name_color: Option<String>,
    /// Show start/end markers
    pub show_markers: bool,
    /// Radius of start/end markers
    pub marker_radius: f64,
}

impl Default for RailroadConfig {
    fn default() -> Self {
        Self {
            use_width: None,
            use_max_width: true,
            theme: None,
            look: None,
            layout: None,
            compact_mode: false,
            padding: 10.0,
            vertical_separation: 8.0,
            horizontal_separation: 10.0,
            arc_radius: 10.0,
            font_size: None,
            font_family: None,
            terminal_fill: None,
            terminal_stroke: None,
            terminal_text_color: None,
            non_terminal_fill: None,
            non_terminal_stroke: None,
            non_terminal_text_color: None,
            line_color: None,
            stroke_width: 2.0,
            marker_fill: None,
            comment_fill: None,
            comment_stroke: None,
            comment_text_color: None,
            special_fill: None,
            special_stroke: None,
            rule_name_color: None,
            show_markers: true,
            marker_radius: 5.0,
        }
    }
}
/// Treemap's configuration, which the schema does not define; `defaultConfig.ts` supplies it
/// whole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TreemapConfig {
    pub use_max_width: bool,
    pub padding: f64,
    pub diagram_padding: f64,
    pub show_values: bool,
    pub node_width: f64,
    pub node_height: f64,
    pub border_width: f64,
    pub value_font_size: f64,
    pub label_font_size: f64,
    pub value_format: String,
}

impl Default for TreemapConfig {
    fn default() -> Self {
        Self {
            use_max_width: true,
            padding: 10.0,
            diagram_padding: 8.0,
            show_values: true,
            node_width: 100.0,
            node_height: 40.0,
            border_width: 1.0,
            value_font_size: 12.0,
            label_font_size: 14.0,
            value_format: ",".to_string(),
        }
    }
}

impl SequenceConfig {
    /// `defaultConfig.ts`'s `messageFont()`: the message font settings as one value.
    pub fn message_font(&self) -> FontConfig {
        FontConfig {
            font_family: self.message_font_family.clone(),
            font_size: self.message_font_size.clone(),
            font_weight: self.message_font_weight.clone(),
        }
    }

    /// `noteFont()`.
    pub fn note_font(&self) -> FontConfig {
        FontConfig {
            font_family: self.note_font_family.clone(),
            font_size: self.note_font_size.clone(),
            font_weight: self.note_font_weight.clone(),
        }
    }

    /// `actorFont()`.
    pub fn actor_font(&self) -> FontConfig {
        FontConfig {
            font_family: self.actor_font_family.clone(),
            font_size: self.actor_font_size.clone(),
            font_weight: self.actor_font_weight.clone(),
        }
    }
}

impl C4Config {
    fn font(family: &str, size: &NumberOrString, weight: &NumberOrString) -> FontConfig {
        FontConfig {
            font_family: family.to_string(),
            font_size: size.clone(),
            font_weight: weight.clone(),
        }
    }

    pub fn person_font(&self) -> FontConfig {
        Self::font(
            &self.person_font_family,
            &self.person_font_size,
            &self.person_font_weight,
        )
    }

    pub fn external_person_font(&self) -> FontConfig {
        Self::font(
            &self.external_person_font_family,
            &self.external_person_font_size,
            &self.external_person_font_weight,
        )
    }

    pub fn system_font(&self) -> FontConfig {
        Self::font(
            &self.system_font_family,
            &self.system_font_size,
            &self.system_font_weight,
        )
    }

    pub fn external_system_font(&self) -> FontConfig {
        Self::font(
            &self.external_system_font_family,
            &self.external_system_font_size,
            &self.external_system_font_weight,
        )
    }

    pub fn system_db_font(&self) -> FontConfig {
        Self::font(
            &self.system_db_font_family,
            &self.system_db_font_size,
            &self.system_db_font_weight,
        )
    }

    pub fn external_system_db_font(&self) -> FontConfig {
        Self::font(
            &self.external_system_db_font_family,
            &self.external_system_db_font_size,
            &self.external_system_db_font_weight,
        )
    }

    pub fn system_queue_font(&self) -> FontConfig {
        Self::font(
            &self.system_queue_font_family,
            &self.system_queue_font_size,
            &self.system_queue_font_weight,
        )
    }

    pub fn external_system_queue_font(&self) -> FontConfig {
        Self::font(
            &self.external_system_queue_font_family,
            &self.external_system_queue_font_size,
            &self.external_system_queue_font_weight,
        )
    }

    pub fn container_font(&self) -> FontConfig {
        Self::font(
            &self.container_font_family,
            &self.container_font_size,
            &self.container_font_weight,
        )
    }

    pub fn external_container_font(&self) -> FontConfig {
        Self::font(
            &self.external_container_font_family,
            &self.external_container_font_size,
            &self.external_container_font_weight,
        )
    }

    pub fn container_db_font(&self) -> FontConfig {
        Self::font(
            &self.container_db_font_family,
            &self.container_db_font_size,
            &self.container_db_font_weight,
        )
    }

    pub fn external_container_db_font(&self) -> FontConfig {
        Self::font(
            &self.external_container_db_font_family,
            &self.external_container_db_font_size,
            &self.external_container_db_font_weight,
        )
    }

    pub fn container_queue_font(&self) -> FontConfig {
        Self::font(
            &self.container_queue_font_family,
            &self.container_queue_font_size,
            &self.container_queue_font_weight,
        )
    }

    pub fn external_container_queue_font(&self) -> FontConfig {
        Self::font(
            &self.external_container_queue_font_family,
            &self.external_container_queue_font_size,
            &self.external_container_queue_font_weight,
        )
    }

    pub fn component_font(&self) -> FontConfig {
        Self::font(
            &self.component_font_family,
            &self.component_font_size,
            &self.component_font_weight,
        )
    }

    pub fn external_component_font(&self) -> FontConfig {
        Self::font(
            &self.external_component_font_family,
            &self.external_component_font_size,
            &self.external_component_font_weight,
        )
    }

    pub fn component_db_font(&self) -> FontConfig {
        Self::font(
            &self.component_db_font_family,
            &self.component_db_font_size,
            &self.component_db_font_weight,
        )
    }

    pub fn external_component_db_font(&self) -> FontConfig {
        Self::font(
            &self.external_component_db_font_family,
            &self.external_component_db_font_size,
            &self.external_component_db_font_weight,
        )
    }

    pub fn component_queue_font(&self) -> FontConfig {
        Self::font(
            &self.component_queue_font_family,
            &self.component_queue_font_size,
            &self.component_queue_font_weight,
        )
    }

    pub fn external_component_queue_font(&self) -> FontConfig {
        Self::font(
            &self.external_component_queue_font_family,
            &self.external_component_queue_font_size,
            &self.external_component_queue_font_weight,
        )
    }

    pub fn boundary_font(&self) -> FontConfig {
        Self::font(
            &self.boundary_font_family,
            &self.boundary_font_size,
            &self.boundary_font_weight,
        )
    }

    pub fn message_font(&self) -> FontConfig {
        Self::font(
            &self.message_font_family,
            &self.message_font_size,
            &self.message_font_weight,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn apply(directive: Value) -> Config {
        let Value::Object(map) = directive else {
            panic!("a directive is an object");
        };
        let mut config = Config::default();
        config.apply(&map).expect("valid directive");
        config
    }

    #[test]
    fn the_defaults_are_the_schema_and_default_config_values() {
        let config = Config::default();
        assert_eq!(config.theme, "default");
        assert_eq!(config.look, "classic");
        assert_eq!(config.layout, "elk");
        assert_eq!(config.max_edges, 500.0);
        assert_eq!(config.font_size, 16.0);
        assert_eq!(
            config.font_family,
            "\"trebuchet ms\", verdana, arial, sans-serif;"
        );
        assert_eq!(config.log_level, NumberOrString::Number(5.0));
        assert_eq!(
            config.secure,
            vec![
                "secure",
                "securityLevel",
                "startOnLoad",
                "maxTextSize",
                "suppressErrorRendering",
                "maxEdges"
            ]
        );
        assert_eq!(config.flowchart.node_spacing, 50.0);
        assert_eq!(config.flowchart.curve, "basis");
        assert_eq!(config.flowchart.html_labels, None);
        assert_eq!(config.sequence.width, 150.0);
        assert_eq!(
            config.sequence.message_font_family,
            "\"trebuchet ms\", verdana, arial, sans-serif"
        );
        assert_eq!(config.gantt.axis_format, "%Y-%m-%d");
        assert_eq!(config.pie.use_width, 984.0);
        assert_eq!(config.pie.text_position, 0.75);
        assert_eq!(config.sankey.link_color, "gradient");
        assert_eq!(config.treemap.node_width, 100.0);
        assert_eq!(config.wardley_beta.width, 900.0);
        assert_eq!(config.c4.flowchart.inherit_dir, false);
        assert_eq!(config.elk.layering_layer_bound, 4.0);
        assert_eq!(config.elk.node_placement_strategy, None);
    }

    #[test]
    fn the_class_section_is_the_one_default_config_rebuilds() {
        let config = Config::default();
        assert_eq!(config.class.padding, None);
        assert_eq!(config.class.title_top_margin, None);
        assert_eq!(config.class.use_max_width, None);
        assert_eq!(config.class.hide_empty_members_box, false);
        assert_eq!(config.class.hierarchical_namespaces, true);
    }

    #[test]
    fn railroad_leaves_its_appearance_to_the_theme() {
        let config = Config::default();
        assert_eq!(config.railroad.font_size, None);
        assert_eq!(config.railroad.terminal_fill, None);
        assert_eq!(config.railroad.comment_text_color, None);
        assert_eq!(config.railroad.compact_mode, false);
        assert_eq!(config.railroad.padding, 10.0);
    }

    #[test]
    fn a_partial_directive_keeps_the_siblings_it_does_not_name() {
        let config = apply(json!({ "quadrantChart": { "chartHeight": 600 } }));
        assert_eq!(config.quadrant_chart.chart_height, 600.0);
        assert_eq!(config.quadrant_chart.chart_width, 500.0);
        assert_eq!(config.quadrant_chart.point_radius, 5.0);
    }

    #[test]
    fn a_secure_key_cannot_be_changed_by_a_directive() {
        let mut config = Config {
            font_size: 12345.0,
            ..Config::default()
        };
        config.secure.push("fontSize".to_string());
        let directive = json!({
            "fontFamily": "baf",
            "fontSize": 54321,
            "securityLevel": "loose"
        });
        config
            .apply(directive.as_object().expect("object"))
            .expect("valid");

        assert_eq!(config.font_family, "baf");
        assert_eq!(config.font_size, 12345.0);
        assert_eq!(config.security_level, "strict");
    }

    #[test]
    fn sanitize_drops_markup_and_object_pollution() {
        let config = apply(json!({
            "__proto__": { "polluted": true },
            "fontFamily": "<script>alert(1)</script>",
            "themeCSS": "body > svg { fill: red }",
            "flowchart": { "curve": "linear", "__x": 1 },
            "themeVariables": { "fontFamily": "Arial" }
        }));
        assert_eq!(config.flowchart.curve, "linear");
        assert_eq!(
            config.font_family,
            "\"trebuchet ms\", verdana, arial, sans-serif;"
        );
        assert_eq!(config.theme_css, None);
        assert_eq!(
            config.theme_variables.get("fontFamily"),
            Some(&json!("Arial"))
        );
    }

    #[test]
    fn a_directive_can_replace_a_whole_object_value() {
        let config = apply(json!({ "xyChart": { "xAxis": { "showLabel": false } } }));
        assert_eq!(config.xy_chart.x_axis.show_label, false);
        assert_eq!(config.xy_chart.x_axis.label_font_size, 14.0);
        assert_eq!(config.xy_chart.y_axis.show_label, true);
    }

    #[test]
    fn assign_with_depth_keeps_dissimilar_types() {
        let mut destination = json!({ "foo": { "bar": "bar" }, "bar": "foo" });
        assign_with_depth(&mut destination, &json!({ "foo": "foo", "bar": "bar" }), 2);
        assert_eq!(
            destination,
            json!({ "foo": { "bar": "bar" }, "bar": "bar" })
        );
    }

    #[test]
    fn assign_with_depth_merges_at_depth_two() {
        let mut destination = json!({ "a": { "b": { "c": 1, "d": 2 } }, "keep": true });
        assign_with_depth(&mut destination, &json!({ "a": { "b": { "c": 3 } } }), 2);
        assert_eq!(
            destination,
            json!({ "a": { "b": { "c": 3, "d": 2 } }, "keep": true })
        );
    }

    #[test]
    fn assign_with_depth_ignores_null() {
        let mut destination = json!({ "keep": 1 });
        assign_with_depth(&mut destination, &json!({ "keep": null }), 2);
        assert_eq!(destination, json!({ "keep": 1 }));
    }

    #[test]
    fn assign_with_depth_appends_missing_array_members() {
        let mut destination = json!({ "secure": ["a", "b"] });
        assign_with_depth(&mut destination, &json!({ "secure": ["b", "c"] }), 2);
        assert_eq!(destination, json!({ "secure": ["a", "b", "c"] }));
    }

    #[test]
    fn later_directives_win_and_siblings_are_kept() {
        let config = Config::with_directives([
            json!({ "flowchart": { "nodeSpacing": 50, "rankSpacing": 100 }, "theme": "default" })
                .as_object()
                .expect("object")
                .clone(),
            json!({ "flowchart": { "nodeSpacing": 75, "curve": "basis" }, "mindmap": { "padding": 20 } })
                .as_object()
                .expect("object")
                .clone(),
        ])
        .expect("valid");
        assert_eq!(config.flowchart.node_spacing, 75.0);
        assert_eq!(config.flowchart.rank_spacing, 100.0);
        assert_eq!(config.flowchart.curve, "basis");
        assert_eq!(config.mindmap.padding, 20.0);
    }

    #[test]
    fn an_unknown_key_is_ignored() {
        let config = apply(json!({ "notAKey": 1, "flowchart": { "alsoNotAKey": 2 } }));
        assert_eq!(config.flowchart.node_spacing, 50.0);
    }

    #[test]
    fn a_wrong_type_is_reported() {
        let mut config = Config::default();
        let error = config
            .apply(
                json!({ "flowchart": { "nodeSpacing": "wide" } })
                    .as_object()
                    .expect("object"),
            )
            .expect_err("a string is not a number");
        assert!(error.message.contains("invalid configuration"));
    }

    #[test]
    fn a_source_config_reaches_the_defaults() {
        let source = crate::preprocess::preprocess(
            "---\ntitle: hello\nconfig:\n  theme: dark\n  flowchart:\n    curve: linear\n---\nflowchart TD\n  A --> B",
        );
        let config = Config::from_source_config(&source.config).expect("valid source config");
        assert_eq!(config.theme, "dark");
        assert_eq!(config.flowchart.curve, "linear");
        assert_eq!(
            config.theme_variables.get("primaryColor"),
            Theme::Dark
                .variables(&Map::new())
                .expect("valid")
                .get("primaryColor")
        );
    }

    #[test]
    fn theme_variable_originals_survive_a_later_theme_change() {
        let mut config = apply(json!({ "themeVariables": { "primaryColor": "#abcdef" } }));
        config
            .apply(json!({ "theme": "dark" }).as_object().expect("object"))
            .expect("valid");
        assert_eq!(
            config.theme_variables.get("primaryColor"),
            Some(&json!("#abcdef"))
        );
    }

    #[test]
    fn a_theme_directive_rebuilds_the_theme_variables() {
        let config = apply(json!({
            "theme": "dark",
            "themeVariables": { "primaryColor": "#abcdef" }
        }));
        let expected = Theme::Dark
            .variables(
                &json!({ "primaryColor": "#abcdef" })
                    .as_object()
                    .expect("object")
                    .clone(),
            )
            .expect("valid color");
        assert_eq!(config.theme_variables, expected.into_map());
    }

    #[test]
    fn a_theme_variable_override_without_a_theme_lands_on_the_generated_variables() {
        let config = apply(json!({ "themeVariables": { "fontFamily": "Arial" } }));
        assert_eq!(
            config.theme_variables.get("fontFamily"),
            Some(&json!("Arial"))
        );
        assert_eq!(
            config.theme_variables.get("primaryColor"),
            Theme::Default
                .variables(&Map::new())
                .expect("valid")
                .get("primaryColor")
        );
    }

    #[test]
    fn the_effective_html_labels_follow_root_then_flowchart_then_true() {
        assert!(Config::default().effective_html_labels());

        let config = Config {
            html_labels: Some(false),
            ..Config::default()
        };
        assert!(!config.effective_html_labels());

        let mut config = Config::default();
        config.flowchart.html_labels = Some(false);
        assert!(!config.effective_html_labels());

        let mut config = Config {
            html_labels: Some(true),
            ..Config::default()
        };
        config.flowchart.html_labels = Some(false);
        assert!(config.effective_html_labels());
    }

    #[test]
    fn the_acronym_keys_keep_their_upstream_spelling() {
        let config = apply(json!({
            "themeCSS": "body { fill: red }",
            "legacyMathML": true,
            "forceLegacyMathML": true,
            "deterministicIDSeed": "seed"
        }));
        assert_eq!(config.theme_css.as_deref(), Some("body { fill: red }"));
        assert!(config.legacy_math_ml);
        assert!(config.force_legacy_math_ml);
        assert_eq!(config.deterministic_id_seed.as_deref(), Some("seed"));

        let json = config.to_json();
        assert_eq!(json["themeCSS"], json!("body { fill: red }"));
        assert_eq!(json["legacyMathML"], json!(true));
        assert_eq!(json["deterministicIDSeed"], json!("seed"));
    }

    #[test]
    fn a_config_round_trips_through_json() {
        let config = Config::default();
        let json = config.to_json();
        let parsed: Config = serde_json::from_value(json.clone()).expect("round trip");
        assert_eq!(parsed, config);
        assert_eq!(serde_json::to_value(parsed).expect("json"), json);
    }

    #[test]
    fn the_serialized_defaults_keep_upstream_keys_and_leave_undefined_ones_out() {
        let json = Config::default().to_json();
        assert_eq!(json["flowchart"]["useMaxWidth"], json!(true));
        assert_eq!(json["flowchart"]["htmlLabels"], Value::Null);
        assert_eq!(json["class"].get("padding"), None);
        assert_eq!(json["class"]["hideEmptyMembersBox"], json!(false));
        assert_eq!(json["c4"]["flowchart"]["inheritDir"], json!(false));
        assert_eq!(json["railroad"].get("fontSize"), None);
        assert_eq!(json["gantt"].get("tickInterval"), None);
        assert_eq!(json["pie"]["useWidth"], json!(984.0));
        assert_eq!(json["sankey"]["linkColor"], json!("gradient"));
        assert_eq!(json["wardley-beta"]["width"], json!(900.0));
        assert_eq!(json["treemap"]["valueFormat"], json!(","));
        assert_eq!(json["elk"].get("nodePlacementStrategy"), None);
        assert_eq!(json["themeVariables"]["primaryColor"], json!("#ECECFF"));
    }

    #[test]
    fn the_sequence_and_c4_font_builders_match_their_fields() {
        let config = Config::default();
        let font = config.sequence.message_font();
        assert_eq!(font.font_family, config.sequence.message_font_family);
        assert_eq!(font.font_size, config.sequence.message_font_size);
        assert_eq!(font.font_weight, config.sequence.message_font_weight);

        let font = config.c4.person_font();
        assert_eq!(font.font_family, config.c4.person_font_family);
        assert_eq!(font.font_size, NumberOrString::Number(14.0));
        assert_eq!(font.font_weight, NumberOrString::Text("normal".to_string()));
    }

    #[test]
    fn line_hops_accepts_a_boolean_or_a_shape() {
        let middle = apply(json!({ "elk": { "lineHops": "gap" } }));
        assert_eq!(middle.elk.line_hops, LineHops::Kind(LineHopsKind::Gap));
        let off = apply(json!({ "elk": { "lineHops": false } }));
        assert_eq!(off.elk.line_hops, LineHops::Enabled(false));
    }
}

#[cfg(test)]
mod upstream_default_tests {
    use super::*;
    use serde_json::json;

    /// The default configuration as upstream's own `defaultConfig.ts` produced it on node,
    /// with the schema's defaults resolved (`defaultConfigJson`) and theme variables built by
    /// the default theme. Function-valued entries (the sequence and c4 font builders) are
    /// dropped by JSON.stringify, which is why they are methods here.
    const UPSTREAM_DEFAULT: &str = r##"{"theme":"default","look":"classic","handDrawnSeed":0,"layout":"elk","maxTextSize":50000,"maxEdges":500,"elk":{"mergeEdges":false,"straightenEdges":true,"lineHops":true,"preset":"default","layeringLayerBound":4,"forceNodeModelOrder":false,"considerModelOrder":"NODES_AND_EDGES","keepEntryNodeOnTop":false,"orientFeedbackEdges":true},"darkMode":false,"fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif;","logLevel":5,"securityLevel":"strict","startOnLoad":true,"arrowMarkerAbsolute":false,"secure":["secure","securityLevel","startOnLoad","maxTextSize","suppressErrorRendering","maxEdges"],"legacyMathML":false,"forceLegacyMathML":false,"deterministicIds":false,"flowchart":{"useMaxWidth":true,"theme":"redux-color","look":"neo","titleTopMargin":25,"subGraphTitleMargin":{"top":0,"bottom":0},"diagramPadding":8,"htmlLabels":null,"nodeSpacing":50,"rankSpacing":50,"curve":"basis","padding":15,"wrappingWidth":120,"minNodeWidth":120,"stepDuration":1,"stepNumbers":false,"inheritDir":false},"swimlane":{"useMaxWidth":true,"theme":"redux-color","look":"neo","layout":"swimlane","lineHops":"arc","ignoreCrossLaneEdges":true,"optimizeRanksByCrossings":true,"automaticLaneOrdering":false},"agentflow":{"useMaxWidth":true,"theme":"redux-color","look":"neo","titleTopMargin":25,"diagramPadding":8,"nodeSpacing":50,"rankSpacing":50,"wrappingWidth":120,"minNodeWidth":120},"sequence":{"useMaxWidth":true,"theme":"redux-color","look":"neo","hideUnusedParticipants":false,"activationWidth":10,"diagramMarginX":50,"diagramMarginY":10,"actorMargin":50,"width":150,"height":65,"boxMargin":10,"boxTextMargin":5,"noteMargin":10,"messageMargin":35,"messageAlign":"center","mirrorActors":true,"forceMenus":false,"bottomMarginAdj":1,"rightAngles":false,"showSequenceNumbers":false,"actorFontSize":14,"actorFontFamily":"\"Open Sans\", sans-serif","actorFontWeight":400,"noteFontSize":14,"noteFontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","noteFontWeight":400,"noteAlign":"center","messageFontSize":16,"messageFontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","messageFontWeight":400,"wrap":false,"wrapPadding":10,"labelBoxWidth":50,"labelBoxHeight":20},"gantt":{"useMaxWidth":true,"titleTopMargin":25,"barHeight":20,"barGap":4,"topPadding":50,"rightPadding":75,"leftPadding":75,"gridLineStartPadding":35,"fontSize":11,"sectionFontSize":11,"numberSectionStyles":4,"axisFormat":"%Y-%m-%d","topAxis":false,"displayMode":"","weekday":"sunday"},"journey":{"useMaxWidth":true,"diagramMarginX":50,"diagramMarginY":10,"leftMargin":150,"maxLabelWidth":360,"width":150,"height":50,"boxMargin":10,"boxTextMargin":5,"noteMargin":10,"messageMargin":35,"messageAlign":"center","bottomMarginAdj":1,"rightAngles":false,"taskFontSize":14,"taskFontFamily":"\"Open Sans\", sans-serif","taskMargin":50,"activationWidth":10,"textPlacement":"fo","actorColours":["#8FBC8F","#7CFC00","#00FFFF","#20B2AA","#B0E0E6","#FFFFE0"],"sectionFills":["#191970","#8B008B","#4B0082","#2F4F4F","#800000","#8B4513","#00008B"],"sectionColours":["#fff"],"titleColor":"","titleFontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","titleFontSize":"4ex"},"timeline":{"useMaxWidth":true,"diagramMarginX":50,"diagramMarginY":10,"leftMargin":150,"width":150,"height":50,"boxMargin":10,"boxTextMargin":5,"noteMargin":10,"messageMargin":35,"messageAlign":"center","bottomMarginAdj":1,"rightAngles":false,"taskFontSize":14,"taskFontFamily":"\"Open Sans\", sans-serif","taskMargin":50,"activationWidth":10,"textPlacement":"fo","actorColours":["#8FBC8F","#7CFC00","#00FFFF","#20B2AA","#B0E0E6","#FFFFE0"],"sectionFills":["#191970","#8B008B","#4B0082","#2F4F4F","#800000","#8B4513","#00008B"],"sectionColours":["#fff"],"disableMulticolor":false},"class":{"theme":"redux-color","look":"neo","hideEmptyMembersBox":false,"hierarchicalNamespaces":true},"state":{"useMaxWidth":true,"theme":"redux-color","look":"neo","layout":"elk","wrappingWidth":120,"minNodeWidth":120,"titleTopMargin":25,"dividerMargin":10,"sizeUnit":5,"padding":8,"textHeight":10,"titleShift":-15,"noteMargin":10,"forkWidth":70,"forkHeight":7,"miniPadding":2,"fontSizeFactor":5.02,"fontSize":24,"labelHeight":16,"edgeLengthFactor":"20","compositTitleSize":35,"radius":5},"er":{"useMaxWidth":true,"theme":"redux-color","look":"neo","titleTopMargin":25,"diagramPadding":20,"layoutDirection":"TB","minEntityWidth":100,"minEntityHeight":75,"entityPadding":15,"nodeSpacing":140,"rankSpacing":80,"stroke":"gray","fill":"honeydew","fontSize":12},"pie":{"useMaxWidth":true,"textPosition":0.75,"donutHole":0,"legendPosition":"right","highlightSlice":"","useWidth":984},"quadrantChart":{"useMaxWidth":true,"chartWidth":500,"chartHeight":500,"titleFontSize":20,"titlePadding":10,"quadrantPadding":5,"xAxisLabelPadding":5,"yAxisLabelPadding":5,"xAxisLabelFontSize":16,"yAxisLabelFontSize":16,"quadrantLabelFontSize":16,"quadrantTextTopPadding":5,"pointTextPadding":5,"pointLabelFontSize":12,"pointRadius":5,"xAxisPosition":"top","yAxisPosition":"left","quadrantInternalBorderStrokeWidth":1,"quadrantExternalBorderStrokeWidth":2},"xyChart":{"useMaxWidth":true,"width":700,"height":500,"titleFontSize":20,"titlePadding":10,"showDataLabel":false,"showDataLabelOutsideBar":false,"showTitle":true,"showLegend":true,"legendFontSize":14,"legendPadding":10,"xAxis":{"showLabel":true,"labelFontSize":14,"labelPadding":5,"showTitle":true,"titleFontSize":16,"titlePadding":5,"showTick":true,"tickLength":5,"tickWidth":2,"showAxisLine":true,"axisLineWidth":2,"labelRotation":0},"yAxis":{"showLabel":true,"labelFontSize":14,"labelPadding":5,"showTitle":true,"titleFontSize":16,"titlePadding":5,"showTick":true,"tickLength":5,"tickWidth":2,"showAxisLine":true,"axisLineWidth":2,"labelRotation":0},"chartOrientation":"vertical","plotReservedSpacePercent":50},"requirement":{"useMaxWidth":true,"theme":"redux-color","look":"neo","rect_fill":"#f9f9f9","text_color":"#333","rect_border_size":"0.5px","rect_border_color":"#bbb","rect_min_width":200,"rect_min_height":200,"fontSize":14,"rect_padding":10,"line_height":20},"architecture":{"useMaxWidth":true,"padding":40,"iconSize":80,"fontSize":16,"randomize":false,"nodeSeparation":75,"idealEdgeLengthMultiplier":1.5,"edgeElasticity":0.45,"numIter":2500,"seed":1},"mindmap":{"useMaxWidth":true,"colorBy":"branch","padding":10,"maxNodeWidth":200,"layoutAlgorithm":"cose-bilkent"},"ishikawa":{"useMaxWidth":false,"diagramPadding":20},"kanban":{"useMaxWidth":true,"padding":8,"sectionWidth":200,"ticketBaseUrl":""},"gitGraph":{"useMaxWidth":true,"titleTopMargin":25,"diagramPadding":8,"nodeLabel":{"width":75,"height":100,"x":-25,"y":0},"mainBranchName":"main","mainBranchOrder":0,"showCommitLabel":true,"showBranches":true,"rotateCommitLabel":true,"parallelCommits":false,"arrowMarkerAbsolute":false},"c4":{"useMaxWidth":true,"diagramMarginX":50,"diagramMarginY":10,"c4ShapeMargin":50,"c4ShapePadding":20,"width":216,"height":60,"boxMargin":10,"c4ShapeInRow":4,"nextLinePaddingX":0,"c4BoundaryInRow":2,"personFontSize":14,"personFontFamily":"\"Open Sans\", sans-serif","personFontWeight":"normal","external_personFontSize":14,"external_personFontFamily":"\"Open Sans\", sans-serif","external_personFontWeight":"normal","systemFontSize":14,"systemFontFamily":"\"Open Sans\", sans-serif","systemFontWeight":"normal","external_systemFontSize":14,"external_systemFontFamily":"\"Open Sans\", sans-serif","external_systemFontWeight":"normal","system_dbFontSize":14,"system_dbFontFamily":"\"Open Sans\", sans-serif","system_dbFontWeight":"normal","external_system_dbFontSize":14,"external_system_dbFontFamily":"\"Open Sans\", sans-serif","external_system_dbFontWeight":"normal","system_queueFontSize":14,"system_queueFontFamily":"\"Open Sans\", sans-serif","system_queueFontWeight":"normal","external_system_queueFontSize":14,"external_system_queueFontFamily":"\"Open Sans\", sans-serif","external_system_queueFontWeight":"normal","boundaryFontSize":14,"boundaryFontFamily":"\"Open Sans\", sans-serif","boundaryFontWeight":"normal","messageFontSize":12,"messageFontFamily":"\"Open Sans\", sans-serif","messageFontWeight":"normal","containerFontSize":14,"containerFontFamily":"\"Open Sans\", sans-serif","containerFontWeight":"normal","external_containerFontSize":14,"external_containerFontFamily":"\"Open Sans\", sans-serif","external_containerFontWeight":"normal","container_dbFontSize":14,"container_dbFontFamily":"\"Open Sans\", sans-serif","container_dbFontWeight":"normal","external_container_dbFontSize":14,"external_container_dbFontFamily":"\"Open Sans\", sans-serif","external_container_dbFontWeight":"normal","container_queueFontSize":14,"container_queueFontFamily":"\"Open Sans\", sans-serif","container_queueFontWeight":"normal","external_container_queueFontSize":14,"external_container_queueFontFamily":"\"Open Sans\", sans-serif","external_container_queueFontWeight":"normal","componentFontSize":14,"componentFontFamily":"\"Open Sans\", sans-serif","componentFontWeight":"normal","external_componentFontSize":14,"external_componentFontFamily":"\"Open Sans\", sans-serif","external_componentFontWeight":"normal","component_dbFontSize":14,"component_dbFontFamily":"\"Open Sans\", sans-serif","component_dbFontWeight":"normal","external_component_dbFontSize":14,"external_component_dbFontFamily":"\"Open Sans\", sans-serif","external_component_dbFontWeight":"normal","component_queueFontSize":14,"component_queueFontFamily":"\"Open Sans\", sans-serif","component_queueFontWeight":"normal","external_component_queueFontSize":14,"external_component_queueFontFamily":"\"Open Sans\", sans-serif","external_component_queueFontWeight":"normal","wrap":true,"wrapPadding":10,"person_bg_color":"#08427B","person_border_color":"#073B6F","external_person_bg_color":"#686868","external_person_border_color":"#8A8A8A","system_bg_color":"#1168BD","system_border_color":"#3C7FC0","system_db_bg_color":"#1168BD","system_db_border_color":"#3C7FC0","system_queue_bg_color":"#1168BD","system_queue_border_color":"#3C7FC0","external_system_bg_color":"#999999","external_system_border_color":"#8A8A8A","external_system_db_bg_color":"#999999","external_system_db_border_color":"#8A8A8A","external_system_queue_bg_color":"#999999","external_system_queue_border_color":"#8A8A8A","container_bg_color":"#438DD5","container_border_color":"#3C7FC0","container_db_bg_color":"#438DD5","container_db_border_color":"#3C7FC0","container_queue_bg_color":"#438DD5","container_queue_border_color":"#3C7FC0","external_container_bg_color":"#B3B3B3","external_container_border_color":"#A6A6A6","external_container_db_bg_color":"#B3B3B3","external_container_db_border_color":"#A6A6A6","external_container_queue_bg_color":"#B3B3B3","external_container_queue_border_color":"#A6A6A6","component_bg_color":"#85BBF0","component_border_color":"#78A8D8","component_db_bg_color":"#85BBF0","component_db_border_color":"#78A8D8","component_queue_bg_color":"#85BBF0","component_queue_border_color":"#78A8D8","external_component_bg_color":"#CCCCCC","external_component_border_color":"#BFBFBF","external_component_db_bg_color":"#CCCCCC","external_component_db_border_color":"#BFBFBF","external_component_queue_bg_color":"#CCCCCC","external_component_queue_border_color":"#BFBFBF","flowchart":{"useMaxWidth":true,"theme":"redux-color","look":"neo","titleTopMargin":25,"subGraphTitleMargin":{"top":0,"bottom":0},"diagramPadding":8,"htmlLabels":null,"nodeSpacing":50,"rankSpacing":50,"curve":"basis","padding":15,"wrappingWidth":120,"minNodeWidth":120,"stepDuration":1,"stepNumbers":false,"inheritDir":false}},"sankey":{"useMaxWidth":false,"width":600,"height":400,"linkColor":"gradient","nodeAlignment":"justify","showValues":true,"prefix":"","suffix":"","nodeWidth":10,"nodePadding":12,"labelStyle":"legacy"},"packet":{"useMaxWidth":true,"rowHeight":32,"bitWidth":32,"bitsPerRow":32,"showBits":true,"bitOrder":"ascending","paddingX":5,"paddingY":5},"block":{"useMaxWidth":true,"padding":8},"eventmodeling":{"useMaxWidth":true,"padding":30,"rowHeight":32},"treeView":{"useMaxWidth":true,"rowIndent":10,"paddingX":5,"paddingY":5,"lineThickness":1,"showIcons":false,"defaultIconPack":"","filenameIcons":{},"extensionIcons":{}},"radar":{"useMaxWidth":true,"width":600,"height":600,"marginTop":50,"marginRight":50,"marginBottom":50,"marginLeft":50,"axisScaleFactor":1,"axisLabelFactor":1.05,"curveTension":0.17},"usecase":{"useMaxWidth":true,"theme":"redux-color","look":"neo","wrappingWidth":120,"minNodeWidth":120,"actorFontSize":14,"actorFontFamily":"\"Open Sans\", sans-serif","actorFontWeight":"normal","usecaseFontSize":12,"usecaseFontFamily":"\"Open Sans\", sans-serif","usecaseFontWeight":"normal","nodeSpacing":50,"rankSpacing":50,"diagramPadding":20,"colorScheme":"role"},"venn":{"useMaxWidth":true,"theme":"redux-color","look":"neo","width":800,"height":450,"padding":8,"useDebugLayout":false},"wardley-beta":{"useMaxWidth":true,"width":900,"height":600,"padding":48,"nodeRadius":6,"nodeLabelOffset":8,"axisFontSize":12,"labelFontSize":10,"showGrid":false},"cynefin":{"useMaxWidth":true,"width":800,"height":600,"padding":40,"showDomainDescriptions":true,"boundaryAmplitude":8,"seed":0},"railroad":{"useMaxWidth":true,"compactMode":false,"padding":10,"verticalSeparation":8,"horizontalSeparation":10,"arcRadius":10,"strokeWidth":2,"showMarkers":true,"markerRadius":5},"fontSize":16,"markdownAutoWrap":true,"suppressErrorRendering":false,"themeVariables":{"background":"white","primaryColor":"#ECECFF","secondaryColor":"#ffffde","tertiaryColor":"hsl(80, 100%, 96.2745098039%)","primaryBorderColor":"hsl(240, 60%, 86.2745098039%)","secondaryBorderColor":"hsl(60, 60%, 83.5294117647%)","tertiaryBorderColor":"hsl(80, 60%, 86.2745098039%)","primaryTextColor":"#131300","secondaryTextColor":"#000021","tertiaryTextColor":"rgb(6.3333333334, 0, 19.0000000001)","lineColor":"#333333","textColor":"#333","mainBkg":"#ECECFF","secondBkg":"#ffffde","border1":"#9370DB","border2":"#aaaa33","arrowheadColor":"#333333","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","labelBackground":"rgba(232,232,232, 0.8)","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#ECECFF","nodeBorder":"#9370DB","clusterBkg":"#ffffde","clusterBorder":"#aaaa33","defaultLinkColor":"#333333","titleColor":"#333","edgeLabelBackground":"rgba(232,232,232, 0.8)","actorBorder":"#9370DB","actorBkg":"#ECECFF","actorTextColor":"black","actorLineColor":"#9370DB","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#ECECFF","labelBoxBorderColor":"#9370DB","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#aaaa33","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"rgba(102, 102, 255, 0.49)","altSectionBkgColor":"white","sectionBkgColor2":"#fff400","excludeBkgColor":"#eeeeee","taskBorderColor":"#534fbc","taskBkgColor":"#8a90dd","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#534fbc","activeTaskBkgColor":"#bfc7ff","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(240, 60%, 86.2745098039%)","personBkg":"#ECECFF","archEdgeColor":"#333333","archEdgeArrowColor":"#333333","archEdgeWidth":"3","archGroupBorderColor":"hsl(240, 60%, 86.2745098039%)","archGroupBorderWidth":"2px","rowOdd":"hsl(240, 100%, 100%)","rowEven":"hsl(240, 100%, 97.2745098039%)","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":false,"gradientStart":"hsl(240, 60%, 86.2745098039%)","gradientStop":"hsl(60, 60%, 83.5294117647%)","dropShadow":"drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))","cScale0":"hsl(240, 100%, 76.2745098039%)","cScale1":"hsl(60, 100%, 73.5294117647%)","cScale2":"hsl(80, 100%, 76.2745098039%)","cScale3":"hsl(270, 100%, 76.2745098039%)","cScale4":"hsl(300, 100%, 76.2745098039%)","cScale5":"hsl(330, 100%, 76.2745098039%)","cScale6":"hsl(0, 100%, 76.2745098039%)","cScale7":"hsl(30, 100%, 76.2745098039%)","cScale8":"hsl(90, 100%, 76.2745098039%)","cScale9":"hsl(150, 100%, 76.2745098039%)","cScale10":"hsl(180, 100%, 76.2745098039%)","cScale11":"hsl(210, 100%, 76.2745098039%)","cScalePeer1":"hsl(60, 100%, 48.5294117647%)","cScalePeer2":"hsl(80, 100%, 56.2745098039%)","cScalePeer0":"hsl(240, 100%, 61.2745098039%)","cScalePeer3":"hsl(270, 100%, 61.2745098039%)","cScalePeer4":"hsl(300, 100%, 61.2745098039%)","cScalePeer5":"hsl(330, 100%, 61.2745098039%)","cScalePeer6":"hsl(0, 100%, 61.2745098039%)","cScalePeer7":"hsl(30, 100%, 61.2745098039%)","cScalePeer8":"hsl(90, 100%, 61.2745098039%)","cScalePeer9":"hsl(150, 100%, 61.2745098039%)","cScalePeer10":"hsl(180, 100%, 61.2745098039%)","cScalePeer11":"hsl(210, 100%, 61.2745098039%)","cScaleInv0":"hsl(60, 100%, 86.2745098039%)","cScaleInv1":"hsl(240, 100%, 83.5294117647%)","cScaleInv2":"hsl(260, 100%, 86.2745098039%)","cScaleInv3":"hsl(90, 100%, 86.2745098039%)","cScaleInv4":"hsl(120, 100%, 86.2745098039%)","cScaleInv5":"hsl(150, 100%, 86.2745098039%)","cScaleInv6":"hsl(180, 100%, 86.2745098039%)","cScaleInv7":"hsl(210, 100%, 86.2745098039%)","cScaleInv8":"hsl(270, 100%, 86.2745098039%)","cScaleInv9":"hsl(330, 100%, 86.2745098039%)","cScaleInv10":"hsl(0, 100%, 86.2745098039%)","cScaleInv11":"hsl(30, 100%, 86.2745098039%)","surface0":"hsl(270, 100%, 91.2745098039%)","surfacePeer0":"hsl(270, 100%, 89.2745098039%)","surface1":"hsl(270, 100%, 86.2745098039%)","surfacePeer1":"hsl(270, 100%, 84.2745098039%)","surface2":"hsl(270, 100%, 81.2745098039%)","surfacePeer2":"hsl(270, 100%, 79.2745098039%)","surface3":"hsl(270, 100%, 76.2745098039%)","surfacePeer3":"hsl(270, 100%, 74.2745098039%)","surface4":"hsl(270, 100%, 71.2745098039%)","surfacePeer4":"hsl(270, 100%, 69.2745098039%)","scaleLabelColor":"black","flowContainerStroke":"hsl(60, 60%, 83.5294117647%)","rectBkgColor":"hsl(80, 100%, 96.2745098039%)","transitionColor":"#333333","transitionLabelColor":"#333","stateLabelColor":"#131300","stateBkg":"#ECECFF","labelBackgroundColor":"#ECECFF","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#ECECFF","compositeBorder":"#9370DB","innerEndBackground":"#9370DB","specialStateColor":"#333333","classText":"#131300","fillType0":"#ECECFF","fillType1":"#ffffde","fillType2":"hsl(304, 100%, 96.2745098039%)","fillType3":"hsl(124, 100%, 93.5294117647%)","fillType4":"hsl(176, 100%, 96.2745098039%)","fillType5":"hsl(-4, 100%, 93.5294117647%)","fillType6":"hsl(8, 100%, 96.2745098039%)","fillType7":"hsl(188, 100%, 93.5294117647%)","pie1":"#ECECFF","pie2":"#ffffde","pie3":"hsl(80, 100%, 56.2745098039%)","pie4":"hsl(240, 100%, 86.2745098039%)","pie5":"hsl(60, 100%, 63.5294117647%)","pie6":"hsl(80, 100%, 76.2745098039%)","pie7":"hsl(300, 100%, 76.2745098039%)","pie8":"hsl(180, 100%, 56.2745098039%)","pie9":"hsl(0, 100%, 56.2745098039%)","pie10":"hsl(300, 100%, 56.2745098039%)","pie11":"hsl(150, 100%, 56.2745098039%)","pie12":"hsl(0, 100%, 66.2745098039%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(240, 100%, 66.2745098039%)","venn2":"hsl(60, 100%, 63.5294117647%)","venn3":"hsl(80, 100%, 56.2745098039%)","venn4":"hsl(300, 100%, 66.2745098039%)","venn5":"hsl(180, 100%, 66.2745098039%)","venn6":"hsl(120, 100%, 63.5294117647%)","venn7":"hsl(0, 100%, 66.2745098039%)","venn8":"hsl(180, 100%, 63.5294117647%)","vennTitleTextColor":"#333","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#333333","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#333333","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#131300"},"quadrant1Fill":"#ECECFF","quadrant2Fill":"#f1f1ff","quadrant3Fill":"#f6f6ff","quadrant4Fill":"#fbfbff","quadrant1TextFill":"#131300","quadrant2TextFill":"#0e0e00","quadrant3TextFill":"#090900","quadrant4TextFill":"#040400","quadrantPointFill":"hsl(240, 100%, NaN%)","quadrantPointTextFill":"#131300","quadrantXAxisTextFill":"#131300","quadrantYAxisTextFill":"#131300","quadrantInternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantExternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantTitleFill":"#131300","radar":{"axisColor":"#333333","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#333333","axisTextColor":"#131300","gridColor":"lightgrey","componentFill":"white","componentStroke":"#333333","componentLabelColor":"#131300","linkStroke":"#333333","evolutionStroke":"#dc3545","annotationStroke":"#333333","annotationTextColor":"#131300","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#131300","dataLabelColor":"#131300","legendTextColor":"#131300","xAxisTitleColor":"#131300","xAxisLabelColor":"#131300","xAxisTickColor":"#131300","xAxisLineColor":"#131300","yAxisTitleColor":"#131300","yAxisLabelColor":"#131300","yAxisTickColor":"#131300","yAxisLineColor":"#131300","plotColorPalette":"#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3"},"requirementBackground":"#ECECFF","requirementBorderColor":"hsl(240, 60%, 86.2745098039%)","requirementBorderSize":"1","requirementTextColor":"#131300","relationColor":"#333333","relationLabelBackground":"rgba(232,232,232, 0.8)","relationLabelColor":"black","git0":"hsl(240, 100%, 46.2745098039%)","git1":"hsl(60, 100%, 43.5294117647%)","git2":"hsl(80, 100%, 46.2745098039%)","git3":"hsl(210, 100%, 46.2745098039%)","git4":"hsl(180, 100%, 46.2745098039%)","git5":"hsl(150, 100%, 46.2745098039%)","git6":"hsl(300, 100%, 46.2745098039%)","git7":"hsl(0, 100%, 46.2745098039%)","gitInv0":"hsl(60, 100%, 3.7254901961%)","gitInv1":"rgb(0, 0, 160.5)","gitInv2":"rgb(48.8333333334, 0, 146.5000000001)","gitInv3":"rgb(146.5000000001, 73.2500000001, 0)","gitInv4":"rgb(146.5000000001, 0, 0)","gitInv5":"rgb(146.5000000001, 0, 73.2500000001)","gitInv6":"rgb(0, 146.5000000001, 0)","gitInv7":"rgb(0, 146.5000000001, 146.5000000001)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#131300","tagLabelBackground":"#ECECFF","tagLabelBorder":"hsl(240, 60%, 86.2745098039%)","tagLabelFontSize":"10px","commitLabelColor":"#000021","commitLabelBackground":"#ffffde","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#333333","emRelationStroke":"#333333","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","cScaleLabel0":"#ffffff","cScaleLabel3":"#ffffff","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black"},"treemap":{"useMaxWidth":true,"padding":10,"diagramPadding":8,"showValues":true,"nodeWidth":100,"nodeHeight":40,"borderWidth":1,"valueFontSize":12,"labelFontSize":14,"valueFormat":","}}"##;

    /// JSON writes an integer either as `5` or as `5` read back into an f64; compare by value.
    fn normalize(value: Value) -> Value {
        match value {
            Value::Number(number) => match number.as_f64() {
                Some(float) if float.fract() == 0.0 && float.abs() < 9007199254740992.0 => {
                    json!(float as i64)
                }
                _ => Value::Number(number),
            },
            Value::Array(values) => Value::Array(values.into_iter().map(normalize).collect()),
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, normalize(value)))
                    .collect(),
            ),
            other => other,
        }
    }

    fn differences(expected: &Value, actual: &Value, path: &str, out: &mut Vec<String>) {
        match (expected, actual) {
            (Value::Object(expected), Value::Object(actual)) => {
                for (key, value) in expected {
                    let child = format!("{path}.{key}");
                    match actual.get(key) {
                        Some(actual) => differences(value, actual, &child, out),
                        None => out.push(format!("{child}: missing")),
                    }
                }
                for key in actual.keys() {
                    if !expected.contains_key(key) {
                        out.push(format!("{path}.{key}: unexpected"));
                    }
                }
            }
            (expected, actual) if expected != actual => {
                out.push(format!("{path}: expected {expected}, got {actual}"));
            }
            _ => {}
        }
    }

    #[test]
    fn the_default_configuration_matches_upstream() {
        let expected =
            normalize(serde_json::from_str(UPSTREAM_DEFAULT).expect("the fixture is valid JSON"));
        let actual = normalize(Config::default().to_json());
        let mut found = Vec::new();
        differences(&expected, &actual, "", &mut found);
        found.sort();
        assert!(found.is_empty(), "{found:#?}");
    }
}
