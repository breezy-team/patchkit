use lazy_static::lazy_static;
use regex::bytes::Regex;
use crate::patch::{Hunk, HunkLine};

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
