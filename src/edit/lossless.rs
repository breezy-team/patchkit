//! Lossless AST structures for patch files
use crate::edit::lex::SyntaxKind;
use rowan::{ast::AstNode, SyntaxNode, SyntaxToken};

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

pub use super::{ParseError, PositionedParseError};

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

    /// Get the old file path.
    pub fn old_path(&self) -> Option<String> {
        self.old_file()
            .and_then(|f| f.path())
            .map(|t| t.text().to_string())
    }

    /// Get the new file path.
    pub fn new_path(&self) -> Option<String> {
        self.new_file()
            .and_then(|f| f.path())
            .map(|t| t.text().to_string())
    }

    /// Get the file path, preferring the new file name.
    pub fn path(&self) -> Option<String> {
        self.new_path().or_else(|| self.old_path())
    }

    /// Get a display name for this file diff.
    ///
    /// Shows "old → new" if the paths differ, otherwise just the path.
    pub fn display_name(&self) -> String {
        match (self.old_path(), self.new_path()) {
            (Some(o), Some(n)) if o == n => o,
            (Some(o), Some(n)) => format!("{o} → {n}"),
            (Some(o), None) => o,
            (None, Some(n)) => n,
            (None, None) => "<unknown>".to_string(),
        }
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

/// Line count statistics for a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunkStats {
    /// Number of context (unchanged) lines
    pub context: u32,
    /// Number of added lines
    pub additions: u32,
    /// Number of deleted lines
    pub deletions: u32,
}

impl Hunk {
    /// Get the hunk header
    pub fn header(&self) -> Option<HunkHeader> {
        self.syntax().children().find_map(HunkHeader::cast)
    }

    /// Get all lines in this hunk
    pub fn lines(&self) -> impl Iterator<Item = HunkLine> {
        self.syntax()
            .children()
            .filter_map(|child| match child.kind() {
                SyntaxKind::CONTEXT_LINE | SyntaxKind::ADD_LINE | SyntaxKind::DELETE_LINE => {
                    Some(HunkLine { syntax: child })
                }
                _ => None,
            })
    }

    /// Fix the line counts in the hunk header to match the actual content.
    ///
    /// Returns `true` if any counts were changed.
    pub fn fix_counts(&self) -> bool {
        let Some(header) = self.header() else {
            return false;
        };
        let stats = self.stats();
        let mut changed = false;

        if let Some(old_range) = header.old_range() {
            let actual = stats.context + stats.deletions;
            if old_range.count() != Some(actual) {
                old_range.set_count(actual);
                changed = true;
            }
        }

        if let Some(new_range) = header.new_range() {
            let actual = stats.context + stats.additions;
            if new_range.count() != Some(actual) {
                new_range.set_count(actual);
                changed = true;
            }
        }

        changed
    }

    /// Count the lines in this hunk by type.
    pub fn stats(&self) -> HunkStats {
        let mut stats = HunkStats {
            context: 0,
            additions: 0,
            deletions: 0,
        };
        for line in self.lines() {
            match line.syntax().kind() {
                SyntaxKind::CONTEXT_LINE => stats.context += 1,
                SyntaxKind::ADD_LINE => stats.additions += 1,
                SyntaxKind::DELETE_LINE => stats.deletions += 1,
                _ => {}
            }
        }
        stats
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

/// Which side of a diff hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HunkSide {
    /// The old (original) side
    Old,
    /// The new (modified) side
    New,
}

impl std::fmt::Display for HunkSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HunkSide::Old => write!(f, "old"),
            HunkSide::New => write!(f, "new"),
        }
    }
}

/// A mismatch between a hunk header's declared line count and the actual count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkCountMismatch {
    /// Which side of the diff
    pub side: HunkSide,
    /// The count declared in the header
    pub expected: u32,
    /// The actual count of lines in the hunk
    pub actual: u32,
}

