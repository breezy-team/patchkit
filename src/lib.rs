pub mod quilt;
pub mod timestamp;
pub mod patch;
pub mod parse;

// TODO: Return a Path instead of a PathBuf
pub fn strip_prefix(path: &std::path::Path, prefix: usize) -> std::path::PathBuf {
    path.components().skip(prefix).collect()
}

#[test]
fn test_strip_prefix() {
    assert_eq!(std::path::PathBuf::from("b"), strip_prefix(std::path::Path::new("a/b"), 1));
    assert_eq!(std::path::PathBuf::from("a/b"), strip_prefix(std::path::Path::new("a/b"), 0));
    assert_eq!(std::path::PathBuf::from(""), strip_prefix(std::path::Path::new("a/b"), 2));
}
