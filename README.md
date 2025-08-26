Parsing and manipulation of patch files
---------------------------------------

This crate provides support for parsing and editing of unified diff files, as
well as related files (e.g. quilt).

## Features

- **Traditional parsing**: Parse patch files into structured data
- **Lossless parsing** (new): Parse patch files while preserving all formatting and whitespace using the `edit` module

## Example

```rust
use patchkit::edit;

let patch_text = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#;

let parsed = edit::parse(patch_text);
let patch = parsed.tree();

for patch_file in patch.patch_files() {
    for hunk in patch_file.hunks() {
        for line in hunk.lines() {
            if let Some(text) = line.text() {
                println!("{}", text);
            }
        }
    }
}
```
