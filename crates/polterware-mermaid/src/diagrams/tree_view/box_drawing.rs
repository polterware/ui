//! `treeView/boxDrawingPreprocessor.ts`: the box-drawing to indentation conversion the
//! treeView parser runs before the Langium grammar sees the text.
//!
//! A body that uses `├──`/`└──`/`│` (or the heavy `┣━━`/`┗━━`/`┃` variants) is rewritten
//! into the indentation-based format, four spaces per depth level. Anything that is already
//! indentation-based, or that has no `treeView-beta` line, is passed through unchanged.
//!
//! Upstream keeps a `Map<number, number>` from output line to original line so that an error
//! from the grammar can be rewritten with `remapErrorLines`. This crate's errors carry a
//! [`Span`], so [`Preprocessed::remap`] moves the span back to the original line and column
//! instead of rewriting the message text.

use crate::error::{ParseError, Span};

/// The header line the format is detected from.
const KEYWORD: &str = super::lexer::KEYWORD;

/// The indentation written for each depth level (`INDENT_UNIT`).
const INDENT_UNIT: &str = "    ";

/// The possibly transformed input, with what is needed to point errors back at it.
pub(crate) struct Preprocessed {
    pub(crate) text: String,
    /// One entry per output line, in order; empty when the input was not transformed.
    origins: Vec<LineOrigin>,
}

/// Where an output line came from: the original line and how its columns shifted.
struct LineOrigin {
    original_line: u32,
    column_delta: i64,
}

impl Preprocessed {
    fn unchanged(input: &str) -> Self {
        Self {
            text: input.to_string(),
            origins: Vec::new(),
        }
    }

    /// The value `lineMap.get(output_line)` has upstream, for tests.
    #[cfg(test)]
    pub(crate) fn original_line(&self, output_line: u32) -> Option<u32> {
        self.origins
            .get(output_line.checked_sub(1)? as usize)
            .map(|origin| origin.original_line)
    }

    /// Points an error raised while parsing [`Self::text`] back at `original`.
    ///
    /// Only the line is exact; the column is shifted by the indentation the conversion
    /// added, and the byte range is the empty range at the resulting position.
    pub(crate) fn remap(&self, error: ParseError, original: &str) -> ParseError {
        let Some(span) = error.span else {
            return error;
        };
        if self.origins.is_empty() {
            return error;
        }
        let Some(origin) = self.origins.get(span.line.saturating_sub(1) as usize) else {
            return error;
        };
        let column = (i64::from(span.column) + origin.column_delta).max(1);
        let column = u32::try_from(column).unwrap_or(u32::MAX);
        let position = span_at(original, origin.original_line as usize, column as usize);
        ParseError::new(
            error.message,
            Some(Span {
                line: origin.original_line,
                column,
                start: position.start,
                end: position.start,
            }),
        )
    }
}

/// `isBoxDrawingFormat`: whether any of the lines contains a box-drawing character.
pub(crate) fn is_box_drawing_format(lines: &[&str]) -> bool {
    lines.iter().any(|line| line.chars().any(is_box_char))
}

