use patchkit::edit;

fn main() {
    let patch_text = r#"--- a/src/main.rs	2023-01-01 00:00:00
+++ b/src/main.rs	2023-01-02 00:00:00
@@ -1,5 +1,6 @@
 fn main() {
-    println!("Hello, world!");
+    println!("Hello, Rust!");
+    println!("This is a patched version.");
 }
 
 fn helper() {
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,7 +10,7 @@
 pub struct Config {
     pub name: String,
-    pub version: u32,
+    pub version: String,
 }
"#;

    let parsed = edit::parse(patch_text);

    if parsed.ok() {
        let patch = parsed.tree();
        println!(
            "Successfully parsed patch with {} file(s)!",
            patch.patch_files().count()
        );
        println!();

        for patch_file in patch.patch_files() {
            println!("=== File Change ===");
            if let Some(old_file) = patch_file.old_file() {
                if let Some(path) = old_file.path() {
                    println!("Old: {}", path.text());
                }
            }

            if let Some(new_file) = patch_file.new_file() {
                if let Some(path) = new_file.path() {
                    println!("New: {}", path.text());
                }
            }

            for hunk in patch_file.hunks() {
                println!("\n--- Hunk ---");
                if let Some(header) = hunk.header() {
                    if let Some(old_range) = header.old_range() {
                        print!("@@ -{}", old_range.start().unwrap_or(0));
                        if let Some(count) = old_range.count() {
                            print!(",{}", count);
                        }
                    }
                    if let Some(new_range) = header.new_range() {
                        print!(" +{}", new_range.start().unwrap_or(0));
                        if let Some(count) = new_range.count() {
                            print!(",{}", count);
                        }
                    }
                    println!(" @@");
                }

                for line in hunk.lines() {
                    if let Some(text) = line.text() {
                        if line.as_add().is_some() {
                            println!("+{}", text);
                        } else if line.as_delete().is_some() {
                            println!("-{}", text);
                        } else if line.as_context().is_some() {
                            println!(" {}", text);
                        }
                    }
                }
            }
            println!();
        }

        // Demonstrate lossless parsing
        println!("=== Lossless Roundtrip ===");
        println!("Original patch preserved exactly:");
        let roundtrip = parsed.syntax_node().text().to_string();
        assert_eq!(patch_text, roundtrip);
        println!("✓ Roundtrip successful!");
    } else {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  {}", error);
        }
    }
}
