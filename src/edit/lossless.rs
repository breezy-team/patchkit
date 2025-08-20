//! Lossless AST structures for patch files
use crate::edit::lex::SyntaxKind;
use rowan::{ast::AstNode, SyntaxNode, SyntaxToken};
use std::fmt;

/// Language definition for patch file syntax
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {}

impl rowan::Language for Lang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::NO_NEWLINE_LINE as u16);
        unsafe { std::mem::transmute(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// Syntax element type for patch files
pub type SyntaxElement = rowan::SyntaxElement<Lang>;

/// The format of a diff
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFormat {
    /// Unified diff format (diff -u)
    Unified,
    /// Context diff format (diff -c)
    Context,
    /// Ed script format (diff -e)
    Ed,
    /// Normal/traditional diff format
    Normal,
}

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
            syntax: SyntaxNode<Lang>,
        }

        impl AstNode for $name {
            type Language = Lang;

            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }

            fn cast(syntax: SyntaxNode<Lang>) -> Option<Self> {
                if Self::can_cast(syntax.kind()) {
                    Some(Self { syntax })
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode<Lang> {
                &self.syntax
            }
        }
    };
}

// Root and generic nodes
ast_node!(Patch, SyntaxKind::ROOT);
ast_node!(PatchFile, SyntaxKind::PATCH_FILE);

// Unified diff nodes
ast_node!(FileHeader, SyntaxKind::FILE_HEADER);
ast_node!(OldFile, SyntaxKind::OLD_FILE);
ast_node!(NewFile, SyntaxKind::NEW_FILE);
ast_node!(Hunk, SyntaxKind::HUNK);
ast_node!(HunkHeader, SyntaxKind::HUNK_HEADER);
ast_node!(HunkRange, SyntaxKind::HUNK_RANGE);
ast_node!(HunkLine, SyntaxKind::HUNK_LINE);
ast_node!(ContextLine, SyntaxKind::CONTEXT_LINE);
ast_node!(AddLine, SyntaxKind::ADD_LINE);
ast_node!(DeleteLine, SyntaxKind::DELETE_LINE);

// Context diff nodes
ast_node!(ContextDiffFile, SyntaxKind::CONTEXT_DIFF_FILE);
ast_node!(ContextOldFile, SyntaxKind::CONTEXT_OLD_FILE);
ast_node!(ContextNewFile, SyntaxKind::CONTEXT_NEW_FILE);
ast_node!(ContextHunk, SyntaxKind::CONTEXT_HUNK);
ast_node!(ContextHunkHeader, SyntaxKind::CONTEXT_HUNK_HEADER);
ast_node!(ContextOldSection, SyntaxKind::CONTEXT_OLD_SECTION);
ast_node!(ContextNewSection, SyntaxKind::CONTEXT_NEW_SECTION);
ast_node!(ContextChangeLine, SyntaxKind::CONTEXT_CHANGE_LINE);

// Ed diff nodes
ast_node!(EdCommand, SyntaxKind::ED_COMMAND);
ast_node!(EdAddCommand, SyntaxKind::ED_ADD_COMMAND);
ast_node!(EdDeleteCommand, SyntaxKind::ED_DELETE_COMMAND);
ast_node!(EdChangeCommand, SyntaxKind::ED_CHANGE_COMMAND);
ast_node!(EdContentLine, SyntaxKind::ED_CONTENT_LINE);

// Normal diff nodes
ast_node!(NormalHunk, SyntaxKind::NORMAL_HUNK);
ast_node!(NormalChangeCommand, SyntaxKind::NORMAL_CHANGE_COMMAND);
ast_node!(NormalOldLines, SyntaxKind::NORMAL_OLD_LINES);
ast_node!(NormalNewLines, SyntaxKind::NORMAL_NEW_LINES);
ast_node!(NormalSeparator, SyntaxKind::NORMAL_SEPARATOR);

impl Patch {
    /// Get all patch files in this patch
    pub fn patch_files(&self) -> impl Iterator<Item = PatchFile> {
        self.syntax().children().filter_map(PatchFile::cast)
    }

    /// Get all context diff files in this patch
    pub fn context_diff_files(&self) -> impl Iterator<Item = ContextDiffFile> {
        self.syntax().children().filter_map(ContextDiffFile::cast)
    }

    /// Get all ed commands in this patch
    pub fn ed_commands(&self) -> impl Iterator<Item = EdCommand> {
        self.syntax().children().filter_map(EdCommand::cast)
    }

