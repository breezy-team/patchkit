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
}
