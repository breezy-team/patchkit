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
#[derive(Debug)]
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
