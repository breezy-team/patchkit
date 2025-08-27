/// Lexer for patch files
pub mod lex;
/// Lossless AST structures for patch files
pub mod lossless;
mod parse;
/// Lossless editor for quilt series files
pub mod quilt;

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
    HunkHeader,
    HunkLine,
    HunkRange,
    Lang,
    NewFile,
    NormalChangeCommand,
    // Normal diff types
    NormalHunk,
    NormalNewLines,
    NormalOldLines,
    NormalSeparator,
    OldFile,
    ParseError,
    Patch,
    PatchFile,

    PositionedParseError,
};
pub use rowan::TextRange;

/// Parse a patch file into a lossless AST
pub fn parse(text: &str) -> crate::parse::Parse<Patch> {
    lossless::parse(text)
}
