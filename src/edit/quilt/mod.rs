//! Lossless editor for quilt series files

mod editor;
/// Lexer for quilt series files
pub mod lex;
/// Lossless AST structures for quilt series files  
pub mod lossless;
mod parse;

pub use lossless::{
    CommentLine, OptionItem, Options, PatchEntry, QuiltLang, SeriesEntry, SeriesFile,
};

use rowan::{ast::AstNode, TextRange};

/// Parse a quilt series file into a lossless AST
pub fn parse(text: &str) -> crate::parse::Parse<SeriesFile> {
    lossless::parse(text)
}

/// Extension methods for quilt Parse results
impl crate::parse::Parse<SeriesFile> {
    /// Get the parsed quilt series tree
    pub fn quilt_tree(&self) -> SeriesFile {
        let green = self.green().clone();
        SeriesFile::new_root(green)
    }

    /// Get a mutable quilt series tree for editing
    pub fn quilt_tree_mut(&self) -> SeriesFile {
        let green = self.green().clone();
        SeriesFile::new_root_mut(green)
    }

    /// Get a mutable root for the quilt tree
    pub fn quilt_root_mut(&self) -> rowan::SyntaxNode<QuiltLang> {
        let green = self.green().clone();
        rowan::SyntaxNode::new_root_mut(green)
    }
}

/// Builder for creating quilt series files programmatically
pub struct SeriesBuilder {
    entries: Vec<SeriesBuilderEntry>,
}

enum SeriesBuilderEntry {
    Patch { name: String, options: Vec<String> },
    Comment(String),
}

impl SeriesBuilder {
    /// Create a new series builder
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a patch entry
    pub fn add_patch(mut self, name: impl Into<String>, options: Vec<String>) -> Self {
        self.entries.push(SeriesBuilderEntry::Patch {
            name: name.into(),
            options,
        });
        self
    }

    /// Add a comment
    pub fn add_comment(mut self, text: impl Into<String>) -> Self {
        self.entries.push(SeriesBuilderEntry::Comment(text.into()));
        self
    }

    /// Build the series file
    pub fn build(self) -> SeriesFile {
        let mut text = String::new();

        for entry in &self.entries {
            match entry {
                SeriesBuilderEntry::Patch { name, options } => {
                    text.push_str(name);
                    for opt in options {
                        text.push(' ');
                        text.push_str(opt);
                    }
                    text.push('\n');
                }
                SeriesBuilderEntry::Comment(comment) => {
                    text.push_str("# ");
                    text.push_str(comment);
                    text.push('\n');
                }
            }
        }

        let parsed = parse(&text);
        parsed.quilt_tree_mut()
    }
}

impl Default for SeriesBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Find a patch entry by name
pub fn find_patch_by_name<'a>(series: &'a SeriesFile, name: &str) -> Option<PatchEntry> {
    series
        .patch_entries()
        .find(|entry| entry.name().as_deref() == Some(name))
}

/// Get the line range for a specific patch entry
pub fn get_patch_line_range(patch: &PatchEntry) -> TextRange {
    patch.syntax().text_range()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod comprehensive_tests;

#[cfg(test)]
mod basic_tests {
    use super::*;

    #[test]
    fn test_builder() {
        let series = SeriesBuilder::new()
            .add_patch("0001-first.patch", vec![])
            .add_comment("Second patch with options")
            .add_patch(
                "0002-second.patch",
                vec!["-p1".to_string(), "--reverse".to_string()],
            )
            .build();

        let patches: Vec<_> = series.patch_entries().collect();
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].name(), Some("0001-first.patch".to_string()));
        assert_eq!(patches[1].name(), Some("0002-second.patch".to_string()));
        assert_eq!(patches[1].option_strings(), vec!["-p1", "--reverse"]);
    }

    #[test]
    fn test_roundtrip() {
        let text = "0001-first.patch\n# Comment\n0002-second.patch -p1\n";
        let parsed = parse(text);
        let series = parsed.quilt_tree();
        assert_eq!(series.syntax().to_string(), text);
    }

    #[test]
    fn test_find_patch() {
        let text = "0001-first.patch\n0002-second.patch\n";
        let parsed = parse(text);
        let series = parsed.quilt_tree();

        assert!(find_patch_by_name(&series, "0001-first.patch").is_some());
        assert!(find_patch_by_name(&series, "0002-second.patch").is_some());
        assert!(find_patch_by_name(&series, "nonexistent.patch").is_none());
    }
}
