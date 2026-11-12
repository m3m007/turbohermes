//! Controlled worktree scope for a dispatched task.
//!
//! **Ported from jcode's own SDK** (upstream commit
//! `7206efd0c1dbf49ef38d06b63f3a96a3a7cee611`, MIT):
//! `crates/jcode-sdk/src/worktrees.rs`, lines 22-155 (the `Worktree` struct,
//! `list`, `parse`, `safe_name`, `create`) and lines 158-318 (its test module,
//! which is the source of the tests at the bottom of this file).
//!
//! The four properties upstream establishes and this port keeps:
//!
//! 1. **`-b`, never `-B`.** Git atomically refuses an already-existing branch,
//!   so a colliding task cannot silently adopt someone else's branch.
//! 2. **No force, no reset.** Creation checks out a new branch from the
//!   invoking worktree's HEAD without copying staged, unstaged or untracked
//!   changes.
//! 3. **`safe_name` percent-encoding, not separator replacement.** `feature/a`
//!   and `feature-a` must not collide, and `feature%2Fa` must not either.
//! 4. **Errors never trigger destructive cleanup.** A Git failure may leave a
//!   newly created branch or directory for an operator to inspect; the caller
//!   decides. That is why [`Scope::remove`] is a separate, explicit call.
//!
//! Upstream's `create` returns the `Worktree` git reports; this port returns a
//! [`Scope`] that owns the path and branch and can remove the worktree. The
//! parsing, encoding and validation logic is upstream's, unchanged.

use std::path::Path;
use std::process::Command;

/// A worktree reported by Git. Branches are full refs (e.g. `refs/heads/main`).
///
/// Ported from `jcode-sdk/src/worktrees.rs` lines 15-24. Bare repositories
/// have an empty `head`. Flag-only lock/prune reasons are
/// `Some(String::new())`. Paths must be valid UTF-8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub head: String,
    pub bare: bool,
    pub detached: bool,
    pub locked: Option<String>,
    pub prunable: Option<String>,
}

/// Errors from worktree operations.
#[derive(Debug)]
pub enum ScopeError {
    /// Git could not be run, or exited non-zero. Carries git's own stderr.
    Git(String),
    /// Git's output was not UTF-8.
    NotUtf8,
    /// The repository has no worktrees, no parent directory, or no name.
    NotARepository(String),
    /// The requested branch name is not a literal, valid branch name.
    InvalidBranch(String),
    /// The worktree destination already exists.
    DestinationExists(String),
    /// The worktree root exists but is not a real directory.
    BadWorktreeRoot(String),
    /// The worktree path is not valid UTF-8.
    PathNotUtf8,
}

impl std::fmt::Display for ScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(m) => write!(f, "{m}"),
            Self::NotUtf8 => write!(f, "git worktree output is not UTF-8"),
            Self::NotARepository(m) => write!(f, "{m}"),
            Self::InvalidBranch(m) => write!(f, "{m}"),
            Self::DestinationExists(p) => write!(f, "worktree destination already exists: {p}"),
            Self::BadWorktreeRoot(m) => write!(f, "{m}"),
            Self::PathNotUtf8 => write!(f, "worktree path is not UTF-8"),
        }
    }
}

impl std::error::Error for ScopeError {}

