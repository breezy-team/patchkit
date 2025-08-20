/// Token types and syntax node kinds for patch files
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens
    /// Minus sign token
    MINUS = 0,
    /// Plus sign token
    PLUS,
    /// At sign token
    AT,
    /// Space character at start of line
    SPACE,
    /// Newline character
    NEWLINE,
    /// Whitespace characters (spaces and tabs)
    WHITESPACE,
    /// Numeric token
    NUMBER,
    /// Comma token
    COMMA,
    /// Colon token
    COLON,
    /// Dot token
    DOT,
    /// Slash token
    SLASH,
    /// File path token
    PATH,
    /// Text content token
    TEXT,
    /// Error token
    ERROR,

    // Additional tokens for other diff formats
    /// Star token (for context diffs)
    STAR,
    /// Exclamation mark (for context diffs)
    EXCLAMATION,
    /// Less than sign (for normal/ed diffs)
    LESS_THAN,
    /// Greater than sign (for normal/ed diffs)
    GREATER_THAN,
    /// Letter 'a' (for ed diff commands)
    LETTER_A,
    /// Letter 'c' (for ed diff commands)
    LETTER_C,
    /// Letter 'd' (for ed diff commands)
    LETTER_D,
    /// Backslash token
    BACKSLASH,

    // Composite nodes
    /// Root node of the syntax tree
    ROOT,
    /// A patch file node (generic, format determined by content)
    PATCH_FILE,

    // Unified diff nodes
    /// File header node
    FILE_HEADER,
    /// Old file header node (unified)
    OLD_FILE,
    /// New file header node (unified)
    NEW_FILE,
    /// Hunk node (unified)
    HUNK,
    /// Hunk header node (unified)
    HUNK_HEADER,
    /// Hunk range node
    HUNK_RANGE,
    /// Context line node
    CONTEXT_LINE,
    /// Add line node
    ADD_LINE,
    /// Delete line node
    DELETE_LINE,

    // Context diff nodes
    /// Context diff file node
    CONTEXT_DIFF_FILE,
    /// Context diff old file header
    CONTEXT_OLD_FILE,
    /// Context diff new file header
    CONTEXT_NEW_FILE,
    /// Context diff hunk
    CONTEXT_HUNK,
    /// Context diff hunk header
    CONTEXT_HUNK_HEADER,
    /// Context diff old section
    CONTEXT_OLD_SECTION,
    /// Context diff new section
    CONTEXT_NEW_SECTION,
    /// Context diff change line (!)
    CONTEXT_CHANGE_LINE,

    // Ed diff nodes
    /// Ed diff command node
    ED_COMMAND,
    /// Ed diff add command
    ED_ADD_COMMAND,
    /// Ed diff delete command
    ED_DELETE_COMMAND,
    /// Ed diff change command
    ED_CHANGE_COMMAND,
    /// Ed diff content line
    ED_CONTENT_LINE,

    // Normal diff nodes
    /// Normal diff hunk
    NORMAL_HUNK,
    /// Normal diff change command
    NORMAL_CHANGE_COMMAND,
    /// Normal diff old lines
    NORMAL_OLD_LINES,
    /// Normal diff new lines
    NORMAL_NEW_LINES,
    /// Normal diff separator
    NORMAL_SEPARATOR,

    // Common nodes
    /// Hunk line node (generic)
    HUNK_LINE,
    /// No newline at end of file line
    NO_NEWLINE_LINE,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// Lex a patch file into tokens
pub fn lex(input: &str) -> impl Iterator<Item = (SyntaxKind, &str)> + '_ {
    Lexer::new(input)
}

