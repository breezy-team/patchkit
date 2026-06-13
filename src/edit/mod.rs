use rowan::{ast::AstNode, GreenNode, NodeOrToken, SyntaxNode, TextSize};
use std::fmt;
use std::marker::PhantomData;

/// Lexer for patch files
pub mod lex;
/// Lossless AST structures for patch files
pub mod lossless;
mod parse;
/// Lossless parser and editor for quilt series files
pub mod series;

pub use lossless::{
    // Common types
    AddLine,
    ContextChangeLine,

    // Context diff types
    ContextDiffFile,
    ContextHunk,
    ContextHunkHeader,
    ContextLine,
    ContextNewFile,
    ContextNewSection,
    ContextOldFile,
    ContextOldSection,
    DeleteLine,
    DiffFormat,

    EdAddCommand,
    EdChangeCommand,
    // Ed diff types
    EdCommand,
    EdContentLine,

    EdDeleteCommand,
    // Unified diff types
    FileHeader,
    Hunk,
    HunkCountMismatch,
    HunkHeader,
    HunkLine,
    HunkRange,
    HunkSide,
    HunkStats,
    Lang,
    NewFile,
    NoNewlineLine,
    NormalChangeCommand,
    // Normal diff types
    NormalHunk,
    NormalNewLines,
    NormalOldLines,
    NormalSeparator,
    OldFile,
    Patch,
    PatchFile,
};
pub use rowan::TextRange;

/// Parse error containing a list of error messages
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseError(pub Vec<String>);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", err)?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

/// Parse warning containing a list of warning messages
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseWarning(pub Vec<String>);

impl fmt::Display for ParseWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", err)?;
        }
        Ok(())
    }
}

/// Parse error with position information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionedParseError {
    /// The error message
    pub message: String,
    /// The position in the source text where the error occurred
    pub position: rowan::TextRange,
}

/// Parse warning with position information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionedParseWarning {
    /// The warning message
    pub message: String,
    /// The position in the source text where the warning occurred
    pub position: rowan::TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Parse result containing a syntax tree and any parse errors
pub struct Parse<T> {
    green: GreenNode,
    errors: Vec<String>,
    positioned_errors: Vec<PositionedParseError>,
    warnings: Vec<String>,
    positioned_warnings: Vec<PositionedParseWarning>,
    _ty: PhantomData<T>,
}

// SAFETY: Parse<T> only contains a GreenNode (which is Arc-based and thread-safe),
// Vec<String>, and PhantomData. The PhantomData<T> does not actually hold a T.
unsafe impl<T> Send for Parse<T> {}
unsafe impl<T> Sync for Parse<T> {}

impl<T> Parse<T> {
    /// Create a new parse result
    pub fn new(green: GreenNode, errors: Vec<String>, warnings: Vec<String>) -> Self {
        Parse {
            green,
            errors,
            positioned_errors: Vec::new(),
            warnings,
            positioned_warnings: Vec::new(),
            _ty: PhantomData,
        }
    }

    /// Create a new parse result with positioned errors
    pub fn new_with_positioned_errors(
        green: GreenNode,
        errors: Vec<String>,
        positioned_errors: Vec<PositionedParseError>,
        warnings: Vec<String>,
        positioned_warnings: Vec<PositionedParseWarning>,
    ) -> Self {
        Parse {
            green,
            errors,
            positioned_errors,
            warnings,
            positioned_warnings,
            _ty: PhantomData,
        }
    }

    /// Get the green node (thread-safe representation)
    pub fn green(&self) -> &GreenNode {
        &self.green
    }

    /// Get the syntax errors
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Get parse errors with position information
    pub fn positioned_errors(&self) -> &[PositionedParseError] {
        &self.positioned_errors
    }

    /// Get the syntax warnings
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Get parse warnings with position information
    pub fn positioned_warnings(&self) -> &[PositionedParseWarning] {
        &self.positioned_warnings
    }

    /// Get parse errors as strings
    pub fn error_messages(&self) -> Vec<String> {
        self.positioned_errors
            .iter()
            .map(|e| e.message.clone())
            .collect()
    }

    /// Get parse warnings as strings
    pub fn warning_messages(&self) -> Vec<String> {
        self.positioned_warnings
            .iter()
            .map(|e| e.message.clone())
            .collect()
    }

    /// Check if parsing succeeded without errors
    pub fn ok(&self) -> bool {
        self.errors.is_empty() && self.positioned_errors.is_empty()
    }

    /// Convert to a Result, returning the tree if there are no errors
    pub fn to_result(self) -> Result<T, ParseError>
    where
        T: AstNode,
    {
        if self.errors.is_empty() && self.positioned_errors.is_empty() {
            let node = SyntaxNode::<T::Language>::new_root(self.green);
            Ok(T::cast(node).expect("root node has wrong type"))
        } else {
            let mut all_errors = self.errors.clone();
            all_errors.extend(self.error_messages());
            Err(ParseError(all_errors))
        }
    }

    /// Get the parsed syntax tree.
    ///
    /// Returns the tree even if there are parse errors, enabling
    /// error-resilient tooling that works with partial/invalid input.
    /// Use `errors()` or `positioned_errors()` to check for errors.
    pub fn tree(&self) -> T
    where
        T: AstNode,
    {
        let node = SyntaxNode::<T::Language>::new_root_mut(self.green.clone());
        T::cast(node).expect("root node has wrong type")
    }

    /// Get the syntax node
    pub fn syntax_node(&self) -> SyntaxNode<T::Language>
    where
        T: AstNode,
    {
        SyntaxNode::<T::Language>::new_root(self.green.clone())
    }

