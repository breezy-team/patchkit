//! Applying patches with fuzz, and checking applicability without modifying anything.
//!
//! Unlike [`crate::ContentPatch::apply_exact`], the functions here match each hunk
//! independently: a hunk may be found at an offset from the position its header
//! claims, and with `fuzz` set, up to that many context lines at the start and end
//! of the hunk may be ignored when matching. This mirrors the behaviour of
//! `patch --fuzz`.
//!
//! # Example
//!
//! ```
//! use patchkit::apply::{apply_fuzzy, ApplyOptions};
//! use patchkit::unified::UnifiedPatch;
//!
//! let patch = UnifiedPatch::parse_patch(patchkit::unified::splitlines(
//!     b"--- a\n+++ b\n@@ -1,3 +1,3 @@\n context\n-old\n+new\n context2\n",
//! ))
//! .unwrap();
//!
//! // The file has an extra line at the top, so the hunk sits one line further down
//! // than its header says. Exact application fails; fuzzy application finds it.
//! let orig = b"prelude\ncontext\nold\ncontext2\n";
//! let result = apply_fuzzy(orig, &patch.hunks, &ApplyOptions::default());
//! assert!(result.is_success());
//! assert_eq!(result.hunks[0].offset, 1);
//! assert_eq!(result.patched.unwrap(), b"prelude\ncontext\nnew\ncontext2\n");
//! ```
use crate::unified::{splitlines, Hunk, HunkLine};

/// Options controlling how a patch is applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyOptions {
    /// Maximum number of context lines to ignore at the start and end of a hunk
    /// when matching it against the original file. A fuzz of 0 requires all
    /// context lines to match.
    pub fuzz: usize,

    /// Maximum number of lines a hunk may be shifted from the position given in
    /// its header. `None` means the whole file is searched.
    pub max_offset: Option<usize>,
}

impl Default for ApplyOptions {
    /// Allow hunks to be found anywhere in the file, but require all context to match.
    fn default() -> Self {
        Self {
            fuzz: 0,
            max_offset: None,
        }
    }
}

impl ApplyOptions {
    /// Create options with the given maximum fuzz.
    pub fn with_fuzz(fuzz: usize) -> Self {
        Self {
            fuzz,
            ..Self::default()
        }
    }
}

/// The outcome of matching a single hunk against the original file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HunkOutcome {
    /// Index of the hunk in the patch.
    pub index: usize,

    /// How the hunk was matched, or why it could not be.
    pub status: HunkStatus,

    /// Number of lines the hunk was shifted from the position in its header.
    /// Negative means it matched earlier in the file than expected. Zero if the
    /// hunk did not apply.
    pub offset: isize,

    /// Number of context lines that had to be ignored to match the hunk. Zero if
    /// the hunk did not apply.
    pub fuzz: usize,
}

impl HunkOutcome {
    /// Whether this hunk applied.
    pub fn applied(&self) -> bool {
        self.status == HunkStatus::Applied
    }
}

/// Whether a hunk could be matched against the original file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HunkStatus {
    /// The hunk matched, possibly at an offset and with some context ignored.
    Applied,

    /// No position in the original file matched the hunk, even with the
    /// configured fuzz.
    Failed,
}

/// The result of applying a patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyResult {
    /// The patched content, or `None` if any hunk failed to apply.
    pub patched: Option<Vec<u8>>,

    /// The outcome of each hunk, in patch order.
    pub hunks: Vec<HunkOutcome>,
}

impl ApplyResult {
    /// Whether every hunk applied.
    pub fn is_success(&self) -> bool {
        self.patched.is_some()
    }

    /// The hunks that could not be applied.
    pub fn rejected(&self) -> impl Iterator<Item = &HunkOutcome> {
        self.hunks.iter().filter(|h| !h.applied())
    }
}

/// Check whether a patch applies, without producing the patched content.
///
/// The returned [`ApplyResult`] has `patched` set to `None`; consult
/// [`ApplyResult::hunks`] for the per-hunk outcome. This is the dry-run
/// counterpart of [`apply_fuzzy`].
pub fn dry_run(orig: &[u8], hunks: &[Hunk], options: &ApplyOptions) -> ApplyResult {
    let mut result = apply_fuzzy(orig, hunks, options);
    result.patched = None;
    result
}

