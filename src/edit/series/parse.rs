//! Parser for quilt series files
use crate::edit::series::lex::{Lexer, SyntaxKind};
use crate::edit::series::lossless::SeriesFile;
use crate::edit::{PositionedParseError, PositionedParseWarning};
use rowan::{GreenNode, GreenNodeBuilder, TextSize};
use std::collections::HashSet;

/// Parse a quilt series file into a lossless AST
pub fn parse_series(text: &str) -> crate::edit::Parse<SeriesFile> {
    let mut lexer = Lexer::new(text);
    let tokens = lexer.tokenize();
    let parser = Parser::new(&tokens);

    let (green, errors, positioned_errors, warnings, positioned_warnings) = parser.parse();

    crate::edit::Parse::new_with_positioned_errors(
        green,
        errors,
        positioned_errors,
        warnings,
        positioned_warnings,
    )
}

struct Parser<'a> {
    tokens: &'a [(SyntaxKind, &'a str)],
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<String>,
    positioned_errors: Vec<PositionedParseError>,
    warnings: Vec<String>,
    positioned_warnings: Vec<PositionedParseWarning>,
    text_pos: TextSize,
    seen_patches: HashSet<String>,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [(SyntaxKind, &'a str)]) -> Self {
        Self {
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            positioned_errors: Vec::new(),
            warnings: Vec::new(),
            positioned_warnings: Vec::new(),
            text_pos: TextSize::from(0),
            seen_patches: HashSet::new(),
        }
    }

    fn parse(
        mut self,
    ) -> (
        GreenNode,
        Vec<String>,
        Vec<PositionedParseError>,
        Vec<String>,
        Vec<PositionedParseWarning>,
    ) {
        self.builder.start_node(SyntaxKind::ROOT.into());

        while !self.at_end() {
            // Skip empty lines
            if self.current_kind() == Some(SyntaxKind::NEWLINE) {
                self.consume();
                continue;
            }

            self.parse_entry();
        }

        self.builder.finish_node();
        (
            self.builder.finish(),
            self.errors,
            self.positioned_errors,
            self.warnings,
            self.positioned_warnings,
        )
    }

    fn parse_entry(&mut self) {
        self.builder.start_node(SyntaxKind::SERIES_ENTRY.into());

        // Skip leading whitespace
        while self.current_kind() == Some(SyntaxKind::SPACE)
            || self.current_kind() == Some(SyntaxKind::TAB)
        {
            self.consume();
        }

        // If we hit a newline after whitespace, it's just an empty line, not an error
        if self.current_kind() == Some(SyntaxKind::NEWLINE) {
            // Consume the newline (handled by parent)
        } else if self.current_kind() == Some(SyntaxKind::HASH) {
            self.parse_comment();
        } else if self.current_kind() == Some(SyntaxKind::PATCH_NAME) {
            self.parse_patch_entry();
        } else {
            self.error("Expected patch name or comment");
            // Skip to next line
            while self.current_kind() != Some(SyntaxKind::NEWLINE) && !self.at_end() {
                self.consume();
            }
        }

        self.builder.finish_node();
    }

    fn parse_comment(&mut self) {
        self.builder.start_node(SyntaxKind::COMMENT_LINE.into());

        // Consume #
        self.expect(SyntaxKind::HASH);

        // Consume whitespace if present
        while self.current_kind() == Some(SyntaxKind::SPACE)
            || self.current_kind() == Some(SyntaxKind::TAB)
        {
            self.consume();
        }

        // Consume comment text
        if self.current_kind() == Some(SyntaxKind::TEXT) {
            self.consume();
        }

        // Consume newline
        if self.current_kind() == Some(SyntaxKind::NEWLINE) {
            self.consume();
        }

        self.builder.finish_node();
    }

    fn parse_patch_entry(&mut self) {
        self.builder.start_node(SyntaxKind::PATCH_ENTRY.into());

        let patch_name = self
            .tokens
            .get(self.pos)
            .map(|(_, name)| *name)
            .unwrap_or("");

        let name = patch_name.to_string();
        if self.seen_patches.contains(&name) {
            self.warning(&format!("Duplicate patch: {}", name));
        } else {
            self.seen_patches.insert(name);
        }

        // Consume patch name
        self.expect(SyntaxKind::PATCH_NAME);

        // Parse options if present
        if self.has_options_ahead() {
            self.parse_options(patch_name);
        }

        // Check for unexpected patch name
        while self.current_kind() != Some(SyntaxKind::NEWLINE) && !self.at_end() {
            if let Some((SyntaxKind::PATCH_NAME, name)) = self.tokens.get(self.pos) {
                self.error(&format!("Unexpected patch name: '{}' ", name));
            }
            self.consume();
        }

        // Consume newline
        if self.current_kind() == Some(SyntaxKind::NEWLINE) {
            self.consume();
        }

        self.builder.finish_node();
    }

    fn parse_options(&mut self, patch_name: &str) {
        self.builder.start_node(SyntaxKind::OPTIONS.into());

        while self.current_kind() == Some(SyntaxKind::SPACE)
            || self.current_kind() == Some(SyntaxKind::TAB)
            || self.current_kind() == Some(SyntaxKind::OPTION)
        {
            if self.current_kind() == Some(SyntaxKind::OPTION) {
                self.builder.start_node(SyntaxKind::OPTION_ITEM.into());

                if let Some((_, option_name)) = self.tokens.get(self.pos) {
                    if !(option_name.starts_with("-p")
                        && option_name.len() > 2
                        && option_name[2..].chars().all(|c| c.is_ascii_digit()))
                    {
                        self.warning(&format!(
                            "Option '{}' is ignored by dpkg-source",
                            option_name
                        ));
                    } else {
                        let count = patch_name.split('/').count() - 1;
                        if let Some(strip_level) = option_name.strip_prefix("-p") {
                            if let Ok(level) = strip_level.parse::<u32>() {
                                if level > count as u32 {
                                    self.warning(&format!("Invalid strip level"));
                                }
                            }
                        }
                    }
                }

                self.consume();

                self.builder.finish_node();
            } else {
                self.consume();
            }
        }

        self.builder.finish_node();
    }

    fn has_options_ahead(&self) -> bool {
        let mut pos = self.pos;

        // Skip whitespace
        while pos < self.tokens.len()
            && (self.tokens[pos].0 == SyntaxKind::SPACE || self.tokens[pos].0 == SyntaxKind::TAB)
        {
            pos += 1;
        }

        // Check if we have an option
        pos < self.tokens.len() && self.tokens[pos].0 == SyntaxKind::OPTION
    }

    fn current_kind(&self) -> Option<SyntaxKind> {
        if self.pos < self.tokens.len() {
            Some(self.tokens[self.pos].0)
        } else {
            None
        }
    }

    fn consume(&mut self) {
        if let Some((kind, text)) = self.tokens.get(self.pos) {
            self.builder.token((*kind).into(), text);
            self.text_pos += TextSize::of(*text);
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: SyntaxKind) {
        if self.current_kind() == Some(expected) {
            self.consume();
        } else {
            self.error(&format!(
                "Expected {:?}, found {:?}",
                expected,
                self.current_kind()
            ));
            // Insert error token
            self.builder.token(SyntaxKind::ERROR.into(), "");
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.current_kind() == Some(SyntaxKind::EOF)
    }

    fn error(&mut self, message: &str) {
        self.errors.push(message.to_string());
        let start = self.text_pos;
        let len = self
            .tokens
            .get(self.pos)
            .map(|(_, text)| TextSize::of(*text))
            .unwrap_or(TextSize::from(0));
        self.positioned_errors.push(PositionedParseError {
            message: message.to_string(),
            position: rowan::TextRange::new(start, start + len),
        });
    }

    fn warning(&mut self, message: &str) {
        self.warnings.push(message.to_string());
        let start = self.text_pos;
        let len = self
            .tokens
            .get(self.pos)
            .map(|(_, text)| TextSize::of(*text))
            .unwrap_or(TextSize::from(0));
        self.positioned_warnings.push(PositionedParseWarning {
            message: message.to_string(),
            position: rowan::TextRange::new(start, start + len),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_patch() {
        let parse = parse_series("patch1.patch\n");
        assert!(parse.errors().is_empty());

        let series_file = parse.quilt_tree();
        let entries: Vec<_> = series_file.patch_entries().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), Some("patch1.patch".to_string()));
    }

    #[test]
    fn test_parse_patch_with_options() {
        let parse = parse_series("patch1.patch -p1 --reverse\n");
        assert!(parse.errors().is_empty());

        let series_file = parse.quilt_tree();
        let entries: Vec<_> = series_file.patch_entries().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), Some("patch1.patch".to_string()));
        assert_eq!(entries[0].option_strings(), vec!["-p1", "--reverse"]);
    }

    #[test]
    fn test_parse_comment() {
        let parse = parse_series("# This is a comment\n");
        assert!(parse.errors().is_empty());

        let series_file = parse.quilt_tree();
        let comments: Vec<_> = series_file.comment_lines().collect();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text(), "This is a comment");
    }

    #[test]
    fn test_parse_mixed() {
        let parse = parse_series("patch1.patch\n# A comment\npatch2.patch -p1\n");
        assert!(parse.errors().is_empty());

        let series_file = parse.quilt_tree();
        let patches: Vec<_> = series_file.patch_entries().collect();
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].name(), Some("patch1.patch".to_string()));
        assert_eq!(patches[1].name(), Some("patch2.patch".to_string()));

        let comments: Vec<_> = series_file.comment_lines().collect();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text(), "A comment");
    }

    #[test]
    fn test_thread_safety() {
        let parse = parse_series("patch1.patch\n");
        let green = parse.green();

        // Should be able to clone the green node (Arc internally)
        let _green_clone = green.clone();

        // Note: Parse<SeriesFile> is not currently Send+Sync due to rowan implementation
        // but the green node itself can be cloned and shared
    }

    #[test]
    fn test_duplicate_patch_warning() {
        let parse = parse_series("patch1.patch\npatch1.patch\n");
        assert!(parse.errors().is_empty());
        assert_eq!(parse.warnings().len(), 1);
        assert!(parse.warnings()[0].contains("Duplicate patch"));
    }

    #[test]
    fn test_multiple_duplicates() {
        let parse = parse_series("patch1.patch\npatch2.patch\npatch1.patch\npatch2.patch\n");
        assert!(parse.errors().is_empty());
        assert_eq!(parse.warnings().len(), 2);
    }

    #[test]
    fn test_unexpected_patch() {
        let parse = parse_series("patch1.patch patch2.patch\n");
        assert_eq!(parse.errors().len(), 1);
        assert!(parse.errors()[0].contains("Unexpected patch name"));
    }

    #[test]
    fn test_multiple_unexpected_patch() {
        let parse = parse_series("patch1.patch patch2.patch patch3.patch patch4.patch\n");
        assert_eq!(parse.errors().len(), 3);
        assert!(parse.errors()[0].contains("Unexpected patch name"));
    }

    #[test]
    fn test_invalid_option() {
        let parse = parse_series("patch1.patch -aa\n");
        assert_eq!(parse.warnings().len(), 1);
        assert!(parse.warnings()[0].contains("ignored by dpkg-source"));
    }

    #[test]
    fn test_multiple_invalid_option() {
        let parse = parse_series("patch1.patch -aa\npatch2.patch -bb\n");
        assert_eq!(parse.warnings().len(), 2);
        assert!(parse.warnings()[1].contains("ignored by dpkg-source"));
    }

    #[test]
    fn test_invalid_strip_level() {
        let parse = parse_series("patch1.patch -p1\npatch/patch2.patch -p2\n");
        assert_eq!(parse.warnings().len(), 2);
        assert!(parse.warnings()[0].contains("Invalid strip level"));
    }
}
