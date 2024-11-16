//! Quilt patch management
use std::collections::HashMap;
use std::io::BufRead;

/// The default directory for patches
pub const DEFAULT_PATCHES_DIR: &str = "patches";

/// The default series file name
pub const DEFAULT_SERIES_FILE: &str = "series";

/// Find the common prefix to use for patches
///
/// # Arguments
/// * `names` - An iterator of patch names
///
/// # Returns
/// The common prefix, or `None` if there is no common prefix
pub fn find_common_patch_suffix<'a>(names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut suffix_count = HashMap::new();

    for name in names {
        if name == "series" || name == "00list" {
            continue;
        }

        if name.starts_with("README") {
            continue;
        }

        let suffix = name.find('.').map(|index| &name[index..]).unwrap_or("");
        suffix_count
            .entry(suffix)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

    // Just find the suffix with the highest count and return it
    suffix_count
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(suffix, _)| suffix)
}

#[cfg(test)]
mod find_common_patch_suffix_tests {
    #[test]
    fn test_find_common_patch_suffix() {
        let names = vec![
            "0001-foo.patch",
            "0002-bar.patch",
            "0003-baz.patch",
            "0004-qux.patch",
        ];
        assert_eq!(
            super::find_common_patch_suffix(names.into_iter()),
            Some(".patch")
        );
    }

    #[test]
    fn test_find_common_patch_suffix_no_common_suffix() {
        let names = vec![
            "0001-foo.patch",
            "0002-bar.patch",
            "0003-baz.patch",
            "0004-qux",
        ];
        assert_eq!(
            super::find_common_patch_suffix(names.into_iter()),
            Some(".patch")
        );
    }

    #[test]
    fn test_find_common_patch_suffix_no_patches() {
        let names = vec![
            "README",
            "0001-foo.patch",
            "0002-bar.patch",
            "0003-baz.patch",
        ];
        assert_eq!(
            super::find_common_patch_suffix(names.into_iter()),
            Some(".patch")
        );
    }
}

/// A entry in a series file
#[derive(Debug, PartialEq, Eq)]
pub enum SeriesEntry {
    /// A patch entry
    Patch {
        /// The name of the patch
        name: String,
        /// The options for patch
        options: Vec<String>,
    },
    /// A comment entry
    Comment(String),
}

/// A quilt series file
#[derive(Debug)]
pub struct Series {
    /// The entries in the series file
    pub entries: Vec<SeriesEntry>,
}

impl Series {
    /// Create a new series file
    pub fn new() -> Self {
        Self { entries: vec![] }
    }

    /// Get the number of patches in the series file
    pub fn len(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, SeriesEntry::Patch { .. }))
            .count()
    }

    /// Check if the series file is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the series file contains a patch
    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| match entry {
            SeriesEntry::Patch {
                name: entry_name, ..
            } => entry_name == name,
            _ => false,
        })
    }

    /// Read a series file from a reader
    pub fn read<R: std::io::Read>(reader: R) -> std::io::Result<Self> {
        let mut series = Self::new();

        let reader = std::io::BufReader::new(reader);

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.starts_with('#') {
                series.entries.push(SeriesEntry::Comment(line.to_string()));
                continue;
            }

            let mut parts = line.split_whitespace();
            let name = parts.next().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "missing patch name in series file",
                )
            })?;
            let options = parts.map(|s| s.to_string()).collect();

            series.entries.push(SeriesEntry::Patch {
                name: name.to_string(),
                options,
            });
        }

        Ok(series)
    }

    /// Remove a patch from the series file
    pub fn remove(&mut self, name: &str) {
        self.entries.retain(|entry| match entry {
            SeriesEntry::Patch {
                name: entry_name, ..
            } => entry_name != name,
            _ => true,
        });
    }

    /// Get an iterator over the patch names in the series file
    pub fn patches(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(|entry| match entry {
            SeriesEntry::Patch { name, .. } => Some(name.as_str()),
            _ => None,
        })
    }

    /// Append a patch to the series file
    pub fn append(&mut self, name: &str, options: Option<&[String]>) {
        self.entries.push(SeriesEntry::Patch {
            name: name.to_string(),
            options: options.map(|options| options.to_vec()).unwrap_or_default(),
        });
    }

    /// Write the series file to a writer
    pub fn write<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for entry in &self.entries {
            match entry {
                SeriesEntry::Patch { name, options } => {
                    write!(writer, "{}", name)?;
                    for option in options {
                        write!(writer, " {}", option)?;
                    }
                    writeln!(writer)?;
                }
                SeriesEntry::Comment(comment) => {
                    writeln!(writer, "# {}", comment)?;
                }
            }
        }

        Ok(())
    }

    /// Get an iterator over the entries in the series file
    pub fn iter(&self) -> std::slice::Iter<SeriesEntry> {
        self.entries.iter()
    }
}

