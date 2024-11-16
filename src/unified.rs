//! Parsing of unified patches
use crate::{ContentPatch, SingleFilePatch};
use regex::bytes::Regex;
use std::num::ParseIntError;

/// Errors that can occur while parsing a patch
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The files are binary and differ
    BinaryFiles(Vec<u8>, Vec<u8>),

    /// A syntax error in the patch
    PatchSyntax(&'static str, Vec<u8>),

    /// A malformed patch header
    MalformedPatchHeader(&'static str, Vec<u8>),

    /// A malformed hunk header
    MalformedHunkHeader(String, Vec<u8>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::BinaryFiles(oldname, newname) => {
                write!(
                    f,
                    "Binary files {} and {} differ",
                    String::from_utf8_lossy(oldname),
                    String::from_utf8_lossy(newname)
                )
            }
            Self::PatchSyntax(msg, line) => write!(f, "Patch syntax error: {} in {:?}", msg, line),
            Self::MalformedPatchHeader(msg, line) => {
                write!(
                    f,
                    "Malformed patch header: {} in {}",
                    msg,
                    String::from_utf8_lossy(line)
                )
            }
            Self::MalformedHunkHeader(msg, line) => {
                write!(
                    f,
                    "Malformed hunk header: {} in {}",
                    msg,
                    String::from_utf8_lossy(line)
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// Split lines but preserve trailing newlines
pub fn splitlines(data: &[u8]) -> impl Iterator<Item = &'_ [u8]> {
    let mut start = 0;
    let mut end = 0;
    std::iter::from_fn(move || loop {
        if end == data.len() {
            if start == end {
                return None;
            }
            let line = &data[start..end];
            start = end;
            return Some(line);
        }
        let c = data[end];
        end += 1;
        if c == b'\n' {
            let line = &data[start..end];
            start = end;
            return Some(line);
        }
    })
}

#[cfg(test)]
mod splitlines_tests {
    #[test]
    fn test_simple() {
        let data = b"line 1\nline 2\nline 3\n";
        let lines: Vec<&[u8]> = super::splitlines(data).collect();
        assert_eq!(
            lines,
            vec![
                "line 1\n".as_bytes(),
                "line 2\n".as_bytes(),
                "line 3\n".as_bytes()
            ]
        );
    }

    #[test]
    fn test_no_trailing() {
        let data = b"line 1\nline 2\nline 3";
        let lines: Vec<&[u8]> = super::splitlines(data).collect();
        assert_eq!(
            lines,
            vec![&b"line 1\n"[..], &b"line 2\n"[..], &b"line 3"[..]]
        );
    }

    #[test]
    fn test_empty_line() {
        let data = b"line 1\n\nline 3\n";
        let lines: Vec<&[u8]> = super::splitlines(data).collect();
        assert_eq!(lines, vec![&b"line 1\n"[..], &b"\n"[..], &b"line 3\n"[..]]);
    }
}

/// The string that indicates that a line has no newline
pub const NO_NL: &[u8] = b"\\ No newline at end of file\n";

/// Iterate through a series of lines, ensuring that lines
/// that originally had no terminating newline are produced
/// without one.
pub fn iter_lines_handle_nl<'a, I>(mut iter_lines: I) -> impl Iterator<Item = &'a [u8]> + 'a
where
    I: Iterator<Item = &'a [u8]> + 'a,
{
    let mut last_line: Option<&'a [u8]> = None;
    std::iter::from_fn(move || {
        for line in iter_lines.by_ref() {
            if line == NO_NL {
                if let Some(last) = last_line.as_mut() {
                    assert!(last.ends_with(b"\n"));
                    // Drop the last newline from `last`
                    *last = &last[..last.len() - 1];
                } else {
                    panic!("No newline indicator without previous line");
                }
            } else {
                if let Some(last) = last_line.take() {
                    last_line = Some(line);
                    return Some(last);
                }
                last_line = Some(line);
            }
        }
        last_line.take()
    })
}

#[test]
fn test_iter_lines_handle_nl() {
    let lines = vec![
        &b"line 1\n"[..],
        &b"line 2\n"[..],
        &b"line 3\n"[..],
        &b"line 4\n"[..],
        &b"\\ No newline at end of file\n"[..],
    ];
    let mut iter = iter_lines_handle_nl(lines.into_iter());
    assert_eq!(iter.next(), Some("line 1\n".as_bytes()));
    assert_eq!(iter.next(), Some("line 2\n".as_bytes()));
    assert_eq!(iter.next(), Some("line 3\n".as_bytes()));
    assert_eq!(iter.next(), Some("line 4".as_bytes()));
    assert_eq!(iter.next(), None);
}

static BINARY_FILES_RE: once_cell::sync::Lazy<regex::bytes::Regex> =
    once_cell::sync::Lazy::new(|| {
        lazy_regex::BytesRegex::new(r"^Binary files (.+) and (.+) differ").unwrap()
    });

/// Get the names of the files in a patch
pub fn get_patch_names<'a, T: Iterator<Item = &'a [u8]>>(
    iter_lines: &mut T,
) -> Result<((Vec<u8>, Option<Vec<u8>>), (Vec<u8>, Option<Vec<u8>>)), Error> {
    let line = iter_lines
        .next()
        .ok_or_else(|| Error::PatchSyntax("No input", vec![]))?;

    if let Some(captures) = BINARY_FILES_RE.captures(line) {
        let orig_name = captures.get(1).unwrap().as_bytes().to_vec();
        let mod_name = captures.get(2).unwrap().as_bytes().to_vec();
        return Err(Error::BinaryFiles(orig_name, mod_name));
    }
    let orig_name = line
        .strip_prefix(b"--- ")
        .ok_or_else(|| Error::MalformedPatchHeader("No orig name", line.to_vec()))?
        .strip_suffix(b"\n")
        .ok_or_else(|| Error::PatchSyntax("missing newline", line.to_vec()))?;
    let (orig_name, orig_ts) = match orig_name.split(|&c| c == b'\t').collect::<Vec<_>>()[..] {
        [name, ts] => (name.to_vec(), Some(ts.to_vec())),
        [name] => (name.to_vec(), None),
        _ => return Err(Error::MalformedPatchHeader("No orig line", line.to_vec())),
    };

    let line = iter_lines
        .next()
        .ok_or_else(|| Error::PatchSyntax("No input", vec![]))?;

    let (mod_name, mod_ts) = match line.strip_prefix(b"+++ ") {
        Some(line) => {
            let mod_name = line
                .strip_suffix(b"\n")
                .ok_or_else(|| Error::PatchSyntax("missing newline", line.to_vec()))?;
            let (mod_name, mod_ts) = match mod_name.split(|&c| c == b'\t').collect::<Vec<_>>()[..] {
                [name, ts] => (name.to_vec(), Some(ts.to_vec())),
                [name] => (name.to_vec(), None),
                _ => return Err(Error::PatchSyntax("Invalid mod name", line.to_vec())),
            };
            (mod_name, mod_ts)
        }
        None => return Err(Error::MalformedPatchHeader("No mod line", line.to_vec())),
    };

    Ok(((orig_name, orig_ts), (mod_name, mod_ts)))
}

#[cfg(test)]
mod get_patch_names_tests {
    #[test]
    fn test_simple() {
        let lines = [
            &b"--- baz	2009-10-14 19:49:59 +0000\n"[..],
            &b"+++ quxx	2009-10-14 19:51:00 +0000\n"[..],
        ];
        let mut iter = lines.into_iter();
        let (old, new) = super::get_patch_names(&mut iter).unwrap();
        assert_eq!(
            old,
            (b"baz".to_vec(), Some(b"2009-10-14 19:49:59 +0000".to_vec()))
        );
        assert_eq!(
            new,
            (
                b"quxx".to_vec(),
                Some(b"2009-10-14 19:51:00 +0000".to_vec())
            )
        );
    }

    #[test]
    fn test_binary() {
        let lines = [&b"Binary files qoo and bar differ\n"[..]];
        let mut iter = lines.into_iter();
        let e = super::get_patch_names(&mut iter).unwrap_err();
        assert_eq!(
            e,
            super::Error::BinaryFiles(b"qoo".to_vec(), b"bar".to_vec())
        );
    }
}

/// Iterate over the hunks in a patch
///
/// # Arguments
/// * `iter_lines`: Iterator over lines
pub fn iter_hunks<'a, I>(iter_lines: &mut I) -> impl Iterator<Item = Result<Hunk, Error>> + '_
where
    I: Iterator<Item = &'a [u8]>,
{
    std::iter::from_fn(move || {
        while let Some(line) = iter_lines.next() {
            if line == b"\n" {
                continue;
            }
            match Hunk::from_header(line) {
                Ok(mut new_hunk) => {
                    let mut orig_size = 0;
                    let mut mod_size = 0;
                    while orig_size < new_hunk.orig_range || mod_size < new_hunk.mod_range {
                        let line = iter_lines.next()?;
                        match HunkLine::parse_line(line) {
                            Err(_) => {
                                return Some(Err(Error::PatchSyntax(
                                    "Invalid hunk line",
                                    line.to_vec(),
                                )));
                            }
                            Ok(hunk_line) => {
                                if matches!(
                                    hunk_line,
                                    HunkLine::RemoveLine(_) | HunkLine::ContextLine(_)
                                ) {
                                    orig_size += 1
                                }
                                if matches!(
                                    hunk_line,
                                    HunkLine::InsertLine(_) | HunkLine::ContextLine(_)
                                ) {
                                    mod_size += 1
                                }
                                new_hunk.lines.push(hunk_line);
                            }
                        }
                    }
                    return Some(Ok(new_hunk));
                }
                Err(MalformedHunkHeader(m, l)) => {
                    return Some(Err(Error::MalformedHunkHeader(m.to_string(), l)));
                }
            }
        }
        None
    })
}

#[cfg(test)]
mod iter_hunks_tests {
    use super::*;

    #[test]
    fn test_iter_hunks() {
        let mut lines = super::splitlines(
            br#"@@ -391,6 +391,8 @@
                 else:
                     assert isinstance(hunk_line, RemoveLine)
                 line_no += 1
+    for line in orig_lines:
+        yield line
                     
 import unittest
 import os.path

"#,
        );

        let hunks = super::iter_hunks(&mut lines)
            .collect::<Result<Vec<Hunk>, Error>>()
            .unwrap();

        let mut expected_hunk = Hunk::new(391, 6, 391, 8, None);
        expected_hunk.lines.extend([
            HunkLine::ContextLine(b"                else:\n".to_vec()),
            HunkLine::ContextLine(
                b"                    assert isinstance(hunk_line, RemoveLine)\n".to_vec(),
            ),
            HunkLine::ContextLine(b"                line_no += 1\n".to_vec()),
            HunkLine::InsertLine(b"    for line in orig_lines:\n".to_vec()),
            HunkLine::InsertLine(b"        yield line\n".to_vec()),
            HunkLine::ContextLine(b"                    \n".to_vec()),
            HunkLine::ContextLine(b"import unittest\n".to_vec()),
            HunkLine::ContextLine(b"import os.path\n".to_vec()),
        ]);

        assert_eq!(&expected_hunk, hunks.first().unwrap());
    }
}

/// Parse a patch file
///
/// # Arguments
/// * `iter_lines`: Iterator over lines
pub fn parse_patch<'a, I>(iter_lines: I) -> Result<PlainOrBinaryPatch, Error>
where
    I: Iterator<Item = &'a [u8]> + 'a,
{
    let mut iter_lines = iter_lines_handle_nl(iter_lines);

    let ((orig_name, orig_ts), (mod_name, mod_ts)) = match get_patch_names(&mut iter_lines) {
        Ok(names) => names,
        Err(Error::BinaryFiles(orig_name, mod_name)) => {
            return Ok(PlainOrBinaryPatch::Binary(BinaryPatch(orig_name, mod_name)));
        }
        Err(e) => return Err(e),
    };

    let mut patch = UnifiedPatch::new(orig_name, orig_ts, mod_name, mod_ts);
    for hunk in iter_hunks(&mut iter_lines) {
        patch.hunks.push(hunk?);
    }
    Ok(PlainOrBinaryPatch::Plain(patch))
}

#[cfg(test)]
mod patches_tests {
    use super::*;
    macro_rules! test_patch {
        ($name:ident, $orig:expr, $mod:expr, $patch:expr) => {
            #[test]
            fn $name() {
                let orig = include_bytes!(concat!("../test_patches_data/", $orig));
                let modi = include_bytes!(concat!("../test_patches_data/", $mod));
                let patch = include_bytes!(concat!("../test_patches_data/", $patch));
                let parsed = super::parse_patch(super::splitlines(patch)).unwrap();
                let mut patched = Vec::new();
                let mut iter = parsed.apply_exact(orig).unwrap().into_iter();
                while let Some(line) = iter.next() {
                    patched.push(line);
                }
                assert_eq!(patched, modi);
            }
        };
    }

    test_patch!(test_patch_2, "orig-2", "mod-2", "diff-2");
    test_patch!(test_patch_3, "orig-3", "mod-3", "diff-3");
    test_patch!(test_patch_4, "orig-4", "mod-4", "diff-4");
    test_patch!(test_patch_5, "orig-5", "mod-5", "diff-5");
    test_patch!(test_patch_6, "orig-6", "mod-6", "diff-6");
    test_patch!(test_patch_7, "orig-7", "mod-7", "diff-7");
}

/// Conflict applying a patch
#[derive(Debug)]
pub struct PatchConflict {
    line_no: usize,
    orig_line: Vec<u8>,
    patch_line: Vec<u8>,
}

impl std::fmt::Display for PatchConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Patch conflict at orig line {}: orig: {:?}, patch: {:?}",
            self.line_no,
            String::from_utf8_lossy(&self.orig_line),
            String::from_utf8_lossy(&self.patch_line)
        )
    }
}

impl std::error::Error for PatchConflict {}

struct PatchedIter<H: Iterator<Item = Hunk>, L: Iterator<Item = Vec<u8>>> {
    orig_lines: L,
    hunk_lines: Vec<HunkLine>,
    hunks: std::iter::Peekable<H>,
    line_no: usize,
}

impl<H: Iterator<Item = Hunk>, L: Iterator<Item = Vec<u8>>> Iterator for PatchedIter<H, L> {
    type Item = Result<Vec<u8>, PatchConflict>;

    fn next(&mut self) -> Option<Result<Vec<u8>, PatchConflict>> {
        loop {
            // First, check if we just need to yield the next line from the original file.
            match self.hunks.peek_mut() {
                // We're ahead of the next hunk. Yield the next line from the original file.
                Some(hunk) if self.line_no < hunk.orig_pos => {
                    self.line_no += 1;
                    if let Some(line) = self.orig_lines.next() {
                        return Some(Ok(line));
                    } else {
                        return Some(Err(PatchConflict {
                            line_no: self.line_no,
                            orig_line: Vec::new(),
                            patch_line: Vec::new(),
                        }));
                    }
                }
                // There are no more hunks. Yield the rest of the original file.
                None => {
                    if let Some(line) = self.orig_lines.next() {
                        return Some(Ok(line));
                    } else {
                        return None;
                    }
                }
                Some(_hunk) => {
                    // We're in a hunk. Check if we need to yield a line from the hunk.
                    if let Some(line) = self.hunk_lines.pop() {
                        match line {
                            HunkLine::ContextLine(bytes) => {
                                if let Some(orig_line) = self.orig_lines.next() {
                                    if orig_line != bytes {
                                        return Some(Err(PatchConflict {
                                            line_no: self.line_no,
                                            orig_line,
                                            patch_line: bytes,
                                        }));
                                    }
                                } else {
                                    return Some(Err(PatchConflict {
                                        line_no: self.line_no,
                                        orig_line: Vec::new(),
                                        patch_line: bytes,
                                    }));
                                }
                                self.line_no += 1;
                                return Some(Ok(bytes));
                            }
                            HunkLine::InsertLine(bytes) => {
                                return Some(Ok(bytes));
                            }
                            HunkLine::RemoveLine(bytes) => {
                                if let Some(orig_line) = self.orig_lines.next() {
                                    if orig_line != bytes {
                                        return Some(Err(PatchConflict {
                                            line_no: self.line_no,
                                            orig_line,
                                            patch_line: bytes,
                                        }));
                                    }
                                } else {
                                    return Some(Err(PatchConflict {
                                        line_no: self.line_no,
                                        orig_line: Vec::new(),
                                        patch_line: bytes,
                                    }));
                                }
                                self.line_no += 1;
                            }
                        }
                    } else {
                        self.hunks.next();
                        if let Some(h) = self.hunks.peek_mut() {
                            let mut hunk_lines = h.lines.drain(..).collect::<Vec<_>>();
                            hunk_lines.reverse();
                            self.hunk_lines = hunk_lines;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod iter_exact_patched_from_hunks_tests {
    use super::*;
    #[test]
    fn test_just_context() {
        let orig_lines = vec![
            b"line 1\n".to_vec(),
            b"line 2\n".to_vec(),
            b"line 3\n".to_vec(),
            b"line 4\n".to_vec(),
        ];
        let mut hunk = Hunk::new(1, 1, 1, 1, None);
        hunk.lines.push(HunkLine::ContextLine(b"line 1\n".to_vec()));
        let hunks = vec![hunk];
        let result =
            super::iter_exact_patched_from_hunks(orig_lines.into_iter(), hunks.into_iter())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        assert_eq!(
            &result,
            &[
                b"line 1\n".to_vec(),
                b"line 2\n".to_vec(),
                b"line 3\n".to_vec(),
                b"line 4\n".to_vec(),
            ]
        );
    }

    #[test]
    fn test_insert() {
        let orig_lines = vec![
            b"line 1\n".to_vec(),
            b"line 2\n".to_vec(),
            b"line 3\n".to_vec(),
            b"line 4\n".to_vec(),
        ];
        let mut hunk = Hunk::new(1, 0, 1, 1, None);
        hunk.lines.push(HunkLine::InsertLine(b"line 0\n".to_vec()));
        hunk.lines.push(HunkLine::ContextLine(b"line 1\n".to_vec()));
        let hunks = vec![hunk];
        let result =
            super::iter_exact_patched_from_hunks(orig_lines.into_iter(), hunks.into_iter())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        assert_eq!(
            &result,
            &[
                b"line 0\n".to_vec(),
                b"line 1\n".to_vec(),
                b"line 2\n".to_vec(),
                b"line 3\n".to_vec(),
                b"line 4\n".to_vec(),
            ]
        );
    }
}

/// Iterate through a series of lines with a patch applied.
///
/// This handles a single file, and does exact, not fuzzy patching.
///
/// Args:
///   orig_lines: The original lines of the file.
///   hunks: The hunks to apply to the file.
pub fn iter_exact_patched_from_hunks<'a>(
    orig_lines: impl Iterator<Item = Vec<u8>> + 'a,
    hunks: impl Iterator<Item = Hunk>,
) -> impl Iterator<Item = Result<Vec<u8>, PatchConflict>> {
    let mut hunks = hunks.peekable();
    let mut hunk_lines = if let Some(h) = hunks.peek_mut() {
        h.lines.drain(..).collect()
    } else {
        Vec::new()
    };
    hunk_lines.reverse();
    PatchedIter {
        orig_lines,
        hunks,
        line_no: 1,
        hunk_lines,
    }
}

/// Find the index of the first character that differs between two texts
pub fn difference_index(atext: &[u8], btext: &[u8]) -> Option<usize> {
    let length = atext.len().min(btext.len());
    (0..length).find(|&i| atext[i] != btext[i])
}

/// Parse a patch file
#[derive(PartialEq, Eq)]
pub enum FileEntry {
    /// Non-patch data
    Junk(Vec<Vec<u8>>),
    /// A meta entry
    Meta(Vec<u8>),
    /// A patch entry
    Patch(Vec<Vec<u8>>),
}

impl std::fmt::Debug for FileEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Junk(lines) => {
                write!(f, "Junk[")?;
                // Print the lines interspersed with commas
                for (i, line) in lines.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", String::from_utf8_lossy(line))?;
                }
                write!(f, "]")?;
                Ok(())
            }
            Self::Meta(line) => write!(f, "Meta({:?})", String::from_utf8_lossy(line)),
            Self::Patch(lines) => {
                write!(f, "Patch[")?;
                // Print the lines interspersed with commas
                for (i, line) in lines.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", String::from_utf8_lossy(line))?;
                }
                write!(f, "]")?;
                Ok(())
            }
        }
    }
}

struct FileEntryIter<I> {
    iter: I,
    saved_lines: Vec<Vec<u8>>,
    is_dirty: bool,
    orig_range: usize,
    mod_range: usize,
}

impl<I> FileEntryIter<I>
where
    I: Iterator<Item = Vec<u8>>,
{
    fn entry(&mut self) -> Option<FileEntry> {
        if !self.saved_lines.is_empty() {
            let lines = self.saved_lines.drain(..).collect();
            if self.is_dirty {
                Some(FileEntry::Junk(lines))
            } else {
                Some(FileEntry::Patch(lines))
            }
        } else {
            None
        }
    }
}

impl<I> Iterator for FileEntryIter<I>
where
    I: Iterator<Item = Vec<u8>>,
{
    type Item = Result<FileEntry, Error>;

    fn next(&mut self) -> Option<Result<FileEntry, Error>> {
        loop {
            let line = match self.iter.next() {
                Some(line) => line,
                None => {
                    if let Some(entry) = self.entry() {
                        return Some(Ok(entry));
                    } else {
                        return None;
                    }
                }
            };
            if line.starts_with(b"=== ") {
                return Some(Ok(FileEntry::Meta(line)));
            } else if line.starts_with(b"*** ") {
                continue;
            } else if line.starts_with(b"#") {
                continue;
            } else if self.orig_range > 0 || self.mod_range > 0 {
                if line.starts_with(b"-") || line.starts_with(b" ") {
                    self.orig_range -= 1;
                }
                if line.starts_with(b"+") || line.starts_with(b" ") {
                    self.mod_range -= 1;
                }
                self.saved_lines.push(line);
            } else if line.starts_with(b"--- ") || BINARY_FILES_RE.is_match(line.as_slice()) {
                let entry = self.entry();
                self.is_dirty = false;
                self.saved_lines.push(line);
                if let Some(entry) = entry {
                    return Some(Ok(entry));
                }
            } else if line.starts_with(b"+++ ") && !self.is_dirty {
                self.saved_lines.push(line);
            } else if line.starts_with(b"@@") {
                let hunk = match Hunk::from_header(line.as_slice()) {
                    Ok(hunk) => hunk,
                    Err(e) => {
                        return Some(Err(Error::MalformedHunkHeader(e.to_string(), line.clone())));
                    }
                };
                self.orig_range = hunk.orig_range;
                self.mod_range = hunk.mod_range;
                self.saved_lines.push(line);
            } else {
                let entry = if !self.is_dirty { self.entry() } else { None };
                self.saved_lines.push(line);
                self.is_dirty = true;
                if let Some(entry) = entry {
                    return Some(Ok(entry));
                }
            }
        }
    }
}

/// Iterate through a series of lines.
///
/// # Arguments
/// * `orig` - The original lines of the file.
pub fn iter_file_patch<I>(orig: I) -> impl Iterator<Item = Result<FileEntry, Error>>
where
    I: Iterator<Item = Vec<u8>>,
{
    FileEntryIter {
        iter: orig,
        orig_range: 0,
        saved_lines: Vec::new(),
        is_dirty: false,
        mod_range: 0,
    }
}

#[cfg(test)]
mod iter_file_patch_tests {
    #[test]
    fn test_simple() {
        let lines = [
            "--- orig-3	2005-09-23 16:23:20.000000000 -0500\n",
            "+++ mod-3	2005-09-23 16:23:38.000000000 -0500\n",
            "@@ -1,3 +1,4 @@\n",
            "+First line change\n",
            " # Copyright (C) 2004, 2005 Aaron Bentley\n",
            " # <aaron.bentley@utoronto.ca>\n",
            " #\n",
        ];
        let iter = super::iter_file_patch(lines.into_iter().map(|l| l.as_bytes().to_vec()));
        let entries = iter.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            entries,
            vec![super::FileEntry::Patch(
                lines
                    .iter()
                    .map(|l| l.as_bytes().to_vec())
                    .collect::<Vec<_>>()
            )]
        );
    }

    #[test]
    fn test_noise() {
        let lines = [
            "=== modified file 'test.txt'\n",
            "--- orig-3	2005-09-23 16:23:20.000000000 -0500\n",
            "+++ mod-3	2005-09-23 16:23:38.000000000 -0500\n",
            "@@ -1,3 +1,4 @@\n",
            "+First line change\n",
            " # Copyright (C) 2004, 2005 Aaron Bentley\n",
            " # <aaron.bentley@utoronto.ca>\n",
            " #\n",
        ];
        let iter = super::iter_file_patch(lines.into_iter().map(|l| l.as_bytes().to_vec()));
        let entries = iter.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            entries,
            vec![
                super::FileEntry::Meta(lines[0].as_bytes().to_vec()),
                super::FileEntry::Patch(
                    lines
                        .iter()
                        .skip(1)
                        .map(|l| l.as_bytes().to_vec())
                        .collect::<Vec<_>>()
                )
            ]
        );
    }

    #[test]
    fn test_allow_dirty() {
        let lines = [
            "Foo bar\n",
            "Bar blah\n",
            "--- orig-3	2005-09-23 16:23:20.000000000 -0500\n",
            "+++ mod-3	2005-09-23 16:23:38.000000000 -0500\n",
            "@@ -1,3 +1,4 @@\n",
            "+First line change\n",
            " # Copyright (C) 2004, 2005 Aaron Bentley\n",
            " # <aaron.bentley@utoronto.ca>\n",
            " #\n",
        ];
        let iter = super::iter_file_patch(lines.into_iter().map(|l| l.as_bytes().to_vec()));
        let entries = iter.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            entries,
            vec![
                super::FileEntry::Junk(
                    lines
                        .iter()
                        .take(2)
                        .map(|l| l.as_bytes().to_vec())
                        .collect::<Vec<_>>()
                ),
                super::FileEntry::Patch(
                    lines
                        .iter()
                        .skip(2)
                        .map(|l| l.as_bytes().to_vec())
                        .collect::<Vec<_>>()
                )
            ]
        );
    }
}