/// Run git in `working_dir`, returning stdout or git's stderr on failure.
///
/// Ported from `jcode-sdk/src/worktrees.rs` lines 26-37. Upstream uses
/// `anyhow::{Context, bail, ensure}` and a `no_console_window` wrapper for
/// Windows; the failure path is preserved as `ScopeError::Git` carrying git's
/// trimmed stderr, because that stderr is the only diagnosis an operator gets.
fn git(working_dir: &str, args: &[&str]) -> Result<Vec<u8>, ScopeError> {
    let output = Command::new("git")
        .current_dir(working_dir)
        .args(args)
        .output()
        .map_err(|e| ScopeError::Git(format!("Failed to run Git: {e}")))?;
    if !output.status.success() {
        return Err(ScopeError::Git(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

/// List all worktrees, including bare, detached, locked, and prunable entries.
/// Accepts the repository root or any directory within a worktree.
///
/// Ported from `jcode-sdk/src/worktrees.rs` line 43.
pub fn list(working_dir: &str) -> Result<Vec<Worktree>, ScopeError> {
    parse(&git(working_dir, &["worktree", "list", "--porcelain", "-z"])?)
}

/// Parse `git worktree list --porcelain -z` output.
///
/// Ported from `jcode-sdk/src/worktrees.rs` lines 49-89, including the two
/// behaviours that a line-oriented parser gets wrong: a NUL byte separates
/// *records*, not lines, so a path or lock reason containing a newline stays
/// intact; and a flag with no value (`bare`, `detached`, `locked` with no
/// reason) is `Some(String::new())`, not `None`.
fn parse(bytes: &[u8]) -> Result<Vec<Worktree>, ScopeError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ScopeError::NotUtf8)?;
    let mut entries = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in text.split('\0') {
        if field.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            continue;
        }
        let (key, value) = field.split_once(' ').unwrap_or((field, ""));
        if key == "worktree" {
            if current.is_some() {
                return Err(ScopeError::Git("Missing worktree record separator".into()));
            }
            current = Some(Worktree {
                path: value.to_owned(),
                branch: None,
                head: String::new(),
                bare: false,
                detached: false,
                locked: None,
                prunable: None,
            });
            continue;
        }
        let entry = current
            .as_mut()
            .ok_or_else(|| ScopeError::Git("Missing worktree path".into()))?;
        match key {
            "HEAD" => entry.head = value.to_owned(),
            "branch" => entry.branch = Some(value.to_owned()),
            "bare" => entry.bare = true,
            "detached" => entry.detached = true,
            "locked" => entry.locked = Some(value.to_owned()),
            "prunable" => entry.prunable = Some(value.to_owned()),
            _ => {} // Forward compatible with additional Git attributes.
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    Ok(entries)
}

/// Encode rather than replace separators, so feature/a and feature-a cannot
/// collide.
///
/// Ported from `jcode-sdk/src/worktrees.rs` lines 92-105.
fn safe_name(branch: &str) -> String {
    let mut name = String::new();
    for byte in branch.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            name.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(name, "%{byte:02X}").expect("writing to String");
        }
    }
    name
}

/// A controlled worktree owned by one dispatched task.
///
/// Created by [`Scope::create`], removed by [`Scope::remove`]. The struct holds
/// what the supervisor needs to record and to clean up: the path, the branch,
/// and the repository it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    path: String,
    branch: String,
    repo: String,
}