impl std::ops::Index<usize> for Series {
    type Output = SeriesEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl Default for Series {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a .pc/.quilt_patches file
pub fn read_quilt_patches<R: std::io::Read>(mut reader: R) -> std::path::PathBuf {
    let mut p = String::new();
    reader.read_to_string(&mut p).unwrap();
    p.into()
}

/// Read a .pc/.quilt_series file
pub fn read_quilt_series<R: std::io::Read>(mut reader: R) -> std::path::PathBuf {
    let mut s = String::new();
    reader.read_to_string(&mut s).unwrap();
    s.into()
}

/// A quilt patch
pub struct QuiltPatch {
    /// The name of the patch
    pub name: String,

    /// The options for the patch
    pub options: Vec<String>,

    /// The patch contents
    pub patch: Vec<u8>,
}

impl QuiltPatch {
    /// Get the patch contents as a byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.patch
    }

    /// Get the name of the patch
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the patch options
    pub fn options(&self) -> &[String] {
        &self.options
    }

    /// Get the patch contents
    pub fn parse(&self) -> Result<Vec<crate::unified::UnifiedPatch>, crate::unified::Error> {
        let lines = self.patch.split(|&b| b == b'\n');
        crate::unified::parse_patches(lines.map(|x| x.to_vec()))
            .filter_map(|patch| match patch {
                Ok(crate::unified::PlainOrBinaryPatch::Plain(patch)) => Some(Ok(patch)),
                Ok(crate::unified::PlainOrBinaryPatch::Binary(_)) => None,
                Err(err) => Some(Err(err)),
            })
            .collect()
    }
}

/// Read quilt patches from a directory.
pub fn iter_quilt_patches(directory: &std::path::Path) -> impl Iterator<Item = QuiltPatch> + '_ {
    let series_path = directory.join("series");

    let series = if series_path.exists() {
        Series::read(std::fs::File::open(series_path).unwrap()).unwrap()
    } else {
        Series::new()
    };

    series
        .iter()
        .filter_map(move |entry| {
            let (patch, options) = match entry {
                SeriesEntry::Patch { name, options } => (name, options),
                SeriesEntry::Comment(_) => return None,
            };
            let p = directory.join(patch);
            let lines = std::fs::read_to_string(p).unwrap();
            Some(QuiltPatch {
                name: patch.to_string(),
                patch: lines.into_bytes(),
                options: options.clone(),
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_series_read() {
        let series = Series::read(
            r#"0001-foo.patch
# This is a comment
0002-bar.patch --reverse
0003-baz.patch --reverse --fuzz=3
"#
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(series.len(), 3);
        assert_eq!(
            series[0],
            SeriesEntry::Patch {
                name: "0001-foo.patch".to_string(),
                options: vec![]
            }
        );
        assert_eq!(
            series[1],
            SeriesEntry::Comment("# This is a comment".to_string())
        );
        assert_eq!(
            series[2],
            SeriesEntry::Patch {
                name: "0002-bar.patch".to_string(),
                options: vec!["--reverse".to_string()]
            }
        );
        assert_eq!(
            series[3],
            SeriesEntry::Patch {
                name: "0003-baz.patch".to_string(),
                options: vec!["--reverse".to_string(), "--fuzz=3".to_string()]
            }
        );
    }

    #[test]
    fn test_series_write() {
        let mut series = Series::new();
        series.append("0001-foo.patch", None);
        series.append("0002-bar.patch", Some(&["--reverse".to_string()]));
        series.append(
            "0003-baz.patch",
            Some(&["--reverse".to_string(), "--fuzz=3".to_string()]),
        );

        let mut writer = vec![];
        series.write(&mut writer).unwrap();
        let series = String::from_utf8(writer).unwrap();
        assert_eq!(
            series,
            "0001-foo.patch\n0002-bar.patch --reverse\n0003-baz.patch --reverse --fuzz=3\n"
        );
    }

    #[test]
    fn test_series_remove() {
        let mut series = Series::new();
        series.append("0001-foo.patch", None);
        series.append("0002-bar.patch", Some(&["--reverse".to_string()]));
        series.append(
            "0003-baz.patch",
            Some(&["--reverse".to_string(), "--fuzz=3".to_string()]),
        );

        series.remove("0002-bar.patch");

        let mut writer = vec![];
        series.write(&mut writer).unwrap();
        let series = String::from_utf8(writer).unwrap();
        assert_eq!(
            series,
            "0001-foo.patch\n0003-baz.patch --reverse --fuzz=3\n"
        );
    }

    #[test]
    fn test_series_contains() {
        let mut series = Series::new();
        series.append("0001-foo.patch", None);
        series.append("0002-bar.patch", Some(&["--reverse".to_string()]));
        series.append(
            "0003-baz.patch",
            Some(&["--reverse".to_string(), "--fuzz=3".to_string()]),
        );

        assert!(series.contains("0002-bar.patch"));
        assert!(!series.contains("0004-qux.patch"));
    }

    #[test]
    fn test_series_patches() {
        let mut series = Series::new();
        series.append("0001-foo.patch", None);
        series.append("0002-bar.patch", Some(&["--reverse".to_string()]));
        series.append(
            "0003-baz.patch",
            Some(&["--reverse".to_string(), "--fuzz=3".to_string()]),
        );

        let patches: Vec<_> = series.patches().collect();
        assert_eq!(
            patches,
            &["0001-foo.patch", "0002-bar.patch", "0003-baz.patch"]
        );
    }

    #[test]
    fn test_series_is_empty() {
        let series = Series::new();
        assert!(series.is_empty());

        let mut series = Series::new();
        series.append("0001-foo.patch", None);
        assert!(!series.is_empty());
    }
}