/// A patch that can be applied to a single file
pub enum PlainOrBinaryPatch {
    /// A unified patch
    Plain(UnifiedPatch),

    /// An indication that two binary files differ
    Binary(BinaryPatch),
}

impl SingleFilePatch for PlainOrBinaryPatch {
    fn oldname(&self) -> &[u8] {
        match self {
            Self::Plain(patch) => patch.orig_name.as_slice(),
            Self::Binary(patch) => patch.0.as_slice(),
        }
    }

    fn newname(&self) -> &[u8] {
        match self {
            Self::Plain(patch) => patch.mod_name.as_slice(),
            Self::Binary(patch) => patch.1.as_slice(),
        }
    }
}

impl crate::ContentPatch for PlainOrBinaryPatch {
    fn apply_exact(&self, orig: &[u8]) -> Result<Vec<u8>, crate::ApplyError> {
        match self {
            Self::Plain(patch) => patch.apply_exact(orig),
            Self::Binary(_) => Err(crate::ApplyError::Unapplyable),
        }
    }
}

/// Parse a patch file
///
/// # Arguments
/// * `iter`: Iterator over lines
pub fn parse_patches<I>(iter: I) -> impl Iterator<Item = Result<PlainOrBinaryPatch, Error>>
where
    I: Iterator<Item = Vec<u8>>,
{
    iter_file_patch(iter).filter_map(|entry| match entry {
        Ok(FileEntry::Patch(lines)) => match parse_patch(lines.iter().map(|l| l.as_slice())) {
            Ok(patch) => Some(Ok(patch)),
            Err(e) => Some(Err(e)),
        },
        Ok(FileEntry::Junk(_)) => None,
        Ok(FileEntry::Meta(_)) => None,
        Err(e) => Some(Err(e)),
    })
}

