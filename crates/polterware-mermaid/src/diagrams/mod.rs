//! One module per diagram type, each with its own `Ast` and `parse`. A module only
//! edits its own folder; detection lives in [`detect`].

use crate::error::ParseError;

pub mod agentflow;
pub mod architecture;
pub mod block;
pub mod c4;
pub mod class;
pub mod cynefin;
pub mod er;
pub mod eventmodeling;
pub mod flowchart;
pub mod gantt;
pub mod git;
pub mod info;
pub mod ishikawa;
pub mod kanban;
pub mod mindmap;
pub mod packet;
pub mod pie;
pub mod quadrant_chart;
pub mod radar;
pub mod railroad;
pub mod requirement;
pub mod sankey;
pub mod sequence;
pub mod state;
pub mod swimlanes;
pub mod timeline;
pub mod tree_view;
pub mod treemap;
pub mod usecase;
pub mod user_journey;
pub mod venn;
pub mod wardley;
pub mod xychart;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagramKind {
    Architecture,
    Block,
    C4,
    Class,
    Cynefin,
    Er,
    Eventmodeling,
    Flowchart,
    Gantt,
    Git,
    Info,
    Ishikawa,
    Kanban,
    Mindmap,
    Packet,
    Pie,
    QuadrantChart,
    Radar,
    Railroad,
    Requirement,
    Sankey,
    Sequence,
    State,
    Swimlanes,
    Timeline,
    TreeView,
    Treemap,
    Usecase,
    UserJourney,
    Venn,
    Wardley,
    Xychart,
    Agentflow,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Diagram {
    Architecture(architecture::Ast),
    Block(block::Ast),
    C4(c4::Ast),
    Class(class::Ast),
    Cynefin(cynefin::Ast),
    Er(er::Ast),
    Eventmodeling(eventmodeling::Ast),
    Flowchart(flowchart::Ast),
    Gantt(gantt::Ast),
    Git(git::Ast),
    Info(info::Ast),
    Ishikawa(ishikawa::Ast),
    Kanban(kanban::Ast),
    Mindmap(mindmap::Ast),
    Packet(packet::Ast),
    Pie(pie::Ast),
    QuadrantChart(quadrant_chart::Ast),
    Radar(radar::Ast),
    Railroad(railroad::Ast),
    Requirement(requirement::Ast),
    Sankey(sankey::Ast),
    Sequence(sequence::Ast),
    State(state::Ast),
    Swimlanes(swimlanes::Ast),
    Timeline(timeline::Ast),
    TreeView(tree_view::Ast),
    Treemap(treemap::Ast),
    Usecase(usecase::Ast),
    UserJourney(user_journey::Ast),
    Venn(venn::Ast),
    Wardley(wardley::Ast),
    Xychart(xychart::Ast),
    Agentflow(agentflow::Ast),
}

/// The diagram type the body declares (upstream `detectType` and each diagram's
/// detector), from its first meaningful line. `None` when no type matches.
///
/// Frontmatter, directives, and `%%` comment lines are skipped first, exactly as
/// `diagram-api/detectType.ts` does before it walks its detector registry. The detectors
/// run in upstream's registration order (`diagram-api/diagram-orchestration.ts`), because
/// the first one to match wins: `flowchart-elk` before `flowchart`, for instance.
///
/// Upstream also registers two pseudo diagrams that no [`DiagramKind`] describes: `error`
/// (the whole text is `error`) and `---` (text that still starts with a frontmatter fence,
/// which means the frontmatter was malformed). Both answer `None` here.
pub fn detect(body: &str) -> Option<DiagramKind> {
    detect_tag(body).map(|(_, kind)| kind)
}

/// The detector id and kind upstream's `detectType` would answer, in the same walk as
/// [`detect`]. `detectInit` needs the id: it moves a directive's `config` value under the
/// id, and `flowchart-elk` keeps its own id even though it parses as a flowchart.
pub(crate) fn detect_tag(body: &str) -> Option<(&'static str, DiagramKind)> {
    let text = crate::preprocess::detection_text(body);
    DETECTORS
        .iter()
        .find(|(_, _, detector)| detector(&text))
        .map(|(id, kind, _)| (*id, *kind))
}

/// A diagram detector, the shape upstream's `DiagramDetector` has.
type Detector = fn(&str) -> bool;

/// Upstream's detectors, with the id `registerLazyLoadedDiagrams` registers them under, in
/// the order it adds them. The large-feature group (`flowchart-elk`, `mindmap`,
/// `architecture`) is registered first when the bundle includes it, and the crate supports
/// every diagram, so it always is.
const DETECTORS: &[(&str, DiagramKind, Detector)] = &[
    (
        "flowchart-elk",
        DiagramKind::Flowchart,
        detects_flowchart_elk,
    ),
    ("mindmap", DiagramKind::Mindmap, detects_mindmap),
    (
        "architecture",
        DiagramKind::Architecture,
        detects_architecture,
    ),
    ("agentflow", DiagramKind::Agentflow, detects_agentflow),
    ("c4", DiagramKind::C4, detects_c4),
    ("kanban", DiagramKind::Kanban, detects_kanban),
    ("classDiagram", DiagramKind::Class, detects_class),
    ("er", DiagramKind::Er, detects_er),
    ("gantt", DiagramKind::Gantt, detects_gantt),
    ("info", DiagramKind::Info, detects_info),
    ("pie", DiagramKind::Pie, detects_pie),
    ("requirement", DiagramKind::Requirement, detects_requirement),
    ("sequence", DiagramKind::Sequence, detects_sequence),
    ("swimlane", DiagramKind::Swimlanes, detects_swimlanes),
    ("flowchart-v2", DiagramKind::Flowchart, detects_flowchart),
    ("timeline", DiagramKind::Timeline, detects_timeline),
    ("gitGraph", DiagramKind::Git, detects_git),
    ("stateDiagram", DiagramKind::State, detects_state),
    ("journey", DiagramKind::UserJourney, detects_journey),
    (
        "quadrantChart",
        DiagramKind::QuadrantChart,
        detects_quadrant_chart,
    ),
    ("sankey", DiagramKind::Sankey, detects_sankey),
    ("packet", DiagramKind::Packet, detects_packet),
    ("xychart", DiagramKind::Xychart, detects_xychart),
    ("block", DiagramKind::Block, detects_block),
    (
        "eventmodeling",
        DiagramKind::Eventmodeling,
        detects_eventmodeling,
    ),
    ("treeView", DiagramKind::TreeView, detects_tree_view),
    ("radar", DiagramKind::Radar, detects_radar),
    ("ishikawa", DiagramKind::Ishikawa, detects_ishikawa),
    ("treemap", DiagramKind::Treemap, detects_treemap),
    ("railroad", DiagramKind::Railroad, detects_railroad),
    ("railroadEbnf", DiagramKind::Railroad, detects_railroad_ebnf),
    ("railroadAbnf", DiagramKind::Railroad, detects_railroad_abnf),
    ("railroadPeg", DiagramKind::Railroad, detects_railroad_peg),
    ("venn", DiagramKind::Venn, detects_venn),
    ("wardley", DiagramKind::Wardley, detects_wardley),
    ("cynefin", DiagramKind::Cynefin, detects_cynefin),
    ("usecase", DiagramKind::Usecase, detects_usecase),
];

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `^\s*prefix`, the shape most detectors have.
fn starts_with_keyword(text: &str, prefix: &str) -> bool {
    text.trim_start_matches(crate::preprocess::is_js_whitespace)
        .starts_with(prefix)
}

/// `^\s*prefix` with the regex `i` flag.
fn starts_with_keyword_ignore_case(text: &str, prefix: &str) -> bool {
    let text = text.trim_start_matches(crate::preprocess::is_js_whitespace);
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// `^\s*prefix\b`: the character after the prefix must not be a word character.
fn starts_with_word(text: &str, prefix: &str) -> bool {
    let text = text.trim_start_matches(crate::preprocess::is_js_whitespace);
    text.strip_prefix(prefix)
        .is_some_and(|rest| rest.chars().next().is_none_or(|c| !is_word_char(c)))
}

/// `^\s*prefix\b` with the regex `i` flag.
fn starts_with_word_ignore_case(text: &str, prefix: &str) -> bool {
    let text = text.trim_start_matches(crate::preprocess::is_js_whitespace);
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        && text[prefix.len()..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c))
}

/// `^\s*prefix(?=allowed|$)`, for the detectors that delimit their keyword.
fn starts_with_then(text: &str, prefix: &str, allowed: impl Fn(char) -> bool) -> bool {
    let text = text.trim_start_matches(crate::preprocess::is_js_whitespace);
    text.strip_prefix(prefix)
        .is_some_and(|rest| rest.chars().next().is_none_or(allowed))
}

fn detects_flowchart_elk(text: &str) -> bool {
    starts_with_keyword(text, "flowchart-elk")
}

fn detects_mindmap(text: &str) -> bool {
    starts_with_keyword(text, "mindmap")
}

fn detects_architecture(text: &str) -> bool {
    starts_with_keyword(text, "architecture")
}

fn detects_agentflow(text: &str) -> bool {
    starts_with_word(text, "agentflow-beta")
}

fn detects_c4(text: &str) -> bool {
    // Upstream's alternation only anchors the first branch: `^\s*C4Context|C4Container|...`.
    starts_with_keyword(text, "C4Context")
        || text.contains("C4Container")
        || text.contains("C4Component")
        || text.contains("C4Dynamic")
        || text.contains("C4Deployment")
}

fn detects_kanban(text: &str) -> bool {
    starts_with_keyword(text, "kanban")
}

fn detects_class(text: &str) -> bool {
    // Both `classDiagram` and `classDiagram-v2`.
    starts_with_keyword(text, "classDiagram")
}

fn detects_er(text: &str) -> bool {
    starts_with_keyword(text, "erDiagram")
}

fn detects_gantt(text: &str) -> bool {
    starts_with_keyword(text, "gantt")
}

fn detects_info(text: &str) -> bool {
    starts_with_keyword(text, "info")
}

fn detects_pie(text: &str) -> bool {
    starts_with_keyword(text, "pie")
}

fn detects_requirement(text: &str) -> bool {
    // `requirement` and `requirementDiagram`.
    starts_with_keyword(text, "requirement")
}

fn detects_sequence(text: &str) -> bool {
    starts_with_keyword(text, "sequenceDiagram")
}

fn detects_swimlanes(text: &str) -> bool {
    starts_with_word(text, "swimlane-beta")
}

fn detects_flowchart(text: &str) -> bool {
    // `graph` and `flowchart` both render with the unified flowchart.
    starts_with_keyword(text, "graph") || starts_with_keyword(text, "flowchart")
}

fn detects_timeline(text: &str) -> bool {
    starts_with_keyword(text, "timeline")
}

fn detects_git(text: &str) -> bool {
    starts_with_keyword(text, "gitGraph")
}

fn detects_state(text: &str) -> bool {
    // Both `stateDiagram` and `stateDiagram-v2`.
    starts_with_keyword(text, "stateDiagram")
}

fn detects_journey(text: &str) -> bool {
    starts_with_keyword(text, "journey")
}

fn detects_quadrant_chart(text: &str) -> bool {
    starts_with_keyword(text, "quadrantChart")
}

fn detects_sankey(text: &str) -> bool {
    // `sankey` and `sankey-beta`.
    starts_with_keyword(text, "sankey")
}

fn detects_packet(text: &str) -> bool {
    // `packet` and `packet-beta`.
    starts_with_keyword(text, "packet")
}

fn detects_xychart(text: &str) -> bool {
    // `xychart` and `xychart-beta`.
    starts_with_keyword(text, "xychart")
}

fn detects_block(text: &str) -> bool {
    // `block` and `block-beta`.
    starts_with_keyword(text, "block")
}

fn detects_eventmodeling(text: &str) -> bool {
    starts_with_keyword(text, "eventmodeling")
}

fn detects_tree_view(text: &str) -> bool {
    starts_with_keyword(text, "treeView-beta")
}

fn detects_radar(text: &str) -> bool {
    starts_with_keyword(text, "radar-beta")
}

fn detects_ishikawa(text: &str) -> bool {
    // `ishikawa` and `ishikawa-beta`, case-insensitively.
    starts_with_word_ignore_case(text, "ishikawa-beta")
        || starts_with_word_ignore_case(text, "ishikawa")
}

fn detects_treemap(text: &str) -> bool {
    starts_with_keyword(text, "treemap")
}

fn detects_railroad(text: &str) -> bool {
    starts_with_keyword_ignore_case(text, "railroad-beta")
}

fn detects_railroad_ebnf(text: &str) -> bool {
    starts_with_keyword_ignore_case(text, "railroad-ebnf-beta")
}

fn detects_railroad_abnf(text: &str) -> bool {
    starts_with_keyword_ignore_case(text, "railroad-abnf-beta")
}

fn detects_railroad_peg(text: &str) -> bool {
    starts_with_keyword_ignore_case(text, "railroad-peg-beta")
}

fn detects_venn(text: &str) -> bool {
    starts_with_keyword(text, "venn-beta")
}

fn detects_wardley(text: &str) -> bool {
    starts_with_keyword_ignore_case(text, "wardley-beta")
}

fn detects_cynefin(text: &str) -> bool {
    starts_with_then(text, "cynefin-beta", |c| {
        crate::preprocess::is_js_whitespace(c) || c == ':'
    })
}

fn detects_usecase(text: &str) -> bool {
    starts_with_then(text, "usecase-beta", crate::preprocess::is_js_whitespace)
}

pub fn parse_kind(kind: DiagramKind, body: &str) -> Result<Diagram, ParseError> {
    match kind {
        DiagramKind::Architecture => architecture::parse(body).map(Diagram::Architecture),
        DiagramKind::Block => block::parse(body).map(Diagram::Block),
        DiagramKind::C4 => c4::parse(body).map(Diagram::C4),
        DiagramKind::Class => class::parse(body).map(Diagram::Class),
        DiagramKind::Cynefin => cynefin::parse(body).map(Diagram::Cynefin),
        DiagramKind::Er => er::parse(body).map(Diagram::Er),
        DiagramKind::Eventmodeling => eventmodeling::parse(body).map(Diagram::Eventmodeling),
        DiagramKind::Flowchart => flowchart::parse(body).map(Diagram::Flowchart),
        DiagramKind::Gantt => gantt::parse(body).map(Diagram::Gantt),
        DiagramKind::Git => git::parse(body).map(Diagram::Git),
        DiagramKind::Info => info::parse(body).map(Diagram::Info),
        DiagramKind::Ishikawa => ishikawa::parse(body).map(Diagram::Ishikawa),
        DiagramKind::Kanban => kanban::parse(body).map(Diagram::Kanban),
        DiagramKind::Mindmap => mindmap::parse(body).map(Diagram::Mindmap),
        DiagramKind::Packet => packet::parse(body).map(Diagram::Packet),
        DiagramKind::Pie => pie::parse(body).map(Diagram::Pie),
        DiagramKind::QuadrantChart => quadrant_chart::parse(body).map(Diagram::QuadrantChart),
        DiagramKind::Radar => radar::parse(body).map(Diagram::Radar),
        DiagramKind::Railroad => railroad::parse(body).map(Diagram::Railroad),
        DiagramKind::Requirement => requirement::parse(body).map(Diagram::Requirement),
        DiagramKind::Sankey => sankey::parse(body).map(Diagram::Sankey),
        DiagramKind::Sequence => sequence::parse(body).map(Diagram::Sequence),
        DiagramKind::State => state::parse(body).map(Diagram::State),
        DiagramKind::Swimlanes => swimlanes::parse(body).map(Diagram::Swimlanes),
        DiagramKind::Timeline => timeline::parse(body).map(Diagram::Timeline),
        DiagramKind::TreeView => tree_view::parse(body).map(Diagram::TreeView),
        DiagramKind::Treemap => treemap::parse(body).map(Diagram::Treemap),
        DiagramKind::Usecase => usecase::parse(body).map(Diagram::Usecase),
        DiagramKind::UserJourney => user_journey::parse(body).map(Diagram::UserJourney),
        DiagramKind::Venn => venn::parse(body).map(Diagram::Venn),
        DiagramKind::Wardley => wardley::parse(body).map(Diagram::Wardley),
        DiagramKind::Xychart => xychart::parse(body).map(Diagram::Xychart),
        DiagramKind::Agentflow => agentflow::parse(body).map(Diagram::Agentflow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DiagramKind::*;

    #[test]
    fn detects_a_graph_definition() {
        assert_eq!(detect("graph TB\nbfs1:queue"), Some(Flowchart));
    }

    #[test]
    fn handles_a_graph_definition_with_leading_spaces() {
        assert_eq!(detect("    graph TB\nbfs1:queue"), Some(Flowchart));
    }

    #[test]
    fn handles_a_graph_definition_with_leading_spaces_and_newline() {
        assert_eq!(detect("  \n  graph TB\nbfs1:queue"), Some(Flowchart));
    }

    #[test]
    fn handles_a_graph_definition_for_git_graph() {
        assert_eq!(detect("  \n  gitGraph TB:\nbfs1:queue"), Some(Git));
    }

    #[test]
    fn handles_frontmatter() {
        assert_eq!(
            detect("---\ntitle: foo\n---\n  gitGraph TB:\nbfs1:queue"),
            Some(Git)
        );
    }

    #[test]
    fn handles_malformed_frontmatter_with_leading_spaces() {
        // Upstream answers the pseudo diagram `---`, which has no `DiagramKind`.
        assert_eq!(
            detect("    ---\ntitle: foo\n---\n  gitGraph TB:\nbfs1:queue"),
            None
        );
    }

    #[test]
    fn skips_init_and_initialize_directives() {
        for directive in ["init", "initialize"] {
            let text = format!(
                "%%{{{directive}: {{ 'logLevel': 0 }}}}%%\nsequenceDiagram\nAlice->Bob: hi"
            );
            assert_eq!(detect(&text), Some(Sequence), "directive: {directive}");
        }
        let multiline =
            "%%{\n  init: {\n    'theme': 'dark'\n  }\n}%%\nsequenceDiagram\nAlice->Bob: hi";
        assert_eq!(detect(multiline), Some(Sequence));
    }

    #[test]
    fn skips_comment_lines() {
        assert_eq!(detect("%% a comment\ngraph LR\nA --> B"), Some(Flowchart));
        assert_eq!(
            detect("%% a comment\n%% another\ngraph LR\nA --> B"),
            Some(Flowchart)
        );
        assert_eq!(detect("graph LR\n%% a comment\nA --> B"), Some(Flowchart));
    }

    #[test]
    fn skips_comments_like_upstreams_comment_spec() {
        let text = "\n\n%% This is a comment\n%% This is another comment\ngraph TD\n    A-->B\n%% This is a comment\n";
        assert_eq!(detect(text), Some(Flowchart));

        let indented =
            "\n%% This is a comment\ngraph TD\n    A-->B\n    %% This is a comment\n    C-->D\n";
        assert_eq!(detect(indented), Some(Flowchart));

        let leading_blank_lines = "\n\n\n\n\n%% This is a comment\ngraph TD\n    A-->B\n";
        assert_eq!(detect(leading_blank_lines), Some(Flowchart));
    }

    #[test]
    fn keeps_init_statements_while_skipping_comments() {
        let text = "\n%% This is a comment\n\n%% This is another comment\n%%{init: {'theme': 'forest'}}%%\n%%{ init: {'theme': 'space before init'}}%%\n%%{init: {'theme': 'space after ending'}}%%\ngraph TD\n    A-->B\n\n    B-->C\n%% This is a comment\n";
        assert_eq!(detect(text), Some(Flowchart));
    }

    #[test]
    fn detects_every_diagram_type() {
        let cases = [
            ("agentflow-beta\n", Agentflow),
            ("architecture-beta\n", Architecture),
            ("block\n", Block),
            ("block-beta\n", Block),
            ("C4Context\n", C4),
            ("classDiagram\n", Class),
            ("classDiagram-v2\n", Class),
            ("cynefin-beta\n", Cynefin),
            ("erDiagram\n", Er),
            ("eventmodeling\n", Eventmodeling),
            ("flowchart TD\n", Flowchart),
            ("graph LR\n", Flowchart),
            ("gantt\n", Gantt),
            ("gitGraph\n", Git),
            ("info\n", Info),
            ("ishikawa\n", Ishikawa),
            ("ishikawa-beta\n", Ishikawa),
            ("kanban\n", Kanban),
            ("mindmap\n", Mindmap),
            ("packet\n", Packet),
            ("packet-beta\n", Packet),
            ("pie\n", Pie),
            ("quadrantChart\n", QuadrantChart),
            ("radar-beta\n", Radar),
            ("railroad-beta\n", Railroad),
            ("railroad-ebnf-beta\n", Railroad),
            ("railroad-abnf-beta\n", Railroad),
            ("railroad-peg-beta\n", Railroad),
            ("requirement\n", Requirement),
            ("requirementDiagram\n", Requirement),
            ("sankey\n", Sankey),
            ("sankey-beta\n", Sankey),
            ("sequenceDiagram\n", Sequence),
            ("stateDiagram\n", State),
            ("stateDiagram-v2\n", State),
            ("swimlane-beta\n", Swimlanes),
            ("timeline\n", Timeline),
            ("treeView-beta\n", TreeView),
            ("treemap\n", Treemap),
            ("usecase-beta\n", Usecase),
            ("journey\n", UserJourney),
            ("venn-beta\n", Venn),
            ("wardley-beta\n", Wardley),
            ("xychart\n", Xychart),
            ("xychart-beta\n", Xychart),
        ];
        for (text, expected) in cases {
            assert_eq!(detect(text), Some(expected), "text: {text:?}");
        }
    }

    #[test]
    fn flowchart_elk_comes_before_flowchart() {
        assert_eq!(detect("flowchart-elk LR\nA --> B"), Some(Flowchart));
        assert_eq!(detect("  flowchart-elk LR\nA --> B"), Some(Flowchart));
    }

    #[test]
    fn railroad_detector_matches_upstream() {
        assert_eq!(detect("railroad-beta\nrule = \"test\" ;"), Some(Railroad));
        assert_eq!(detect("  railroad-beta\nrule = \"test\" ;"), Some(Railroad));
        assert_eq!(detect("RAILROAD-BETA\nrule = \"test\" ;"), Some(Railroad));
        assert_eq!(detect("Railroad-Beta\nrule = \"test\" ;"), Some(Railroad));
        assert_eq!(detect("flowchart TD\nA --> B"), Some(Flowchart));
        assert_eq!(detect("some text railroad-beta\nrule = \"test\" ;"), None);
        assert_eq!(detect(""), None);
        assert_eq!(detect("   \n  \t  "), None);
        assert_eq!(
            detect("\n\nrailroad-beta\nrule = \"test\" ;"),
            Some(Railroad)
        );
    }

    #[test]
    fn detectors_that_delimit_their_keyword() {
        assert_eq!(detect("agentflow-beta"), Some(Agentflow));
        assert_eq!(detect("agentflow-beta extra"), Some(Agentflow));
        assert_eq!(detect("agentflow-betax"), None);
        assert_eq!(detect("swimlane-beta\n"), Some(Swimlanes));
        assert_eq!(detect("swimlane-betas"), None);
        assert_eq!(detect("cynefin-beta"), Some(Cynefin));
        assert_eq!(detect("cynefin-beta:"), Some(Cynefin));
        assert_eq!(detect("cynefin-beta map"), Some(Cynefin));
        assert_eq!(detect("cynefin-betax"), None);
        assert_eq!(detect("usecase-beta"), Some(Usecase));
        assert_eq!(detect("usecase-beta x"), Some(Usecase));
        assert_eq!(detect("usecase-betax"), None);
    }

    #[test]
    fn c4_matches_like_upstreams_alternation() {
        assert_eq!(detect("  C4Context"), Some(C4));
        assert_eq!(detect("C4Container"), Some(C4));
        assert_eq!(detect("C4Component"), Some(C4));
        assert_eq!(detect("C4Dynamic"), Some(C4));
        assert_eq!(detect("C4Deployment"), Some(C4));
        // Only the first branch is anchored, exactly as upstream writes it.
        assert_eq!(detect("a title mentioning C4Container"), Some(C4));
        assert_eq!(detect("c4Context"), None);
    }

    #[test]
    fn diagram_keywords_are_matched_case_sensitively_where_upstream_does() {
        assert_eq!(detect("GRAPH TD"), None);
        assert_eq!(detect("ISHIKAWA"), Some(Ishikawa));
        assert_eq!(detect("Wardley-Beta"), Some(Wardley));
    }

    #[test]
    fn pseudo_diagrams_are_not_kinds() {
        assert_eq!(detect("error"), None);
        assert_eq!(detect("  ERROR  "), None);
        assert_eq!(detect("---\ntitle: foo\n---"), None);
    }

    #[test]
    fn comments_and_whitespace_alone_have_no_diagram() {
        assert_eq!(detect("%% only a comment\n"), None);
        assert_eq!(detect("\n\n\t\n"), None);
    }

    #[test]
    fn beta_suffixes_are_optional_only_for_graduated_diagrams() {
        for (stable, beta) in [
            ("sankey", "sankey-beta"),
            ("packet", "packet-beta"),
            ("xychart", "xychart-beta"),
            ("block", "block-beta"),
            ("ishikawa", "ishikawa-beta"),
            ("architecture", "architecture-beta"),
            ("treemap", "treemap-beta"),
        ] {
            assert!(detect(stable).is_some(), "stable: {stable}");
            assert_eq!(detect(stable), detect(beta), "beta: {beta}");
        }
        // Diagrams still in beta reject the bare keyword.
        for keyword in ["venn", "radar", "wardley", "treeView", "usecase", "cynefin"] {
            assert_eq!(detect(keyword), None, "keyword: {keyword}");
        }
    }
}
