#[cfg(test)]
mod tests {
    use crate::edit::lex::{lex, SyntaxKind};

    fn collect_tokens(input: &str) -> Vec<(SyntaxKind, String)> {
        lex(input)
            .map(|(kind, text)| (kind, text.to_string()))
            .collect()
    }

    #[test]
    fn test_lex_empty() {
        let tokens = collect_tokens("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_lex_file_headers() {
        let input = "--- a/file.txt\n+++ b/file.txt\n";
        let tokens = collect_tokens(input);

        // First line: --- a/file.txt
        assert_eq!(tokens[0], (SyntaxKind::MINUS, "-".to_string()));
        assert_eq!(tokens[1], (SyntaxKind::MINUS, "-".to_string()));
        assert_eq!(tokens[2], (SyntaxKind::MINUS, "-".to_string()));
        assert_eq!(tokens[3], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[4], (SyntaxKind::TEXT, "a".to_string()));
        assert_eq!(tokens[5], (SyntaxKind::SLASH, "/".to_string()));
        assert_eq!(tokens[6], (SyntaxKind::TEXT, "file".to_string()));
        assert_eq!(tokens[7], (SyntaxKind::DOT, ".".to_string()));
        assert_eq!(tokens[8], (SyntaxKind::TEXT, "txt".to_string()));
        assert_eq!(tokens[9], (SyntaxKind::NEWLINE, "\n".to_string()));

        // Second line: +++ b/file.txt
        assert_eq!(tokens[10], (SyntaxKind::PLUS, "+".to_string()));
        assert_eq!(tokens[11], (SyntaxKind::PLUS, "+".to_string()));
        assert_eq!(tokens[12], (SyntaxKind::PLUS, "+".to_string()));
        assert_eq!(tokens[13], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[14], (SyntaxKind::TEXT, "b".to_string()));
        assert_eq!(tokens[15], (SyntaxKind::SLASH, "/".to_string()));
        assert_eq!(tokens[16], (SyntaxKind::TEXT, "file".to_string()));
        assert_eq!(tokens[17], (SyntaxKind::DOT, ".".to_string()));
        assert_eq!(tokens[18], (SyntaxKind::TEXT, "txt".to_string()));
        assert_eq!(tokens[19], (SyntaxKind::NEWLINE, "\n".to_string()));
    }

    #[test]
    fn test_lex_hunk_header() {
        let input = "@@ -1,3 +1,4 @@\n";
        let tokens = collect_tokens(input);

        assert_eq!(tokens[0], (SyntaxKind::AT, "@".to_string()));
        assert_eq!(tokens[1], (SyntaxKind::AT, "@".to_string()));
        assert_eq!(tokens[2], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[3], (SyntaxKind::MINUS, "-".to_string()));
        assert_eq!(tokens[4], (SyntaxKind::NUMBER, "1".to_string()));
        assert_eq!(tokens[5], (SyntaxKind::COMMA, ",".to_string()));
        assert_eq!(tokens[6], (SyntaxKind::NUMBER, "3".to_string()));
        assert_eq!(tokens[7], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[8], (SyntaxKind::PLUS, "+".to_string()));
        assert_eq!(tokens[9], (SyntaxKind::NUMBER, "1".to_string()));
        assert_eq!(tokens[10], (SyntaxKind::COMMA, ",".to_string()));
        assert_eq!(tokens[11], (SyntaxKind::NUMBER, "4".to_string()));
        assert_eq!(tokens[12], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[13], (SyntaxKind::AT, "@".to_string()));
        assert_eq!(tokens[14], (SyntaxKind::AT, "@".to_string()));
        assert_eq!(tokens[15], (SyntaxKind::NEWLINE, "\n".to_string()));
    }

    #[test]
    fn test_lex_hunk_lines() {
        let input = " context line\n-deleted line\n+added line\n";
        let tokens = collect_tokens(input);

        // Context line
        assert_eq!(tokens[0], (SyntaxKind::SPACE, " ".to_string()));
        assert_eq!(tokens[1], (SyntaxKind::TEXT, "context".to_string()));
        assert_eq!(tokens[2], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[3], (SyntaxKind::TEXT, "line".to_string()));
        assert_eq!(tokens[4], (SyntaxKind::NEWLINE, "\n".to_string()));

        // Deleted line
        assert_eq!(tokens[5], (SyntaxKind::MINUS, "-".to_string()));
        assert_eq!(tokens[6], (SyntaxKind::TEXT, "deleted".to_string()));
        assert_eq!(tokens[7], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[8], (SyntaxKind::TEXT, "line".to_string()));
        assert_eq!(tokens[9], (SyntaxKind::NEWLINE, "\n".to_string()));

        // Added line
        assert_eq!(tokens[10], (SyntaxKind::PLUS, "+".to_string()));
        assert_eq!(tokens[11], (SyntaxKind::TEXT, "added".to_string()));
        assert_eq!(tokens[12], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[13], (SyntaxKind::TEXT, "line".to_string()));
        assert_eq!(tokens[14], (SyntaxKind::NEWLINE, "\n".to_string()));
    }

    #[test]
    fn test_lex_whitespace_handling() {
        let input = "   \t  multiple spaces\n";
        let tokens = collect_tokens(input);

        // At start of line, first space is SPACE, then rest is TEXT
        assert_eq!(tokens[0], (SyntaxKind::SPACE, " ".to_string()));
        assert_eq!(tokens[1], (SyntaxKind::WHITESPACE, "  \t  ".to_string()));
        assert_eq!(tokens[2], (SyntaxKind::TEXT, "multiple".to_string()));
        assert_eq!(tokens[3], (SyntaxKind::WHITESPACE, " ".to_string()));
        assert_eq!(tokens[4], (SyntaxKind::TEXT, "spaces".to_string()));
        assert_eq!(tokens[5], (SyntaxKind::NEWLINE, "\n".to_string()));
    }

    #[test]
    fn test_lex_windows_newlines() {
        let input = "line1\r\nline2\r\n";
        let tokens = collect_tokens(input);

        assert_eq!(tokens[0], (SyntaxKind::TEXT, "line1".to_string()));
        assert_eq!(tokens[1], (SyntaxKind::NEWLINE, "\r\n".to_string()));
        assert_eq!(tokens[2], (SyntaxKind::TEXT, "line2".to_string()));
        assert_eq!(tokens[3], (SyntaxKind::NEWLINE, "\r\n".to_string()));
    }
}