#[cfg(test)]
mod parse_patches_tests {
    #[test]
    fn test_simple() {
        let lines = [
            "--- orig-3	2005-09-23 16:23:20.000000000 -0500\n",
            "+++ mod-3	2005-09-23 16:23:38.000000000 -0500\n",
            "@@ -1,3 +1,4 @@\n",
            "+First line change\n",
            " # Copyright (C) 2004, 2005 Aaron Bentley\n",
            " # <aaron.bentley@utoronto.ca>\n",
            " #\n",
        ];
        let patches =
            super::parse_patches(lines.iter().map(|l| l.as_bytes().to_vec())).collect::<Vec<_>>();
        assert_eq!(patches.len(), 1);
    }
}

/// A binary patch
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryPatch(pub Vec<u8>, pub Vec<u8>);

impl SingleFilePatch for BinaryPatch {
    fn oldname(&self) -> &[u8] {
        &self.0
    }

    fn newname(&self) -> &[u8] {
        &self.1
    }
}

impl crate::ContentPatch for BinaryPatch {
    fn apply_exact(&self, _orig: &[u8]) -> Result<Vec<u8>, crate::ApplyError> {
        Err(crate::ApplyError::Unapplyable)
    }
}

/// A unified diff style patch
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedPatch {
    /// Name of the original file
    pub orig_name: Vec<u8>,

    /// Timestamp for the original file
    pub orig_ts: Option<Vec<u8>>,

    /// Name of the modified file
    pub mod_name: Vec<u8>,

    /// Timestamp for the modified file
    pub mod_ts: Option<Vec<u8>>,

    /// List of hunks
    pub hunks: Vec<Hunk>,
}

