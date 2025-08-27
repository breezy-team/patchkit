//! Comprehensive tests for the quilt series lossless parser and editor

use crate::edit::quilt::{self, SeriesFile};
use rowan::ast::AstNode;

#[test]
fn test_parse_empty_file() {
    let parsed = quilt::parse("");
    assert!(parsed.errors().is_empty());
    let series = parsed.quilt_tree();
    assert_eq!(series.entries().count(), 0);
}

#[test]
fn test_parse_whitespace_only() {
    let parsed = quilt::parse("   \n\t\n  \n");
    assert!(parsed.errors().is_empty());
    let series = parsed.quilt_tree();
    assert_eq!(series.patch_entries().count(), 0);
}

#[test]
fn test_parse_comments_only() {
    let text = "# First comment\n# Second comment\n";
    let parsed = quilt::parse(text);
    assert!(parsed.errors().is_empty());
    let series = parsed.quilt_tree();

    let comments: Vec<_> = series.comment_lines().collect();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].text(), "First comment");
    assert_eq!(comments[1].text(), "Second comment");
}

#[test]
fn test_parse_patches_with_various_options() {
    let text = "patch1.patch\npatch2.patch -p1\npatch3.patch -p2 --reverse --fuzz=3\n";
    let parsed = quilt::parse(text);
    assert!(parsed.errors().is_empty());
    let series = parsed.quilt_tree();

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches.len(), 3);

    assert_eq!(patches[0].name(), Some("patch1.patch".to_string()));
    assert_eq!(patches[0].option_strings(), Vec::<String>::new());

    assert_eq!(patches[1].name(), Some("patch2.patch".to_string()));
    assert_eq!(patches[1].option_strings(), vec!["-p1"]);

    assert_eq!(patches[2].name(), Some("patch3.patch".to_string()));
    assert_eq!(
        patches[2].option_strings(),
        vec!["-p2", "--reverse", "--fuzz=3"]
    );
}

#[test]
fn test_parse_mixed_content() {
    let text = r#"# Debian patch series
# First set of patches
001-fix-build.patch -p1
002-add-feature.patch

# Security fixes
CVE-2023-1234.patch --reverse
CVE-2023-5678.patch -p2 --fuzz=2

# Backports
backport-upstream-fix.patch
"#;
    let parsed = quilt::parse(text);
    assert!(parsed.errors().is_empty());
    let series = parsed.quilt_tree();

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches.len(), 5);

    let comments: Vec<_> = series.comment_lines().collect();
    assert_eq!(comments.len(), 4);
}

#[test]
fn test_preserve_formatting() {
    let text = "patch1.patch  \t-p1   \t--reverse\n# Comment with   spaces\npatch2.patch\n";
    let parsed = quilt::parse(text);
    let series = parsed.quilt_tree();
    assert_eq!(series.syntax().to_string(), text);
}

#[test]
fn test_thread_safety() {
    let text = "patch1.patch\n";
    let parsed = quilt::parse(text);

    // Test that we can clone the green node
    let green1 = parsed.green().clone();
    let green2 = parsed.green().clone();

    // Test that we can create multiple roots from the same green node
    let root1 = SeriesFile::new_root(green1);
    let root2 = SeriesFile::new_root(green2);

    assert_eq!(root1.syntax().to_string(), root2.syntax().to_string());
}

#[test]
fn test_edit_insert_first() {
    let text = "patch2.patch\n";
    let parsed = quilt::parse(text);
    let mut series = parsed.quilt_tree_mut();

    series.insert(0, "patch1.patch", Vec::<&str>::new());
    let patches: Vec<_> = series.patch_entries().collect();

    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0].name(), Some("patch1.patch".to_string()));
    assert_eq!(patches[1].name(), Some("patch2.patch".to_string()));
}

#[test]
fn test_edit_insert_last() {
    let text = "patch1.patch\n";
    let parsed = quilt::parse(text);
    let mut series = parsed.quilt_tree_mut();

    series.insert(1, "patch2.patch", vec!["-p1".to_string()]);
    let patches: Vec<_> = series.patch_entries().collect();

    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0].name(), Some("patch1.patch".to_string()));
    assert_eq!(patches[1].name(), Some("patch2.patch".to_string()));
    assert_eq!(patches[1].option_strings(), vec!["-p1"]);
}

#[test]
fn test_edit_remove_preserves_comments() {
    let text = "# Header\npatch1.patch\n# Middle\npatch2.patch\n# Footer\n";
    let parsed = quilt::parse(text);
    let mut series = parsed.quilt_tree_mut();

    assert!(series.remove("patch1.patch"));

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].name(), Some("patch2.patch".to_string()));

    let comments: Vec<_> = series.comment_lines().collect();
    assert_eq!(comments.len(), 3);
}

#[test]
fn test_edit_update_options() {
    let text = "patch1.patch -p1\n";
    let parsed = quilt::parse(text);
    let mut series = parsed.quilt_tree_mut();

    assert!(series.set_options(
        "patch1.patch",
        vec!["-p2".to_string(), "--fuzz=3".to_string()],
    ));

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches[0].option_strings(), vec!["-p2", "--fuzz=3"]);
}

#[test]
fn test_edit_chain_operations() {
    let text = "patch1.patch\n";
    let parsed = quilt::parse(text);
    let mut series = parsed.quilt_tree_mut();

    series.insert(1, "patch2.patch", vec!["-p1".to_string()]);
    series.add_comment("Added patch2");
    series.insert(0, "patch0.patch", Vec::<&str>::new());

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches.len(), 3);
    assert_eq!(patches[0].name(), Some("patch0.patch".to_string()));
    assert_eq!(patches[1].name(), Some("patch1.patch".to_string()));
    assert_eq!(patches[2].name(), Some("patch2.patch".to_string()));

    let comments: Vec<_> = series.comment_lines().collect();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text(), "Added patch2");
}

#[test]
fn test_error_recovery() {
    // Even with malformed input, we should get a best-effort parse
    let text = "patch1.patch\n\n  \npatch2.patch -p1\n";
    let parsed = quilt::parse(text);
    let series = parsed.quilt_tree();

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches.len(), 2);
}

#[test]
fn test_special_patch_names() {
    let text = "debian/patches/fix-build.patch -p1\n../other/patch.diff\nCVE-2023-1234.patch\n";
    let parsed = quilt::parse(text);
    assert!(parsed.errors().is_empty());
    let series = parsed.quilt_tree();

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches.len(), 3);
    assert_eq!(
        patches[0].name(),
        Some("debian/patches/fix-build.patch".to_string())
    );
    assert_eq!(patches[1].name(), Some("../other/patch.diff".to_string()));
    assert_eq!(patches[2].name(), Some("CVE-2023-1234.patch".to_string()));
}

#[test]
fn test_builder_comprehensive() {
    let series = quilt::SeriesBuilder::new()
        .add_comment("Debian patch series for package foo")
        .add_comment("")
        .add_patch("debian/patches/01-fix-build.patch", vec!["-p1".to_string()])
        .add_patch("debian/patches/02-add-feature.patch", vec![])
        .add_comment("Security fixes")
        .add_patch(
            "debian/patches/CVE-2023-1234.patch",
            vec!["-p2".to_string(), "--fuzz=3".to_string()],
        )
        .build();

    let patches: Vec<_> = series.patch_entries().collect();
    assert_eq!(patches.len(), 3);

    let comments: Vec<_> = series.comment_lines().collect();
    assert_eq!(comments.len(), 3);
}