struct Lexer<'a> {
    input: &'a str,
    cursor: usize,
    start_of_line: bool,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            cursor: 0,
            start_of_line: true,
        }
    }

    fn current_char(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn advance(&mut self, n: usize) -> &'a str {
        let start = self.cursor;
        self.cursor = (self.cursor + n).min(self.input.len());
        &self.input[start..self.cursor]
    }

    fn consume_while<F>(&mut self, mut predicate: F) -> &'a str
    where
        F: FnMut(char) -> bool,
    {
        let start = self.cursor;
        while let Some(c) = self.current_char() {
            if !predicate(c) {
                break;
            }
            self.cursor += c.len_utf8();
        }
        &self.input[start..self.cursor]
    }

    fn lex_number(&mut self) -> (SyntaxKind, &'a str) {
        let text = self.consume_while(|c| c.is_ascii_digit());
        (SyntaxKind::NUMBER, text)
    }

    fn lex_whitespace(&mut self) -> (SyntaxKind, &'a str) {
        let text = self.consume_while(|c| c == ' ' || c == '\t');
        (SyntaxKind::WHITESPACE, text)
    }

    fn could_be_ed_command(&self) -> bool {
        // Ed commands appear after line numbers (e.g., "5a", "3,7d", "2c")
        // Look back to see if we have digits before current position
        if self.cursor == 0 {
            return false;
        }

        // Check if previous character was a digit
        let prev_char = self.input[..self.cursor].chars().last();
        matches!(prev_char, Some('0'..='9'))
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = (SyntaxKind, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.input.len() {
            return None;
        }

        let c = self.current_char()?;

        match c {
            '\n' => {
                self.start_of_line = true;
                Some((SyntaxKind::NEWLINE, self.advance(1)))
            }
            '\r' => {
                self.start_of_line = true;
                // Check if this is \r\n
                if self.cursor + 1 < self.input.len()
                    && self.input.as_bytes()[self.cursor + 1] == b'\n'
                {
                    // Consume both \r and \n as a single NEWLINE token
                    Some((SyntaxKind::NEWLINE, self.advance(2)))
                } else {
                    // Just \r
                    Some((SyntaxKind::NEWLINE, self.advance(1)))
                }
            }
            ' ' => {
                if self.start_of_line {
                    self.start_of_line = false;
                    Some((SyntaxKind::SPACE, self.advance(1)))
                } else {
                    Some(self.lex_whitespace())
                }
            }
            '\t' => Some(self.lex_whitespace()),
            '-' => {
                if self.start_of_line {
                    self.start_of_line = false;
                }
                Some((SyntaxKind::MINUS, self.advance(1)))
            }
            '+' => {
                if self.start_of_line {
                    self.start_of_line = false;
                }
                Some((SyntaxKind::PLUS, self.advance(1)))
            }
            '@' => {
                if self.start_of_line {
                    self.start_of_line = false;
                }
                Some((SyntaxKind::AT, self.advance(1)))
            }
            ',' => Some((SyntaxKind::COMMA, self.advance(1))),
            ':' => Some((SyntaxKind::COLON, self.advance(1))),
            '.' => Some((SyntaxKind::DOT, self.advance(1))),
            '/' => Some((SyntaxKind::SLASH, self.advance(1))),
            '*' => {
                if self.start_of_line {
                    self.start_of_line = false;
                }
                Some((SyntaxKind::STAR, self.advance(1)))
            }
            '!' => {
                if self.start_of_line {
                    self.start_of_line = false;
                }
                Some((SyntaxKind::EXCLAMATION, self.advance(1)))
            }
            '<' => {
                if self.start_of_line {
                    self.start_of_line = false;
                }
                Some((SyntaxKind::LESS_THAN, self.advance(1)))
            }
            '>' => {
                if self.start_of_line {
                    self.start_of_line = false;
                }
                Some((SyntaxKind::GREATER_THAN, self.advance(1)))
            }
            '\\' => Some((SyntaxKind::BACKSLASH, self.advance(1))),
            'a' if self.could_be_ed_command() => Some((SyntaxKind::LETTER_A, self.advance(1))),
            'c' if self.could_be_ed_command() => Some((SyntaxKind::LETTER_C, self.advance(1))),
            'd' if self.could_be_ed_command() => Some((SyntaxKind::LETTER_D, self.advance(1))),
            '0'..='9' => Some(self.lex_number()),
            _ => {
                if self.start_of_line {
                    self.start_of_line = false;
                }

                // For now, consume everything else as TEXT until special characters
                let start = self.cursor;
                while let Some(ch) = self.current_char() {
                    match ch {
                        '\n' | '\r' | ' ' | '\t' | '-' | '+' | '@' | ',' | ':' | '.' | '/'
                        | '*' | '!' | '<' | '>' | '\\' => break,
                        'a' | 'c' | 'd' if self.could_be_ed_command() => break,
                        _ => self.cursor += ch.len_utf8(),
                    }
                }
                if self.cursor > start {
                    Some((SyntaxKind::TEXT, &self.input[start..self.cursor]))
                } else {
                    // Single character that doesn't match anything
                    Some((SyntaxKind::TEXT, self.advance(c.len_utf8())))
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "lex_tests.rs"]
mod tests;