impl UnifiedPatch {
    /// Create a new patch
    pub fn new(
        orig_name: Vec<u8>,
        orig_ts: Option<Vec<u8>>,
        mod_name: Vec<u8>,
        mod_ts: Option<Vec<u8>>,
    ) -> Self {
        Self {
            orig_name,
            orig_ts,
            mod_name,
            mod_ts,
            hunks: Vec::new(),
        }
    }

    /// Serialize this patch to a byte vector
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.write(&mut bytes).unwrap();
        bytes
    }

    /// Write this patch to a writer
    pub fn write<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_all(
            &format!(
                "--- {}{}\n",
                String::from_utf8_lossy(&self.orig_name),
                match &self.orig_ts {
                    Some(ts) => format!("\t{}", String::from_utf8_lossy(ts)),
                    None => "".to_string(),
                }
            )
            .into_bytes(),
        )?;
        w.write_all(
            &format!(
                "+++ {}{}\n",
                String::from_utf8_lossy(&self.mod_name),
                match &self.mod_ts {
                    Some(ts) => format!("\t{}", String::from_utf8_lossy(ts)),
                    None => "".to_string(),
                }
            )
            .into_bytes(),
        )?;
        for hunk in &self.hunks {
            hunk.write(w)?;
        }
        Ok(())
    }

    /// Parse a unified patch file
    ///
    /// # Arguments
    /// * `iter_lines`: Iterator over lines
    pub fn parse_patch<'a, I>(iter_lines: I) -> Result<Self, Error>
    where
        I: Iterator<Item = &'a [u8]> + 'a,
    {
        let mut iter_lines = iter_lines_handle_nl(iter_lines);

        let ((orig_name, orig_ts), (mod_name, mod_ts)) = match get_patch_names(&mut iter_lines) {
            Ok(names) => names,
            Err(e) => return Err(e),
        };

        let mut patch = Self::new(orig_name, orig_ts, mod_name, mod_ts);
        for hunk in iter_hunks(&mut iter_lines) {
            patch.hunks.push(hunk?);
        }
        Ok(patch)
    }

    /// Parse a unified patch file
    ///
    /// # Arguments
    /// * `iter`: Iterator over lines
    pub fn parse_patches<I>(iter: I) -> Result<Vec<PlainOrBinaryPatch>, Error>
    where
        I: Iterator<Item = Vec<u8>>,
    {
        iter_file_patch(iter)
            .filter_map(|entry| match entry {
                Ok(FileEntry::Patch(lines)) => {
                    match Self::parse_patch(lines.iter().map(|l| l.as_slice())) {
                        Ok(patch) => Some(Ok(PlainOrBinaryPatch::Plain(patch))),
                        Err(e) => Some(Err(e)),
                    }
                }
                Ok(FileEntry::Junk(_)) => None,
                Ok(FileEntry::Meta(_)) => None,
                Err(Error::BinaryFiles(orig_name, mod_name)) => Some(Ok(
                    PlainOrBinaryPatch::Binary(BinaryPatch(orig_name, mod_name)),
                )),
                Err(e) => Some(Err(e)),
            })
            .collect()
    }
}