impl Scope {
    /// Create a new branch from the caller's HEAD and check it out in a
    /// sibling `<main-repo-name>-worktrees/<encoded-branch>` directory.
    ///
    /// Ported from `jcode-sdk/src/worktrees.rs` lines 111-154. The order of
    /// checks is upstream's and matters: validate the branch name, resolve the
    /// destination root, refuse a destination that already exists, then run
    /// `git worktree add -b branch -- path HEAD`.
    ///
    /// Nested and linked worktree invocations share the same destination root,
    /// because the root is derived from the *first* entry in `worktree list`,
    /// which is always the main worktree.
    pub fn create(repo: &str, branch: &str) -> Result<Self, ScopeError> {
        if branch.is_empty() || branch.starts_with('-') {
            return Err(ScopeError::InvalidBranch("Invalid branch name".into()));
        }
        let checked = git(repo, &["check-ref-format", "--branch", branch])?;
        // Reject Git shorthand such as @{-1}, which check-ref-format expands.
        if checked != format!("{branch}\n").as_bytes() {
            return Err(ScopeError::InvalidBranch(
                "Branch name must be literal".into(),
            ));
        }
        let entries = list(repo)?;
        let main = Path::new(
            &entries
                .first()
                .ok_or_else(|| ScopeError::NotARepository("Repository has no worktrees".into()))?
                .path,
        );
        let parent = main
            .parent()
            .ok_or_else(|| ScopeError::NotARepository("Repository has no parent directory".into()))?;
        let name = main
            .file_name()
            .ok_or_else(|| ScopeError::NotARepository("Repository has no directory name".into()))?
            .to_str()
            .ok_or(ScopeError::PathNotUtf8)?;
        let root = parent.join(format!("{name}-worktrees"));
        if let Ok(meta) = std::fs::symlink_metadata(&root) {
            if !(meta.is_dir() && !meta.file_type().is_symlink()) {
                return Err(ScopeError::BadWorktreeRoot(
                    "Worktree root is not a real directory".into(),
                ));
            }
        }
        let path = root.join(safe_name(branch));
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                return Err(ScopeError::DestinationExists(
                    path.display().to_string(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ScopeError::Git(format!(
                    "Cannot inspect worktree destination: {error}"
                )));
            }
        }
        let path = path.to_str().ok_or(ScopeError::PathNotUtf8)?;
        // -b, not -B: Git atomically refuses an already-existing branch.
        git(repo, &["worktree", "add", "-b", branch, "--", path, "HEAD"])?;
        let created = list(repo)?
            .into_iter()
            .find(|entry| entry.path == path)
            .ok_or_else(|| {
                ScopeError::Git("Created worktree was not present in Git's worktree list".into())
            })?;
        Ok(Self {
            path: created.path,
            branch: created.branch.unwrap_or_default(),
            repo: repo.to_owned(),
        })
    }

    /// The worktree's absolute path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The full ref of the branch this worktree holds, e.g.
    /// `refs/heads/feature/one`.
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// The repository the worktree was created from.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// Remove the worktree and its branch.
    ///
    /// Deliberately explicit and separate from [`Scope::create`]: upstream's
    /// rule is that errors never trigger destructive cleanup, so a failed
    /// create leaves things behind for inspection rather than removing them.
    /// This is the one destructive call, and it is the caller's decision.
    ///
    /// The branch is deleted with `-d` (safe delete), not `-D`, so a branch
    /// with unmerged work survives and is reported rather than discarded.
    pub fn remove(&self) -> Result<(), ScopeError> {
        git(&self.repo, &["worktree", "remove", &self.path])?;
        let short = self
            .branch
            .strip_prefix("refs/heads/")
            .unwrap_or(&self.branch);
        git(&self.repo, &["branch", "-d", short])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `jcode-sdk/src/worktrees.rs` lines 164-181. The repo name
    /// carries a space on purpose: it exercises the path handling that a
    /// `String::split(' ')` parser would break.
    fn repo() -> (tempfile::TempDir, String) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo with spaces");
        std::fs::create_dir(&path).unwrap();
        let path = path.to_str().unwrap().to_owned();
        git(&path, &["init", "-b", "main"]).unwrap();
        // Fixture-only identity, never used for repository development commits.
        git(&path, &["config", "user.name", "SDK Test"]).unwrap();
        git(&path, &["config", "user.email", "sdk-test@example.invalid"]).unwrap();
        std::fs::write(Path::new(&path).join("tracked"), "original").unwrap();
        git(&path, &["add", "tracked"]).unwrap();
        git(&path, &["commit", "-m", "initial"]).unwrap();
        (temp, path)
    }

