//! Applying a patch set to files on disk, mirroring `patch(1)`.
//!
//! [`apply_fuzzy`](crate::apply::apply_fuzzy) works on a single file's bytes in
//! memory. This module is the filesystem counterpart: it walks a set of
//! [`UnifiedPatch`]es, reads each target file, applies its hunks, and writes the
//! result back, leaving `.orig` backups and removing emptied files the way GNU
//! patch does.
//!
//! Application is not atomic. As with `patch(1)`, a file whose hunks fail keeps
//! its original content (with a backup) while files patched before it keep their
//! changes.

use crate::apply::{apply_fuzzy, ApplyOptions, HunkOutcome};
use crate::unified::UnifiedPatch;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Options for applying a patch set to files on disk.
#[derive(Debug, Clone)]
pub struct ApplyToTreeOptions {
    /// Per-hunk fuzz and offset limits, passed through to
    /// [`apply_fuzzy`](crate::apply::apply_fuzzy).
    pub apply: ApplyOptions,

    /// Leading path segments to strip from patch file names, like `patch -pN`.
    pub strip: u32,

    /// Apply the patches in reverse.
    pub reverse: bool,

    /// Do not touch the filesystem; report what would happen.
    pub dry_run: bool,

    /// Suffix for the backup left beside a file that did not match exactly, or
    /// `None` to leave no backup. `patch(1)` uses `.orig`.
    pub backup_suffix: Option<String>,

    /// Remove a file whose patched content is empty, like
    /// `patch --remove-empty-files`.
    pub remove_empty_files: bool,
}

impl Default for ApplyToTreeOptions {
    /// Mirror `patch -p0 --remove-empty-files` at fuzz 2.
    fn default() -> Self {
        ApplyToTreeOptions {
            apply: ApplyOptions::with_fuzz(2),
            strip: 0,
            reverse: false,
            dry_run: false,
            backup_suffix: Some(".orig".to_string()),
            remove_empty_files: true,
        }
    }
}

/// Something went wrong applying a patch set. A failed *hunk* is not an error; it
/// is recorded in the [`FileReport`]. This is reserved for I/O failures,
/// patches that name no file to write, and patches naming a path that is not a
/// plain relative path or that leads out of the tree.
#[derive(Debug)]
pub enum Error {
    /// An I/O error touching the tree.
    Io(std::io::Error),
    /// A patch that cannot be applied as written, because it names `/dev/null`
    /// on both sides or names a path that is not a plain relative path or leads
    /// out of the tree.
    Malformed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{}", e),
            Error::Malformed(m) => write!(f, "{}", m),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// What happened to one file in the set.
#[derive(Debug, Clone)]
pub struct FileReport {
    /// The file that was read from (before any rename).
    pub path: PathBuf,

    /// Every hunk matched, though possibly at an offset or with fuzz. This is
    /// what `patch(1)` treats as success (exit 0), even when it leaves a backup.
    pub applied: bool,

    /// Every hunk matched exactly: no failure, offset, or fuzz. Whenever this is
    /// false a backup is left, as `patch(1)` does for an inexact match.
    pub clean: bool,

    /// The outcome of each hunk, in patch order, as from
    /// [`apply_fuzzy`](crate::apply::apply_fuzzy).
    pub hunks: Vec<HunkOutcome>,

    /// The file was removed (patched to empty, or the patch deletes it).
    pub removed: bool,

    /// A backup was written at `path` + the configured suffix.
    pub backed_up: bool,
}

/// The outcome of applying a whole patch set.
#[derive(Debug, Clone)]
pub struct TreeReport {
    /// One entry per patch, in order.
    pub files: Vec<FileReport>,
}

impl TreeReport {
    /// Whether every file's every hunk matched, allowing offset and fuzz. This
    /// is `patch(1)`'s notion of success.
    pub fn applied(&self) -> bool {
        self.files.iter().all(|f| f.applied)
    }