/// Apply a patch, allowing hunks to be shifted and their context to be fuzzed.
///
/// Every hunk that matches is applied; if any hunk fails to match,
/// [`ApplyResult::patched`] is `None` and the failures are reported in
/// [`ApplyResult::hunks`].
pub fn apply_fuzzy(orig: &[u8], hunks: &[Hunk], options: &ApplyOptions) -> ApplyResult {
    let orig_lines: Vec<&[u8]> = splitlines(orig).collect();

    let mut outcomes = Vec::with_capacity(hunks.len());
    // Where each matched hunk starts in the original file, so they can be spliced
    // in one pass afterwards.
    let mut matches: Vec<(usize, &Hunk, usize)> = Vec::new();
    let mut failed = false;

    // Lines of the original file already consumed by an earlier hunk. Later hunks
    // must match after this point, so overlapping matches are impossible.
    let mut consumed = 0;
    // Running offset applied by earlier hunks; a hunk that moved is a good hint
    // for where the next one is.
    let mut last_offset: isize = 0;

    for (index, hunk) in hunks.iter().enumerate() {
        match find_hunk(&orig_lines, hunk, consumed, last_offset, options) {
            Some(m) => {
                // `m.start` is where the hunk's body matched; the fuzzed-away
                // leading context sits before it.
                let hunk_start = m.start - m.fuzz_start;
                let expected = hunk.orig_pos.saturating_sub(1) as isize;
                consumed = hunk_start + orig_line_count(hunk);
                last_offset = hunk_start as isize - expected;
                outcomes.push(HunkOutcome {
                    index,
                    status: HunkStatus::Applied,
                    offset: last_offset,
                    fuzz: m.fuzz_start.max(m.fuzz_end),
                });
                matches.push((m.start, hunk, m.fuzz_start));
            }
            None => {
                failed = true;
                outcomes.push(HunkOutcome {
                    index,
                    status: HunkStatus::Failed,
                    offset: 0,
                    fuzz: 0,
                });
            }
        }
    }

    let patched = if failed {
        None
    } else {
        Some(splice(&orig_lines, &matches))
    };

    ApplyResult {
        patched,
        hunks: outcomes,
    }
}

/// A successful match of a hunk against the original file.
struct Match {
    /// Index into the original lines where the hunk's first non-fuzzed line matched.
    start: usize,

    /// Number of leading context lines ignored.
    fuzz_start: usize,

    /// Number of trailing context lines ignored.
    fuzz_end: usize,
}

/// The lines of `hunk` that must be present in the original file.
fn orig_side(hunk: &Hunk) -> Vec<&[u8]> {
    hunk.lines
        .iter()
        .filter_map(|l| match l {
            HunkLine::ContextLine(b) | HunkLine::RemoveLine(b) => Some(b.as_slice()),
            HunkLine::InsertLine(_) => None,
        })
        .collect()
}

fn orig_line_count(hunk: &Hunk) -> usize {
    hunk.lines
        .iter()
        .filter(|l| matches!(l, HunkLine::ContextLine(_) | HunkLine::RemoveLine(_)))
        .count()
}

/// Number of leading lines of the hunk that are context (and so may be fuzzed away).
fn leading_context(hunk: &Hunk) -> usize {
    hunk.lines
        .iter()
        .take_while(|l| matches!(l, HunkLine::ContextLine(_)))
        .count()
}

/// Number of trailing lines of the hunk that are context.
fn trailing_context(hunk: &Hunk) -> usize {
    hunk.lines
        .iter()
        .rev()
        .take_while(|l| matches!(l, HunkLine::ContextLine(_)))
        .count()
}

