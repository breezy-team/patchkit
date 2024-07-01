use lazy_static::lazy_static;
use regex::bytes::Regex;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum Error {
    BinaryFiles(Vec<u8>, Vec<u8>),
    PatchSyntax(&'static str, Vec<u8>),
    MalformedPatchHeader(&'static str, Vec<u8>),
    MalformedHunkHeader(&'static str, Vec<u8>),
    MalformedLine(&'static str, Vec<u8>),
}

pub trait Patch {
    fn oldname(&self) -> &[u8];
    fn newname(&self) -> &[u8];
}

pub struct BinaryPatch(pub Vec<u8>, pub Vec<u8>);

impl Patch for BinaryPatch {
    fn oldname(&self) -> &[u8] {
        &self.0
    }

    fn newname(&self) -> &[u8] {
        &self.1
    }
}

pub struct UnifiedPatch {
    pub orig_name: Vec<u8>,
    pub orig_ts: Option<Vec<u8>>,
    pub mod_name: Vec<u8>,
    pub mod_ts: Option<Vec<u8>>,
    pub hunks: Vec<Hunk>,
}

impl UnifiedPatch {
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
}

impl Patch for UnifiedPatch {
    fn oldname(&self) -> &[u8] {
        &self.orig_name
    }

    fn newname(&self) -> &[u8] {
        &self.mod_name
    }
}

pub fn get_patch_names<'a, T: Iterator<Item = &'a [u8]>>(
    mut iter_lines: T,
) -> Result<((Vec<u8>, Option<Vec<u8>>), (Vec<u8>, Option<Vec<u8>>)), Error> {
    lazy_static! {
        static ref BINARY_FILES_RE: Regex =
            Regex::new(r"^Binary files (.+) and (.+) differ").unwrap();
    }

    let line = iter_lines
        .next()
        .ok_or_else(|| Error::PatchSyntax("No input", vec![]))?;

    let (orig_name, orig_ts) = match BINARY_FILES_RE.captures(&line) {
        Some(captures) => {
            let orig_name = captures.get(1).unwrap().as_bytes().to_vec();
            let orig_ts = captures.get(2).unwrap().as_bytes().to_vec();
            return Err(Error::BinaryFiles(orig_name, orig_ts));
        }
        None => {
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
            (orig_name, orig_ts)
        }
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

pub const NO_NL: &[u8] = b"\\ No newline at end of file\n";

/// Iterate through a series of lines, ensuring that lines
/// that originally had no terminating newline are produced
/// without one.
pub fn iter_lines_handle_nl<'a, I>(mut iter_lines: I) -> impl Iterator<Item = &'a [u8]>
where
    I: Iterator<Item = &'a [u8]>,
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
        b"line 1\n",
        b"line 2\n",
        NO_NL,
        b"line 3\n",
        b"line 4\n",
        NO_NL,
    ];
    let mut iter = iter_lines_handle_nl(lines.iter());
    assert_eq!(iter.next(), Some(b"line 1".to_vec()));
    assert_eq!(iter.next(), Some(b"line 2".to_vec()));
    assert_eq!(iter.next(), Some(b"line 3".to_vec()));
    assert_eq!(iter.next(), Some(b"line 4".to_vec()));
    assert_eq!(iter.next(), None);
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

/// Find the index of the first character that differs between two texts
pub fn difference_index(atext: &[u8], btext: &[u8]) -> Option<usize> {
    let length = atext.len().min(btext.len());
    (0..length).find(|&i| atext[i] != btext[i])
}

#[derive(Clone)]
pub enum HunkLine {
    ContextLine(Vec<u8>),
    InsertLine(Vec<u8>),
    RemoveLine(Vec<u8>),
}

impl HunkLine {
    pub fn get_str(&self, leadchar: u8) -> Vec<u8> {
        match self {
            HunkLine::ContextLine(contents)
            | HunkLine::InsertLine(contents)
            | HunkLine::RemoveLine(contents) => {
                let terminator = if !contents.ends_with(&b"\n"[..]) {
                    [b"\n".to_vec(), NO_NL.to_vec()].concat()
                } else {
                    b"".to_vec()
                };
                [vec![leadchar], contents.clone(), terminator].concat()
            }
        }
    }

    pub fn char(&self) -> u8 {
        match self {
            HunkLine::ContextLine(_) => b' ',
            HunkLine::InsertLine(_) => b'+',
            HunkLine::RemoveLine(_) => b'-',
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.get_str(self.char())
    }
}

pub struct Hunk {
    orig_pos: usize,
    orig_range: usize,
    mod_pos: usize,
    mod_range: usize,
    tail: Option<Vec<u8>>,
    lines: Vec<HunkLine>,
}

impl Hunk {
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

    pub fn lines(&self) -> &[HunkLine] {
        &self.lines
    }

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

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut lines = vec![self.get_header()];
        for line in &self.lines {
            lines.push(match line {
                HunkLine::ContextLine(bytes) => bytes.clone(),
                HunkLine::InsertLine(bytes) => bytes.clone(),
                HunkLine::RemoveLine(bytes) => bytes.clone(),
            });
        }
        lines.concat()
    }

    pub fn shift_to_mod(&self, pos: usize) -> Option<isize> {
        if pos < self.orig_pos - 1 {
            Some(0)
        } else if pos > self.orig_pos + self.orig_range {
            Some((self.mod_range as isize) - (self.orig_range as isize))
        } else {
            self.shift_to_mod_lines(pos)
        }
    }

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

pub fn hunk_from_header(line: &[u8]) -> Result<Hunk, Error> {
    let re = Regex::new(r"\@\@ ([^@]*) \@\@( (.*))?\n").unwrap();
    let captures = re
        .captures(line)
        .ok_or_else(|| Error::MalformedHunkHeader("Does not match format.", line.to_vec()))?;
    let (orig, modi) = match captures[1].split(|b| *b == b' ').collect::<Vec<&[u8]>>()[..] {
        [orig, modi] => Ok((orig, modi)),
        _ => {
            return Err(Error::MalformedHunkHeader(
                "Does not match format.",
                line.to_vec(),
            ))
        }
    }?;

    if orig[1] != b'-' || modi[1] != b'+' {
        return Err(Error::MalformedHunkHeader(
            "Positions don't start with + or -.",
            line.to_vec(),
        ));
    }
    let (orig_pos, orig_range) =
        parse_range(&String::from_utf8_lossy(&orig[2..])).map_err(|_| {
            Error::MalformedHunkHeader("Original range is not a number.", line.to_vec())
        })?;
    let (mod_pos, mod_range) =
        parse_range(&String::from_utf8_lossy(modi[2..].as_ref())).map_err(|_| {
            Error::MalformedHunkHeader("Modified range is not a number.", line.to_vec())
        })?;
    let tail = captures.get(3).map(|m| m.as_bytes().to_vec());
    Ok(Hunk::new(orig_pos, orig_range, mod_pos, mod_range, tail))
}

pub fn parse_line(line: &[u8]) -> Result<HunkLine, Error> {
    if line.starts_with(b"\n") {
        Ok(HunkLine::ContextLine(line.to_vec()))
    } else if line.starts_with(b" ") {
        Ok(HunkLine::ContextLine(line[1..].to_vec()))
    } else if line.starts_with(b"+") {
        Ok(HunkLine::InsertLine(line[1..].to_vec()))
    } else if line.starts_with(b"-") {
        Ok(HunkLine::RemoveLine(line[1..].to_vec()))
    } else {
        Err(Error::MalformedLine("Unknown line type", line.to_vec()))
    }
}

pub fn iter_hunks<'a, I>(
    iter_lines: &mut I,
    allow_dirty: bool,
) -> impl Iterator<Item = Result<Hunk, Error>> + '_
where
    I: Iterator<Item = &'a [u8]>,
{
    std::iter::from_fn(move || loop {
        while let Some(line) = iter_lines.next() {
            if line == b"\n" {
                continue;
            }
            match hunk_from_header(line) {
                Ok(mut new_hunk) => {
                    let mut orig_size = 0;
                    let mut mod_size = 0;
                    while orig_size < new_hunk.orig_range || mod_size < new_hunk.mod_range {
                        match parse_line(iter_lines.next()?) {
                            Err(e) => {
                                return Some(Err(e));
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
                Err(Error::MalformedHunkHeader(m, l)) => {
                    if allow_dirty {
                        // If the line isn't a hunk header, then we've reached the end of this
                        // patch and there's "junk" at the end. Ignore the rest of the patch.
                        return None;
                    } else {
                        return Some(Err(Error::MalformedHunkHeader(m, l)));
                    }
                }
                Err(e) => return Some(Err(e)),
            }
        }
    })
}

pub fn parse_patch<'a, I>(iter_lines: I, allow_dirty: bool) -> Result<Box<dyn Patch>, Error>
where
    I: Iterator<Item = &'a [u8]>,
{
    let mut iter_lines = iter_lines_handle_nl(iter_lines);

    let ((orig_name, orig_ts), (mod_name, mod_ts)) = match get_patch_names(iter_lines) {
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

#[derive(Debug)]
struct PatchConflict {
    line_no: usize,
    orig_line: Vec<u8>,
    patch_line: Vec<u8>,
}

impl std::fmt::Display for PatchConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Patch conflict at line {}: orig: {:?}, patch: {:?}",
            self.line_no,
            String::from_utf8_lossy(&self.orig_line),
            String::from_utf8_lossy(&self.patch_line)
        )
    }
}

impl std::error::Error for PatchConflict {}

struct PatchedIter<'a, H: Iterator<Item = Hunk>> {
    orig_lines: Box<dyn Iterator<Item = Vec<u8>> + 'a>,
    hunks: std::iter::Peekable<H>,
    line_no: usize,
}

impl<H: Iterator<Item = Hunk>> Iterator for PatchedIter<'_, H> {
    type Item = Result<Vec<u8>, PatchConflict>;

    fn next(&mut self) -> Option<Result<Vec<u8>, PatchConflict>> {
        loop {
            // First, check if we just need to yield the next line from the original file.
            match self.hunks.peek() {
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
                Some(hunk) => {
                    // We're in a hunk. Check if we need to yield a line from the hunk.
                    if let Some(line) = hunk.lines.pop() {
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
                    }
                }
            }
        }
    }
}

/// Iterate through a series of lines with a patch applied.
///
/// This handles a single file, and does exact, not fuzzy patching.
///
/// Args:
///   orig_lines: The original lines of the file.
///   hunks: The hunks to apply to the file.
fn iter_exact_patched_from_hunks(
    orig_lines: impl Iterator<Item = Vec<u8>>,
    hunks: impl Iterator<Item = Hunk>,
) -> impl Iterator<Item = Result<Vec<u8>, PatchConflict>> {
    PatchedIter {
        orig_lines: Box::new(orig_lines),
        hunks: hunks.peekable(),
        line_no: 1,
    }
}

// Split lines but preserve trailing newlines
fn splitlines<'a>(data: &'a [u8]) -> impl Iterator<Item = &'a [u8]> {
    let mut start = 0;
    let mut end = 0;
    std::iter::from_fn(move || {
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
        None
    })
}

#[cfg(test)]
mod splitlines_tests {
    #[test]
    fn test_simple() {
        let data = b"line 1\nline 2\nline 3\n";
        let lines: Vec<&[u8]> = super::splitlines(data).collect();
        assert_eq!(lines, vec![b"line 1\n", b"line 2\n", b"line 3\n"]);
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

#[cfg(test)]
mod patches_tests {
    macro_rules! test_patch {
        ($name:ident, $orig:expr, $mod:expr, $patch:expr) => {
            #[test]
            fn $name() {
                let orig = include_bytes!(concat!("../test_patches_data/", $orig));
                let modi = include_bytes!(concat!("../test_patches_data/", $mod));
                let patch = include_bytes!(concat!("../test_patches_data/", $patch));
                let parsed = super::parse_patch(super::splitlines(patch), false);
                let mut patched = Vec::new();
                let mut iter =
                    crate::iter_patched_from_hunks(orig, parsed.hunks.iter().map(|h| h.as_bytes()));
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