    /// Whether every file's every hunk matched exactly, with no offset or fuzz.
    pub fn is_success(&self) -> bool {
        self.files.iter().all(|f| f.clean)
    }
}

/// Apply a parsed patch set to files under `directory`.
///
/// Human-readable progress (`patching file X`, `Hunk #N FAILED`) is written to
/// `out` when it is `Some`; pass `None` to stay quiet.
pub fn apply_to_tree(
    directory: &Path,
    patches: &[UnifiedPatch],
    options: &ApplyToTreeOptions,
    mut out: Option<&mut dyn Write>,
) -> Result<TreeReport, Error> {
    let mut files = Vec::with_capacity(patches.len());
    for patch in patches {
        let patch = if options.reverse {
            patch.reverse()
        } else {
            patch.clone()
        };
        files.push(apply_one(directory, &patch, options, &mut out)?);
    }
    Ok(TreeReport { files })
}

/// Which file on disk a patch reads from and writes to.
fn target_of(patch: &UnifiedPatch, strip: u32) -> (Option<PathBuf>, Option<PathBuf>) {
    let orig = (!is_dev_null(&patch.orig_name)).then(|| strip_path(&patch.orig_name, strip));
    let modified = (!is_dev_null(&patch.mod_name)).then(|| strip_path(&patch.mod_name, strip));
    (orig, modified)
}

/// Strip `strip` leading segments from a patch path.
fn strip_path(name: &[u8], strip: u32) -> PathBuf {
    let name = String::from_utf8_lossy(name);
    let name = name.split('\t').next().unwrap_or(&name);
    PathBuf::from(name.splitn(strip as usize + 1, '/').last().unwrap_or(name))
}

/// Reject a patch path that is empty or has any component other than an
/// ordinary name or `.`, so a patch cannot point out of the tree with `..`, a
/// leading slash or a Windows prefix. Components split the way the host does,
/// so `..\x` is one plain name on Unix and is refused on Windows.
fn check_contained(name: &Path) -> Result<(), Error> {
    if name.as_os_str().is_empty()
        || name
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(Error::Malformed(format!(
            "patch path is not a plain relative path: {}",
            name.display()
        )));
    }
    Ok(())
}

/// Join `name` to the canonical `root`, refusing a path that is a symlink or
/// whose deepest existing directory resolves outside `root`.
fn resolve(root: &Path, name: &Path) -> Result<PathBuf, Error> {
    let path = root.join(name);
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(Error::Malformed(format!(
            "patch path is a symlink: {}",
            name.display()
        )));
    }
    let existing = path
        .ancestors()
        .skip(1)
        .find(|p| p.symlink_metadata().is_ok())
        .unwrap_or(root);
    if !existing.canonicalize()?.starts_with(root) {
        return Err(Error::Malformed(format!(
            "patch path leaves the tree through a symlink: {}",
            name.display()
        )));
    }
    Ok(path)
}

fn is_dev_null(name: &[u8]) -> bool {
    let name = String::from_utf8_lossy(name);
    name.split('\t').next().unwrap_or(&name) == "/dev/null"
}