/// Find where `hunk` matches in `orig_lines`.
///
/// This follows `locate_hunk` in GNU patch. A hunk's two context runs are compared
/// against each other: if one side is shorter, `diff` must have run into the start
/// or end of the original file there, and the hunk is pinned to that edge. Only a
/// hunk with balanced context is free to slide.
///
/// Fuzz is tried in increasing order, so a match ignoring less context always wins
/// over one ignoring more. Within a fuzz level, positions are tried in increasing
/// distance from the expected one, preferring later positions on a tie.
fn find_hunk(
    orig_lines: &[&[u8]],
    hunk: &Hunk,
    min_start: usize,
    last_offset: isize,
    options: &ApplyOptions,
) -> Option<Match> {
    let lines = orig_side(hunk);
    let leading = leading_context(hunk);
    let trailing = trailing_context(hunk);
    let context = leading.max(trailing);

    // A hunk with no lines on the original side (a pure insertion) matches anywhere.
    // The position the header claims, shifted by how far earlier hunks moved.
    let expected = (hunk.orig_pos.saturating_sub(1) as isize + last_offset).max(0) as usize;

    if lines.is_empty() {
        return (expected >= min_start && expected <= orig_lines.len()).then_some(Match {
            start: expected,
            fuzz_start: 0,
            fuzz_end: 0,
        });
    }

    for fuzz in 0..=options.fuzz.min(context) {
        // How many lines to ignore at each end. A side with less context than the
        // other starts out "over-fuzzed": the shortfall is negative, and marks that
        // side as pinned to its edge of the file rather than something to skip.
        let prefix_fuzz = fuzz as isize + leading as isize - context as isize;
        let suffix_fuzz = fuzz as isize + trailing as isize - context as isize;

        let at_file_start = prefix_fuzz < 0 && hunk.orig_pos <= 1;
        let at_file_end = suffix_fuzz < 0;

        let fuzz_start = prefix_fuzz.max(0) as usize;
        let fuzz_end = suffix_fuzz.max(0) as usize;
        let body = &lines[fuzz_start..lines.len() - fuzz_end];

        // The ignored leading context still occupies lines in the original, so the
        // body starts that many lines after the hunk does.
        let body_expected = expected + fuzz_start;
        let body_min_start = min_start + fuzz_start;

        let found = if at_file_start {
            // Must sit at the top of the file.
            matches_here(orig_lines, body, fuzz_start, body_min_start).then_some(fuzz_start)
        } else if at_file_end {
            // Must end at the bottom of the file.
            orig_lines
                .len()
                .checked_sub(body.len())
                .filter(|&start| matches_here(orig_lines, body, start, body_min_start))
        } else {
            search(orig_lines, body, body_expected, body_min_start, options)
        };

        if let Some(start) = found {
            return Some(Match {
                start,
                fuzz_start,
                fuzz_end,
            });
        }
    }
    None
}

/// Whether `body` sits at exactly `start` in `orig_lines`.
fn matches_here(orig_lines: &[&[u8]], body: &[&[u8]], start: usize, min_start: usize) -> bool {
    start >= min_start
        && start + body.len() <= orig_lines.len()
        && matches_at(orig_lines, body, start)
}

/// Search `orig_lines` for `body`, scanning outward from `expected`.
///
/// `body` is never empty: a hunk with nothing on the original side is handled by
/// the caller.
///
/// Returns the index in `orig_lines` where `body` matches, or `None`.
fn search(
    orig_lines: &[&[u8]],
    body: &[&[u8]],
    expected: usize,
    min_start: usize,
    options: &ApplyOptions,
) -> Option<usize> {
    if body.len() > orig_lines.len() {
        return None;
    }

    let last_start = orig_lines.len() - body.len();
    // Furthest the search needs to wander to have considered every valid start
    // position, in either direction.
    let full = expected.max(last_start.saturating_sub(expected));
    let limit = options.max_offset.map_or(full, |m| m.min(full));

    for distance in 0..=limit {
        // Later positions first on a tie, matching patch(1).
        let forward = expected.checked_add(distance);
        if let Some(start) = forward {
            if start >= min_start && start <= last_start && matches_at(orig_lines, body, start) {
                return Some(start);
            }
        }
        if distance > 0 {
            if let Some(start) = expected.checked_sub(distance) {
                if start >= min_start && start <= last_start && matches_at(orig_lines, body, start)
                {
                    return Some(start);
                }
            }
        }
    }
    None
}

fn matches_at(orig_lines: &[&[u8]], body: &[&[u8]], start: usize) -> bool {
    orig_lines[start..start + body.len()] == *body
}