impl SingleFilePatch for UnifiedPatch {
    /// Old file name
    fn oldname(&self) -> &[u8] {
        &self.orig_name
    }

    /// New file name
    fn newname(&self) -> &[u8] {
        &self.mod_name
    }
}

impl ContentPatch for UnifiedPatch {
    /// Apply this patch to a file
    fn apply_exact(&self, orig: &[u8]) -> Result<Vec<u8>, crate::ApplyError> {
        let orig_lines = splitlines(orig).map(|l| l.to_vec());
        let lines = iter_exact_patched_from_hunks(orig_lines, self.hunks.clone().into_iter())
            .collect::<Result<Vec<Vec<u8>>, PatchConflict>>()
            .map_err(|e| crate::ApplyError::Conflict(e.to_string()))?;
        Ok(lines.concat())
    }
}

#[cfg(test)]
mod patch_tests {
    #[test]
    fn test_as_bytes_empty_hunks() {
        let patch = super::UnifiedPatch {
            orig_name: b"foo".to_vec(),
            orig_ts: None,
            mod_name: b"bar".to_vec(),
            mod_ts: None,
            hunks: vec![],
        };
        assert_eq!(patch.as_bytes(), b"--- foo\n+++ bar\n");
    }

    #[test]
    fn test_as_bytes() {
        let patch = super::UnifiedPatch {
            orig_name: b"foo".to_vec(),
            orig_ts: None,
            mod_name: b"bar".to_vec(),
            mod_ts: None,
            hunks: vec![super::Hunk {
                orig_pos: 1,
                orig_range: 1,
                mod_pos: 2,
                mod_range: 1,
                tail: None,
                lines: vec![super::HunkLine::ContextLine(b"foo\n".to_vec())],
            }],
        };
        assert_eq!(patch.as_bytes(), b"--- foo\n+++ bar\n@@ -1 +2 @@\n foo\n");
    }
}

