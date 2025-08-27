//! Lossless AST structures for quilt series files
use crate::edit::quilt::lex::SyntaxKind;
use rowan::{ast::AstNode, GreenNode, SyntaxNode, SyntaxToken};
use std::fmt;

/// Language definition for quilt series file syntax
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuiltLang {}

impl rowan::Language for QuiltLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::OPTION_ITEM as u16);
        unsafe { std::mem::transmute(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// Syntax element type for quilt series files
pub type SyntaxElement = rowan::SyntaxElement<QuiltLang>;

/// Parse error containing a list of error messages
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseError(pub Vec<String>);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, "\n")?;
            }
            write!(f, "{}", err)?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

/// Parse error with position information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionedParseError {
    /// The error message
    pub message: String,
    /// The position in the source text where the error occurred
    pub position: rowan::TextRange,
}

macro_rules! ast_node {
    ($name:ident, $kind:expr) => {
        #[doc = concat!("AST node for ", stringify!($name))]
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name {
            syntax: SyntaxNode<QuiltLang>,
        }

        impl AstNode for $name {
            type Language = QuiltLang;

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }

            fn cast(syntax: SyntaxNode<QuiltLang>) -> Option<Self> {
                if Self::can_cast(syntax.kind()) {
                    Some(Self { syntax })
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode<QuiltLang> {
                &self.syntax
            }
        }
    };
}

// Root and entry nodes
ast_node!(SeriesFile, SyntaxKind::ROOT);
ast_node!(SeriesEntry, SyntaxKind::SERIES_ENTRY);
ast_node!(PatchEntry, SyntaxKind::PATCH_ENTRY);
ast_node!(CommentLine, SyntaxKind::COMMENT_LINE);
ast_node!(Options, SyntaxKind::OPTIONS);
ast_node!(OptionItem, SyntaxKind::OPTION_ITEM);

impl SeriesFile {
    /// Get all entries in the series file
    pub fn entries(&self) -> impl Iterator<Item = SeriesEntry> {
        self.syntax().children().filter_map(SeriesEntry::cast)
    }

    /// Get all patch entries in the series file
    pub fn patch_entries(&self) -> impl Iterator<Item = PatchEntry> {
        self.entries().filter_map(|entry| entry.as_patch_entry())
    }

    /// Get all comment lines in the series file
    pub fn comment_lines(&self) -> impl Iterator<Item = CommentLine> {
        self.entries().filter_map(|entry| entry.as_comment_line())
    }

    /// Get parse errors from the syntax tree
    pub fn errors(&self) -> Vec<PositionedParseError> {
        let mut errors = Vec::new();

        for element in self.syntax().descendants_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = element {
                if token.kind() == SyntaxKind::ERROR {
                    errors.push(PositionedParseError {
                        message: "Invalid token".to_string(),
                        position: token.text_range(),
                    });
                }
            }
        }

        errors
    }

    /// Create a mutable root node from a green node
    pub fn new_root(green: GreenNode) -> Self {
        let node = SyntaxNode::new_root_mut(green);
        Self::cast(node).unwrap()
    }

    /// Create a mutable root node for editing  
    pub fn new_root_mut(green: GreenNode) -> Self {
        let node = SyntaxNode::new_root_mut(green);
        Self::cast(node).unwrap()
    }
}

impl SeriesEntry {
    /// Try to cast this entry as a patch entry
    pub fn as_patch_entry(&self) -> Option<PatchEntry> {
        self.syntax().children().find_map(PatchEntry::cast)
    }

    /// Try to cast this entry as a comment line
    pub fn as_comment_line(&self) -> Option<CommentLine> {
        self.syntax().children().find_map(CommentLine::cast)
    }
}

impl PatchEntry {
    /// Get the patch name
    pub fn name(&self) -> Option<String> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::PATCH_NAME)
            .map(|token| token.text().to_string())
    }

    /// Get the patch name token
    pub fn name_token(&self) -> Option<SyntaxToken<QuiltLang>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::PATCH_NAME)
    }

    /// Get the options for this patch
    pub fn options(&self) -> Option<Options> {
        self.syntax().children().find_map(Options::cast)
    }

    /// Get option strings
    pub fn option_strings(&self) -> Vec<String> {
        self.options()
            .map(|opts| opts.option_strings())
            .unwrap_or_default()
    }

    /// Set the patch name (modifies the tree in place)
    /// Note: This requires the containing tree to be mutable
    pub fn set_name(&self, new_name: &str) {
        // Build a new token using GreenNodeBuilder with proper node structure
        let mut builder = rowan::GreenNodeBuilder::new();
        builder.start_node(SyntaxKind::ROOT.into());
        builder.token(SyntaxKind::PATCH_NAME.into(), new_name);
        builder.finish_node();
        let token_green = builder.finish();

        // Create a new syntax node from the green node
        let token_node = SyntaxNode::new_root_mut(token_green);
        let new_token = token_node.first_token().unwrap();

        // Find the existing patch name token and replace it
        for (index, element) in self.syntax().children_with_tokens().enumerate() {
            if let rowan::NodeOrToken::Token(token) = element {
                if token.kind() == SyntaxKind::PATCH_NAME {
                    self.syntax().splice_children(
                        index..index + 1,
                        vec![rowan::NodeOrToken::Token(new_token)],
                    );
                    return;
                }
            }
        }

        // If no existing patch name, insert at the beginning
        self.syntax()
            .splice_children(0..0, vec![rowan::NodeOrToken::Token(new_token)]);
    }
}

impl CommentLine {
    /// Get the comment text (without the # prefix)
    pub fn text(&self) -> String {
        let mut text = String::new();
        let mut found_hash = false;

        for element in self.syntax().children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = element {
                if token.kind() == SyntaxKind::HASH {
                    found_hash = true;
                } else if found_hash && token.kind() == SyntaxKind::TEXT {
                    text.push_str(token.text());
                }
            }
        }

        text
    }

    /// Get the full comment text (including the # prefix)
    pub fn full_text(&self) -> String {
        self.syntax().text().to_string()
    }
}

impl Options {
    /// Get the option items
    pub fn option_items(&self) -> impl Iterator<Item = OptionItem> {
        self.syntax().children().filter_map(OptionItem::cast)
    }

    /// Get option strings
    pub fn option_strings(&self) -> Vec<String> {
        self.option_items()
            .filter_map(|item| item.value())
            .collect()
    }
}

impl OptionItem {
    /// Get the option value
    pub fn value(&self) -> Option<String> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::OPTION)
            .map(|token| token.text().to_string())
    }
}

/// Parse a quilt series file into a lossless AST
pub fn parse(text: &str) -> crate::parse::Parse<SeriesFile> {
    crate::edit::quilt::parse::parse_series(text)
}
