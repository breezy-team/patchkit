use lazy_static::lazy_static;
use regex::bytes::Regex;
use crate::patch::{Hunk, HunkLine, Patch, UnifiedPatch, BinaryPatch};

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    BinaryFiles(Vec<u8>, Vec<u8>),
    PatchSyntax(&'static str, Vec<u8>),
    MalformedPatchHeader(&'static str, Vec<u8>),
    MalformedHunkHeader(&'static str, Vec<u8>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::BinaryFiles(oldname, newname) => write!(f, "Binary files {:?} and {:?} differ", oldname, newname),
            Self::PatchSyntax(msg, line) => write!(f, "Patch syntax error: {} in {:?}", msg, line),
            Self::MalformedPatchHeader(msg, line) => write!(f, "Malformed patch header: {} in {:?}", msg, line),
            Self::MalformedHunkHeader(msg, line) => write!(f, "Malformed hunk header: {} in {:?}", msg, line),
        }
    }
}

impl std::error::Error for Error {}

/// Split lines but preserve trailing newlines
pub fn splitlines(data: &[u8]) -> impl Iterator<Item = &'_ [u8]> {
    let mut start = 0;
    let mut end = 0;
    std::iter::from_fn(move || {
        loop {
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
        }
    })
}

#[cfg(test)]
mod splitlines_tests {
    #[test]
    fn test_simple() {
        let data = b"line 1\nline 2\nline 3\n";
        let lines: Vec<&[u8]> = super::splitlines(data).collect();
        assert_eq!(lines, vec!["line 1\n".as_bytes(), "line 2\n".as_bytes(), "line 3\n".as_bytes()]);
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

pub const NO_NL: &[u8] = b"\\ No newline at end of file\n";

/// Iterate through a series of lines, ensuring that lines
/// that originally had no terminating newline are produced
/// without one.
pub fn iter_lines_handle_nl<'a, I>(mut iter_lines: I) -> impl Iterator<Item = &'a [u8]> + 'a
where
    I: Iterator<Item = &'a [u8]> + 'a
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
        &b"\\ No newline at end of file\n"[..]
    ];
    let mut iter = iter_lines_handle_nl(lines.into_iter());
    assert_eq!(iter.next(), Some("line 1\n".as_bytes()));
    assert_eq!(iter.next(), Some("line 2\n".as_bytes()));
    assert_eq!(iter.next(), Some("line 3\n".as_bytes()));
    assert_eq!(iter.next(), Some("line 4".as_bytes()));
    assert_eq!(iter.next(), None);
}

fn get_patch_names<'a, T: Iterator<Item = &'a [u8]>>(
    iter_lines: &mut T,
) -> Result<((Vec<u8>, Option<Vec<u8>>), (Vec<u8>, Option<Vec<u8>>)), Error> {
    lazy_static! {
        static ref BINARY_FILES_RE: Regex =
            Regex::new(r"^Binary files (.+) and (.+) differ").unwrap();
    }

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
    let (orig_name, orig_ts) = match orig_name.split(|&c| c == b'\t').collect::<Vec<_>>()[..]
    {
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
            &b"+++ quxx	2009-10-14 19:51:00 +0000\n"[..]];
        let mut iter = lines.into_iter();
        let (old, new) = super::get_patch_names(&mut iter).unwrap();
        assert_eq!(old, (b"baz".to_vec(), Some(b"2009-10-14 19:49:59 +0000".to_vec())));
        assert_eq!(new, (b"quxx".to_vec(), Some(b"2009-10-14 19:51:00 +0000".to_vec())));
    }

    #[test]
    fn test_binary() {
        let lines = [
            &b"Binary files qoo and bar differ\n"[..]
        ];
        let mut iter = lines.into_iter();
        let e = super::get_patch_names(&mut iter).unwrap_err();
        assert_eq!(e, super::Error::BinaryFiles(b"qoo".to_vec(), b"bar".to_vec()));
    }
}

pub fn iter_hunks<'a, I>(
    iter_lines: &mut I,
    allow_dirty: bool,
) -> impl Iterator<Item = Result<Hunk, Error>> + '_
where
    I: Iterator<Item = &'a [u8]>
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
                                return Some(Err(Error::PatchSyntax("Invalid hunk line", line.to_vec())));
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
                Err(crate::patch::MalformedHunkHeader(m, l)) => {
                    if allow_dirty {
                        // If the line isn't a hunk header, then we've reached the end of this
                        // patch and there's "junk" at the end. Ignore the rest of the patch.
                        return None;
                    } else {
                        return Some(Err(Error::MalformedHunkHeader(m, l)));
                    }
                }
            }
        }
        None
    }
    )
}

