use regex::bytes::Regex;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum ApplyError {
    Conflict(String),

    Unapplyable,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Conflict(reason) => write!(f, "Conflict: {}", reason),
            Self::Unapplyable => write!(f, "Patch unapplyable"),
        }
    }
}

impl std::error::Error for ApplyError {}

/// A patch of some sort
pub trait Patch {
    /// Old file name
    fn oldname(&self) -> &[u8];

    /// New file name
    fn newname(&self) -> &[u8];

    fn apply_exact(&self, orig: &[u8]) -> Result<Vec<u8>, ApplyError>;
}

/// A binary patch
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryPatch(pub Vec<u8>, pub Vec<u8>);

impl Patch for BinaryPatch {
    fn oldname(&self) -> &[u8] {
        &self.0
    }

    fn newname(&self) -> &[u8] {
        &self.1
    }

    fn apply_exact(&self, _orig: &[u8]) -> Result<Vec<u8>, ApplyError> {
        Err(ApplyError::Unapplyable)
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

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut lines = vec![
            format!(
                "--- {}{}\n",
                String::from_utf8_lossy(&self.orig_name),
                match &self.orig_ts {
                    Some(ts) => format!("\t{}", String::from_utf8_lossy(ts)),
                    None => "".to_string(),
                }
            )
            .into_bytes(),
            format!(
                "+++ {}{}\n",
                String::from_utf8_lossy(&self.mod_name),
                match &self.mod_ts {
                    Some(ts) => format!("\t{}", String::from_utf8_lossy(ts)),
                    None => "".to_string(),
                }
            )
            .into_bytes(),
        ];
        for hunk in &self.hunks {
            lines.push(hunk.as_bytes());
        }
        lines.concat()
    }

    pub fn parse_patch<'a, I>(iter_lines: I) -> Result<Self, crate::parse::Error>
    where
        I: Iterator<Item = &'a [u8]> + 'a,
    {
        let mut iter_lines = crate::parse::iter_lines_handle_nl(iter_lines);

        let ((orig_name, orig_ts), (mod_name, mod_ts)) =
            match crate::parse::get_patch_names(&mut iter_lines) {
                Ok(names) => names,
                Err(e) => return Err(e),
            };

        let mut patch = Self::new(orig_name, orig_ts, mod_name, mod_ts);
        for hunk in crate::parse::iter_hunks(&mut iter_lines) {
            patch.hunks.push(hunk?);
        }
        Ok(patch)
    }

    /// Parse a unified patch file
    ///
    /// # Arguments
    /// * `iter`: Iterator over lines
    pub fn parse_patches<'a, I>(iter: I) -> Result<Vec<UnifiedPatch>, crate::parse::Error>
    where
        I: Iterator<Item = Vec<u8>>,
    {
        crate::parse::iter_file_patch(iter)
            .filter_map(|entry| match entry {
                Ok(crate::parse::FileEntry::Patch(lines)) => {
                    match Self::parse_patch(lines.iter().map(|l| l.as_slice())) {
                        Ok(patch) => Some(Ok(patch)),
                        Err(e) => Some(Err(e)),
                    }
                }
                Ok(crate::parse::FileEntry::Junk(_)) => None,
                Ok(crate::parse::FileEntry::Meta(_)) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }
}

impl Patch for UnifiedPatch {
    fn oldname(&self) -> &[u8] {
        &self.orig_name
    }

    fn newname(&self) -> &[u8] {
        &self.mod_name
    }

    fn apply_exact(&self, orig: &[u8]) -> Result<Vec<u8>, ApplyError> {
        let orig_lines = crate::parse::splitlines(orig).map(|l| l.to_vec());
        let lines =
            crate::parse::iter_exact_patched_from_hunks(orig_lines, self.hunks.clone().into_iter())
                .collect::<Result<Vec<Vec<u8>>, crate::parse::PatchConflict>>()
                .map_err(|e| ApplyError::Conflict(e.to_string()))?;
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
        assert_eq!(patch.as_bytes(), b"--- foo\n+++ bar\n@@ -1 +2 @@\nfoo\n");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HunkLine {
    ContextLine(Vec<u8>),
    InsertLine(Vec<u8>),
    RemoveLine(Vec<u8>),
}

impl HunkLine {
    pub fn get_str(&self, leadchar: u8) -> Vec<u8> {
        match self {
            Self::ContextLine(contents)
            | Self::InsertLine(contents)
            | Self::RemoveLine(contents) => {
                let terminator = if !contents.ends_with(&b"\n"[..]) {
                    [b"\n".to_vec(), crate::parse::NO_NL.to_vec()].concat()
                } else {
                    b"".to_vec()
                };
                [vec![leadchar], contents.clone(), terminator].concat()
            }
        }
    }

    pub fn char(&self) -> u8 {
        match self {
            Self::ContextLine(_) => b' ',
            Self::InsertLine(_) => b'+',
            Self::RemoveLine(_) => b'-',
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.get_str(self.char())
    }

    pub fn parse_line(line: &[u8]) -> Result<Self, MalformedLine> {
        if line.starts_with(b"\n") {
            Ok(Self::ContextLine(line.to_vec()))
        } else if line.starts_with(b" ") {
            Ok(Self::ContextLine(line[1..].to_vec()))
        } else if line.starts_with(b"+") {
            Ok(Self::InsertLine(line[1..].to_vec()))
        } else if line.starts_with(b"-") {
            Ok(Self::RemoveLine(line[1..].to_vec()))
        } else {
            Err(MalformedLine(line.to_vec()))
        }
    }
}

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MalformedHunkHeader(pub &'static str, pub Vec<u8>);

impl std::fmt::Display for MalformedHunkHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Malformed hunk header: {}: {:?}", self.0, self.1)
    }
}

impl std::error::Error for MalformedHunkHeader {}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Hunk {
    pub orig_pos: usize,
    pub orig_range: usize,
    pub mod_pos: usize,
    pub mod_range: usize,
    pub tail: Option<Vec<u8>>,
    pub lines: Vec<HunkLine>,
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

    pub fn from_header(line: &[u8]) -> Result<Self, MalformedHunkHeader> {
        let re = Regex::new(r"\@\@ ([^@]*) \@\@( (.*))?\n").unwrap();
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
mod parse_range_tests {
    use super::parse_range;

    #[test]
    fn parse_range_test() {
        assert_eq!((2, 1), parse_range("2").unwrap());
        assert_eq!((2, 1), parse_range("2,1").unwrap());
        parse_range("foo").unwrap_err();
    }
}