/// A line in a hunk
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HunkLine {
    /// A line that is unchanged
    ContextLine(Vec<u8>),

    /// A line that was inserted
    InsertLine(Vec<u8>),

    /// A line that was removed
    RemoveLine(Vec<u8>),
}

impl HunkLine {
    /// Get the character that represents this hunk line
    pub fn char(&self) -> u8 {
        match self {
            Self::ContextLine(_) => b' ',
            Self::InsertLine(_) => b'+',
            Self::RemoveLine(_) => b'-',
        }
    }

    /// Get the contents of this hunk line
    pub fn contents(&self) -> &[u8] {
        match self {
            Self::ContextLine(bytes) => bytes,
            Self::InsertLine(bytes) => bytes,
            Self::RemoveLine(bytes) => bytes,
        }
    }

    /// Serialize this hunk line to a byte vector
    pub fn as_bytes(&self) -> Vec<u8> {
        let leadchar = self.char();
        let contents = self.contents();
        let terminator = if !contents.ends_with(&b"\n"[..]) {
            [b"\n".to_vec(), NO_NL.to_vec()].concat()
        } else {
            b"".to_vec()
        };
        [vec![leadchar], contents.to_vec(), terminator].concat()
    }

    /// Parse a hunk line
    pub fn parse_line(line: &[u8]) -> Result<Self, MalformedLine> {
        if line.starts_with(b"\n") {
            Ok(Self::ContextLine(line.to_vec()))
        } else if let Some(line) = line.strip_prefix(b" ") {
            Ok(Self::ContextLine(line.to_vec()))
        } else if let Some(line) = line.strip_prefix(b"+") {
            Ok(Self::InsertLine(line.to_vec()))
        } else if let Some(line) = line.strip_prefix(b"-") {
            Ok(Self::RemoveLine(line.to_vec()))
        } else {
            Err(MalformedLine(line.to_vec()))
        }
    }
}

