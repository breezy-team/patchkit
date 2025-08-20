#[cfg(test)]
mod tests {
    use crate::edit;

    #[test]
    fn test_multiple_malformed_sections() {
        let input = r#"garbage before patch
--- malformed header without proper path
+++ another bad header
@@ this is not a valid hunk header @@
some random content
--- a/good_file.txt
+++ b/good_file.txt
@@ -1,2 +1,2 @@
 good line
-old content
+new content
@@ malformed hunk in good file
more garbage
@@ -5,1 +5,1 @@
-another change
+that should work
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok()); // Should still return ok even with errors

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();

        // Should parse what it can
        assert!(files.len() >= 1);

        // The good file should be parsed
        let good_file = files.iter().find(|f| {
            f.old_file()
                .and_then(|old| old.path())
                .map(|p| p.text() == "a/good_file.txt")
                .unwrap_or(false)
        });
        assert!(good_file.is_some());

        // Should have at least one valid hunk
        let hunks: Vec<_> = good_file.unwrap().hunks().collect();
        assert!(hunks.len() >= 1);
    }

    #[test]
    fn test_incomplete_file_headers() {
        let input = r#"--- a/file1.txt
@@ -1,1 +1,1 @@
-content
+changed
--- a/file2.txt
+++ b/file2.txt
@@ -1,1 +1,1 @@
-more
+changes
+++ b/orphan_new_file.txt
@@ -0,0 +1,1 @@
+new file content
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();

        // Should parse all files even with missing headers
        assert_eq!(files.len(), 3);

        // First file has only old header
        assert!(files[0].old_file().is_some());
        assert!(files[0].new_file().is_none());

        // Second file has both headers
        assert!(files[1].old_file().is_some());
        assert!(files[1].new_file().is_some());

        // Third file has only new header
        assert!(files[2].old_file().is_none());
        assert!(files[2].new_file().is_some());
    }

    #[test]
    fn test_malformed_hunk_headers() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-missing comma in range
+but should still parse
@@ -2,2 +2,2 @@ with context
 context line
-deleted
+added
@@ invalid @@ completely
 should skip this section
@@ -10,1 +10,1 @@
-valid again
+after invalid
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunks: Vec<_> = file.hunks().collect();

        // Should have parsed valid hunks
        assert!(hunks.len() >= 2);

        // Check that valid hunks have proper headers
        for hunk in &hunks {
            if let Some(header) = hunk.header() {
                // Valid headers should have ranges
                assert!(header.old_range().is_some() || header.new_range().is_some());
            }
        }
    }

    #[test]
    fn test_mixed_line_endings() {
        let input = "--- a/file.txt\r\n+++ b/file.txt\n@@ -1,2 +1,2 @@\r\n line1\n-line2\r\n+line2 modified\n";
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text().unwrap(), "line1");
        assert_eq!(lines[1].text().unwrap(), "line2");
        assert_eq!(lines[2].text().unwrap(), "line2 modified");
    }

    #[test]
    fn test_empty_hunks() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,0 +1,0 @@
--- a/file2.txt
+++ b/file2.txt
@@ -1,1 +1,1 @@
-content
+changed
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 2);

        // First file has empty hunk
        let hunk1 = files[0].hunks().next().unwrap();
        assert_eq!(hunk1.lines().count(), 0);

        // Second file has normal hunk
        let hunk2 = files[1].hunks().next().unwrap();
        assert_eq!(hunk2.lines().count(), 2);
    }

    #[test]
    fn test_binary_patch_notation() {
        let input = r#"--- a/image.png
+++ b/image.png
Binary files differ
--- a/text.txt
+++ b/text.txt
@@ -1,1 +1,1 @@
-old
+new
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 2);

        // First file is binary (no hunks, just the notation)
        assert_eq!(files[0].hunks().count(), 0);

        // Second file has normal hunk
        assert_eq!(files[1].hunks().count(), 1);
    }

    #[test]
    fn test_context_after_errors() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ COMPLETELY INVALID @@
this should be skipped
but parsing should continue
@@ -5,2 +5,2 @@ and we should find this
 context line
-old line
+new line
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunks: Vec<_> = file.hunks().collect();

        // Should find the valid hunk after the invalid one
        assert!(hunks.len() >= 1);

        let valid_hunk = hunks.into_iter().find(|h| {
            h.header()
                .and_then(|header| header.old_range())
                .and_then(|range| range.start())
                .map(|start| start == 5)
                .unwrap_or(false)
        });
        assert!(valid_hunk.is_some());
    }

    #[test]
    fn test_truncated_patch() {
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,5 +1,5 @@
 line 1
 line 2
-line 3
+line 3 modif"#;
        // Patch is truncated mid-line
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        // Should parse what's there
        assert!(lines.len() >= 3);

        // Last line should have the truncated content
        let last_line = lines.last().unwrap();
        assert!(last_line.as_add().is_some());
        assert_eq!(last_line.text().unwrap(), "line 3 modif");
    }
}