/// Build the patched content by replacing each matched hunk's original lines with
/// its modified lines.
fn splice(orig_lines: &[&[u8]], matches: &[(usize, &Hunk, usize)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut pos = 0;

    for (start, hunk, fuzz_start) in matches {
        // The hunk's leading fuzzed-away context lines are copied from the original
        // rather than from the hunk, since they did not have to match.
        let hunk_start = start - fuzz_start;
        for line in &orig_lines[pos..hunk_start] {
            out.extend_from_slice(line);
        }

        let mut orig_pos = hunk_start;
        for line in &hunk.lines {
            match line {
                HunkLine::InsertLine(b) => out.extend_from_slice(b),
                HunkLine::ContextLine(_) => {
                    // Copy context from the original: with fuzz, the two may differ.
                    if let Some(l) = orig_lines.get(orig_pos) {
                        out.extend_from_slice(l);
                    }
                    orig_pos += 1;
                }
                HunkLine::RemoveLine(_) => {
                    orig_pos += 1;
                }
            }
        }
        pos = orig_pos;
    }

    for line in &orig_lines[pos.min(orig_lines.len())..] {
        out.extend_from_slice(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unified::UnifiedPatch;

    fn parse(patch: &[u8]) -> UnifiedPatch {
        UnifiedPatch::parse_patch(splitlines(patch)).unwrap()
    }

    const SIMPLE: &[u8] = b"--- a\n+++ b\n@@ -1,3 +1,3 @@\n line 1\n-line 2\n+line two\n line 3\n";

    #[test]
    fn exact_match() {
        let patch = parse(SIMPLE);
        let result = apply_fuzzy(
            b"line 1\nline 2\nline 3\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(result.patched.unwrap(), b"line 1\nline two\nline 3\n");
        assert_eq!(result.hunks[0].offset, 0);
        assert_eq!(result.hunks[0].fuzz, 0);
    }

    #[test]
    fn offset_forward() {
        let patch = parse(SIMPLE);
        let result = apply_fuzzy(
            b"header\nheader\nline 1\nline 2\nline 3\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(
            result.patched.unwrap(),
            b"header\nheader\nline 1\nline two\nline 3\n"
        );
        assert_eq!(result.hunks[0].offset, 2);
    }

    #[test]
    fn offset_backward() {
        let patch = parse(b"--- a\n+++ b\n@@ -5,3 +5,3 @@\n line 1\n-line 2\n+line two\n line 3\n");
        let result = apply_fuzzy(
            b"line 1\nline 2\nline 3\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(result.patched.unwrap(), b"line 1\nline two\nline 3\n");
        assert_eq!(result.hunks[0].offset, -4);
    }

    /// With a match equidistant before and after the expected position, patch(1)
    /// takes the later one.
    #[test]
    fn ties_prefer_the_later_match() {
        let patch = parse(b"--- a\n+++ b\n@@ -3,2 +3,2 @@\n a\n-b\n+B\n");
        let result = apply_fuzzy(b"a\nb\nX\na\nb\n", &patch.hunks, &ApplyOptions::default());
        assert_eq!(result.patched.unwrap(), b"a\nb\nX\na\nB\n");
        assert_eq!(result.hunks[0].offset, 1);
    }

    #[test]
    fn max_offset_rejects_distant_match() {
        let patch = parse(SIMPLE);
        let orig = b"a\nb\nc\nd\ne\nline 1\nline 2\nline 3\n";
        let options = ApplyOptions {
            max_offset: Some(2),
            ..ApplyOptions::default()
        };
        let result = apply_fuzzy(orig, &patch.hunks, &options);
        assert!(!result.is_success());

        let options = ApplyOptions {
            max_offset: Some(5),
            ..ApplyOptions::default()
        };
        let result = apply_fuzzy(orig, &patch.hunks, &options);
        assert!(result.is_success());
        assert_eq!(result.hunks[0].offset, 5);
    }

    #[test]
    fn fuzz_ignores_changed_leading_context() {
        let patch = parse(
            b"--- a\n+++ b\n@@ -1,4 +1,4 @@\n line 1\n line 2\n-line 3\n+line three\n line 4\n",
        );
        let orig = b"CHANGED\nline 2\nline 3\nline 4\n";

        let result = apply_fuzzy(orig, &patch.hunks, &ApplyOptions::default());
        assert!(!result.is_success());
        assert_eq!(result.hunks[0].status, HunkStatus::Failed);

        let result = apply_fuzzy(orig, &patch.hunks, &ApplyOptions::with_fuzz(1));
        assert_eq!(
            result.patched.unwrap(),
            b"CHANGED\nline 2\nline three\nline 4\n"
        );
        assert_eq!(result.hunks[0].fuzz, 1);
        assert_eq!(result.hunks[0].offset, 0);
    }

    #[test]
    fn fuzz_ignores_changed_trailing_context() {
        let patch = parse(
            b"--- a\n+++ b\n@@ -1,4 +1,4 @@\n line 1\n-line 2\n+line two\n line 3\n line 4\n",
        );
        let orig = b"line 1\nline 2\nline 3\nCHANGED\n";

        let result = apply_fuzzy(orig, &patch.hunks, &ApplyOptions::default());
        assert!(!result.is_success());

        let result = apply_fuzzy(orig, &patch.hunks, &ApplyOptions::with_fuzz(1));
        assert_eq!(
            result.patched.unwrap(),
            b"line 1\nline two\nline 3\nCHANGED\n"
        );
        assert_eq!(result.hunks[0].fuzz, 1);
    }

    #[test]
    fn fuzz_does_not_ignore_removed_lines() {
        let patch = parse(SIMPLE);
        // The line the hunk removes does not match, and no amount of fuzz makes a
        // removed line optional.
        let result = apply_fuzzy(
            b"line 1\nCHANGED\nline 3\n",
            &patch.hunks,
            &ApplyOptions::with_fuzz(3),
        );
        assert!(!result.is_success());
    }

    #[test]
    fn fuzz_prefers_exact_match() {
        let patch = parse(b"--- a\n+++ b\n@@ -1,3 +1,3 @@\n line 1\n-line 2\n+line two\n line 3\n");
        let result = apply_fuzzy(
            b"line 1\nline 2\nline 3\n",
            &patch.hunks,
            &ApplyOptions::with_fuzz(2),
        );
        assert_eq!(result.hunks[0].fuzz, 0);
        assert_eq!(result.patched.unwrap(), b"line 1\nline two\nline 3\n");
    }

    #[test]
    fn dry_run_reports_without_patching() {
        let patch = parse(SIMPLE);
        let result = dry_run(
            b"line 1\nline 2\nline 3\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(result.patched, None);
        assert!(result.hunks[0].applied());
        assert_eq!(result.rejected().count(), 0);
    }

    #[test]
    fn dry_run_reports_rejected_hunks() {
        let patch = parse(
            b"--- a\n+++ b\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n@@ -4,3 +4,3 @@\n d\n-e\n+E\n f\n",
        );
        // The second hunk's `e` has been replaced, so only the first applies.
        let orig = b"a\nb\nc\nd\nNOPE\nf\n";
        let result = dry_run(orig, &patch.hunks, &ApplyOptions::default());
        assert_eq!(result.patched, None);
        assert_eq!(result.hunks.len(), 2);
        assert!(result.hunks[0].applied());
        assert!(!result.hunks[1].applied());
        assert_eq!(
            result.rejected().map(|h| h.index).collect::<Vec<_>>(),
            vec![1]
        );
    }

    #[test]
    fn multiple_hunks() {
        let patch = parse(
            b"--- a\n+++ b\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n@@ -4,3 +4,3 @@\n d\n-e\n+E\n f\n",
        );
        let result = apply_fuzzy(
            b"a\nb\nc\nd\ne\nf\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(result.patched.unwrap(), b"a\nB\nc\nd\nE\nf\n");
    }

    #[test]
    fn hunks_do_not_overlap() {
        // Both hunks match the same text; the second must be found after the first
        // rather than settling on the same lines.
        let patch = parse(
            b"--- a\n+++ b\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n@@ -4,3 +4,3 @@\n a\n-b\n+C\n c\n",
        );
        let result = apply_fuzzy(
            b"a\nb\nc\na\nb\nc\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(result.patched.unwrap(), b"a\nB\nc\na\nC\nc\n");
        assert_eq!(result.hunks[1].offset, 0);
    }

    #[test]
    fn pure_insertion() {
        let patch = parse(b"--- a\n+++ b\n@@ -0,0 +1,1 @@\n+new\n");
        let result = apply_fuzzy(b"a\nb\n", &patch.hunks, &ApplyOptions::default());
        assert_eq!(result.patched.unwrap(), b"new\na\nb\n");
    }

    #[test]
    fn empty_patch_is_identity() {
        let result = apply_fuzzy(b"a\nb\n", &[], &ApplyOptions::default());
        assert_eq!(result.patched.unwrap(), b"a\nb\n");
        assert!(result.hunks.is_empty());
    }

    #[test]
    fn missing_trailing_newline_is_preserved() {
        let patch = parse(
            b"--- a\n+++ b\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+B\n\\ No newline at end of file\n",
        );
        let result = apply_fuzzy(b"a\nb", &patch.hunks, &ApplyOptions::default());
        assert_eq!(result.patched.unwrap(), b"a\nB");
    }

    /// A hunk with less trailing than leading context is one where `diff` ran into
    /// the end of the file, so it has to end at the end of the file here too. It
    /// will not slide to a match that has lines after it.
    #[test]
    fn short_trailing_context_pins_the_hunk_to_eof() {
        // Three leading context lines, one trailing: the original ended at `e`.
        let patch = parse(b"--- a\n+++ b\n@@ -1,5 +1,5 @@\n c1\n c2\n c3\n-d\n+D\n e\n");

        // The file carries on past the match, so the hunk is not at EOF: rejected.
        let result = apply_fuzzy(
            b"c1\nc2\nc3\nd\ne\nEXTRA\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert!(!result.is_success());

        // At the real end of the file it applies.
        let result = apply_fuzzy(
            b"c1\nc2\nc3\nd\ne\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(result.patched.unwrap(), b"c1\nc2\nc3\nD\ne\n");
    }

    /// The EOF-anchored hunk may still shift, as long as it lands against the end.
    #[test]
    fn eof_anchored_hunk_still_shifts() {
        let patch = parse(b"--- a\n+++ b\n@@ -1,5 +1,5 @@\n c1\n c2\n c3\n-d\n+D\n e\n");
        let result = apply_fuzzy(
            b"PRE\nc1\nc2\nc3\nd\ne\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(result.patched.unwrap(), b"PRE\nc1\nc2\nc3\nD\ne\n");
        assert_eq!(result.hunks[0].offset, 1);
    }

    /// Symmetrically, a hunk at line 1 with less leading than trailing context ran
    /// into the start of the file, and must stay there.
    #[test]
    fn short_leading_context_pins_the_hunk_to_file_start() {
        let patch = parse(b"--- a\n+++ b\n@@ -1,4 +1,4 @@\n-a\n+A\n b\n c\n d\n");

        assert!(
            !apply_fuzzy(b"PRE\na\nb\nc\nd\n", &patch.hunks, &ApplyOptions::default()).is_success()
        );

        let result = apply_fuzzy(b"a\nb\nc\nd\n", &patch.hunks, &ApplyOptions::default());
        assert_eq!(result.patched.unwrap(), b"A\nb\nc\nd\n");
    }

    /// A hunk with balanced context is free to slide in either direction.
    #[test]
    fn balanced_context_hunks_are_free_to_move() {
        let patch = parse(b"--- a\n+++ b\n@@ -2,7 +2,7 @@\n c1\n c2\n c3\n-d\n+D\n e1\n e2\n e3\n");
        let result = apply_fuzzy(
            b"PRE\nPRE\nc1\nc2\nc3\nd\ne1\ne2\ne3\nTAIL\n",
            &patch.hunks,
            &ApplyOptions::default(),
        );
        assert_eq!(
            result.patched.unwrap(),
            b"PRE\nPRE\nc1\nc2\nc3\nD\ne1\ne2\ne3\nTAIL\n"
        );
        assert_eq!(result.hunks[0].offset, 1);
    }

    #[test]
    fn hunk_longer_than_file_fails() {
        let patch = parse(SIMPLE);
        let result = apply_fuzzy(b"line 1\n", &patch.hunks, &ApplyOptions::with_fuzz(2));
        assert!(!result.is_success());
    }
}
