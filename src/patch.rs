use std::num::ParseIntError;
use regex::bytes::Regex;

/// A patch of some sort
pub trait Patch {
    /// Old file name
    fn oldname(&self) -> &[u8];

    /// New file name
    fn newname(&self) -> &[u8];
}

/// A binary patch
pub struct BinaryPatch(pub Vec<u8>, pub Vec<u8>);

impl Patch for BinaryPatch {
    fn oldname(&self) -> &[u8] {
        &self.0
    }

    fn newname(&self) -> &[u8] {
        &self.1
    }
}

/// A unified diff style patch
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
}

impl Patch for UnifiedPatch {
    fn oldname(&self) -> &[u8] {
        &self.orig_name
    }

    fn newname(&self) -> &[u8] {
        &self.mod_name
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
        assert_eq!(HunkLine::parse_line(&b" foo\n"[..]).unwrap(), HunkLine::ContextLine(b"foo\n".to_vec()));
        assert_eq!(HunkLine::parse_line(&b"-foo\n"[..]).unwrap(), HunkLine::RemoveLine(b"foo\n".to_vec()));
        assert_eq!(HunkLine::parse_line(&b"+foo\n"[..]).unwrap(), HunkLine::InsertLine(b"foo\n".to_vec()));
        assert_eq!(HunkLine::parse_line(&b"\n"[..]).unwrap(), HunkLine::ContextLine(b"\n".to_vec()));
        assert_eq!(HunkLine::parse_line(&b"aaaaa\n"[..]).unwrap_err(), MalformedLine(b"aaaaa\n".to_vec()));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MalformedHunkHeader(&'static str, Vec<u8>);

impl std::fmt::Display for MalformedHunkHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Malformed hunk header: {}: {:?}", self.0, self.1)
    }
}

impl std::error::Error for MalformedHunkHeader {}

#[derive(PartialEq, Eq, Debug, Clone)]
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

    pub fn from_header(line: &[u8]) -> Result<Self, MalformedHunkHeader> {
        let re = Regex::new(r"\@\@ ([^@]*) \@\@( (.*))?\n").unwrap();
        let captures = re
            .captures(line)
            .ok_or_else(|| MalformedHunkHeader("Does not match format.", line.to_vec()))?;
        let (orig, modi) = match captures[1].split(|b| *b == b' ').collect::<Vec<&[u8]>>()[..] {
            [orig, modi] => Ok((orig, modi)),
            _ => {
                return Err(MalformedHunkHeader(
                    "Does not match format.",
                    line.to_vec(),
                ))
            }
        }?;

        if orig[0] != b'-' || modi[0] != b'+' {
            return Err(MalformedHunkHeader(
                "Positions don't start with + or -.",
                line.to_vec(),
            ));
        }
        let (orig_pos, orig_range) =
            parse_range(&String::from_utf8_lossy(&orig[1..])).map_err(|_| {
                MalformedHunkHeader("Original range is not a number.", line.to_vec())
            })?;
        let (mod_pos, mod_range) =
            parse_range(&String::from_utf8_lossy(modi[1..].as_ref())).map_err(|_| {
                MalformedHunkHeader("Modified range is not a number.", line.to_vec())
            })?;
        let tail = captures.get(3).map(|m| m.as_bytes().to_vec());
        Ok(Self::new(orig_pos, orig_range, mod_pos, mod_range, tail))
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
