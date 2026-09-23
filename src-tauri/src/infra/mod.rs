pub mod commit_draft_store;
pub mod commit_provider;
pub mod config_store;
pub mod fs_watcher;
pub mod git2_write;
pub mod gix_git;

pub use commit_draft_store::FileCommitDraftStore;
pub use commit_provider::StubCommitProvider;
pub use config_store::FileConfigStore;
pub use fs_watcher::WatcherManager;
pub use git2_write::Git2Write;
pub use gix_git::GixGit;