#[cfg(test)]
mod tests {
    use crate::edit;

    #[test]
    fn test_parse_empty() {
        let parsed = edit::parse("");
        assert!(parsed.ok());
        let patch = parsed.tree();
        assert_eq!(patch.patch_files().count(), 0);
    }

    #[test]
    fn test_parse_simple_patch() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let mut files = patch.patch_files();
        let file = files.next().unwrap();
        assert!(files.next().is_none());

        // Check file headers
        let old_file = file.old_file().unwrap();
        assert_eq!(old_file.path().unwrap().text(), "a/file.txt");

        let new_file = file.new_file().unwrap();
        assert_eq!(new_file.path().unwrap().text(), "b/file.txt");

        // Check hunk
        let mut hunks = file.hunks();
        let hunk = hunks.next().unwrap();
        assert!(hunks.next().is_none());

        let header = hunk.header().unwrap();
        let old_range = header.old_range().unwrap();
        assert_eq!(old_range.start(), Some(1));
        assert_eq!(old_range.count(), Some(3));

        let new_range = header.new_range().unwrap();
        assert_eq!(new_range.start(), Some(1));
        assert_eq!(new_range.count(), Some(3));

        // Check lines
        let lines: Vec<_> = hunk.lines().collect();
        assert_eq!(lines.len(), 4);

        assert!(lines[0].as_context().is_some());
        assert_eq!(lines[0].text().unwrap(), "line 1");

        assert!(lines[1].as_delete().is_some());
        assert_eq!(lines[1].text().unwrap(), "line 2");

        assert!(lines[2].as_add().is_some());
        assert_eq!(lines[2].text().unwrap(), "line 2 modified");

        assert!(lines[3].as_context().is_some());
        assert_eq!(lines[3].text().unwrap(), "line 3");
    }

    #[test]
    fn test_parse_multiple_files() {
        let input = r#"--- a/file1.txt
+++ b/file1.txt
@@ -1,1 +1,1 @@
-old
+new
--- a/file2.txt
+++ b/file2.txt
@@ -1,1 +1,1 @@
-foo
+bar
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 2);

        assert_eq!(
            files[0].old_file().unwrap().path().unwrap().text(),
            "a/file1.txt"
        );
        assert_eq!(
            files[1].old_file().unwrap().path().unwrap().text(),
            "a/file2.txt"
        );
    }

    #[test]
    fn test_parse_malformed_header() {
        // Missing +++ line
        let input = r#"--- a/file.txt
@@ -1,1 +1,1 @@
-old
+new
"#;
        let parsed = edit::parse(input);
        // Should still parse what it can
        assert!(parsed.ok());
        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 1);

        // Should have old file but no new file
        assert!(files[0].old_file().is_some());
        assert!(files[0].new_file().is_none());
    }

    #[test]
    fn test_parse_with_junk_before() {
        let input = r#"Some random text
that should be ignored
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
        assert_eq!(
            files[0].old_file().unwrap().path().unwrap().text(),
            "a/file.txt"
        );
    }

    #[test]
    fn test_parse_incomplete_hunk() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,2 @@
 line 1
-line 2
"#;
        // Missing expected lines but should still parse
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        // Should have parsed the lines that are there
        assert_eq!(lines.len(), 2);
        assert!(lines[0].as_context().is_some());
        assert!(lines[1].as_delete().is_some());
    }

    #[test]
    fn test_parse_no_newline_at_eof() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,1 +1,1 @@
-old
+new
\ No newline at end of file
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        assert_eq!(lines.len(), 3);
        // The "\ No newline at end of file" should be parsed as text
        assert_eq!(lines[2].text().unwrap(), "\\ No newline at end of file");
    }

    #[test]
    fn test_partial_parsing_recovery() {
        // Test that parser can recover from errors and continue
        let input = r#"--- a/file1.txt
+++ b/file1.txt
@@ INVALID HUNK HEADER
 some content
--- a/file2.txt
+++ b/file2.txt
@@ -1,1 +1,1 @@
-valid
+content
"#;
        let parsed = edit::parse(input);
        // Even with invalid content, parsing should continue
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();

        // Should have parsed both files despite the error in the first
        assert_eq!(files.len(), 2);

        // Second file should be parsed correctly
        let second_file = &files[1];
        assert_eq!(
            second_file.old_file().unwrap().path().unwrap().text(),
            "a/file2.txt"
        );
        let hunk = second_file.hunks().next().unwrap();
        assert!(hunk.header().is_some());
    }

    #[test]
    fn test_lossless_roundtrip() {
        let input = r#"--- a/file.txt	2023-01-01 00:00:00
+++ b/file.txt	2023-01-02 00:00:00
@@ -1,3 +1,3 @@ function context
 line 1
-line 2
+line 2 modified
 line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        // Get the syntax node and convert back to text
        let syntax = parsed.syntax_node();
        let output = syntax.text().to_string();

        // Should preserve the original input exactly
        assert_eq!(input, output);
    }
}
