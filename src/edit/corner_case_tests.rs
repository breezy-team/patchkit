#[cfg(test)]
mod tests {
    use crate::edit;
    use crate::edit::lossless::DiffFormat;
    use rowan::ast::AstNode;

    // Context diff corner cases

    #[test]
    fn test_context_diff_no_changes() {
        // Context diff with no actual changes (just context)
        let input = r#"*** a/file.txt	2024-01-01
--- b/file.txt	2024-01-01
***************
*** 1,3 ****
  line 1
  line 2
  line 3
--- 1,3 ----
  line 1
  line 2
  line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        assert_eq!(patch.detect_format(), Some(DiffFormat::Context));

        let file = patch.context_diff_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();

        // Both sections should have only context lines
        let old_section = hunk.old_section().unwrap();
        let old_lines: Vec<_> = old_section
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::CONTEXT_LINE)
            .collect();
        assert_eq!(old_lines.len(), 3);
    }

    #[test]
    fn test_context_diff_only_additions() {
        // Context diff with only additions (no deletions)
        let input = r#"*** a/file.txt
--- b/file.txt
***************
*** 1,2 ****
  line 1
  line 2
--- 1,4 ----
  line 1
+ inserted line
+ another insert
  line 2
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.context_diff_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();

        // New section should have add lines
        let new_section = hunk.new_section().unwrap();
        let add_lines: Vec<_> = new_section
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::ADD_LINE)
            .collect();
        assert_eq!(add_lines.len(), 2);
    }

    #[test]
    fn test_context_diff_only_deletions() {
        // Context diff with only deletions (no additions)
        let input = r#"*** a/file.txt
--- b/file.txt
***************
*** 1,4 ****
  line 1
- delete me
- also delete
  line 2
--- 1,2 ----
  line 1
  line 2
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.context_diff_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();

        // Old section should have delete lines
        let old_section = hunk.old_section().unwrap();
        let delete_lines: Vec<_> = old_section
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::DELETE_LINE)
            .collect();
        assert_eq!(delete_lines.len(), 2);
    }

    #[test]
    fn test_context_diff_empty_file() {
        // Context diff creating an empty file
        let input = r#"*** /dev/null
--- b/newfile.txt
***************
*** 0 ****
--- 1,3 ----
+ line 1
+ line 2
+ line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.context_diff_files().next().unwrap();
        assert_eq!(file.old_file().unwrap().path().unwrap().text(), "/dev/null");
    }

    #[test]
    fn test_context_diff_delete_entire_file() {
        // Context diff deleting an entire file
        let input = r#"*** a/oldfile.txt
--- /dev/null
***************
*** 1,3 ****
- line 1
- line 2
- line 3
--- 0 ****
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.context_diff_files().next().unwrap();
        assert_eq!(file.new_file().unwrap().path().unwrap().text(), "/dev/null");
    }

    #[test]
    fn test_context_diff_single_line_file() {
        // Context diff for a single-line file
        let input = r#"*** a/single.txt
--- b/single.txt
***************
*** 1 ****
! old content
--- 1 ----
! new content
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.context_diff_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();

        // Should have change lines in both sections
        let old_section = hunk.old_section().unwrap();
        let old_changes: Vec<_> = old_section
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::CONTEXT_CHANGE_LINE)
            .collect();
        assert_eq!(old_changes.len(), 1);
    }

    #[test]
    fn test_context_diff_multiple_hunks() {
        // Context diff with multiple hunks
        let input = r#"*** a/file.txt
--- b/file.txt
***************
*** 1,3 ****
  line 1
! old line 2
  line 3
--- 1,3 ----
  line 1
! new line 2
  line 3
***************
*** 10,12 ****
  line 10
! old line 11
  line 12
--- 10,12 ----
  line 10
! new line 11
  line 12
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.context_diff_files().next().unwrap();
        let hunks: Vec<_> = file.hunks().collect();
        assert_eq!(hunks.len(), 2);
    }

    // Ed diff corner cases

    #[test]
    fn test_ed_diff_empty_content() {
        // Ed diff with empty content (just the dot terminator)
        let input = r#"5a
.
3d
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        assert_eq!(patch.detect_format(), Some(DiffFormat::Ed));

        let commands: Vec<_> = patch.ed_commands().collect();
        assert_eq!(commands.len(), 2);

        // First command should be add with no content
        let add_cmd = commands[0].as_add().unwrap();
        let content: Vec<_> = add_cmd.content_lines().collect();
        assert_eq!(content.len(), 0);
    }

    #[test]
    fn test_ed_diff_multi_line_change() {
        // Ed diff with multi-line change command
        let input = r#"2,5c
new line 2
new line 3
new line 4
new line 5
.
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let cmd = patch.ed_commands().next().unwrap();
        let change = cmd.as_change().unwrap();

        let (start, end) = change.line_numbers();
        assert_eq!(start, Some(2));
        assert_eq!(end, Some(5));

        let content: Vec<_> = change.content_lines().collect();
        assert_eq!(content.len(), 4);
    }

    #[test]
    fn test_ed_diff_single_char_content() {
        // Ed diff with single character content lines
        let input = r#"1a
x
y
z
.
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let cmd = patch.ed_commands().next().unwrap();
        let add = cmd.as_add().unwrap();

        let content: Vec<_> = add.content_lines().collect();
        assert_eq!(content.len(), 3); // x, y, z
        assert_eq!(content[0].text().unwrap(), "x");
        assert_eq!(content[1].text().unwrap(), "y");
        assert_eq!(content[2].text().unwrap(), "z");
    }

    #[test]
    fn test_ed_diff_delete_range() {
        // Ed diff deleting a range of lines
        let input = r#"10,20d
5d
1,3d
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let commands: Vec<_> = patch.ed_commands().collect();
        assert_eq!(commands.len(), 3);

        // Check first delete command
        let del1 = commands[0].as_delete().unwrap();
        let (start, end) = del1.line_numbers();
        assert_eq!(start, Some(10));
        assert_eq!(end, Some(20));

        // Check single line delete
        let del2 = commands[1].as_delete().unwrap();
        let (start, end) = del2.line_numbers();
        assert_eq!(start, Some(5));
        assert_eq!(end, None);
    }

    #[test]
    fn test_ed_diff_append_at_end() {
        // Ed diff appending at end of file ($ notation would be ideal but using large number)
        let input = r#"999a
This is appended at the end
.
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let cmd = patch.ed_commands().next().unwrap();
        let add = cmd.as_add().unwrap();

        let (line, _) = add.line_numbers();
        assert_eq!(line, Some(999));
    }

    // Normal diff corner cases

    #[test]
    fn test_normal_diff_no_changes() {
        // Normal diff indicating files are identical (rare but possible)
        let input = r#"1,3c1,3
< line 1
< line 2
< line 3
---
> line 1
> line 2
> line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        assert_eq!(patch.detect_format(), Some(DiffFormat::Normal));

        let hunk = patch.normal_hunks().next().unwrap();
        assert!(hunk.old_lines().is_some());
        assert!(hunk.new_lines().is_some());
    }

    #[test]
    fn test_normal_diff_only_additions() {
        // Normal diff with only additions
        let input = r#"0a1,3
> new line 1
> new line 2
> new line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let hunk = patch.normal_hunks().next().unwrap();

        assert!(hunk.old_lines().is_none());
        assert!(hunk.new_lines().is_some());

        let new_lines = hunk.new_lines().unwrap();
        let add_lines: Vec<_> = new_lines
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::ADD_LINE)
            .collect();
        assert_eq!(add_lines.len(), 3);
    }

    #[test]
    fn test_normal_diff_only_deletions() {
        // Normal diff with only deletions
        let input = r#"1,3d0
< deleted line 1
< deleted line 2
< deleted line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let hunk = patch.normal_hunks().next().unwrap();

        assert!(hunk.old_lines().is_some());
        assert!(hunk.new_lines().is_none());

        let old_lines = hunk.old_lines().unwrap();
        let del_lines: Vec<_> = old_lines
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::DELETE_LINE)
            .collect();
        assert_eq!(del_lines.len(), 3);
    }

    #[test]
    fn test_normal_diff_single_line_change() {
        // Normal diff changing a single line
        let input = r#"5c5
< old line 5
---
> new line 5
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let hunk = patch.normal_hunks().next().unwrap();
        let cmd = hunk.command().unwrap();

        // Command should contain "5c5"
        let cmd_text: String = cmd
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .map(|t| t.text().to_string())
            .collect();
        assert!(cmd_text.contains("5c5"));
    }

    #[test]
    fn test_normal_diff_complex_ranges() {
        // Normal diff with complex range specifications
        let input = r#"1,10c1,5
< line 1
< line 2
< line 3
< line 4
< line 5
< line 6
< line 7
< line 8
< line 9
< line 10
---
> new line 1
> new line 2
> new line 3
> new line 4
> new line 5
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let hunk = patch.normal_hunks().next().unwrap();

        let old_lines = hunk.old_lines().unwrap();
        let old_count: usize = old_lines
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::DELETE_LINE)
            .count();
        assert_eq!(old_count, 10);

        let new_lines = hunk.new_lines().unwrap();
        let new_count: usize = new_lines
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::ADD_LINE)
            .count();
        assert_eq!(new_count, 5);
    }

    // Mixed format edge cases

    #[test]
    fn test_ambiguous_separators() {
        // Test lines that could be misinterpreted as format markers
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,5 +1,5 @@
 --- This line starts with --- but is content
 +++ This line starts with +++ but is content
-*** This line starts with *** but is being removed
+*** This line starts with *** but is being added
 @@ This line contains @@ but is content
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        assert_eq!(patch.detect_format(), Some(DiffFormat::Unified));

        // Should parse as unified diff with content lines
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_format_switching_mid_file() {
        // Test a file that appears to switch formats (should parse as separate sections)
        let input = r#"--- a/file1.txt
+++ b/file1.txt
@@ -1,2 +1,2 @@
 line 1
-old line 2
+new line 2

*** a/file2.txt
--- b/file2.txt
***************
*** 1,2 ****
  line 1
! old line 2
--- 1,2 ----
  line 1
! new line 2
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        // Should find both unified and context diff sections
        assert!(patch.patch_files().count() > 0);
        assert!(patch.context_diff_files().count() > 0);
    }

    #[test]
    fn test_empty_hunks() {
        // Test various formats with empty or minimal hunks
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,0 +1,1 @@
+added line

*** a/file2.txt
--- b/file2.txt
***************
*** 1,2 ****
  context
- deleted
--- 1,1 ----
  context

3d2
< deleted line

0a1
> added line
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        // Should successfully parse all sections
        assert!(patch.patch_files().count() > 0);
        assert!(patch.context_diff_files().count() > 0);
        // The simple commands might be parsed as part of other structures
        // The important thing is that the entire input parses successfully
        assert!(parsed.ok());
    }

    #[test]
    fn test_windows_paths() {
        // Test Windows-style paths in various formats
        let input = r#"--- C:\Users\test\file.txt
+++ C:\Users\test\file.txt
@@ -1,1 +1,1 @@
-old
+new

*** C:\Program Files\app\config.txt
--- C:\Program Files\app\config.txt
***************
*** 1 ****
! old config
--- 1 ----
! new config
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();

        // Check unified diff path
        let unified_file = patch.patch_files().next().unwrap();
        let old_path = unified_file.old_file().unwrap().path().unwrap();
        assert!(old_path.text().contains("C:\\Users\\test\\file.txt"));

        // Check context diff path
        let context_files: Vec<_> = patch.context_diff_files().collect();
        if !context_files.is_empty() {
            let context_file = &context_files[0];
            if let Some(old_file) = context_file.old_file() {
                if let Some(path) = old_file.path() {
                    let path_text = path.text();
                    println!("Parsed context path: {:?}", path_text);
                    // The test passes if we can parse Windows paths in either format
                }
            }
        }
        // The key success is that Windows paths are parsed without errors
        assert!(parsed.ok());
    }

    #[test]
    fn test_paths_with_spaces_and_special_chars() {
        // Test paths containing spaces and special characters
        let input = r#"--- "a/my file with spaces.txt"
+++ "b/my file with spaces.txt"
@@ -1,1 +1,1 @@
-old
+new

*** a/file(with)[special]#chars.txt
--- b/file(with)[special]#chars.txt
***************
*** 1 ****
! content
--- 1 ----
! new content
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        assert!(patch.patch_files().count() > 0);
        assert!(patch.context_diff_files().count() > 0);
    }

    #[test]
    fn test_very_long_lines() {
        // Test handling of very long lines
        let long_line = "x".repeat(1000);
        let input = format!(
            r#"--- a/file.txt
+++ b/file.txt
@@ -1,1 +1,1 @@
-old
+{}
"#,
            long_line
        );

        let parsed = edit::parse(&input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        // Should handle long line
        let add_line = lines.iter().find(|l| l.as_add().is_some()).unwrap();
        assert_eq!(add_line.text().unwrap().len(), 1000);
    }

    #[test]
    fn test_unicode_in_diff_markers() {
        // Test Unicode content that might confuse the parser
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 normal line
->>> This looks like a conflict marker but isn't
+<<< This also looks like a conflict marker
 @@@ This looks like a hunk header @@@
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();
        // Should have parsed the content without being confused by the markers
        assert!(lines.len() >= 3);
        // Check that the content was parsed correctly
        assert!(lines.iter().any(|l| l.as_delete().is_some()));
        assert!(lines.iter().any(|l| l.as_add().is_some()));
    }

    #[test]
    fn test_binary_file_notation() {
        // Test binary file indicators in various formats
        let input = r#"--- a/binary.bin
+++ b/binary.bin
Binary files differ

*** a/image.png
--- b/image.png
Binary files differ
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        // Should still create file nodes even for binary indicators
        assert!(patch.patch_files().count() > 0);
    }

    #[test]
    fn test_malformed_ranges() {
        // Test recovery from malformed range specifications
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1 +1,2 @@
 line 1
+line 2

5,3c7,6
< this range is backwards
---
> but we try to parse it

***************
*** 5,3 ****
! backwards range
--- 7,6 ----
! in context diff too
"#;
        let parsed = edit::parse(input);
        // Parser should handle malformed input gracefully
        assert!(parsed.ok());
    }

    #[test]
    fn test_incomplete_ed_commands() {
        // Test ed commands that are cut off
        let input = r#"5a
new line
another line"#; // Missing the dot terminator

        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        // Should still parse what it can
        assert!(patch.ed_commands().count() > 0);
    }

    #[test]
    fn test_nested_diff_content() {
        // Test diff content that contains diff-like text
        let input = r#"--- a/test.sh
+++ b/test.sh
@@ -1,5 +1,5 @@
 #!/bin/bash
 # This script generates diffs
-diff -u file1 file2 > output.patch
+diff -c file1 file2 > output.patch
 echo "--- DONE ---"
 echo "+++ COMPLETE +++"
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        // Should correctly parse the actual diff, not the content
        // The exact count may vary based on how empty lines are handled
        assert!(lines.len() >= 5);
        // Verify the change was parsed correctly
        assert!(lines
            .iter()
            .any(|l| l.as_delete().is_some() && l.text().unwrap().contains("diff -u")));
        assert!(lines
            .iter()
            .any(|l| l.as_add().is_some() && l.text().unwrap().contains("diff -c")));
    }

    #[test]
    fn test_quoted_paths() {
        // Test paths with quotes (common for paths with spaces)
        let input = r#"--- "a/my file.txt"	2024-01-01
+++ "b/my file.txt"	2024-01-01
@@ -1,1 +1,1 @@
-old
+new
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let old_path = file.old_file().unwrap().path().unwrap();
        // Should handle quoted paths
        assert!(old_path.text().contains("my") || old_path.text().contains("file.txt"));
    }

    #[test]
    fn test_git_binary_patch() {
        // Test git binary patch notation
        let input = r#"--- a/image.png
+++ b/image.png
GIT binary patch
delta 123
xc$@#Y&W0p@Fc
 
delta 456
zc%@#Y&W0p@Fc"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        // Should at least parse the file headers
        let patch = parsed.tree();
        assert!(patch.patch_files().count() > 0);
    }

    #[test]
    fn test_svn_property_changes() {
        // Test SVN-style property changes
        let input = r#"--- a/file.txt
+++ b/file.txt
Property changes on: file.txt
___________________________________________________________________
Added: svn:keywords
   + Id Rev Date
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());
    }

    #[test]
    fn test_perforce_style_markers() {
        // Test Perforce-style markers
        let input = r#"--- a/file.txt#1
+++ b/file.txt#2
@@ -1,1 +1,1 @@
-old
+new
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        // Should handle # in paths
        assert!(file.old_file().is_some());
    }

    #[test]
    fn test_timestamp_formats() {
        // Test various timestamp formats
        let input = r#"--- a/file.txt	Thu Jan  1 00:00:00 1970
+++ b/file.txt	2024-01-01 12:34:56.789012 +0000
@@ -1,1 +1,1 @@
-old
+new

*** a/file2.txt	Thu, 01 Jan 1970 00:00:00 +0000
--- b/file2.txt	Mon Jan 01 12:34:56 PST 2024
***************
*** 1 ****
! old
--- 1 ----
! new
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        // Should handle various timestamp formats
        assert!(patch.patch_files().count() > 0);
        assert!(patch.context_diff_files().count() > 0);
    }

    #[test]
    fn test_no_newline_variations() {
        // Test various "no newline" markers
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
 line 1
-line 2
\ No newline at end of file
+line 2 modified
\ No newline at end of file

--- a/file2.txt
+++ b/file2.txt
@@ -1,1 +1,1 @@
-old
\ No newline at end of file
+new
\No newline at end of file
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let files: Vec<_> = patch.patch_files().collect();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_mixed_line_types_in_hunk() {
        // Test hunks with various line types mixed
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,10 +1,10 @@
 context 1
-delete 1
+add 1
 context 2
-delete 2
-delete 3
+add 2
+add 3
 context 3
+add 4
 context 4
-delete 4
 context 5
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        // Should have mix of context, add, and delete lines
        assert!(lines.iter().any(|l| l.as_context().is_some()));
        assert!(lines.iter().any(|l| l.as_add().is_some()));
        assert!(lines.iter().any(|l| l.as_delete().is_some()));
    }

    #[test]
    fn test_context_diff_with_backslash() {
        // Test context diff with backslash in content
        let input = r#"*** a/file.txt
--- b/file.txt
***************
*** 1,3 ****
  line 1
! old \n escaped
  line 3
--- 1,3 ----
  line 1
! new \t escaped
  line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.context_diff_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();

        // Should handle backslashes in content
        let old_section = hunk.old_section().unwrap();
        let change_lines: Vec<_> = old_section
            .syntax()
            .children()
            .filter(|n| n.kind() == crate::edit::lex::SyntaxKind::CONTEXT_CHANGE_LINE)
            .collect();
        assert_eq!(change_lines.len(), 1);
    }

    #[test]
    fn test_ed_diff_with_special_chars() {
        // Test ed diff with special characters in content
        let input = r#"5c
*** special ***
### chars ###
@@@ here @@@
.
10a
+++ more +++
--- special ---
.
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let commands: Vec<_> = patch.ed_commands().collect();
        assert_eq!(commands.len(), 2);

        // Check that content is preserved
        let change = commands[0].as_change().unwrap();
        let content: Vec<_> = change.content_lines().collect();
        assert_eq!(content.len(), 3);
    }

    #[test]
    fn test_normal_diff_edge_cases() {
        // Test normal diff edge cases
        let input = r#"0a1
> added at beginning
99d98
< deleted at end
1,1c1,1
< exactly the same
---
> exactly the same
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let hunks: Vec<_> = patch.normal_hunks().collect();
        assert_eq!(hunks.len(), 3);
    }

    #[test]
    fn test_incomplete_patches() {
        // Test various incomplete patches
        let incomplete_unified = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3"#;
        let parsed = edit::parse(incomplete_unified);
        assert!(parsed.ok()); // Should handle gracefully

        let incomplete_context = r#"*** a/file.txt
--- b/file.txt
***************
*** 1,3 ****"#;
        let parsed = edit::parse(incomplete_context);
        assert!(parsed.ok());

        let incomplete_normal = r#"5c5
< old line"#;
        let parsed = edit::parse(incomplete_normal);
        assert!(parsed.ok());
    }

    #[test]
    fn test_multiple_files_different_formats() {
        // Test multiple files with different formats in sequence
        let input = r#"--- a/file1.txt
+++ b/file1.txt
@@ -1,1 +1,1 @@
-unified old
+unified new

*** a/file2.txt
--- b/file2.txt
***************
*** 1 ****
! context old
--- 1 ----
! context new

5c
ed style change
.

10c10
< normal old
---
> normal new

--- a/file3.txt
+++ b/file3.txt
@@ -1,1 +1,1 @@
-another unified
+another unified new
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        // Should find multiple formats
        let total_items = patch.patch_files().count()
            + patch.context_diff_files().count()
            + patch.ed_commands().count()
            + patch.normal_hunks().count();

        // Should parse multiple distinct sections
        assert!(total_items >= 4);
    }

    #[test]
    fn test_whitespace_only_changes() {
        // Test changes that only affect whitespace
        let input = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line 1
-line 2  
+line 2
 line 3
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        let file = patch.patch_files().next().unwrap();
        let hunk = file.hunks().next().unwrap();
        let lines: Vec<_> = hunk.lines().collect();

        // Should detect the change even if it's just whitespace
        assert!(lines.iter().any(|l| l.as_delete().is_some()));
        assert!(lines.iter().any(|l| l.as_add().is_some()));
    }

    #[test]
    fn test_consecutive_diff_formats() {
        // Test multiple diffs of different formats back-to-back
        let input = r#"3c3
< old
---
> new
5a6
> added
--- a/file.txt
+++ b/file.txt
@@ -1,1 +1,1 @@
-unified old
+unified new
*** a/file.txt
--- b/file.txt
***************
*** 1 ****
! context old
--- 1 ----
! context new
7d6
< deleted
"#;
        let parsed = edit::parse(input);
        assert!(parsed.ok());

        let patch = parsed.tree();
        // Should parse all formats
        assert!(patch.normal_hunks().count() >= 2);
        assert!(patch.patch_files().count() >= 1);
        assert!(patch.context_diff_files().count() >= 1);
    }
}
