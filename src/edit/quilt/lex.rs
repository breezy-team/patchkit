/// Token types and syntax node kinds for quilt series files
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens
    /// Hash/pound sign for comments
    HASH = 0,
    /// Space character
    SPACE,
    /// Tab character
    TAB,
    /// Newline character
    NEWLINE,
    /// Whitespace characters (spaces and tabs)
    WHITESPACE,
    /// Patch file name/path
    PATCH_NAME,
    /// Patch option (e.g., -p1, --reverse)
    OPTION,
    /// Text content (for comments)
    TEXT,
    /// Error token
    ERROR,
    /// End of file
    EOF,

    // Composite nodes
    /// Root node of the syntax tree
    ROOT,
    /// A series entry (either patch or comment)
    SERIES_ENTRY,
    /// A patch entry with name and options
    PATCH_ENTRY,
    /// A comment line
    COMMENT_LINE,
    /// Patch options section
    OPTIONS,
    /// Individual option
    OPTION_ITEM,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// Lexer for quilt series files
pub struct Lexer<'a> {
    input: &'a str,
    chars: std::str::Chars<'a>,
    pos: usize,       // character position for logic
    byte_pos: usize,  // byte position for slicing
    in_comment: bool, // track if we're inside a comment line
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given input text
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars(),
            pos: 0,
            byte_pos: 0,
            in_comment: false,
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(&mut self) -> Vec<(SyntaxKind, String)> {
        let mut tokens = Vec::new();

        while self.byte_pos < self.input.len() {
            let token = self.next_token();
            tokens.push(token);
        }

        tokens.push((SyntaxKind::EOF, String::new()));
        tokens
    }

    fn next_token(&mut self) -> (SyntaxKind, String) {
        let ch = self.current_char();

        match ch {
            Some('#') => {
                self.advance();
                self.in_comment = true;
                (SyntaxKind::HASH, "#".to_string())
            }
            Some(' ') => {
                self.advance();
                (SyntaxKind::SPACE, " ".to_string())
            }
            Some('\t') => {
                self.advance();
                (SyntaxKind::TAB, "\t".to_string())
            }
            Some('\n') => {
                self.advance();
                self.in_comment = false; // reset comment state at end of line
                (SyntaxKind::NEWLINE, "\n".to_string())
            }
            Some(_) => {
                // If we're in a comment, everything is text
                if self.in_comment {
                    self.read_text()
                // Check if we're at the start of a line or after whitespace
                } else if self.at_line_start() || self.prev_is_whitespace() {
                    if self.peek_option() {
                        self.read_option()
                    } else {
                        self.read_patch_name()
                    }
                } else {
                    self.read_text()
                }
            }
            None => (SyntaxKind::ERROR, String::new()),
        }
    }

    fn current_char(&self) -> Option<char> {
        self.chars.as_str().chars().next()
    }

    fn advance(&mut self) {
        if let Some(ch) = self.chars.next() {
            self.byte_pos += ch.len_utf8();
            self.pos += 1;
        }
    }

    fn at_line_start(&self) -> bool {
        self.pos == 0
            || (self.byte_pos > 0 && self.input[..self.byte_pos].chars().last() == Some('\n'))
    }

    fn prev_is_whitespace(&self) -> bool {
        if self.byte_pos == 0 {
            return false;
        }
        matches!(
            self.input[..self.byte_pos].chars().last(),
            Some(' ') | Some('\t')
        )
    }

    fn peek_option(&self) -> bool {
        match self.current_char() {
            Some('-') => true,
            _ => false,
        }
    }

    fn read_option(&mut self) -> (SyntaxKind, String) {
        let start_byte = self.byte_pos;

        // Read until whitespace or newline
        while let Some(ch) = self.current_char() {
            if ch == ' ' || ch == '\t' || ch == '\n' {
                break;
            }
            self.advance();
        }

        let text = self.input[start_byte..self.byte_pos].to_string();
        (SyntaxKind::OPTION, text)
    }

    fn read_patch_name(&mut self) -> (SyntaxKind, String) {
        let start_byte = self.byte_pos;

        // Read until whitespace or newline
        while let Some(ch) = self.current_char() {
            if ch == ' ' || ch == '\t' || ch == '\n' {
                break;
            }
            self.advance();
        }

        let text = self.input[start_byte..self.byte_pos].to_string();
        (SyntaxKind::PATCH_NAME, text)
    }

    fn read_text(&mut self) -> (SyntaxKind, String) {
        let start_byte = self.byte_pos;

        // Read until newline
        while let Some(ch) = self.current_char() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }

        let text = self.input[start_byte..self.byte_pos].to_string();
        (SyntaxKind::TEXT, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_options() {
        let text = "patch.patch -p1\n";
        let mut lexer = Lexer::new(text);
        let tokens = lexer.tokenize();

        println!("Input text: {:?}", text);
        println!("Tokens:");
        for (i, (kind, text)) in tokens.iter().enumerate() {
            println!("  {}: {:?} = {:?}", i, kind, text);
        }
    }

    #[test]
    fn test_debug_unicode() {
        let text = "# Pätch sériès with ünïcødé\npatch-ñame.patch\n# Comment with émojis 🚀\nspëcial-patch.patch -p1\n";
        let mut lexer = Lexer::new(text);
        let tokens = lexer.tokenize();

        println!("Input text: {:?}", text);
        println!("Tokens:");
        for (i, (kind, text)) in tokens.iter().enumerate() {
            println!("  {}: {:?} = {:?}", i, kind, text);
        }
    }

    #[test]
    fn test_lex_simple_patch() {
        let mut lexer = Lexer::new("patch1.patch\n");
        let tokens = lexer.tokenize();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].0, SyntaxKind::PATCH_NAME);
        assert_eq!(tokens[0].1, "patch1.patch");
        assert_eq!(tokens[1].0, SyntaxKind::NEWLINE);
        assert_eq!(tokens[2].0, SyntaxKind::EOF);
    }

    #[test]
    fn test_lex_patch_with_options() {
        let mut lexer = Lexer::new("patch1.patch -p1 --reverse\n");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0].0, SyntaxKind::PATCH_NAME);
        assert_eq!(tokens[0].1, "patch1.patch");
        assert_eq!(tokens[1].0, SyntaxKind::SPACE);
        assert_eq!(tokens[2].0, SyntaxKind::OPTION);
        assert_eq!(tokens[2].1, "-p1");
        assert_eq!(tokens[3].0, SyntaxKind::SPACE);
        assert_eq!(tokens[4].0, SyntaxKind::OPTION);
        assert_eq!(tokens[4].1, "--reverse");
    }

    #[test]
    fn test_lex_comment() {
        let mut lexer = Lexer::new("# This is a comment\n");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0].0, SyntaxKind::HASH);
        assert_eq!(tokens[1].0, SyntaxKind::SPACE);
        assert_eq!(tokens[2].0, SyntaxKind::TEXT);
        assert_eq!(tokens[2].1, "This is a comment");
    }
}
