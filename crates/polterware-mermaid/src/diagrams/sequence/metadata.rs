//! Participant metadata (`participant Alice@{ "type": "database", "alias": "Public API" }`).
//!
//! Upstream feeds the text between `@{` and `}` to `js-yaml` with `JSON_SCHEMA` (wrapping
//! it in braces when it has no newline), and reads `type` and `alias` back. The YAML reader
//! is [`crate::common::yaml`], the same subset the flowchart metadata and the frontmatter
//! use: a flow mapping (possibly spread over lines) or a block mapping. Anything else is an
//! error, as `js-yaml` throws for malformed metadata.

use serde_json::Value;

use crate::common::yaml;

/// The fields `addActor` reads from participant metadata.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Metadata {
    pub type_: Option<String>,
    pub alias: Option<String>,
}

pub(crate) fn parse(raw: &str) -> Result<Metadata, String> {
    let trimmed = raw.trim();
    let value = if trimmed.starts_with('{') || trimmed.starts_with('[') {
        yaml::parse_flow(trimmed)?
    } else if raw.contains('\n') {
        yaml::parse_document(raw)?
    } else {
        yaml::parse_flow_mapping(raw)?
    };
    let Value::Object(map) = value else {
        return Err("participant metadata is not a mapping".into());
    };
    Ok(Metadata {
        type_: map.get("type").and_then(Value::as_str).map(str::to_string),
        alias: map.get("alias").and_then(Value::as_str).map(str::to_string),
    })
}
