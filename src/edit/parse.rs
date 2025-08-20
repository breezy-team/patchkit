use super::lex::SyntaxKind;
use super::lossless::{Patch, PositionedParseError};
use rowan::{GreenNodeBuilder, TextSize};

pub(crate) struct Parser<'a> {
    tokens: Vec<(SyntaxKind, &'a str)>,
    cursor: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<String>,
    positioned_errors: Vec<PositionedParseError>,
    text_position: TextSize,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: impl Iterator<Item = (SyntaxKind, &'a str)>) -> Self {
        let tokens: Vec<_> = tokens.collect();
        Self {
            tokens,
            cursor: 0,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            positioned_errors: Vec::new(),
            text_position: TextSize::from(0),
        }
    }

    pub fn parse(mut self) -> crate::parse::Parse<Patch> {
        self.builder.start_node(SyntaxKind::ROOT.into());
        self.parse_patch();
        self.builder.finish_node();

        let green = self.builder.finish();
        crate::parse::Parse::new_with_positioned_errors(green, self.errors, self.positioned_errors)
    }

    fn parse_patch(&mut self) {
        while !self.at_end() {
            // Try to detect format and parse accordingly
            if self.at(SyntaxKind::STAR) && self.peek_text(0) == Some("***") {
                // Could be context diff file header or context hunk without file header
                if self.looks_like_context_hunk_range() {
                    // It's a context hunk without file headers - create a minimal context diff file
                    self.builder
                        .start_node(SyntaxKind::CONTEXT_DIFF_FILE.into());
                    self.parse_context_hunk_without_separator();
                    self.builder.finish_node();
                } else {
                    // Context diff file with headers
                    self.parse_context_diff_file();
                }
            } else if self.at(SyntaxKind::MINUS)
                && self.peek_text(0) == Some("---")
                && !self
                    .peek_text(3)
                    .map(|t| t.starts_with('>'))
                    .unwrap_or(false)
                && !self.looks_like_context_new_section()
            {
                // Unified diff
                self.parse_patch_file();
            } else if self.at(SyntaxKind::PLUS) && self.peek_text(0) == Some("+++") {
                // Orphan new file header (unified)
                self.parse_patch_file();
            } else if self.looks_like_normal_diff() {
                // Normal diff
                self.parse_normal_hunk();
            } else if self.looks_like_ed_command() {
                // Ed diff
                self.parse_ed_command();
            } else {
                // Skip unknown content - advance to next line
                self.skip_to_next_line();
            }
        }
    }

    fn parse_patch_file(&mut self) {
        self.builder.start_node(SyntaxKind::PATCH_FILE.into());

        // Parse old file header
        if self.at(SyntaxKind::MINUS) && self.peek_text(0) == Some("---") {
            self.parse_old_file();
        }

        // Parse new file header
        if self.at(SyntaxKind::PLUS) && self.peek_text(0) == Some("+++") {
            self.parse_new_file();
        }

        // Parse hunks
        while self.at(SyntaxKind::AT) && self.peek_text(0) == Some("@@") {
            self.parse_hunk();
        }

        self.builder.finish_node();
    }

    fn parse_old_file(&mut self) {
        self.builder.start_node(SyntaxKind::OLD_FILE.into());

        // Consume "---"
        self.advance(); // -
        self.advance(); // -
        self.advance(); // -

        // Skip whitespace
        self.skip_whitespace();

        // Parse path - collect all tokens that make up the path
        let mut path_parts = Vec::new();
        let mut collecting_path = true;
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() && collecting_path {
            match self.current_kind() {
                Some(SyntaxKind::TEXT)
                | Some(SyntaxKind::SLASH)
                | Some(SyntaxKind::DOT)
                | Some(SyntaxKind::NUMBER)
                | Some(SyntaxKind::COLON)
                | Some(SyntaxKind::BACKSLASH) => {
                    if let Some(text) = self.current_text() {
                        path_parts.push(text.to_string());
                    }
                    self.advance_without_emit();
                }
                Some(SyntaxKind::WHITESPACE) if !path_parts.is_empty() => {
                    // Stop at whitespace after we've collected some path parts (timestamp follows)
                    collecting_path = false;
                }
                _ => {
                    collecting_path = false;
                }
            }
        }

        if !path_parts.is_empty() {
            let path = path_parts.join("");
            self.builder.token(SyntaxKind::PATH.into(), &path);
        }

        // Skip to end of line
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }

        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }

        self.builder.finish_node();
    }

    fn parse_new_file(&mut self) {
        self.builder.start_node(SyntaxKind::NEW_FILE.into());

        // Consume "+++"
        self.advance(); // +
        self.advance(); // +
        self.advance(); // +

        // Skip whitespace
        self.skip_whitespace();

        // Parse path - collect all tokens that make up the path
        let mut path_parts = Vec::new();
        let mut collecting_path = true;
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() && collecting_path {
            match self.current_kind() {
                Some(SyntaxKind::TEXT)
                | Some(SyntaxKind::SLASH)
                | Some(SyntaxKind::DOT)
                | Some(SyntaxKind::NUMBER)
                | Some(SyntaxKind::COLON)
                | Some(SyntaxKind::BACKSLASH) => {
                    if let Some(text) = self.current_text() {
                        path_parts.push(text.to_string());
                    }
                    self.advance_without_emit();
                }
                Some(SyntaxKind::WHITESPACE) if !path_parts.is_empty() => {
                    // Stop at whitespace after we've collected some path parts (timestamp follows)
                    collecting_path = false;
                }
                _ => {
                    collecting_path = false;
                }
            }
        }

        if !path_parts.is_empty() {
            let path = path_parts.join("");
            self.builder.token(SyntaxKind::PATH.into(), &path);
        }

        // Skip to end of line
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }

        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }

        self.builder.finish_node();
    }

    fn parse_hunk(&mut self) {
        // Check if this looks like a valid hunk header before committing
        let checkpoint = self.builder.checkpoint();
        let _start_cursor = self.cursor;

        // Peek ahead to see if this is a valid hunk header
        let mut temp_cursor = self.cursor;

        // Skip @@
        temp_cursor += 2;

        // Skip whitespace
        while temp_cursor < self.tokens.len()
            && self
                .tokens
                .get(temp_cursor)
                .map(|(k, _)| *k == SyntaxKind::WHITESPACE)
                .unwrap_or(false)
        {
            temp_cursor += 1;
        }

        // Check if we have at least one valid range (-N or +N)
        let has_valid_range = temp_cursor < self.tokens.len() && {
            let (kind, _) = self.tokens.get(temp_cursor).unwrap();
            (*kind == SyntaxKind::MINUS || *kind == SyntaxKind::PLUS)
                && temp_cursor + 1 < self.tokens.len()
                && self
                    .tokens
                    .get(temp_cursor + 1)
                    .map(|(k, _)| *k == SyntaxKind::NUMBER)
                    .unwrap_or(false)
        };

        if !has_valid_range {
            // Invalid hunk header - skip the @@ line but continue parsing
            self.skip_to_next_line();

            // Continue parsing lines that might belong to this invalid hunk
            // until we find another hunk or file boundary
            while !self.at_end() && !self.is_hunk_end() {
                self.skip_to_next_line();
            }
            return;
        }

        self.builder
            .start_node_at(checkpoint, SyntaxKind::HUNK.into());

        // Parse hunk header
        self.parse_hunk_header();

        // Parse hunk lines
        while !self.at_end() && !self.is_hunk_end() {
            self.parse_hunk_line();
        }

        self.builder.finish_node();
    }

    fn parse_hunk_header(&mut self) {
        self.builder.start_node(SyntaxKind::HUNK_HEADER.into());

        // Consume "@@"
        self.advance(); // @
        self.advance(); // @

        self.skip_whitespace();

        // Parse old range
        if self.at(SyntaxKind::MINUS) {
            self.parse_hunk_range();
        }

        self.skip_whitespace();

        // Parse new range
        if self.at(SyntaxKind::PLUS) {
            self.parse_hunk_range();
        }

        self.skip_whitespace();

        // Consume closing "@@"
        if self.at(SyntaxKind::AT) {
            self.advance(); // @
            self.advance(); // @
        }

        // Skip to end of line
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }

        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }

        self.builder.finish_node();
    }

    fn parse_hunk_range(&mut self) {
        self.builder.start_node(SyntaxKind::HUNK_RANGE.into());

        // Consume +/- sign
        self.advance();

        // Parse start line number
        if self.at(SyntaxKind::NUMBER) {
            self.advance();
        }

        // Parse optional count
        if self.at(SyntaxKind::COMMA) {
            self.advance();
            if self.at(SyntaxKind::NUMBER) {
                self.advance();
            }
        }

        self.builder.finish_node();
    }

    fn parse_hunk_line(&mut self) {
        let checkpoint = self.builder.checkpoint();

        match self.current_kind() {
            Some(SyntaxKind::SPACE) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::CONTEXT_LINE.into());
                self.advance(); // space
            }
            Some(SyntaxKind::PLUS) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ADD_LINE.into());
                self.advance(); // +
            }
            Some(SyntaxKind::MINUS) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::DELETE_LINE.into());
                self.advance(); // -
            }
            _ => {
                // Unknown line type, treat as context
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::CONTEXT_LINE.into());
            }
        }

        // Parse the line content
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }

        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }

        self.builder.finish_node();
    }

    fn is_hunk_end(&self) -> bool {
        // Check if we're at the start of a new hunk or file
        (self.at(SyntaxKind::AT) && self.peek_text(0) == Some("@@"))
            || (self.at(SyntaxKind::MINUS) && self.peek_text(0) == Some("---"))
            || (self.at(SyntaxKind::PLUS) && self.peek_text(0) == Some("+++"))
    }

    fn current_kind(&self) -> Option<SyntaxKind> {
        self.tokens.get(self.cursor).map(|(kind, _)| *kind)
    }

    fn current_text(&self) -> Option<&str> {
        self.tokens.get(self.cursor).map(|(_, text)| *text)
    }

    fn peek_text(&self, offset: usize) -> Option<&str> {
        let start = self.cursor + offset;

        // For header detection, we need to look at multiple tokens
        let mut text = String::new();
        for i in 0..3 {
            if let Some((_, t)) = self.tokens.get(start + i) {
                text.push_str(t);

                // Check if we've found a header pattern
                if text.len() >= 2 {
                    if text.starts_with("---") {
                        return Some("---");
                    } else if text.starts_with("+++") {
                        return Some("+++");
                    } else if text.starts_with("@@") {
                        return Some("@@");
                    }
                }

                // Check for three-character patterns
                if text.len() >= 3 {
                    if text.starts_with("***") {
                        return Some("***");
                    }
                }
            } else {
                break;
            }
        }

        // If no pattern found, return the single token at offset
        self.tokens.get(start + offset).map(|(_, text)| *text)
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.current_kind() == Some(kind)
    }

    fn at_end(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    fn advance(&mut self) {
        if let Some((kind, text)) = self.tokens.get(self.cursor) {
            self.builder.token((*kind).into(), text);
            self.text_position += TextSize::from(text.len() as u32);
            self.cursor += 1;
        }
    }

    fn advance_without_emit(&mut self) {
        if let Some((_, text)) = self.tokens.get(self.cursor) {
            self.text_position += TextSize::from(text.len() as u32);
            self.cursor += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while self.at(SyntaxKind::WHITESPACE) {
            self.advance();
        }
    }

    fn skip_to_next_line(&mut self) {
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }
        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }
    }

    // Helper methods for format detection
    fn looks_like_ed_command(&self) -> bool {
        // Ed commands: 5a, 3,7d, 2c
        let mut offset = 0;

        // Must start with a number
        if !matches!(self.peek_kind(offset), Some(SyntaxKind::NUMBER)) {
            return false;
        }

        // Skip numbers and commas
        while matches!(
            self.peek_kind(offset),
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::COMMA)
        ) {
            offset += 1;
        }

        // Must be followed by a, c, or d
        matches!(
            self.peek_kind(offset),
            Some(SyntaxKind::LETTER_A) | Some(SyntaxKind::LETTER_C) | Some(SyntaxKind::LETTER_D)
        )
    }

    fn looks_like_normal_diff(&self) -> bool {
        // Normal diff: 2c2 or 5,7d10 or 3a4,6
        let mut offset = 0;

        // Must start with a number
        if !matches!(self.peek_kind(offset), Some(SyntaxKind::NUMBER)) {
            return false;
        }

        // Skip first range
        while matches!(
            self.peek_kind(offset),
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::COMMA)
        ) {
            offset += 1;
        }

        // Must have a, c, or d
        if !matches!(
            self.peek_kind(offset),
            Some(SyntaxKind::LETTER_A) | Some(SyntaxKind::LETTER_C) | Some(SyntaxKind::LETTER_D)
        ) {
            return false;
        }
        offset += 1;

        // Must be followed by another number
        matches!(self.peek_kind(offset), Some(SyntaxKind::NUMBER))
    }

    fn peek_kind(&self, offset: usize) -> Option<SyntaxKind> {
        self.tokens.get(self.cursor + offset).map(|(kind, _)| *kind)
    }

    // Context diff parsing
    fn parse_context_diff_file(&mut self) {
        self.builder
            .start_node(SyntaxKind::CONTEXT_DIFF_FILE.into());

        // Parse old file header (*** file)
        if self.at(SyntaxKind::STAR) && self.peek_text(0) == Some("***") {
            self.parse_context_old_file();
        }

        // Parse new file header (--- file)
        if self.at(SyntaxKind::MINUS) && self.peek_text(0) == Some("---") {
            self.parse_context_new_file();
        }

        // Parse hunks (*************** markers)
        while self.at(SyntaxKind::STAR) && self.looks_like_context_hunk_separator() {
            self.parse_context_hunk();
        }

        self.builder.finish_node();
    }

    fn parse_context_old_file(&mut self) {
        self.builder.start_node(SyntaxKind::CONTEXT_OLD_FILE.into());

        // Consume "***"
        self.advance(); // *
        self.advance(); // *
        self.advance(); // *

        // Parse similar to unified diff headers
        self.skip_whitespace();
        self.parse_file_path();
        self.skip_to_eol();

        self.builder.finish_node();
    }

    fn parse_context_new_file(&mut self) {
        self.builder.start_node(SyntaxKind::CONTEXT_NEW_FILE.into());

        // Consume "---"
        self.advance(); // -
        self.advance(); // -
        self.advance(); // -

        self.skip_whitespace();
        self.parse_file_path();
        self.skip_to_eol();

        self.builder.finish_node();
    }

    fn parse_context_hunk(&mut self) {
        self.builder.start_node(SyntaxKind::CONTEXT_HUNK.into());

        // Parse hunk header (***************...)
        self.builder
            .start_node(SyntaxKind::CONTEXT_HUNK_HEADER.into());
        while self.at(SyntaxKind::STAR) {
            self.advance();
        }
        self.skip_to_eol();
        self.builder.finish_node();

        // Parse old section (*** range ****)
        if self.at(SyntaxKind::STAR) && self.peek_text(0) == Some("***") {
            self.parse_context_old_section();
        }

        // Parse new section (--- range ----)
        if self.at(SyntaxKind::MINUS) && self.peek_text(0) == Some("---") {
            self.parse_context_new_section();
        }

        self.builder.finish_node();
    }

    fn parse_context_hunk_without_separator(&mut self) {
        self.builder.start_node(SyntaxKind::CONTEXT_HUNK.into());

        // No hunk header in this case

        // Parse old section (*** range ****)
        if self.at(SyntaxKind::STAR) && self.peek_text(0) == Some("***") {
            self.parse_context_old_section();
        }

        // Parse new section (--- range ----)
        if self.at(SyntaxKind::MINUS) && self.peek_text(0) == Some("---") {
            self.parse_context_new_section();
        }

        self.builder.finish_node();
    }

    fn parse_context_old_section(&mut self) {
        self.builder
            .start_node(SyntaxKind::CONTEXT_OLD_SECTION.into());

        // Parse section header (*** 1,3 ****)
        self.advance(); // *
        self.advance(); // *
        self.advance(); // *

        self.skip_whitespace();
        self.parse_hunk_range(); // Reuse unified diff range parser

        // Skip to end of header
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }
        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }

        // Parse lines
        while !self.at_end() && !self.is_context_section_end() {
            self.parse_context_line();
        }

        self.builder.finish_node();
    }

    fn parse_context_new_section(&mut self) {
        self.builder
            .start_node(SyntaxKind::CONTEXT_NEW_SECTION.into());

        // Parse section header (--- 1,3 ----)
        self.advance(); // -
        self.advance(); // -
        self.advance(); // -

        self.skip_whitespace();
        self.parse_hunk_range();

        // Skip to end of header
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }
        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }

        // Parse lines
        while !self.at_end() && !self.is_context_section_end() {
            self.parse_context_line();
        }

        self.builder.finish_node();
    }

    fn parse_context_line(&mut self) {
        let checkpoint = self.builder.checkpoint();

        match self.current_kind() {
            Some(SyntaxKind::SPACE) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::CONTEXT_LINE.into());
                self.advance(); // space
            }
            Some(SyntaxKind::EXCLAMATION) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::CONTEXT_CHANGE_LINE.into());
                self.advance(); // !
            }
            Some(SyntaxKind::PLUS) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ADD_LINE.into());
                self.advance(); // +
            }
            Some(SyntaxKind::MINUS) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::DELETE_LINE.into());
                self.advance(); // -
            }
            _ => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::CONTEXT_LINE.into());
            }
        }

        // Parse line content
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }
        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }

        self.builder.finish_node();
    }

    fn is_context_section_end(&self) -> bool {
        // Check for section markers
        (self.at(SyntaxKind::MINUS) && self.peek_text(0) == Some("---"))
            || (self.at(SyntaxKind::STAR)
                && (self.peek_text(0) == Some("***") || self.looks_like_context_hunk_separator()))
    }

    // Ed diff parsing
    fn parse_ed_command(&mut self) {
        self.builder.start_node(SyntaxKind::ED_COMMAND.into());

        let checkpoint = self.builder.checkpoint();

        // Parse line numbers
        while self.at(SyntaxKind::NUMBER) || self.at(SyntaxKind::COMMA) {
            self.advance();
        }

        // Parse command letter
        let cmd = self.current_kind();
        match cmd {
            Some(SyntaxKind::LETTER_A) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ED_ADD_COMMAND.into());
                self.advance();
                self.skip_to_eol();
                self.parse_ed_content_lines();
                self.builder.finish_node();
            }
            Some(SyntaxKind::LETTER_D) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ED_DELETE_COMMAND.into());
                self.advance();
                self.skip_to_eol();
                self.builder.finish_node();
            }
            Some(SyntaxKind::LETTER_C) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ED_CHANGE_COMMAND.into());
                self.advance();
                self.skip_to_eol();
                self.parse_ed_content_lines();
                self.builder.finish_node();
            }
            _ => {
                // Invalid command
                self.skip_to_eol();
            }
        }

        self.builder.finish_node();
    }

    fn parse_ed_content_lines(&mut self) {
        // Ed content lines end with a single "."
        while !self.at_end() {
            if self.at(SyntaxKind::DOT) && self.peek_kind(1) == Some(SyntaxKind::NEWLINE) {
                self.advance(); // .
                self.advance(); // newline
                break;
            }

            self.builder.start_node(SyntaxKind::ED_CONTENT_LINE.into());
            while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
                self.advance();
            }
            if self.at(SyntaxKind::NEWLINE) {
                self.advance();
            }
            self.builder.finish_node();
        }
    }

    // Normal diff parsing
    fn parse_normal_hunk(&mut self) {
        self.builder.start_node(SyntaxKind::NORMAL_HUNK.into());

        // Parse change command (2c2, 5,7d10, etc.)
        self.builder
            .start_node(SyntaxKind::NORMAL_CHANGE_COMMAND.into());

        // Parse first range
        while self.at(SyntaxKind::NUMBER) || self.at(SyntaxKind::COMMA) {
            self.advance();
        }

        // Parse command (a, c, d)
        if matches!(
            self.current_kind(),
            Some(SyntaxKind::LETTER_A) | Some(SyntaxKind::LETTER_C) | Some(SyntaxKind::LETTER_D)
        ) {
            self.advance();
        }

        // Parse second range
        while self.at(SyntaxKind::NUMBER) || self.at(SyntaxKind::COMMA) {
            self.advance();
        }

        self.skip_to_eol();
        self.builder.finish_node();

        // Parse old lines (< lines)
        if self.at(SyntaxKind::LESS_THAN) {
            self.parse_normal_old_lines();
        }

        // Parse separator (---)
        if self.at(SyntaxKind::MINUS) && self.peek_text(0) == Some("---") {
            self.parse_normal_separator();
        }

        // Parse new lines (> lines)
        if self.at(SyntaxKind::GREATER_THAN) {
            self.parse_normal_new_lines();
        }

        self.builder.finish_node();
    }

    fn parse_normal_old_lines(&mut self) {
        self.builder.start_node(SyntaxKind::NORMAL_OLD_LINES.into());

        while self.at(SyntaxKind::LESS_THAN) {
            self.builder.start_node(SyntaxKind::DELETE_LINE.into());
            self.advance(); // <
            self.skip_whitespace();

            while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
                self.advance();
            }
            if self.at(SyntaxKind::NEWLINE) {
                self.advance();
            }

            self.builder.finish_node();
        }

        self.builder.finish_node();
    }

    fn parse_normal_separator(&mut self) {
        self.builder.start_node(SyntaxKind::NORMAL_SEPARATOR.into());

        // Consume "---"
        self.advance(); // -
        self.advance(); // -
        self.advance(); // -

        self.skip_to_eol();

        self.builder.finish_node();
    }

    fn parse_normal_new_lines(&mut self) {
        self.builder.start_node(SyntaxKind::NORMAL_NEW_LINES.into());

        while self.at(SyntaxKind::GREATER_THAN) {
            self.builder.start_node(SyntaxKind::ADD_LINE.into());
            self.advance(); // >
            self.skip_whitespace();

            while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
                self.advance();
            }
            if self.at(SyntaxKind::NEWLINE) {
                self.advance();
            }

            self.builder.finish_node();
        }

        self.builder.finish_node();
    }

    fn parse_file_path(&mut self) {
        let mut path_parts = Vec::new();
        let mut collecting_path = true;

        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() && collecting_path {
            match self.current_kind() {
                Some(SyntaxKind::TEXT)
                | Some(SyntaxKind::SLASH)
                | Some(SyntaxKind::DOT)
                | Some(SyntaxKind::NUMBER)
                | Some(SyntaxKind::MINUS)
                | Some(SyntaxKind::STAR)
                | Some(SyntaxKind::COLON)
                | Some(SyntaxKind::BACKSLASH) => {
                    if let Some(text) = self.current_text() {
                        path_parts.push(text.to_string());
                    }
                    self.advance_without_emit();
                }
                Some(SyntaxKind::WHITESPACE) if !path_parts.is_empty() => {
                    collecting_path = false;
                }
                _ => {
                    collecting_path = false;
                }
            }
        }

        if !path_parts.is_empty() {
            let path = path_parts.join("");
            self.builder.token(SyntaxKind::PATH.into(), &path);
        }
    }

    fn skip_to_eol(&mut self) {
        while !self.at(SyntaxKind::NEWLINE) && !self.at_end() {
            self.advance();
        }
        if self.at(SyntaxKind::NEWLINE) {
            self.advance();
        }
    }

    fn looks_like_context_hunk_separator(&self) -> bool {
        // Context hunk separators are lines of 15 or more asterisks
        let mut offset = 0;
        let mut star_count = 0;

        while matches!(self.peek_kind(offset), Some(SyntaxKind::STAR)) {
            star_count += 1;
            offset += 1;
        }

        // Check if we have at least 7 stars followed by newline or end (15 is standard but be flexible)
        star_count >= 7 && matches!(self.peek_kind(offset), Some(SyntaxKind::NEWLINE) | None)
    }

    fn looks_like_context_hunk_range(&self) -> bool {
        // Context hunk range: *** 1,4 **** or *** 1 ****
        if !self.at(SyntaxKind::STAR) || self.peek_text(0) != Some("***") {
            return false;
        }

        let mut offset = 3; // Skip ***

        // Skip whitespace
        while matches!(self.peek_kind(offset), Some(SyntaxKind::WHITESPACE)) {
            offset += 1;
        }

        // Must have a number
        if !matches!(self.peek_kind(offset), Some(SyntaxKind::NUMBER)) {
            return false;
        }

        // Skip numbers and commas
        while matches!(
            self.peek_kind(offset),
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::COMMA)
        ) {
            offset += 1;
        }

        // Skip whitespace
        while matches!(self.peek_kind(offset), Some(SyntaxKind::WHITESPACE)) {
            offset += 1;
        }

        // Check for trailing stars
        let mut star_count = 0;
        while matches!(self.peek_kind(offset), Some(SyntaxKind::STAR)) {
            star_count += 1;
            offset += 1;
        }

        // Should have at least 3 trailing stars
        star_count >= 3
    }

    fn looks_like_context_new_section(&self) -> bool {
        // Context new section: --- 1,4 ---- or --- 1 ----
        if !self.at(SyntaxKind::MINUS) || self.peek_text(0) != Some("---") {
            return false;
        }

        let mut offset = 3; // Skip ---

        // Skip whitespace
        while matches!(self.peek_kind(offset), Some(SyntaxKind::WHITESPACE)) {
            offset += 1;
        }

        // Must have a number
        if !matches!(self.peek_kind(offset), Some(SyntaxKind::NUMBER)) {
            return false;
        }

        // Skip numbers and commas
        while matches!(
            self.peek_kind(offset),
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::COMMA)
        ) {
            offset += 1;
        }

        // Skip whitespace
        while matches!(self.peek_kind(offset), Some(SyntaxKind::WHITESPACE)) {
            offset += 1;
        }

        // Check for trailing minuses
        let mut minus_count = 0;
        while matches!(self.peek_kind(offset), Some(SyntaxKind::MINUS)) {
            minus_count += 1;
            offset += 1;
        }

        // Should have at least 3 trailing minuses
        minus_count >= 3
    }
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "error_recovery_tests.rs"]
mod error_recovery_tests;

#[cfg(test)]
#[path = "additional_tests.rs"]
mod additional_tests;

#[cfg(test)]
#[path = "format_tests.rs"]
mod format_tests;

#[cfg(test)]
#[path = "corner_case_tests.rs"]
mod corner_case_tests;
