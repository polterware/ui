//! One module per diagram type, each with its own `Ast` and `parse`. A module only
//! edits its own folder; detection lives in [`detect`].

use crate::error::ParseError;

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
pub mod agentflow;

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
pub fn detect(_body: &str) -> Option<DiagramKind> {
    None
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