impl HunkHeader {
    /// Check whether the declared line counts match the actual hunk content.
    ///
    /// Returns a list of mismatches (empty if everything matches).
    /// Requires the parent `Hunk` node to count the lines.
    pub fn check_counts(&self, hunk: &Hunk) -> Vec<HunkCountMismatch> {
        let stats = hunk.stats();
        let mut mismatches = Vec::new();

        if let Some(old_range) = self.old_range() {
            let expected = old_range.count().unwrap_or(1);
            let actual = stats.context + stats.deletions;
            if expected != actual {
                mismatches.push(HunkCountMismatch {
                    side: HunkSide::Old,
                    expected,
                    actual,
                });
            }
        }

        if let Some(new_range) = self.new_range() {
            let expected = new_range.count().unwrap_or(1);
            let actual = stats.context + stats.additions;
            if expected != actual {
                mismatches.push(HunkCountMismatch {
                    side: HunkSide::New,
                    expected,
                    actual,
                });
            }
        }

        mismatches
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

    /// Set the line count, modifying the syntax tree in place.
    ///
    /// If the range already has a `,count` part, replaces the count token.
    /// If it only has a start number, inserts `,count` after it.
    pub fn set_count(&self, new_count: u32) {
        let count_str = new_count.to_string();

        // Build a replacement NUMBER token
        let new_token = Self::make_token(SyntaxKind::NUMBER, &count_str);

        // Find the second NUMBER token (the count)
        let mut number_indices = Vec::new();
        for (index, element) in self.syntax().children_with_tokens().enumerate() {
            if let rowan::NodeOrToken::Token(token) = element {
                if token.kind() == SyntaxKind::NUMBER {
                    number_indices.push(index);
                }
            }
        }

        if number_indices.len() >= 2 {
            // Replace the existing count token
            let idx = number_indices[1];
            self.syntax()
                .splice_children(idx..idx + 1, vec![rowan::NodeOrToken::Token(new_token)]);
        } else if !number_indices.is_empty() {
            // No count yet - insert comma + count after the start number
            let insert_at = number_indices[0] + 1;
            let comma_token = Self::make_token(SyntaxKind::COMMA, ",");
            self.syntax().splice_children(
                insert_at..insert_at,
                vec![
                    rowan::NodeOrToken::Token(comma_token),
                    rowan::NodeOrToken::Token(new_token),
                ],
            );
        }
    }

    fn make_token(kind: SyntaxKind, text: &str) -> rowan::SyntaxToken<Lang> {
        let mut builder = rowan::GreenNodeBuilder::new();
        builder.start_node(SyntaxKind::ROOT.into());
        builder.token(kind.into(), text);
        builder.finish_node();
        let green = builder.finish();
        let node = SyntaxNode::new_root_mut(green);
        node.first_token().unwrap()
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
pub fn parse(text: &str) -> super::Parse<Patch> {
    let tokens = super::lex::lex(text);
    let parser = super::parse::Parser::new(tokens);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_counts_correct() {
        let text = "--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n ctx\n-old\n+new\n ctx2\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let hunk = patch.patch_files().next().unwrap().hunks().next().unwrap();
        assert!(!hunk.fix_counts());
        assert_eq!(patch.syntax().to_string(), text);
    }

    #[test]
    fn test_fix_counts_wrong_old() {
        let text = "--- a/f\n+++ b/f\n@@ -1,99 +1,2 @@\n ctx\n-old\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let hunk = patch.patch_files().next().unwrap().hunks().next().unwrap();
        assert!(hunk.fix_counts());
        assert_eq!(
            patch.syntax().to_string(),
            "--- a/f\n+++ b/f\n@@ -1,2 +1,1 @@\n ctx\n-old\n"
        );
    }

    #[test]
    fn test_fix_counts_no_count_present() {
        let text = "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-old\n+new1\n+new2\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let hunk = patch.patch_files().next().unwrap().hunks().next().unwrap();
        assert!(hunk.fix_counts());
        assert_eq!(
            patch.syntax().to_string(),
            "--- a/f\n+++ b/f\n@@ -1,1 +1,2 @@\n-old\n+new1\n+new2\n"
        );
    }

    #[test]
    fn test_set_count_replace() {
        let text = "--- a/f\n+++ b/f\n@@ -1,5 +1,5 @@\n ctx\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let hunk = patch.patch_files().next().unwrap().hunks().next().unwrap();
        let header = hunk.header().unwrap();
        header.old_range().unwrap().set_count(42);
        assert_eq!(
            patch.syntax().to_string(),
            "--- a/f\n+++ b/f\n@@ -1,42 +1,5 @@\n ctx\n"
        );
    }

    #[test]
    fn test_hunk_stats() {
        let text = "--- a/f\n+++ b/f\n@@ -1,4 +1,5 @@\n ctx1\n ctx2\n-del\n+add1\n+add2\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let hunk = patch.patch_files().next().unwrap().hunks().next().unwrap();
        let stats = hunk.stats();
        assert_eq!(stats.context, 2);
        assert_eq!(stats.deletions, 1);
        assert_eq!(stats.additions, 2);
    }

    #[test]
    fn test_hunk_does_not_absorb_next_file_metadata() {
        // git diff puts `diff --git` and `index` lines between files. The
        // parser must not absorb them as context lines of the previous hunk
        // — that would inflate the hunk's line counts and produce spurious
        // hunk-line-count-mismatch diagnostics.
        let text = "\
diff --git a/f1 b/f1
index aaa..bbb 100644
--- a/f1
+++ b/f1
@@ -1,2 +1,3 @@
 ctx1
+add1
 ctx2
diff --git a/f2 b/f2
index ccc..ddd 100644
--- a/f2
+++ b/f2
@@ -1,1 +1,1 @@
-old
+new
";
        let parsed = parse(text);
        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 2);

        let hunk1 = files[0].hunks().next().unwrap();
        let stats1 = hunk1.stats();
        assert_eq!(stats1.context, 2);
        assert_eq!(stats1.additions, 1);
        assert_eq!(stats1.deletions, 0);
        assert_eq!(hunk1.header().unwrap().check_counts(&hunk1), vec![]);

        let hunk2 = files[1].hunks().next().unwrap();
        let stats2 = hunk2.stats();
        assert_eq!(stats2.context, 0);
        assert_eq!(stats2.additions, 1);
        assert_eq!(stats2.deletions, 1);
        assert_eq!(hunk2.header().unwrap().check_counts(&hunk2), vec![]);
    }

    #[test]
    fn test_hunk_does_not_absorb_trailing_blank_line() {
        // A bare newline at end of file (no leading space, +, or -) is not
        // a valid hunk body line. It must terminate the hunk rather than
        // be counted as an extra context line.
        let text = "\
--- a/f
+++ b/f
@@ -1,1 +1,2 @@
 ctx
+add

";
        let parsed = parse(text);
        let patch = parsed.tree();
        let hunk = patch.patch_files().next().unwrap().hunks().next().unwrap();
        let stats = hunk.stats();
        assert_eq!(stats.context, 1);
        assert_eq!(stats.additions, 1);
        assert_eq!(stats.deletions, 0);
        assert_eq!(hunk.header().unwrap().check_counts(&hunk), vec![]);
    }

    #[test]
    fn test_check_counts_mismatch() {
        let text = "--- a/f\n+++ b/f\n@@ -1,99 +1,99 @@\n ctx\n-old\n+new\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let hunk = patch.patch_files().next().unwrap().hunks().next().unwrap();
        let mismatches = hunk.header().unwrap().check_counts(&hunk);
        assert_eq!(mismatches.len(), 2);
        assert_eq!(mismatches[0].side, HunkSide::Old);
        assert_eq!(mismatches[0].expected, 99);
        assert_eq!(mismatches[0].actual, 2);
        assert_eq!(mismatches[1].side, HunkSide::New);
        assert_eq!(mismatches[1].expected, 99);
        assert_eq!(mismatches[1].actual, 2);
    }

    #[test]
    fn test_patch_file_display_name() {
        let text = "--- a/old.txt\n+++ b/new.txt\n@@ -1 +1 @@\n-a\n+b\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        assert_eq!(file.display_name(), "a/old.txt → b/new.txt");
    }

    #[test]
    fn test_patch_file_display_name_same() {
        let text = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-a\n+b\n";
        let parsed = parse(text);
        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        // Paths differ (a/file.txt vs b/file.txt), so shows arrow
        assert_eq!(file.display_name(), "a/file.txt → b/file.txt");
    }
}
