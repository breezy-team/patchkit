//! Parsing of ed-style patches

/// A patch in the ed format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdPatch {
    /// The hunks in the patch.
    pub hunks: Vec<EdHunk>,
}

impl crate::ContentPatch for EdPatch {
    fn apply_exact(&self, orig: &[u8]) -> Result<Vec<u8>, crate::ApplyError> {
        let lines = splitlines(orig).collect::<Vec<_>>();
        let result = self
            .apply(&lines)
            .map_err(crate::ApplyError::Conflict)?;
        Ok(result)
    }
}

impl EdPatch {
    /// Apply the patch to the data.
    pub fn apply(&self, data: &[&[u8]]) -> Result<Vec<u8>, String> {
        let mut data = data.to_vec();
        for hunk in &self.hunks {
            match hunk {
                EdHunk::Remove(start, end, expected) | EdHunk::Change(start, end, expected, _) => {
                    assert_eq!(start, end);
                    let existing = match data.get(start - 1) {
                        Some(existing) => existing,
                        None => return Err(format!("line {} does not exist", start)),
                    };
                    if existing != expected {
                        return Err(format!(
                            "line {} does not match expected: {:?} != {:?}",
                            start,
                            String::from_utf8_lossy(existing).to_string(),
                            String::from_utf8_lossy(expected).to_string(),
                        ));
                    }
                    data.remove(start - 1);
                }
                _ => {}
            }
            match hunk {
                EdHunk::Add(start, end, added) | EdHunk::Change(start, end, _, added) => {
                    assert_eq!(start, end);
                    data.insert(start - 1, added);
                }
                _ => {}
            }
        }
        Ok(data.concat())
    }
}

/// A hunk in an ed patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EdHunk {
    /// Add lines.
    Add(usize, usize, Vec<u8>),

    /// Remove lines.
    Remove(usize, usize, Vec<u8>),

    /// Change lines
    Change(usize, usize, Vec<u8>, Vec<u8>),
}

/// Parse a hunk header.
pub fn parse_hunk_header(line: &[u8]) -> Option<(char, usize, usize)> {
    let cap = lazy_regex::BytesRegex::new("(\\d+)([adc])(\\d+)\n")
        .unwrap()
        .captures(line)?;

    let start = std::str::from_utf8(cap.get(1).unwrap().as_bytes())
        .ok()?
        .parse()
        .ok()?;
    let cmd = std::str::from_utf8(cap.get(2).unwrap().as_bytes())
        .ok()?
        .chars()
        .next()?;
    let end = std::str::from_utf8(cap.get(3).unwrap().as_bytes())
        .ok()?
        .parse()
        .ok()?;
    Some((cmd, start, end))
}

#[cfg(test)]
mod parse_hunk_header_tests {
    use super::*;

    #[test]
    fn test_parse_hunk_header() {
        assert_eq!(parse_hunk_header(b"5a10\n"), Some(('a', 5, 10)));
        assert_eq!(parse_hunk_header(b"5d10\n"), Some(('d', 5, 10)));
        assert_eq!(parse_hunk_header(b"5c10\n"), Some(('c', 5, 10)));
        assert_eq!(parse_hunk_header(b"5a\n"), None);
        assert_eq!(parse_hunk_header(b"a10\n"), None);
        assert_eq!(parse_hunk_header(b"5\n"), None);
        assert_eq!(parse_hunk_header(b"a\n"), None);
        assert_eq!(parse_hunk_header(b"\n"), None);
    }
}

/// Parse a line in a hunk.
pub fn parse_hunk_line<'a>(prefix: &[u8], line: &'a [u8]) -> Option<&'a [u8]> {
    if line.starts_with(prefix) {
        Some(&line[prefix.len()..])
    } else {
        None
    }
}

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

impl EdPatch {
    /// Parse a patch in the ed format.
    pub fn parse_patch(patch: &[u8]) -> Result<EdPatch, Vec<u8>> {
        let mut hunks = Vec::new();
        let mut lines = splitlines(patch);
        while let Some(line) = lines.next() {
            if line.is_empty() {
                continue;
            }

            let (cmd, start, end) = match parse_hunk_header(line) {
                Some((cmd, start, end)) => (cmd, start, end),
                None => return Err(line.to_vec()),
            };

            let hunk = match cmd {
                'a' => {
                    let line = lines.next().ok_or_else(|| line.to_vec())?;
                    let data = parse_hunk_line(b"> ", line).ok_or_else(|| line.to_vec())?;
                    EdHunk::Add(start, end, data.to_vec())
                }
                'd' => {
                    let line = lines.next().ok_or_else(|| line.to_vec())?;
                    let data = parse_hunk_line(b"< ", line).ok_or_else(|| line.to_vec())?;
                    EdHunk::Remove(start, end, data.to_vec())
                }
                'c' => {
                    let line = lines.next().ok_or_else(|| line.to_vec())?;
                    let data = parse_hunk_line(b"< ", line).ok_or_else(|| line.to_vec())?;
                    if let Some(line) = lines.next() {
                        if line != b"---\n" {
                            return Err(line.to_vec());
                        }
                    } else {
                        return Err(line.to_vec());
                    }
                    let line = lines.next().ok_or_else(|| line.to_vec())?;
                    let data2 = parse_hunk_line(b"> ", line).ok_or_else(|| line.to_vec())?;
                    EdHunk::Change(start, end, data.to_vec(), data2.to_vec())
                }
                _ => return Err(line.to_vec()),
            };
            hunks.push(hunk)
        }
        Ok(EdPatch { hunks })
    }
}

#[cfg(test)]
mod apply_patch_tests {
    use super::*;

    #[test]
    fn test_apply_add() {
        let patch = EdPatch {
            hunks: vec![EdHunk::Add(1, 1, b"hello\n".to_vec())],
        };
        let data = &[&b"world\n"[..]][..];
        assert_eq!(patch.apply(data).unwrap(), b"hello\nworld\n".to_vec());
    }

    #[test]
    fn test_apply_remove() {
        let patch = EdPatch {
            hunks: vec![EdHunk::Remove(2, 2, b"world\n".to_vec())],
        };
        let data = &[&b"hello\n"[..], &b"world\n"[..]];
        assert_eq!(patch.apply(data).unwrap(), b"hello\n".to_vec());
    }

    #[test]
    fn test_apply_change() {
        let patch = EdPatch {
            hunks: vec![EdHunk::Change(
                2,
                2,
                b"world\n".to_vec(),
                b"hello\n".to_vec(),
            )],
        };
        let data = &[&b"hello\n"[..], &b"world\n"[..]];
        assert_eq!(patch.apply(data).unwrap(), b"hello\nhello\n".to_vec());
    }
}

#[cfg(test)]
mod parse_patch_tests {
    use super::*;

    #[test]
    fn test_parse_patch() {
        let patch = b"5a10
> hello
5d10
< hello
5c10
< hello
---
> hello
";
        let patch = EdPatch::parse_patch(patch).unwrap();
        assert_eq!(
            patch,
            EdPatch {
                hunks: vec![
                    EdHunk::Add(5, 10, b"hello\n".to_vec()),
                    EdHunk::Remove(5, 10, b"hello\n".to_vec()),
                    EdHunk::Change(5, 10, b"hello\n".to_vec(), b"hello\n".to_vec()),
                ]
            }
        );
    }
}
