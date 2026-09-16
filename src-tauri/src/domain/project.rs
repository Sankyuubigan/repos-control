use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub has_upstream: bool,
    pub ahead: u32,
    pub behind: u32,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub changed_files: Vec<ChangedFile>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
}

impl Default for ProjectStatus {
    fn default() -> Self {
        Self {
            is_repo: false,
            branch: None,
            has_upstream: false,
            ahead: 0,
            behind: 0,
            staged: 0,
            unstaged: 0,
            untracked: 0,
            changed_files: Vec::new(),
            error: None,
        }
    }
}

pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub old_oid: Option<gix::hash::ObjectId>,
    pub new_oid: Option<gix::hash::ObjectId>,
    pub worktree: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
}