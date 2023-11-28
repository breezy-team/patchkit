use std::collections::HashMap;
pub struct SeriesEntry {
    pub name: String,
}

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
        let names = vec!["0001-foo.patch", "0002-bar.patch", "0003-baz.patch", "0004-qux"];
        assert_eq!(super::find_common_patch_suffix(names.into_iter()), Some(".patch"));
    }

    #[test]
    fn test_find_common_patch_suffix_no_patches() {
        let names = vec!["README", "0001-foo.patch", "0002-bar.patch", "0003-baz.patch"];
        assert_eq!(super::find_common_patch_suffix(names.into_iter()), Some(".patch"));
    }
}
