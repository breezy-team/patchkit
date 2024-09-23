#![deny(missing_docs)]
//! A crate for parsing and manipulating patches.
//!
//! # Examples
//!
//! ```
//! use patchkit::parse::parse_patch;
//! use patchkit::patch::{Patch as _, UnifiedPatch, Hunk, HunkLine};
//!
//! let patch = UnifiedPatch::parse_patch(vec![
//!     "--- a/file1\n",
//!     "+++ b/file1\n",
//!     "@@ -1,1 +1,1 @@\n",
//!     "-a\n",
//!     "+b\n",
//! ].into_iter().map(|s| s.as_bytes())).unwrap();
//!
//! assert_eq!(patch, UnifiedPatch {
//!     orig_name: b"a/file1".to_vec(),
//!     mod_name: b"b/file1".to_vec(),
//!     orig_ts: None,
//!     mod_ts: None,
//!     hunks: vec![
//!         Hunk {
//!             mod_pos: 1,
//!             mod_range: 1,
//!             orig_pos: 1,
//!             orig_range: 1,
//!             lines: vec![
//!                 HunkLine::RemoveLine(b"a\n".to_vec()),
//!                 HunkLine::InsertLine(b"b\n".to_vec()),
//!             ],
//!             tail: None
//!         },
//!     ],
//! });
//!
//! let applied = patch.apply_exact(&b"a\n"[..]).unwrap();
//! assert_eq!(applied, b"b\n");
//! ```

pub mod ed;
pub mod parse;
pub mod patch;
pub mod quilt;
pub mod timestamp;

/// Strip the specified number of path components from the beginning of the path.
pub fn strip_prefix(path: &std::path::Path, prefix: usize) -> &std::path::Path {
    let mut components = path.components();
    for _ in 0..prefix {
        components.next();
    }
    std::path::Path::new(components.as_path())
}

#[test]
fn test_strip_prefix() {
    assert_eq!(
        std::path::PathBuf::from("b"),
        strip_prefix(std::path::Path::new("a/b"), 1)
    );
    assert_eq!(
        std::path::PathBuf::from("a/b"),
        strip_prefix(std::path::Path::new("a/b"), 0)
    );
    assert_eq!(
        std::path::PathBuf::from(""),
        strip_prefix(std::path::Path::new("a/b"), 2)
    );
}