    /// Cast this parse result to a different AST node type
    pub fn cast<U>(self) -> Option<Parse<U>>
    where
        T: AstNode,
        U: AstNode<Language = T::Language>,
    {
        let node = SyntaxNode::<T::Language>::new_root(self.green.clone());
        U::cast(node)?;
        Some(Parse {
            green: self.green,
            errors: self.errors,
            positioned_errors: self.positioned_errors,
            warnings: self.warnings,
            positioned_warnings: self.positioned_warnings,
            _ty: PhantomData,
        })
    }

    /// Incrementally reparse after a text edit.
    ///
    /// Given the new full text and the range that was edited (in the *new* text
    /// coordinates after the edit has been applied), this tries to reuse
    /// unchanged children from the previous parse and only reparse the
    /// affected region.
    ///
    /// The `full_parse` function is called to parse text regions; it should
    /// be the same function used for the initial parse.
    ///
    /// Falls back to a full reparse if the edit spans the entire file or if
    /// incremental reparsing is not beneficial.
    pub fn reparse(
        &self,
        new_text: &str,
        edit: rowan::TextRange,
        full_parse: impl Fn(&str) -> Parse<T>,
    ) -> Self {
        // Collect children with their text ranges
        let mut children: Vec<(
            NodeOrToken<&rowan::GreenNodeData, &rowan::GreenTokenData>,
            TextSize,
            TextSize,
        )> = Vec::new();
        let mut offset = TextSize::from(0);
        for child in self.green.children() {
            let len = match &child {
                NodeOrToken::Node(n) => n.text_len(),
                NodeOrToken::Token(t) => t.text_len(),
            };
            children.push((child, offset, offset + len));
            offset += len;
        }

        let old_len = offset;

        // If there are very few children, just do a full reparse
        if children.len() <= 2 {
            return full_parse(new_text);
        }

        let new_len = TextSize::of(new_text);
        let len_delta: i64 = i64::from(u32::from(new_len)) - i64::from(u32::from(old_len));

        // In old-text coordinates, the edit covered:
        let old_edit_start = edit.start();
        let old_edit_end = TextSize::from((i64::from(u32::from(edit.end())) - len_delta) as u32);

        // Find first and last affected child indices
        let first_affected = children
            .iter()
            .position(|(_, _, end)| *end >= old_edit_start);
        let last_affected = children
            .iter()
            .rposition(|(_, start, _)| *start <= old_edit_end);

        let (first_affected, last_affected) = match (first_affected, last_affected) {
            (Some(f), Some(l)) => (f, l),
            _ => return full_parse(new_text),
        };

        let reparse_start = children[first_affected].1;
        let reparse_old_end = children[last_affected].2;

        // In new-text coordinates, the end of the affected region is shifted
        let reparse_new_end =
            TextSize::from((i64::from(u32::from(reparse_old_end)) + len_delta) as u32);

        // Bounds check
        if u32::from(reparse_start) > u32::from(new_len)
            || u32::from(reparse_new_end) > u32::from(new_len)
        {
            return full_parse(new_text);
        }

        let reparse_slice = &new_text[usize::from(reparse_start)..usize::from(reparse_new_end)];

        // Parse just the affected region
        let reparsed = full_parse(reparse_slice);

        // Build new root by splicing: prefix children + reparsed children + suffix children
        let to_owned =
            |c: &NodeOrToken<&rowan::GreenNodeData, &rowan::GreenTokenData>| -> NodeOrToken<GreenNode, rowan::GreenToken> {
                match c {
                    NodeOrToken::Node(n) => NodeOrToken::Node((*n).to_owned()),
                    NodeOrToken::Token(t) => NodeOrToken::Token((*t).to_owned()),
                }
            };

        let mut new_children = Vec::new();
        for (c, _, _) in &children[..first_affected] {
            new_children.push(to_owned(c));
        }
        for c in reparsed.green.children() {
            new_children.push(c.to_owned());
        }
        for (c, _, _) in &children[last_affected + 1..] {
            new_children.push(to_owned(c));
        }

        let root_kind = self
            .green
            .children()
            .next()
            .map(|_| self.green.kind())
            .unwrap_or(self.green.kind());
        let new_green = GreenNode::new(root_kind, new_children);

        // Offset-shift positioned errors from the reparsed region
        let positioned_errors: Vec<_> = reparsed
            .positioned_errors
            .iter()
            .map(|e| PositionedParseError {
                message: e.message.clone(),
                position: rowan::TextRange::new(
                    e.position.start() + reparse_start,
                    e.position.end() + reparse_start,
                ),
            })
            .collect();

        let errors: Vec<_> = positioned_errors
            .iter()
            .map(|e| e.message.clone())
            .collect();

        // Offset-shift positioned warnings from the reparsed region
        let positioned_warnings: Vec<_> = reparsed
            .positioned_warnings
            .iter()
            .map(|e| PositionedParseWarning {
                message: e.message.clone(),
                position: rowan::TextRange::new(
                    e.position.start() + reparse_start,
                    e.position.end() + reparse_start,
                ),
            })
            .collect();

        let warnings: Vec<_> = positioned_warnings
            .iter()
            .map(|e| e.message.clone())
            .collect();

        Parse::new_with_positioned_errors(
            new_green,
            errors,
            positioned_errors,
            warnings,
            positioned_warnings,
        )
    }
}

/// Parse a patch file into a lossless AST
pub fn parse(text: &str) -> Parse<Patch> {
    lossless::parse(text)
}

#[cfg(test)]
#[path = "reparse_tests.rs"]
mod reparse_tests;