    #[test]
    fn parses_nul_records_without_trimming_paths_or_reasons() {
        // Ported from upstream's test of the same name, line 180.
        let entries = parse(b"worktree /repo with\nnewline \0HEAD abc\0branch refs/heads/main\0locked reason\nwith newline \0\0worktree /bare\0bare\0\0worktree /detached\0HEAD def\0detached\0locked\0prunable missing path\0\0").unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "/repo with\nnewline ");
        assert_eq!(entries[0].locked.as_deref(), Some("reason\nwith newline "));
        assert!(entries[1].bare);
        assert_eq!(entries[1].head, "");
        assert!(entries[2].detached);
        assert_eq!(entries[2].branch, None);
        assert_eq!(entries[2].locked.as_deref(), Some(""));
        assert_eq!(entries[2].prunable.as_deref(), Some("missing path"));
        assert!(parse(b"worktree /\xff\0\0").is_err());
    }

    #[test]
    fn creation_isolates_dirty_edits_and_nested_invocation() {
        // Ported from upstream's test of the same name, line 200.
        let (_temp, path) = repo();
        std::fs::write(Path::new(&path).join("tracked"), "staged").unwrap();
        git(&path, &["add", "tracked"]).unwrap();
        std::fs::write(Path::new(&path).join("tracked"), "unstaged").unwrap();
        std::fs::write(Path::new(&path).join("untracked"), "private").unwrap();
        let nested = Path::new(&path).join("nested");
        std::fs::create_dir(&nested).unwrap();
        let created = Scope::create(nested.to_str().unwrap(), "feature/one").unwrap();
        // Upstream asserts the exact destination path; the tempdir prefix is
        // not stable across runs, so assert the shape and the reported path.
        assert!(created.path().ends_with("repo with spaces-worktrees/feature%2Fone"));
        assert_eq!(created.branch(), "refs/heads/feature/one");
        assert_eq!(
            std::fs::read_to_string(Path::new(&created.path()).join("tracked")).unwrap(),
            "original"
        );
        assert!(!Path::new(&created.path()).join("untracked").exists());
        assert_eq!(
            std::fs::read_to_string(Path::new(&path).join("tracked")).unwrap(),
            "unstaged"
        );
        assert_eq!(git(&path, &["show", ":tracked"]).unwrap(), b"staged");
    }

    #[test]
    fn rejects_invalid_branches_collisions_and_non_repositories() {
        // Ported from upstream's test of the same name, line 255.
        let (temp, path) = repo();
        for branch in [
            "",
            "-oops",
            "--detach",
            "a b",
            "../escape",
            "a..b",
            "a\nb",
            "@{-1}",
        ] {
            assert!(
                Scope::create(&path, branch).is_err(),
                "accepted {branch:?}"
            );
        }
        assert!(Scope::create(&path, "main").is_err());
        let created = Scope::create(&path, "new").unwrap();
        assert!(Scope::create(&path, "new").is_err());
        assert!(Path::new(created.path()).exists());
        let occupied = temp
            .path()
            .join("repo with spaces-worktrees/occupied");
        std::fs::create_dir(&occupied).unwrap();
        assert!(Scope::create(&path, "occupied").is_err());
        assert!(git(&path, &["show-ref", "--verify", "refs/heads/occupied"]).is_err());
        assert!(list(temp.path().to_str().unwrap()).is_err());
        assert!(Scope::create(temp.path().to_str().unwrap(), "valid").is_err());
        assert_ne!(safe_name("feature/a"), safe_name("feature-a"));
        assert_ne!(safe_name("feature/a"), safe_name("feature%2Fa"));
    }

    #[test]
    fn remove_deletes_the_worktree_and_its_branch() {
        let (_temp, path) = repo();
        let created = Scope::create(&path, "cleanup-me").unwrap();
        assert!(Path::new(created.path()).exists());
        created.remove().unwrap();
        assert!(!Path::new(created.path()).exists());
        assert!(git(&path, &["show-ref", "--verify", "refs/heads/cleanup-me"]).is_err());
    }

    #[test]
    fn unborn_head_does_not_create_a_branch() {
        // Ported from upstream's test of the same name, line 311.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_str().unwrap();
        git(path, &["init", "-b", "main"]).unwrap();
        assert!(Scope::create(path, "new").is_err());
        assert!(git(path, &["show-ref", "--verify", "refs/heads/new"]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_destination_root() {
        // Ported from upstream's test of the same name, line 317.
        let (temp, path) = repo();
        let elsewhere = temp.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(
            &elsewhere,
            temp.path().join("repo with spaces-worktrees"),
        )
        .unwrap();
        assert!(Scope::create(&path, "new").is_err());
        assert_eq!(std::fs::read_dir(elsewhere).unwrap().count(), 0);
    }
}
