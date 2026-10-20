//! A Markdown document: a frontmatter block and a body.
//!
//! A document says what it is in YAML between `---` lines at its top, and
//! that block is all this reads of a document's structure. The body
//! is rendered to HTML for the page. Raw HTML in it is passed through, since a
//! README that centres its logo in a `<div>` should look the way its author
//! meant; a published repository is somebody else's writing, so the page runs
//! everything this renders through a sanitiser before it reaches the DOM.

use pulldown_cmark::{Options, Parser, html};

/// A document's metadata and its body.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The frontmatter, or `None` where there is no block or it does not
    /// read.
    pub meta: Option<fig::Value>,
    /// Everything after the block.
    pub body: String,
}

impl Document {
    /// Split `text` at its frontmatter.
    ///
    /// A block that opens and never closes, or does not parse, leaves the
    /// whole text as the body: a stray `---` is not worth refusing a file.
    pub fn read(text: &str) -> Self {
        let whole = || Self {
            meta: None,
            body: text.to_owned(),
        };
        let Some(rest) = text
            .strip_prefix("---\n")
            .or_else(|| text.strip_prefix("---\r\n"))
        else {
            return whole();
        };
        let mut at = 0;
        for line in rest.split_inclusive('\n') {
            let bare = line.trim_end();
            if bare == "---" || bare == "..." {
                let block = &rest[..at];
                let body = &rest[at + line.len()..];
                let Some(meta) = parse(block) else {
                    return whole();
                };
                return Self {
                    meta: Some(meta),
                    body: body.to_owned(),
                };
            }
            at += line.len();
        }
        whole()
    }

    /// One field, as text.
    ///
    /// A scalar of any kind reads as the text it was written as, so a
    /// timestamp YAML recognises comes back as the timestamp it spelled.
    pub fn field(&self, key: &str) -> Option<String> {
        scalar(self.meta.as_ref()?.get(key)?)
    }

    /// The body, as HTML.
    pub fn html(&self) -> String {
        render(&self.body)
    }
}

fn parse(block: &str) -> Option<fig::Value> {
    let format = fig_yaml::format().ok()?;
    let document = fig::Document::parse(block.as_bytes(), format).ok()?;
    document.to_value().ok()
}

fn scalar(value: &fig::Value) -> Option<String> {
    Some(match value {
        fig::Value::Str(text) => text.clone(),
        fig::Value::Bool(value) => value.to_string(),
        fig::Value::Int(value) => value.to_string(),
        fig::Value::Uint(value) => value.to_string(),
        fig::Value::Float(value) => value.to_string(),
        fig::Value::Extended { text, .. } => text.clone(),
        fig::Value::Null | fig::Value::Seq(_) | fig::Value::Map(_) => return None,
    })
}

/// Markdown as HTML, unsanitised: see the module documentation.
pub fn render(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(markdown, options));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_splits_from_the_body() {
        let document = Document::read("---\ntitle: A task\nstatus: open\n---\n# A task\n\nBody.\n");
        assert_eq!(document.field("title").as_deref(), Some("A task"));
        assert_eq!(document.field("status").as_deref(), Some("open"));
        assert_eq!(document.field("author"), None);
        assert_eq!(document.body, "# A task\n\nBody.\n");
    }

    #[test]
    fn a_document_with_no_block_is_all_body() {
        let document = Document::read("# Just prose\n");
        assert!(document.meta.is_none());
        assert_eq!(document.body, "# Just prose\n");
        let unclosed = Document::read("---\ntitle: never closed\n");
        assert!(unclosed.meta.is_none());
    }

    #[test]
    fn a_timestamp_reads_as_written() {
        let document = Document::read("---\ncreated: 2026-10-14T16:47:13.324124Z\n---\n");
        assert_eq!(
            document.field("created").as_deref(),
            Some("2026-10-14T16:47:13.324124Z")
        );
    }

    #[test]
    fn markdown_renders_with_tables_and_task_lists() {
        let html = render("| a |\n|---|\n| b |\n\n- [x] done\n");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("checkbox"), "{html}");
    }
}