    /// Get all normal diff hunks in this patch
    pub fn normal_hunks(&self) -> impl Iterator<Item = NormalHunk> {
        self.syntax().children().filter_map(NormalHunk::cast)
    }

    /// Try to detect the format of this patch
    pub fn detect_format(&self) -> Option<DiffFormat> {
        // Check for unified diff
        if self.patch_files().next().is_some() {
            return Some(DiffFormat::Unified);
        }

        // Check for context diff
        if self.context_diff_files().next().is_some() {
            return Some(DiffFormat::Context);
        }

        // Check for ed diff
        if self.ed_commands().next().is_some() {
            return Some(DiffFormat::Ed);
        }

        // Check for normal diff
        if self.normal_hunks().next().is_some() {
            return Some(DiffFormat::Normal);
        }

        None
    }
}

impl PatchFile {
    /// Get the old file header
    pub fn old_file(&self) -> Option<OldFile> {
        self.syntax().children().find_map(OldFile::cast)
    }

    /// Get the new file header
    pub fn new_file(&self) -> Option<NewFile> {
        self.syntax().children().find_map(NewFile::cast)
    }

    /// Get all hunks in this patch file
    pub fn hunks(&self) -> impl Iterator<Item = Hunk> {
        self.syntax().children().filter_map(Hunk::cast)
    }
}

impl OldFile {
    /// Get the file path
    pub fn path(&self) -> Option<SyntaxToken<Lang>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::PATH)
    }
}

impl NewFile {
    /// Get the file path
    pub fn path(&self) -> Option<SyntaxToken<Lang>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::PATH)
    }
}

impl Hunk {
    /// Get the hunk header
    pub fn header(&self) -> Option<HunkHeader> {
        self.syntax().children().find_map(HunkHeader::cast)
    }

    /// Get all lines in this hunk
    pub fn lines(&self) -> impl Iterator<Item = HunkLine> {
        // HunkLine is not a real syntax kind - the actual kinds are CONTEXT_LINE, ADD_LINE, DELETE_LINE
        // But they all share the same structure, so we can cast any of them as HunkLine
        self.syntax().children().filter_map(|child| {
            match child.kind() {
                SyntaxKind::CONTEXT_LINE | SyntaxKind::ADD_LINE | SyntaxKind::DELETE_LINE => {
                    // These line types all have the same structure, cast them as HunkLine
                    Some(HunkLine { syntax: child })
                }
                _ => None,
            }
        })
    }
}

impl HunkHeader {
    /// Get the old file range for this hunk
    pub fn old_range(&self) -> Option<HunkRange> {
        self.syntax().children().find_map(HunkRange::cast)
    }

    /// Get the new file range for this hunk
    pub fn new_range(&self) -> Option<HunkRange> {
        self.syntax().children().filter_map(HunkRange::cast).nth(1)
    }
}

impl HunkRange {
    /// Get the starting line number
    pub fn start(&self) -> Option<u32> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::NUMBER)
            .and_then(|token| token.text().parse().ok())
    }

    /// Get the number of lines in this range
    pub fn count(&self) -> Option<u32> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| token.kind() == SyntaxKind::NUMBER)
            .nth(1)
            .and_then(|token| token.text().parse().ok())
    }
}

impl HunkLine {
    /// Attempt to cast this line as a context line
    pub fn as_context(&self) -> Option<ContextLine> {
        ContextLine::cast(self.syntax().clone())
    }

    /// Attempt to cast this line as an add line
    pub fn as_add(&self) -> Option<AddLine> {
        AddLine::cast(self.syntax().clone())
    }

    /// Attempt to cast this line as a delete line
    pub fn as_delete(&self) -> Option<DeleteLine> {
        DeleteLine::cast(self.syntax().clone())
    }

    /// Get the text content of this line
    pub fn text(&self) -> Option<String> {
        // Collect all tokens, skipping only the first one if it's a line prefix
        let tokens = self
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| token.kind() != SyntaxKind::NEWLINE);

        // Skip the first token if it's a line prefix (SPACE, MINUS, or PLUS)
        let mut iter = tokens.peekable();
        if let Some(first) = iter.peek() {
            if matches!(
                first.kind(),
                SyntaxKind::SPACE | SyntaxKind::MINUS | SyntaxKind::PLUS
            ) {
                iter.next(); // Skip the prefix
            }
        }

        let remaining: Vec<_> = iter.collect();
        if remaining.is_empty() {
            None
        } else {
            // Concatenate all tokens to form the line content
            Some(remaining.iter().map(|t| t.text()).collect::<String>())
        }
    }
}