fn apply_one(
    directory: &Path,
    patch: &UnifiedPatch,
    options: &ApplyToTreeOptions,
    out: &mut Option<&mut dyn Write>,
) -> Result<FileReport, Error> {
    let (orig, modified) = target_of(patch, options.strip);

    for name in [orig.as_deref(), modified.as_deref()].into_iter().flatten() {
        check_contained(name)?;
    }

    let read_from = orig
        .as_ref()
        .or(modified.as_ref())
        .ok_or_else(|| Error::Malformed("patch names /dev/null on both sides".to_string()))?;
    let write_to = modified.as_ref().unwrap_or(read_from);
    let root = directory.canonicalize()?;
    let path = resolve(&root, read_from)?;
    let dest = resolve(&root, write_to)?;

    let original = match std::fs::read(&path) {
        Ok(content) => content,
        // A patch that creates a file has nothing to read.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && orig.is_none() => Vec::new(),
        Err(e) => return Err(e.into()),
    };

    if let Some(out) = out.as_deref_mut() {
        writeln!(out, "patching file {}", read_from.display())?;
    }

    let result = apply_fuzzy(&original, &patch.hunks, &options.apply);

    // patch(1) hedges whenever it did not match exactly: any hunk applied at an
    // offset or with fuzz, or any that failed, leaves the original as a backup.
    let clean = result
        .hunks
        .iter()
        .all(|h| h.applied() && h.offset == 0 && h.fuzz == 0);
    let applied_cleanly = result.patched.is_some();

    let mut backed_up = false;
    if !clean && !options.dry_run {
        if let Some(suffix) = &options.backup_suffix {
            replace_file(&resolve(&root, &with_suffix(read_from, suffix))?, &original)?;
            backed_up = true;
        }
    }

    if !applied_cleanly {
        if let Some(out) = out.as_deref_mut() {
            for hunk in result.rejected() {
                writeln!(out, "Hunk #{} FAILED.", hunk.index + 1)?;
            }
            let failed = result.rejected().count();
            writeln!(
                out,
                "{} out of {} hunk{} FAILED",
                failed,
                patch.hunks.len(),
                if patch.hunks.len() == 1 { "" } else { "s" }
            )?;
        }
    }

    if options.dry_run {
        return Ok(FileReport {
            path: read_from.clone(),
            applied: applied_cleanly,
            clean,
            hunks: result.hunks,
            removed: false,
            backed_up,
        });
    }

    // Each hunk stands on its own, as in patch(1): the ones that matched are
    // written out even when a sibling failed.
    let patched = result
        .partial
        .expect("apply_fuzzy yields content outside a dry run");

    // A file only goes away if the patch that empties it actually applied.
    if applied_cleanly && (modified.is_none() || (options.remove_empty_files && patched.is_empty()))
    {
        std::fs::remove_file(&path)?;
        return Ok(FileReport {
            path: read_from.clone(),
            applied: applied_cleanly,
            clean,
            hunks: result.hunks,
            removed: true,
            backed_up,
        });
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    replace_file(&dest, &patched)?;
    if dest != path && orig.is_some() {
        std::fs::remove_file(&path)?;
    }
    Ok(FileReport {
        path: read_from.clone(),
        applied: applied_cleanly,
        clean,
        hunks: result.hunks,
        removed: false,
        backed_up,
    })
}

/// Write `data` to a new file and rename it over `path`, so a hard link to `path`
/// from outside the tree keeps its old content.
fn replace_file(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = with_suffix(path, ".patchkit-new");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let result = std::fs::metadata(path)
        .map_or(Ok(()), |meta| file.set_permissions(meta.permissions()))
        .and_then(|()| file.write_all(data))
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Append `suffix` to a path's file name, rather than replacing its extension.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unified::UnifiedPatch;

    fn parse(patch: &[u8]) -> UnifiedPatch {
        UnifiedPatch::parse_patch(patch.split_inclusive(|&b| b == b'\n')).unwrap()
    }

    /// Defaults with `strip: 1`, matching the `a/`/`b/` prefixes the test
    /// patches use.
    fn opts() -> ApplyToTreeOptions {
        ApplyToTreeOptions {
            strip: 1,
            ..Default::default()
        }
    }

    fn tree(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn clean_hunk_applies_and_leaves_no_backup() {
        let patch = b"--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n line0\n-line1\n+changed\n";
        let dir = tree(&[("f", b"line0\nline1\n")]);
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts(), None).unwrap();
        assert!(report.is_success());
        assert_eq!(
            std::fs::read(dir.path().join("f")).unwrap(),
            b"line0\nchanged\n"
        );
        assert!(!dir.path().join("f.orig").exists());
    }

    #[test]
    fn a_failing_hunk_keeps_the_matching_ones_and_backs_up() {
        // Second hunk cannot match; the first still applies (partial), and the
        // original is backed up because the result is inexact.
        let patch = b"--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-line0\n+changed0\n@@ -5,1 +5,1 @@\n-nope\n+never\n";
        let dir = tree(&[("f", b"line0\nline1\n")]);
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts(), None).unwrap();
        assert!(!report.is_success());
        assert!(report.files[0].backed_up);
        assert_eq!(
            std::fs::read(dir.path().join("f.orig")).unwrap(),
            b"line0\nline1\n"
        );
        assert_eq!(
            std::fs::read(dir.path().join("f")).unwrap(),
            b"changed0\nline1\n"
        );
    }

    #[test]
    fn dry_run_touches_nothing() {
        let patch = b"--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-line0\n+changed\n";
        let dir = tree(&[("f", b"line0\n")]);
        let opts = ApplyToTreeOptions {
            dry_run: true,
            ..opts()
        };
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts, None).unwrap();
        assert!(report.is_success());
        assert_eq!(std::fs::read(dir.path().join("f")).unwrap(), b"line0\n");
        assert!(!dir.path().join("f.orig").exists());
    }

    #[test]
    fn removes_a_file_patched_to_empty() {
        let patch = b"--- a/f\n+++ b/f\n@@ -1,1 +0,0 @@\n-only\n";
        let dir = tree(&[("f", b"only\n")]);
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts(), None).unwrap();
        assert!(report.files[0].removed);
        assert!(!dir.path().join("f").exists());
    }

    #[test]
    fn keeps_an_emptied_file_when_remove_empty_files_is_off() {
        let patch = b"--- a/f\n+++ b/f\n@@ -1,1 +0,0 @@\n-only\n";
        let dir = tree(&[("f", b"only\n")]);
        let opts = ApplyToTreeOptions {
            remove_empty_files: false,
            ..opts()
        };
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts, None).unwrap();
        assert!(!report.files[0].removed);
        assert_eq!(std::fs::read(dir.path().join("f")).unwrap(), b"");
    }

    #[test]
    fn reverse_undoes_a_patch() {
        let patch = b"--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-line0\n+changed\n";
        let dir = tree(&[("f", b"changed\n")]);
        let opts = ApplyToTreeOptions {
            reverse: true,
            ..opts()
        };
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts, None).unwrap();
        assert!(report.is_success());
        assert_eq!(std::fs::read(dir.path().join("f")).unwrap(), b"line0\n");
    }

    #[test]
    fn no_backup_when_suffix_is_none() {
        let patch = b"--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-line0\n+changed0\n@@ -5,1 +5,1 @@\n-nope\n+never\n";
        let dir = tree(&[("f", b"line0\nline1\n")]);
        let opts = ApplyToTreeOptions {
            backup_suffix: None,
            ..opts()
        };
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts, None).unwrap();
        assert!(!report.files[0].backed_up);
        assert!(!dir.path().join("f.orig").exists());
    }

    #[test]
    fn patches_a_file_in_a_subdirectory() {
        let patch = b"--- a/sub/f\n+++ b/sub/f\n@@ -1,1 +1,1 @@\n-old\n+new\n";
        let dir = tree(&[]);
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/f"), b"old\n").unwrap();
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts(), None).unwrap();
        assert!(report.is_success());
        assert_eq!(std::fs::read(dir.path().join("sub/f")).unwrap(), b"new\n");
    }

    #[test]
    fn creates_a_file_from_dev_null() {
        let patch = b"--- /dev/null\n+++ b/new\n@@ -0,0 +1,1 @@\n+hello\n";
        let dir = tree(&[]);
        let report = apply_to_tree(dir.path(), &[parse(patch)], &opts(), None).unwrap();
        assert!(report.is_success());
        assert_eq!(std::fs::read(dir.path().join("new")).unwrap(), b"hello\n");
    }

    #[test]
    fn refuses_a_path_outside_the_tree() {
        let dir = tree(&[]);
        for name in ["../x", "/x"] {
            let patch = format!("--- {name}\n+++ {name}\n@@ -1,1 +1,1 @@\n-old\n+new\n");
            let result = apply_one(
                dir.path(),
                &parse(patch.as_bytes()),
                &Default::default(),
                &mut None,
            );
            assert!(matches!(result, Err(Error::Malformed(_))), "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_out_of_the_tree() {
        let outside = tree(&[("file", b"old\n")]);
        let dir = tree(&[]);
        std::os::unix::fs::symlink(outside.path(), dir.path().join("a")).unwrap();
        let patch = b"--- a/a/file\n+++ b/a/file\n@@ -1,1 +1,1 @@\n-old\n+new\n";
        let result = apply_to_tree(dir.path(), &[parse(patch)], &opts(), None);
        assert!(matches!(result, Err(Error::Malformed(_))));
        assert_eq!(
            std::fs::read(outside.path().join("file")).unwrap(),
            b"old\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn leaves_a_hard_link_out_of_the_tree_alone() {
        let outside = tree(&[("file", b"old\n")]);
        let dir = tree(&[]);
        std::fs::hard_link(outside.path().join("file"), dir.path().join("file")).unwrap();
        let patch = b"--- a/file\n+++ b/file\n@@ -1,1 +1,1 @@\n-old\n+new\n";
        apply_to_tree(dir.path(), &[parse(patch)], &opts(), None).unwrap();
        assert_eq!(std::fs::read(dir.path().join("file")).unwrap(), b"new\n");
        assert_eq!(
            std::fs::read(outside.path().join("file")).unwrap(),
            b"old\n"
        );
    }
}