/// An error that occurs when parsing a hunk line
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MalformedLine(Vec<u8>);

impl std::fmt::Display for MalformedLine {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Malformed line: {:?}", self.0)
    }
}

impl std::error::Error for MalformedLine {}

#[cfg(test)]
mod hunkline_tests {
    use super::HunkLine;
    use super::MalformedLine;

    #[test]
    fn test_parse_line() {
        assert_eq!(
            HunkLine::parse_line(&b" foo\n"[..]).unwrap(),
            HunkLine::ContextLine(b"foo\n".to_vec())
        );
        assert_eq!(
            HunkLine::parse_line(&b"-foo\n"[..]).unwrap(),
            HunkLine::RemoveLine(b"foo\n".to_vec())
        );
        assert_eq!(
            HunkLine::parse_line(&b"+foo\n"[..]).unwrap(),
            HunkLine::InsertLine(b"foo\n".to_vec())
        );
        assert_eq!(
            HunkLine::parse_line(&b"\n"[..]).unwrap(),
            HunkLine::ContextLine(b"\n".to_vec())
        );
        assert_eq!(
            HunkLine::parse_line(&b"aaaaa\n"[..]).unwrap_err(),
            MalformedLine(b"aaaaa\n".to_vec())
        );
    }

    #[test]
    fn as_bytes() {
        assert_eq!(
            HunkLine::ContextLine(b"foo\n".to_vec()).as_bytes(),
            b" foo\n"
        );
        assert_eq!(
            HunkLine::InsertLine(b"foo\n".to_vec()).as_bytes(),
            b"+foo\n"
        );
        assert_eq!(
            HunkLine::RemoveLine(b"foo\n".to_vec()).as_bytes(),
            b"-foo\n"
        );
    }

    #[test]
    fn as_bytes_no_nl() {
        assert_eq!(
            HunkLine::ContextLine(b"foo".to_vec()).as_bytes(),
            b" foo\n\\ No newline at end of file\n"
        );
    }
}

/// An error that occurs when parsing a hunk header
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MalformedHunkHeader(pub &'static str, pub Vec<u8>);

impl std::fmt::Display for MalformedHunkHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Malformed hunk header: {}: {}",
            self.0,
            String::from_utf8_lossy(&self.1)
        )
    }
}

impl std::error::Error for MalformedHunkHeader {}

/// A hunk in a patch
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Hunk {
    /// Position in the original file
    pub orig_pos: usize,

    /// Number of lines in the original file
    pub orig_range: usize,

    /// Position in the modified file
    pub mod_pos: usize,

    /// Number of lines in the modified file
    pub mod_range: usize,

    /// Tail of the hunk header
    pub tail: Option<Vec<u8>>,

    /// Lines in the hunk
    pub lines: Vec<HunkLine>,
}

impl Hunk {
    /// Create a new hunk
    pub fn new(
        orig_pos: usize,
        orig_range: usize,
        mod_pos: usize,
        mod_range: usize,
        tail: Option<Vec<u8>>,
    ) -> Self {
        Self {
            orig_pos,
            orig_range,
            mod_pos,
            mod_range,
            tail,
            lines: Vec::new(),
        }
    }

    /// Parse a hunk header
    pub fn from_header(line: &[u8]) -> Result<Self, MalformedHunkHeader> {
        let re = lazy_regex::regex!(r"\@\@ ([^@]*) \@\@( (.*))?\n"B);
        let captures = re
            .captures(line)
            .ok_or_else(|| MalformedHunkHeader("Does not match format.", line.to_vec()))?;
        let (orig, modi) = match captures[1].split(|b| *b == b' ').collect::<Vec<&[u8]>>()[..] {
            [orig, modi] => Ok((orig, modi)),
            _ => return Err(MalformedHunkHeader("Does not match format.", line.to_vec())),
        }?;

        if orig[0] != b'-' || modi[0] != b'+' {
            return Err(MalformedHunkHeader(
                "Positions don't start with + or -.",
                line.to_vec(),
            ));
        }
        let (orig_pos, orig_range) = parse_range(&String::from_utf8_lossy(&orig[1..]))
            .map_err(|_| MalformedHunkHeader("Original range is not a number.", line.to_vec()))?;
        let (mod_pos, mod_range) = parse_range(&String::from_utf8_lossy(modi[1..].as_ref()))
            .map_err(|_| MalformedHunkHeader("Modified range is not a number.", line.to_vec()))?;
        let tail = captures.get(3).map(|m| m.as_bytes().to_vec());
        Ok(Self::new(orig_pos, orig_range, mod_pos, mod_range, tail))
    }