/// `preprocessBoxDrawing`: the transformed text and its line mapping.
pub(crate) fn preprocess(input: &str) -> Result<Preprocessed, ParseError> {
    let lines: Vec<&str> = input.split('\n').collect();
    let Some(keyword_idx) = lines
        .iter()
        .position(|line| line.trim_matches(is_js_space) == KEYWORD)
    else {
        return Ok(Preprocessed::unchanged(input));
    };

    // Content lines the format is detected from: skip blanks, comments, metadata, and
    // decoration, and expand tabs so column positions are consistent.
    let content_lines: Vec<String> = lines[keyword_idx + 1..]
        .iter()
        .filter(|line| {
            let trimmed = line.trim_matches(is_js_space);
            !trimmed.is_empty()
                && !is_comment_line(line)
                && !is_metadata_line(line)
                && !is_decoration_only(line)
        })
        .map(|line| line.replace('\t', INDENT_UNIT))
        .collect();
    if !is_box_drawing_format(&content_lines.iter().map(String::as_str).collect::<Vec<_>>()) {
        return Ok(Preprocessed::unchanged(input));
    }
    let segment_width = infer_segment_width(&content_lines);

    let mut output = Vec::with_capacity(lines.len());
    let mut origins = Vec::with_capacity(lines.len());

    for (index, line) in lines[..=keyword_idx].iter().enumerate() {
        output.push((*line).to_string());
        origins.push(LineOrigin {
            original_line: index as u32 + 1,
            column_delta: 0,
        });
    }

    for (index, line) in lines[keyword_idx + 1..].iter().enumerate() {
        let original_line = (keyword_idx + index + 2) as u32;
        let trimmed = line.trim_matches(is_js_space);
        if trimmed.is_empty() || is_comment_line(line) || is_metadata_line(line) {
            output.push((*line).to_string());
            origins.push(LineOrigin {
                original_line,
                column_delta: 0,
            });
            continue;
        }
        // Decoration-only lines (`│` and whitespace) carry no node and are dropped.
        if is_decoration_only(line) {
            continue;
        }

        let normalized: Vec<char> = line.replace('\t', INDENT_UNIT).chars().collect();
        let branch = normalized.iter().position(|&ch| is_branch_char(ch));
        let Some(branch) = branch else {
            if normalized
                .iter()
                .all(|&ch| is_js_space(ch) || is_dash_char(ch) || is_pipe(ch))
            {
                // A whole line of box-drawing decoration: it carries no node.
                continue;
            }
            if normalized.iter().any(|&ch| is_box_char(ch)) {
                // A box character inside the name, not a prefix: a root-level node.
                output.push((*line).to_string());
                origins.push(LineOrigin {
                    original_line,
                    column_delta: 0,
                });
                continue;
            }
            if normalized.first().is_some_and(|&ch| is_js_space(ch)) {
                let leading = normalized.iter().take_while(|&&ch| is_js_space(ch)).count();
                return Err(ParseError::at(
                    format!(
                        "Line {original_line}: Unexpected indentation without box-drawing characters. \
                         In box-drawing format, use ├── or └── prefixes for indented nodes."
                    ),
                    span_at(input, original_line as usize, leading + 1),
                ));
            }
            output.push((*line).to_string());
            origins.push(LineOrigin {
                original_line,
                column_delta: 0,
            });
            continue;
        };

        let depth = ((branch as f64 / segment_width as f64).round() as usize) + 1;
        let mut pos = branch + 1;
        while pos < normalized.len() && is_dash_char(normalized[pos]) {
            pos += 1;
        }
        while pos < normalized.len() && normalized[pos] == ' ' {
            pos += 1;
        }
        let content: String = normalized[pos..].iter().collect();
        let content = content.trim_end_matches(is_js_space);
        if content.is_empty() {
            return Err(ParseError::at(
                format!(
                    "Line {original_line}: Empty node — expected a filename or directory name \
                     after the box-drawing prefix"
                ),
                span_at(input, original_line as usize, branch + 1),
            ));
        }
        output.push(format!("{}{content}", INDENT_UNIT.repeat(depth)));
        origins.push(LineOrigin {
            original_line,
            column_delta: pos as i64 - (INDENT_UNIT.len() * depth) as i64,
        });
    }

    Ok(Preprocessed {
        text: output.join("\n"),
        origins,
    })
}

/// `inferSegmentWidth`: the column of the first branch character past column 0, else 4.
fn infer_segment_width(content_lines: &[String]) -> usize {
    for line in content_lines {
        if let Some(index) = line.chars().position(is_branch_char)
            && index > 0
        {
            return index;
        }
    }
    4
}

/// `COMMENT_LINE: /^\s*%%/`.
fn is_comment_line(line: &str) -> bool {
    line.trim_start_matches(is_js_space).starts_with("%%")
}

/// `METADATA_LINE: /^\s*(title[\t ]|accTitle[\t ]*:|accDescr[\t ]*[:{])/`.
fn is_metadata_line(line: &str) -> bool {
    let rest = line.trim_start_matches(is_js_space);
    if let Some(after) = rest.strip_prefix("title") {
        return after.starts_with([' ', '\t']);
    }
    for key in ["accTitle", "accDescr"] {
        if let Some(after) = rest.strip_prefix(key) {
            let after = after.trim_start_matches([' ', '\t']);
            return if key == "accDescr" {
                after.starts_with(':') || after.starts_with('{')
            } else {
                after.starts_with(':')
            };
        }
    }
    false
}

/// `DECORATION_ONLY: /^[\s│┃]+$/`.
fn is_decoration_only(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|ch| is_js_space(ch) || is_pipe(ch))
}

fn is_box_char(ch: char) -> bool {
    is_branch_char(ch) || is_dash_char(ch) || is_pipe(ch)
}

/// `BRANCH_CHAR: /[└┗├┣]/`.
fn is_branch_char(ch: char) -> bool {
    matches!(ch, '└' | '┗' | '├' | '┣')
}

/// `DASH_CHAR: /[─━]/`.
fn is_dash_char(ch: char) -> bool {
    matches!(ch, '─' | '━')
}

/// The vertical lines `│` and `┃`.
fn is_pipe(ch: char) -> bool {
    matches!(ch, '│' | '┃')
}

fn is_js_space(ch: char) -> bool {
    crate::preprocess::is_js_whitespace(ch)
}

/// The span of a 1-based line and column of `text`.
fn span_at(text: &str, line: usize, column: usize) -> Span {
    let column = column.max(1);
    let mut line_start = 0;
    for (index, text_line) in text.split('\n').enumerate() {
        if index + 1 == line {
            let offset = text_line
                .char_indices()
                .nth(column - 1)
                .map_or(text_line.len(), |(offset, _)| offset);
            return Span {
                line: line as u32,
                column: column as u32,
                start: line_start + offset,
                end: line_start + offset,
            };
        }
        line_start += text_line.len() + 1;
    }
    Span {
        line: line as u32,
        column: column as u32,
        start: text.len(),
        end: text.len(),
    }
}
