#[cfg(test)]
mod tests {
    use crate::edit;

    #[test]
    fn test_unicode_content() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 Hello 世界
-Rust 🦀 is great
+Rust 🦀 is awesome 🎉
 Unicode works: αβγδ
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        assert_eq!(lines[0].text().unwrap(), "Hello 世界");
        assert_eq!(lines[1].text().unwrap(), "Rust 🦀 is great");
        assert_eq!(lines[2].text().unwrap(), "Rust 🦀 is awesome 🎉");
        assert_eq!(lines[3].text().unwrap(), "Unicode works: αβγδ");
    }

    #[test]
    fn test_tabs_in_content() {
        let input = "--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@\n \tindented\twith\ttabs\n-\told\tcontent\n+\tnew\tcontent\n";
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        assert_eq!(lines[0].text().unwrap(), "\tindented\twith\ttabs");
        assert_eq!(lines[1].text().unwrap(), "\told\tcontent");
        assert_eq!(lines[2].text().unwrap(), "\tnew\tcontent");
    }

    #[test]
    fn test_empty_lines_in_hunk() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,5 +1,5 @@
 line 1
 
-line 3
+line 3 modified
 
 line 5
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0].text().unwrap(), "line 1");
        assert_eq!(lines[1].text(), None); // Empty line
        assert_eq!(lines[2].text().unwrap(), "line 3");
        assert_eq!(lines[3].text().unwrap(), "line 3 modified");
        assert_eq!(lines[4].text(), None); // Empty line
        assert_eq!(lines[5].text().unwrap(), "line 5");
    }

    #[test]
    fn test_special_characters_in_paths() {
        // Note: The current parser stops at whitespace, so paths with spaces
        // need to be handled differently in real usage
        let input = r#"--- a/file-with-dashes.txt
+++ b/file-with-dashes.txt
@@ -1,1 +1,1 @@
-old
+new
--- a/special!@#$%^&*()chars.txt
+++ b/special!@#$%^&*()chars.txt
@@ -1,1 +1,1 @@
-content
+changed
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 2);

        // First file with dashes
        let path_token = files[0].old_file().unwrap().path().unwrap();
        let path = path_token.text();
        println!("First file path: '{}'", path);
        assert!(path.contains("a/file"));

        // Second file with special characters (but no spaces)
        let special_path = files[1].old_file().unwrap().path().unwrap();
        let special_text = special_path.text();
        // The parser tokenizes some special chars separately
        assert!(special_text.contains("special"));
    }

    #[test]
    fn test_git_extended_headers() {
        let input = r#"diff --git a/file.txt b/file.txt
index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,1 +1,1 @@
-old
+new
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 1);

        // Should still parse the core patch content
        let file = &files[0];
        assert!(file.old_file().is_some());
        assert!(file.new_file().is_some());

        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_multiple_hunks_per_file() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
 line 1
-line 2
+line 2 modified
@@ -10,3 +10,4 @@
 line 10
 line 11
+line 11.5 added
 line 12
@@ -20,1 +21,1 @@
-line 20
+line 20 changed
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunks: Vec<_> = file.hunks().collect();

        assert_eq!(hunks.len(), 3);

        // Verify each hunk's range
        assert_eq!(
            hunks[0].header().unwrap().old_range().unwrap().start(),
            Some(1)
        );
        assert_eq!(
            hunks[1].header().unwrap().old_range().unwrap().start(),
            Some(10)
        );
        assert_eq!(
            hunks[2].header().unwrap().old_range().unwrap().start(),
            Some(20)
        );
    }

    #[test]
    fn test_no_trailing_newline() {
        let input = "--- a/file.txt\n+++ b/file.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        // Note: no trailing newline after "+new"
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text().unwrap(), "old");
        assert_eq!(lines[1].text().unwrap(), "new");
    }

    #[test]
    fn test_extremely_long_lines() {
        let long_content = "x".repeat(1000);
        let input = format!(
            "--- a/file.txt\n+++ b/file.txt\n@@ -1,1 +1,1 @@\n-{}\n+{}modified\n",
            long_content, long_content
        );

        let parsed = edit::parse(&input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        assert_eq!(lines[0].text().unwrap(), long_content);
        assert_eq!(
            lines[1].text().unwrap(),
            format!("{}modified", long_content)
        );
    }
}
