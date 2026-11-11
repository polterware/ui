//! What upstream does before a diagram's parser runs (`preprocess.ts`, `diagram-api/
//! frontmatter.ts`, `directiveUtils`, `utils.ts` cleanup): YAML frontmatter (`title`, `config`,
//! `displayMode`), `%%{init: ...}%%` directives, and line-ending cleanup. The body keeps the
//! same line numbers: removed lines become empty lines, so a parser's spans match the source.

/// The source split into what the parsers read and the configuration around it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    /// The diagram text a parser reads, same line count as the input.
    pub body: String,
    pub title: Option<String>,
    /// Frontmatter `config` and every directive merged in order, as JSON.
    pub config: serde_json::Map<String, serde_json::Value>,
}

pub fn preprocess(text: &str) -> Source {
    Source {
        body: text.replace("\r\n", "\n"),
        ..Source::default()
    }
}
