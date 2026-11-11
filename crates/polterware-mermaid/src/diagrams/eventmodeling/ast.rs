//! The event modeling database after parsing: what upstream's `setAst` stores in `store.ast`
//! (`packages/mermaid/src/diagrams/eventmodeling/db.ts`) plus the common fields
//! `populateCommonDb` copies in.
//!
//! Upstream's `EventModel` AST keeps cross-references as Langium `Reference`s. Mermaid parses
//! a diagram with the bare `LangiumParser` and never links it, so each reference carries only
//! the source text it matched (`$refText`); this module keeps that text as the name. The frame
//! tag lives in `$type` (`EmTimeFrame` or `EmResetFrame`), which `isEmResetFrame` tests, and
//! becomes [`FrameKind`]. `getConfig`, `getDiagramProps`, and the `getState` projection are not
//! data the database holds, so they are not here.

/// Which grammar rule produced a frame: `EmTimeFrame` or `EmResetFrame` (`isEmResetFrame`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameKind {
    /// `EmTimeFrame`, introduced by `tf` or `timeframe`.
    Time,
    /// `EmResetFrame`, introduced by `rf` or `resetframe`.
    Reset,
}

/// `EmModelEntityType returns string`: the closed set of frame type keywords.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelEntityType {
    Rmo,
    Readmodel,
    Ui,
    Cmd,
    Command,
    Evt,
    Event,
    Pcr,
    Processor,
}

impl ModelEntityType {
    /// The keyword the grammar matched, as upstream's `modelEntityType` string holds it.
    pub fn as_str(self) -> &'static str {
        match self {
            ModelEntityType::Rmo => "rmo",
            ModelEntityType::Readmodel => "readmodel",
            ModelEntityType::Ui => "ui",
            ModelEntityType::Cmd => "cmd",
            ModelEntityType::Command => "command",
            ModelEntityType::Evt => "evt",
            ModelEntityType::Event => "event",
            ModelEntityType::Pcr => "pcr",
            ModelEntityType::Processor => "processor",
        }
    }
}

/// One `EmTimeFrame` or `EmResetFrame`.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub kind: FrameKind,
    /// `name`, the 1-to-3 digit frame id.
    pub name: String,
    pub model_entity_type: ModelEntityType,
    /// `entityIdentifier`, a `QualifiedName` such as `Product.PriceChanged`.
    pub entity_identifier: String,
    /// `sourceFrames`, the `$refText` of each `->>` reference, in source order.
    pub source_frames: Vec<String>,
    /// `dataReference`, the `$refText` of the `[[Name]]` reference.
    pub data_reference: Option<String>,
    /// `dataType` from the optional backtick group, e.g. `jsobj`.
    pub data_type: Option<String>,
    /// `dataInlineValue`, the raw `EM_DATA_INLINE` text (braces or quotes included).
    pub data_inline_value: Option<String>,
}

/// One `EmDataEntity` (`data Name { ... }`).
#[derive(Clone, Debug, PartialEq)]
pub struct DataEntity {
    pub name: String,
    /// `dataType` from the optional backtick group.
    pub data_type: Option<String>,
    /// `dataBlockValue`, the raw `EM_DATA_BLOCK` text (braces and line breaks included).
    pub data_block_value: String,
}

/// One `EmModelEntity` (`entity Name`).
#[derive(Clone, Debug, PartialEq)]
pub struct ModelEntity {
    /// `name`, a `QualifiedName`.
    pub name: String,
}

/// One `EmNoteEntity` (`note <frame> { ... }`).
#[derive(Clone, Debug, PartialEq)]
pub struct NoteEntity {
    /// `sourceFrame`, the `$refText` of the frame the note belongs to.
    pub source_frame: String,
    /// `dataType` from the optional backtick group.
    pub data_type: Option<String>,
    /// `dataBlockValue`, the raw `EM_DATA_BLOCK` text.
    pub data_block_value: String,
}

/// One `EmGwt` given/when/then block.
#[derive(Clone, Debug, PartialEq)]
pub struct Gwt {
    /// `sourceFrame`, the `$refText` of the frame the block belongs to.
    pub source_frame: String,
    pub given: Vec<GwtStatement>,
    /// Empty when the block has no `when` section.
    pub when: Vec<GwtStatement>,
    pub then: Vec<GwtStatement>,
}

/// One `EmGwtStatement` (`<type> <entity>`).
///
/// The rule calls `EmModelEntityType` without assigning it, so upstream's AST keeps only the
/// entity reference; the type keyword is parsed and discarded.
#[derive(Clone, Debug, PartialEq)]
pub struct GwtStatement {
    /// `entityIdentifier`, the `$refText` of the referenced model entity.
    pub entity_identifier: String,
}

/// What upstream's event modeling database holds after parsing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ast {
    /// `frames`, in source order and of both frame kinds.
    pub frames: Vec<Frame>,
    /// `dataEntities`, in source order.
    pub data_entities: Vec<DataEntity>,
    /// `modelEntities`, in source order.
    pub model_entities: Vec<ModelEntity>,
    /// `noteEntities`, in source order.
    pub note_entities: Vec<NoteEntity>,
    /// `gwtEntities`, in source order.
    pub gwt_entities: Vec<Gwt>,
    /// `getDiagramTitle()`; empty when the diagram has no title.
    pub diagram_title: String,
    /// `getAccTitle()`; empty when the diagram has no accessible title.
    pub acc_title: String,
    /// `getAccDescription()`; empty when the diagram has no accessible description.
    pub acc_description: String,
}