#[cfg(test)]
mod iter_hunks_tests {
    use super::{Hunk, HunkLine};
    #[test]
    fn test_iter_hunks() {
        let mut lines = super::splitlines(br#"@@ -391,6 +391,8 @@
                 else:
                     assert isinstance(hunk_line, RemoveLine)
                 line_no += 1
+    for line in orig_lines:
+        yield line
                     
 import unittest
 import os.path

"#);

        let hunks = super::iter_hunks(&mut lines, false).collect::<Result<Vec<Hunk>, crate::parse::Error>>().unwrap();

        let mut expected_hunk = Hunk::new(391, 6, 391, 8, None);
        expected_hunk.lines.extend([
            HunkLine::ContextLine(b"                else:\n".to_vec()),
            HunkLine::ContextLine(b"                    assert isinstance(hunk_line, RemoveLine)\n".to_vec()),
            HunkLine::ContextLine(b"                line_no += 1\n".to_vec()),
            HunkLine::InsertLine(b"    for line in orig_lines:\n".to_vec()),
            HunkLine::InsertLine(b"        yield line\n".to_vec()),
            HunkLine::ContextLine(b"                    \n".to_vec()),
            HunkLine::ContextLine(b"import unittest\n".to_vec()),
            HunkLine::ContextLine(b"import os.path\n".to_vec())
        ]);

        assert_eq!(&expected_hunk, hunks.iter().next().unwrap());
    }
}

pub fn parse_patch<'a, I>(iter_lines: I, allow_dirty: bool) -> Result<Box<dyn Patch>, Error>
where
    I: Iterator<Item = &'a [u8]> + 'a,
{
    let mut iter_lines = iter_lines_handle_nl(iter_lines);

    let ((orig_name, orig_ts), (mod_name, mod_ts)) = match get_patch_names(&mut iter_lines) {
        Ok(names) => names,
        Err(Error::BinaryFiles(orig_name, mod_name)) => {
            return Ok(Box::new(BinaryPatch(orig_name, mod_name)));
        }
        Err(e) => return Err(e),
    };

    let mut patch = UnifiedPatch::new(orig_name, orig_ts, mod_name, mod_ts);
    for hunk in iter_hunks(&mut iter_lines, allow_dirty) {
        patch.hunks.push(hunk?);
    }
    Ok(Box::new(patch))
}


#[cfg(test)]
mod patches_tests {
    macro_rules! test_patch {
        ($name:ident, $orig:expr, $mod:expr, $patch:expr) => {
            #[test]
            fn $name() {
                let orig = include_bytes!(concat!("../test_patches_data/", $orig));
                let modi = include_bytes!(concat!("../test_patches_data/", $mod));
                let patch = include_bytes!(concat!("../test_patches_data/", $patch));
                let parsed = super::parse_patch(super::splitlines(patch), false).unwrap();
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
    #[test]
    fn test_just_context() {
        let orig_lines = vec![
            b"line 1\n".to_vec(),
            b"line 2\n".to_vec(),
            b"line 3\n".to_vec(),
            b"line 4\n".to_vec(),
        ];
        let mut hunk = crate::patch::Hunk::new(1, 1, 1, 1, None);
        hunk.lines.push(crate::patch::HunkLine::ContextLine(b"line 1\n".to_vec()));
        let hunks = vec![hunk];
        let result = super::iter_exact_patched_from_hunks(orig_lines.into_iter(), hunks.into_iter()).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(&result, &[
            b"line 1\n".to_vec(),
            b"line 2\n".to_vec(),
            b"line 3\n".to_vec(),
            b"line 4\n".to_vec(),
        ]);
    }

    #[test]
    fn test_insert() {
        let orig_lines = vec![
            b"line 1\n".to_vec(),
            b"line 2\n".to_vec(),
            b"line 3\n".to_vec(),
            b"line 4\n".to_vec(),
        ];
        let mut hunk = crate::patch::Hunk::new(1, 0, 1, 1, None);
        hunk.lines.push(crate::patch::HunkLine::InsertLine(b"line 0\n".to_vec()));
        hunk.lines.push(crate::patch::HunkLine::ContextLine(b"line 1\n".to_vec()));
        let hunks = vec![hunk];
        let result = super::iter_exact_patched_from_hunks(orig_lines.into_iter(), hunks.into_iter()).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(&result, &[
            b"line 0\n".to_vec(),
            b"line 1\n".to_vec(),
            b"line 2\n".to_vec(),
            b"line 3\n".to_vec(),
            b"line 4\n".to_vec(),
        ]);
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


