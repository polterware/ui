//! The error diagram: what Mermaid draws in place of a diagram whose source failed to parse.
//!
//! Upstream has no syntax to parse here. `diagram-api/diagram-orchestration.ts` registers the
//! pseudo diagram `error`, and `mermaidAPI.render` falls back to it when `Diagram.fromText`
//! throws, handing the original failure's message to its database
//! (`error/errorDiagram.ts`). Its renderer (`error/errorRenderer.ts`) then draws a bomb icon,
//! a headline, the Mermaid version, and the wrapped message. [`render`] ports that renderer;
//! the message is already known by the time a caller has a [`RenderError`](crate::RenderError),
//! so this module takes it directly instead of keeping a database.
//!
//! [`render`]: render::render

pub mod render;
