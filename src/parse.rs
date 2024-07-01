/// Split lines but preserve trailing newlines
pub fn splitlines<'a>(data: &'a [u8]) -> impl Iterator<Item = &'a [u8]> {
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