// Context diff methods
impl ContextDiffFile {
    /// Get the old file header
    pub fn old_file(&self) -> Option<ContextOldFile> {
        self.syntax().children().find_map(ContextOldFile::cast)
    }

    /// Get the new file header
    pub fn new_file(&self) -> Option<ContextNewFile> {
        self.syntax().children().find_map(ContextNewFile::cast)
    }

    /// Get all hunks in this context diff file
    pub fn hunks(&self) -> impl Iterator<Item = ContextHunk> {
        self.syntax().children().filter_map(ContextHunk::cast)
    }
}

impl ContextOldFile {
    /// Get the file path token
    pub fn path(&self) -> Option<SyntaxToken<Lang>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::PATH)
    }
}

impl ContextNewFile {
    /// Get the file path token
    pub fn path(&self) -> Option<SyntaxToken<Lang>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::PATH)
    }
}

impl ContextHunk {
    /// Get the hunk header
    pub fn header(&self) -> Option<ContextHunkHeader> {
        self.syntax().children().find_map(ContextHunkHeader::cast)
    }

    /// Get the old section
    pub fn old_section(&self) -> Option<ContextOldSection> {
        self.syntax().children().find_map(ContextOldSection::cast)
    }

    /// Get the new section
    pub fn new_section(&self) -> Option<ContextNewSection> {
        self.syntax().children().find_map(ContextNewSection::cast)
    }
}

// Ed diff methods
impl EdCommand {
    /// Try to cast as an add command
    pub fn as_add(&self) -> Option<EdAddCommand> {
        self.syntax().children().find_map(EdAddCommand::cast)
    }

    /// Try to cast as a delete command
    pub fn as_delete(&self) -> Option<EdDeleteCommand> {
        self.syntax().children().find_map(EdDeleteCommand::cast)
    }

    /// Try to cast as a change command
    pub fn as_change(&self) -> Option<EdChangeCommand> {
        self.syntax().children().find_map(EdChangeCommand::cast)
    }
}

impl EdAddCommand {
    /// Get the line numbers
    pub fn line_numbers(&self) -> (Option<u32>, Option<u32>) {
        let numbers: Vec<_> = self
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| token.kind() == SyntaxKind::NUMBER)
            .filter_map(|token| token.text().parse().ok())
            .collect();

        (numbers.get(0).copied(), numbers.get(1).copied())
    }

    /// Get content lines
    pub fn content_lines(&self) -> impl Iterator<Item = EdContentLine> {
        self.syntax().children().filter_map(EdContentLine::cast)
    }
}

impl EdDeleteCommand {
    /// Get the line numbers
    pub fn line_numbers(&self) -> (Option<u32>, Option<u32>) {
        let numbers: Vec<_> = self
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| token.kind() == SyntaxKind::NUMBER)
            .filter_map(|token| token.text().parse().ok())
            .collect();

        (numbers.get(0).copied(), numbers.get(1).copied())
    }
}

impl EdChangeCommand {
    /// Get the line numbers
    pub fn line_numbers(&self) -> (Option<u32>, Option<u32>) {
        let numbers: Vec<_> = self
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| token.kind() == SyntaxKind::NUMBER)
            .filter_map(|token| token.text().parse().ok())
            .collect();

        (numbers.get(0).copied(), numbers.get(1).copied())
    }

    /// Get content lines
    pub fn content_lines(&self) -> impl Iterator<Item = EdContentLine> {
        self.syntax().children().filter_map(EdContentLine::cast)
    }
}

impl EdContentLine {
    /// Get the text content of this line
    pub fn text(&self) -> Option<String> {
        let tokens: Vec<_> = self
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| token.kind() != SyntaxKind::NEWLINE)
            .collect();

        if tokens.is_empty() {
            None
        } else {
            Some(tokens.iter().map(|t| t.text()).collect::<String>())
        }
    }
}

// Normal diff methods
impl NormalHunk {
    /// Get the change command
    pub fn command(&self) -> Option<NormalChangeCommand> {
        self.syntax().children().find_map(NormalChangeCommand::cast)
    }

    /// Get old lines section
    pub fn old_lines(&self) -> Option<NormalOldLines> {
        self.syntax().children().find_map(NormalOldLines::cast)
    }

    /// Get new lines section
    pub fn new_lines(&self) -> Option<NormalNewLines> {
        self.syntax().children().find_map(NormalNewLines::cast)
    }
}

/// Parse a patch file from text
pub fn parse(text: &str) -> crate::parse::Parse<Patch> {
    let tokens = super::lex::lex(text);
    let parser = super::parse::Parser::new(tokens);
    parser.parse()
}