    /// Get the lines in this hunk
    pub fn lines(&self) -> &[HunkLine] {
        &self.lines
    }

    /// Get the header of this hunk
    pub fn get_header(&self) -> Vec<u8> {
        let tail_str = match &self.tail {
            Some(tail) => [b" ".to_vec(), tail.to_vec()].concat(),
            None => Vec::new(),
        };
        format!(
            "@@ -{} +{} @@{}\n",
            self.range_str(self.orig_pos, self.orig_range),
            self.range_str(self.mod_pos, self.mod_range),
            String::from_utf8_lossy(&tail_str),
        )
        .into_bytes()
    }

    fn range_str(&self, pos: usize, range: usize) -> String {
        if range == 1 {
            format!("{}", pos)
        } else {
            format!("{},{}", pos, range)
        }
    }

    /// Write this hunk to a writer
    pub fn write<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_all(&self.get_header())?;
        for line in &self.lines {
            w.write_all(&line.as_bytes())?;
        }
        Ok(())
    }

    /// Serialize this hunk to a byte vector
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.write(&mut bytes).unwrap();
        bytes
    }

    /// Shift a position to the modified file
    pub fn shift_to_mod(&self, pos: usize) -> Option<isize> {
        if pos < self.orig_pos - 1 {
            Some(0)
        } else if pos > self.orig_pos + self.orig_range {
            Some((self.mod_range as isize) - (self.orig_range as isize))
        } else {
            self.shift_to_mod_lines(pos)
        }
    }

    /// Shift a position to the original file
    fn shift_to_mod_lines(&self, pos: usize) -> Option<isize> {
        let mut position = self.orig_pos - 1;
        let mut shift = 0;
        for line in &self.lines {
            match line {
                HunkLine::InsertLine(_) => shift += 1,
                HunkLine::RemoveLine(_) => {
                    if position == pos {
                        return None;
                    }
                    shift -= 1;
                    position += 1;
                }
                HunkLine::ContextLine(_) => position += 1,
            }
            if position > pos {
                break;
            }
        }
        Some(shift)
    }
}

/// Parse a patch range, handling the "1" special-case
pub fn parse_range(textrange: &str) -> Result<(usize, usize), ParseIntError> {
    let tmp: Vec<&str> = textrange.split(',').collect();
    let (pos, brange) = if tmp.len() == 1 {
        (tmp[0], "1")
    } else {
        (tmp[0], tmp[1])
    };
    let pos = pos.parse::<usize>()?;
    let range = brange.parse::<usize>()?;
    Ok((pos, range))
}

#[cfg(test)]
mod hunk_tests {
    use super::Hunk;

    #[test]
    fn from_header_test() {
        let hunk = Hunk::from_header(&b"@@ -1 +2 @@\n"[..]).unwrap();
        assert_eq!(hunk, Hunk::new(1, 1, 2, 1, None));
    }

    #[test]
    fn from_header_tail() {
        let hunk = Hunk::from_header(&b"@@ -1 +2 @@ function()\n"[..]).unwrap();
        assert_eq!(hunk, Hunk::new(1, 1, 2, 1, Some(b"function()".to_vec())));
    }

    #[test]
    fn test_valid_hunk_header() {
        let header = b"@@ -34,11 +50,6 @@\n";
        let hunk = Hunk::from_header(&header[..]).unwrap();
        assert_eq!(hunk.orig_pos, 34);
        assert_eq!(hunk.orig_range, 11);
        assert_eq!(hunk.mod_pos, 50);
        assert_eq!(hunk.mod_range, 6);
        assert_eq!(hunk.as_bytes(), &header[..]);
    }

    #[test]
    fn test_valid_hunk_header2() {
        let header = b"@@ -1 +0,0 @@\n";
        let hunk = Hunk::from_header(&header[..]).unwrap();
        assert_eq!(hunk.orig_pos, 1);
        assert_eq!(hunk.orig_range, 1);
        assert_eq!(hunk.mod_pos, 0);
        assert_eq!(hunk.mod_range, 0);
        assert_eq!(hunk.as_bytes(), header);
    }

    /// Parse a hunk header produced by diff -p.
    #[test]
    fn test_pdiff() {
        let header = b"@@ -407,7 +292,7 @@ bzr 0.18rc1  2007-07-10\n";
        let hunk = Hunk::from_header(header).unwrap();
        assert_eq!(&b"bzr 0.18rc1  2007-07-10"[..], hunk.tail.as_ref().unwrap());
        assert_eq!(&header[..], hunk.as_bytes());
    }

    fn assert_malformed_header(header: &[u8]) {
        let err = Hunk::from_header(header).unwrap_err();
        assert!(matches!(err, super::MalformedHunkHeader(..)));
    }

    #[test]
    fn test_invalid_header() {
        assert_malformed_header(&b" -34,11 +50,6 \n"[..]);
        assert_malformed_header(&b"@@ +50,6 -34,11 @@\n"[..]);
        assert_malformed_header(&b"@@ -34,11 +50,6 @@"[..]);
        assert_malformed_header(&b"@@ -34.5,11 +50,6 @@\n"[..]);
        assert_malformed_header(&b"@@-34,11 +50,6@@\n"[..]);
        assert_malformed_header(&b"@@ 34,11 50,6 @@\n"[..]);
        assert_malformed_header(&b"@@ -34,11 @@\n"[..]);
        assert_malformed_header(&b"@@ -34,11 +50,6.5 @@\n"[..]);
        assert_malformed_header(&b"@@ -34,11 +50,-6 @@\n"[..]);
    }
}

#[cfg(test)]
mod parse_range_tests {
    use super::parse_range;

    #[test]
    fn parse_range_test() {
        assert_eq!((2, 1), parse_range("2").unwrap());
        assert_eq!((2, 1), parse_range("2,1").unwrap());
        parse_range("foo").unwrap_err();
    }
}
