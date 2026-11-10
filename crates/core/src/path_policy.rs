use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tracing::{info, warn};

fn format_policy_parse_error(policy_file: &Path, error: &json5::Error) -> String {
    format!(
        "Path access policy JSON5 parse error in {}: {}",
        policy_file.display(),
        error
    )
}

/// Which operation is being performed on a path.
///
/// [`PathOp::List`] is also the directory-walk op (Glob / Grep): a walk root is
/// gated by [`PathPolicy::evaluate_deny_only`] and its results are then filtered
/// per entry with [`PathPolicy::evaluate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathOp {
    Read,
    Write,
    List,
    Exec,
}

impl PathOp {
    /// Derive the operation type from a tool name.
    pub fn from_tool_name(tool_name: &str) -> Self {
        match tool_name {
            "read_file" => PathOp::Read,
            "list_dir" => PathOp::List,
            "exec" => PathOp::Exec,
            // write-class tools
            "write_file" | "edit_file" | "file_ops" | "data_process" | "audio_transcribe"
            | "chart_generate" | "office_write" | "video_process" | "health_api" | "encrypt" => {
                PathOp::Write
            }
            _ => PathOp::Read,
        }
    }
}

/// What the policy engine decides for a given path + operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    /// Access allowed immediately — no confirmation required.
    Allow,
    /// User must confirm before access is granted.
    Confirm,
    /// Access denied; cannot be overridden by confirmation.
    Deny,
}

/// A single path rule entry inside `path_access.json5`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRule {
    /// Friendly name for the rule (for logging and documentation).
    pub name: String,
    /// The access decision when this rule matches.
    pub action: PolicyAction,
    /// Which operations this rule applies to.
    pub ops: Vec<PathOp>,
    /// Path prefixes this rule covers. `~` and `~/` prefixes are expanded.
    pub paths: Vec<String>,
}

/// The contents of the `path_access.json5` policy file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathPolicyFileConfig {
    /// Schema version — currently must be 1.
    #[serde(default = "default_version")]
    pub version: u32,

    /// What to do when no rule matches a path.
    /// Default: `confirm` (requires user approval).
    #[serde(default = "default_policy_confirm")]
    pub default_policy: PolicyAction,

    /// Whether the session-level directory cache is used to avoid re-confirming
    /// the same directory. Only applies to `confirm` outcomes.
    #[serde(default = "crate::default_true")]
    pub cache_confirmed_dirs: bool,

    /// When `true`, a built-in list of sensitive system paths (e.g. `~/.ssh`, `/etc`)
    /// is always denied, even if no explicit rule covers them.
    #[serde(default = "crate::default_true")]
    pub builtin_protected_paths: bool,

    /// User-defined rules, evaluated in priority order (deny > allow > confirm).
    #[serde(default)]
    pub rules: Vec<PathRule>,

    /// When `true`, a path whose **final component** is a symbolic link is
    /// denied outright instead of being canonicalized and evaluated at its
    /// target. Defaults to `false` so existing deployments keep today's
    /// canonicalizing behaviour.
    ///
    /// Ported from upstream `reject_if_symlink`
    /// (`moltis/crates/tools/src/fs/shared.rs:669`), which stats the
    /// *original* user-supplied path rather than the canonical one: a
    /// canonicalized path can never itself be a symlink, so checking it would
    /// be a no-op.
    #[serde(default)]
    pub reject_symlinks: bool,
}

fn default_version() -> u32 {
    1
}
fn default_policy_confirm() -> PolicyAction {
    PolicyAction::Confirm
}
impl Default for PathPolicyFileConfig {
    fn default() -> Self {
        Self {
            version: 1,
            default_policy: PolicyAction::Confirm,
            cache_confirmed_dirs: true,
            builtin_protected_paths: true,
            reject_symlinks: false,
            rules: Vec::new(),
        }
    }
}

/// Validate the path globs of a [`PathPolicyFileConfig`], returning one warning
/// string per suspicious entry. Never returns an error, so a config that would
/// have loaded before still loads.
///
/// Ported from upstream `moltis/crates/config/src/validate/semantic.rs:285-309`,
/// which *warns* on `allow_paths` / `deny_paths` entries that are neither
/// absolute nor `**` globs, while a relative `workspace_root` (line 273) is a
/// hard error. The warning-only shape is kept here because a rejected config
/// would fall back to safe defaults, which is a bigger behaviour change than
/// the typo it is guarding against.
///
/// Divergences from upstream: upstream has no tilde expansion, so a `~/...`
/// entry warns there; here `~` and `~/...` are first-class prefixes handled by
/// [`expand_tilde`] and are therefore valid.
pub fn validate_path_rules(config: &PathPolicyFileConfig) -> Vec<String> {
    let mut warnings = Vec::new();
    for (rule_idx, rule) in config.rules.iter().enumerate() {
        for (path_idx, entry) in rule.paths.iter().enumerate() {
            let acceptable = Path::new(entry).is_absolute()
                || entry == "~"
                || entry.starts_with("~/")
                || entry.starts_with("**");
            if !acceptable {
                warnings.push(format!(
                    "rules[{rule_idx}] ('{}').paths[{path_idx}] = '{entry}' is not an absolute path, \
                     a '~' prefixed path, or a '**' glob — it will never match a resolved path",
                    rule.name
                ));
            }
        }
    }
    warnings
}

/// Built-in protected path prefixes used when `builtin_protected_paths = true`
/// (the default): the union of [`builtin_secret_paths`] and [`builtin_system_paths`].
pub fn builtin_sensitive_paths() -> &'static [&'static str] {
    &[
        "~/.ssh",
        "~/.aws",
        "~/.gnupg",
        "~/.kube",
        "~/.config/gcloud",
        "~/.azure",
        "~/.netrc",
        "/etc",
        "/System",
        "/private/etc",
        "/private/var",
        "/usr/bin",
        "/usr/sbin",
        "/bin",
        "/sbin",
    ]
}

/// Credential and secret locations: every operation is denied.
pub fn builtin_secret_paths() -> &'static [&'static str] {
    &[
        "~/.ssh",
        "~/.aws",
        "~/.gnupg",
        "~/.kube",
        "~/.config/gcloud",
        "~/.azure",
        "~/.netrc",
        "/etc/shadow",
        "/etc/gshadow",
        "/etc/sudoers",
        "/etc/sudoers.d",
        "/etc/ssl/private",
        "/private/etc/master.passwd",
        "/private/etc/sudoers",
        "/private/etc/sudoers.d",
        "/private/var",
    ]
}

/// System configuration and binary directories: read/list/exec follow the normal
/// rules, writes are denied unless a user `allow` rule names a more specific path.
pub fn builtin_system_paths() -> &'static [&'static str] {
    &[
        "/etc",
        "/System",
        "/private/etc",
        "/usr/bin",
        "/usr/sbin",
        "/bin",
        "/sbin",
    ]
}

/// The runtime path-policy engine. Loaded from the policy file at startup.
#[derive(Debug, Clone)]
pub struct PathPolicy {
    config: PathPolicyFileConfig,
    /// `true` when loaded successfully from a file (rather than using safe defaults).
    pub from_file: bool,
}

impl PathPolicy {
    /// Load the policy from the given file path.
    ///
    /// If the file does not exist, or is unreadable / unparseable, falls back to
    /// safe defaults (workspace = allow, sensitive = deny, everything else = confirm).
    pub fn load(policy_file: &Path) -> Self {
        if !policy_file.exists() {
            info!(
                path = %policy_file.display(),
                "Path policy file not found — using safe defaults"
            );
            return Self::safe_default();
        }

        match std::fs::read_to_string(policy_file) {
            Ok(content) => match json5::from_str::<PathPolicyFileConfig>(&content) {
                Ok(config) => {
                    if config.version != 1 {
                        warn!(
                            path = %policy_file.display(),
                            version = config.version,
                            "Unsupported path access policy version — using safe defaults"
                        );
                        return Self::safe_default();
                    }
                    info!(
                        path = %policy_file.display(),
                        rules = config.rules.len(),
                        "Loaded path access policy"
                    );
                    for warning in validate_path_rules(&config) {
                        warn!(path = %policy_file.display(), "{warning}");
                    }
                    Self {
                        config,
                        from_file: true,
                    }
                }
                Err(e) => {
                    warn!(
                        path = %policy_file.display(),
                        error = %format_policy_parse_error(policy_file, &e),
                        "Failed to parse path access policy file — using safe defaults"
                    );
                    Self::safe_default()
                }
            },
            Err(e) => {
                warn!(
                    path = %policy_file.display(),
                    error = %e,
                    "Failed to read path access policy file — using safe defaults"
                );
                Self::safe_default()
            }
        }
    }

    /// Construct the safe default policy (no user-defined rules).
    pub fn safe_default() -> Self {
        Self {
            config: PathPolicyFileConfig::default(),
            from_file: false,
        }
    }

    /// Evaluate the policy for a given **resolved** (canonicalized / normalized) path
    /// and operation type.
    ///
    /// Evaluation priority:
    /// 1. Built-in secret paths → always `Deny`; built-in system dirs → `Deny` for
    ///    writes not covered by a more specific user `allow` rule
    /// 2. User `deny` rules (most-specific prefix wins within the same action)
    /// 3. User `allow` rules (most-specific prefix)
    ///    — an `allow` rule that is MORE specific than a `deny` rule wins
    /// 4. User `confirm` rules
    /// 5. `default_policy`
    ///
    /// When [`PathPolicyFileConfig::reject_symlinks`] is `true`, step 0 refuses
    /// any path that is itself a symbolic link. That step stats the path exactly
    /// as it was handed in — never a canonicalized copy of it — because
    /// canonicalization resolves symlinks away, so checking the canonical path
    /// would always answer "not a symlink" and implement a no-op. Callers that
    /// hold a canonical path must therefore keep the original alongside it and
    /// run this check against the original; see [`PathPolicy::reject_symlink`].
    pub fn evaluate(&self, resolved: &Path, op: PathOp) -> PolicyAction {
        // 0. Symlink refusal (opt-in, off by default).
        if self.config.reject_symlinks && Self::reject_symlink(resolved) {
            return PolicyAction::Deny;
        }

        // 1. Built-in sensitive paths
        if self.config.builtin_protected_paths {
            for secret in builtin_secret_paths() {
                if path_starts_with_normalized(resolved, &expand_tilde(secret)) {
                    return PolicyAction::Deny;
                }
            }
            if op == PathOp::Write {
                for system in builtin_system_paths() {
                    let expanded = expand_tilde(system);
                    if path_starts_with_normalized(resolved, &expanded) {
                        let explicit_allow = self
                            .best_match_len(resolved, op, PolicyAction::Allow)
                            .is_some_and(|len| len > expanded.as_os_str().len());
                        if !explicit_allow {
                            return PolicyAction::Deny;
                        }
                        break;
                    }
                }
            }
        }

        // Find the most specific matching rule for each action type
        let deny_len = self.best_match_len(resolved, op, PolicyAction::Deny);
        let allow_len = self.best_match_len(resolved, op, PolicyAction::Allow);
        let confirm_len = self.best_match_len(resolved, op, PolicyAction::Confirm);

        // 2. Deny vs allow — more specific wins (longer prefix)
        if let Some(dl) = deny_len {
            if let Some(al) = allow_len {
                if al > dl {
                    return PolicyAction::Allow;
                }
            }
            return PolicyAction::Deny;
        }

        // 3. Allow
        if allow_len.is_some() {
            return PolicyAction::Allow;
        }

        // 4. Confirm
        if confirm_len.is_some() {
            return PolicyAction::Confirm;
        }

        // 5. Default
        self.config.default_policy
    }

    /// Whether a path is itself a symbolic link.
    ///
    /// Sync port of upstream `reject_if_symlink`
    /// (`moltis/crates/tools/src/fs/shared.rs:669`), which is async and takes
    /// the **original** user-supplied `&str`. The upstream doc comment states the
    /// reason for that choice: `canonicalize_existing` (same file, line 635) has
    /// already resolved symlinks, so the canonical path is never a symlink and
    /// checking it would be a no-op. Same reasoning is preserved here.
    ///
    /// Upstream turns a failed `stat` into an error; this returns `false`
    /// instead, because a path that cannot be stat'd at all (typically because
    /// it does not exist yet, as in a write to a new file) is not a symlink and
    /// must not be denied by a check that only exists to refuse links.
    pub fn reject_symlink(path: &Path) -> bool {
        match std::fs::symlink_metadata(path) {
            Ok(meta) => meta.file_type().is_symlink(),
            Err(_) => false,
        }
    }

    /// Deny-only check used to gate a directory-walk root (Glob / Grep / List).
    ///
    /// Returns `true` when the path is denied and the whole call should be
    /// rejected, `false` when the walk may proceed and be filtered per entry.
    ///
    /// Ported from upstream `check_deny_only`
    /// (`moltis/crates/tools/src/fs/shared.rs:384`) together with
    /// `enforce_path_policy_deny_only` (same file, line 427) and
    /// `path_denied_payload` (line 432, keys `kind` / `file_path` / `error` /
    /// `detail`). Upstream keeps two `GlobSet`s and this variant consults only
    /// the deny one: a walk root normally does not match a file-granular allow
    /// list even though its children do, so the allow list has to filter
    /// results rather than gate the call. Deny-list matches still reject the
    /// entire call, and per-entry filtering continues to apply the full
    /// [`PathPolicy::evaluate`].
    ///
    /// The deny list here is: the built-in secret paths when
    /// [`PathPolicyFileConfig::builtin_protected_paths`] is on (same
    /// short-circuit [`PathPolicy::evaluate`] performs), plus every user rule
    /// with `action: "deny"` whose `ops` cover `op`. In `path_access.json5`
    /// that means a `deny` rule whose `ops` include the walk op (`"list"` for
    /// Glob / Grep / `list_dir`) *is* the deny-only list — no separate config
    /// key exists or is needed for it.
    ///
    /// Divergences from [`PathPolicy::evaluate`], both deliberate:
    ///
    /// * A more specific user `allow` rule does **not** override a matching deny
    ///   rule here. Upstream `check_deny_only` is a pure deny test for the same
    ///   reason. An operator who wants `~/dev` walked out from under a broader
    ///   `deny` on `~` should scope that deny rule's `paths` or `ops` instead.
    /// * Built-in *system* paths (`/etc`, `/usr/bin`, …) are not part of this
    ///   check. They are conditional — denied for `Write` only when no more
    ///   specific `allow` rule exists — so folding them in would reject a walk
    ///   root wholesale while the per-entry policy would have allowed most of
    ///   it, which is exactly what the deny-only variant exists to avoid.
    pub fn evaluate_deny_only(&self, resolved: &Path, op: PathOp) -> bool {
        if self.config.builtin_protected_paths {
            for secret in builtin_secret_paths() {
                if path_starts_with_normalized(resolved, &expand_tilde(secret)) {
                    return true;
                }
            }
        }
        self.best_match_len(resolved, op, PolicyAction::Deny)
            .is_some()
    }

    /// Paths explicitly allowed for writes, resolved for use as sandbox roots.
    pub fn allowed_write_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for rule in &self.config.rules {
            if rule.action != PolicyAction::Allow || !rule.ops.contains(&PathOp::Write) {
                continue;
            }
            for path in &rule.paths {
                let root = resolve_for_policy(&expand_tilde(path));
                if root.parent().is_some()
                    && self.evaluate(&root, PathOp::Write) == PolicyAction::Allow
                    && !roots.contains(&root)
                {
                    roots.push(root);
                }
            }
        }
        roots
    }

    /// Returns the prefix length (in bytes) of the best-matching rule for the given
    /// action type, or `None` if no rule matches.
    fn best_match_len(&self, resolved: &Path, op: PathOp, action: PolicyAction) -> Option<usize> {
        let mut best: Option<usize> = None;
        for rule in &self.config.rules {
            if rule.action != action {
                continue;
            }
            // PathOp::Exec 兼具读写能力：只声明 read/write 的 deny 规则也必须
            // 拦截 exec 提取出的路径，否则 `exec "cat > <deny-dir>/f"` 可绕过。
            // allow/confirm 方向不扩展——用户声明 write 不等于授权 exec。
            let op_matches = rule.ops.contains(&op)
                || (action == PolicyAction::Deny
                    && op == PathOp::Exec
                    && (rule.ops.contains(&PathOp::Read) || rule.ops.contains(&PathOp::Write)));
            if !op_matches {
                continue;
            }
            for pattern in &rule.paths {
                let expanded = expand_tilde(pattern);
                if path_starts_with_normalized(resolved, &expanded) {
                    let len = expanded.as_os_str().len();
                    if best.map(|b| len > b).unwrap_or(true) {
                        best = Some(len);
                    }
                }
            }
        }
        best
    }

    /// Whether session-level confirmation caching is enabled.
    pub fn cache_confirmed_dirs(&self) -> bool {
        self.config.cache_confirmed_dirs
    }
}

impl Default for PathPolicy {
    fn default() -> Self {
        Self::safe_default()
    }
}

// ── Path helpers ─────────────────────────────────────────────────────────────

/// Expand a `~/...` or `~` path prefix to an absolute path.
pub fn expand_tilde(path_str: &str) -> PathBuf {
    if let Some(rest) = path_str.strip_prefix("~/") {
        dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| PathBuf::from(path_str))
    } else if path_str == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(path_str)
    }
}

/// Check whether `path` starts with `base`, after resolving both sides.
///
/// Both sides go through [`resolve_for_policy`], which canonicalizes the
/// longest existing prefix (resolving any symlinks in it) before appending the
/// remaining non-existing components. This prevents a symlinked directory from
/// being used to escape a deny rule when the final target does not exist yet
/// (e.g. a write to `~/allowed/link/newfile` where `link -> ~/.ssh`), which a
/// purely lexical normalization would miss.
pub fn path_starts_with_normalized(path: &Path, base: &Path) -> bool {
    let p = resolve_for_policy(path);
    let b = resolve_for_policy(base);
    p.starts_with(&b)
}

/// Resolve a path for policy evaluation.
///
/// Fully canonicalizes the path when it exists. Otherwise it canonicalizes the
/// longest existing ancestor (so symlinks in real directories are resolved) and
/// re-applies the remaining components lexically (handling `..`/`.`). Only when
/// no ancestor exists does it fall back to pure lexical normalization.
///
/// Resolving both the candidate and the rule prefix through the same routine
/// keeps their representations consistent (e.g. avoids mixing Windows `\\?\C:\`
/// with `C:\`) while closing the symlink-escape gap of lexical-only handling.
pub fn resolve_for_policy(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        if let Ok(canonical) = std::fs::canonicalize(ancestor) {
            let mut out = canonical;
            if let Ok(rest) = path.strip_prefix(ancestor) {
                for component in rest.components() {
                    match component {
                        Component::ParentDir => {
                            out.pop();
                        }
                        Component::CurDir => {}
                        c => out.push(c.as_os_str()),
                    }
                }
            }
            return out;
        }
    }
    normalize_path(path)
}

fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in p.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

// ── Template ─────────────────────────────────────────────────────────────────

/// Returns the content of a starter `path_access.json5` template.
/// Written to `~/.blockcell/path_access.json5` on first agent startup when
/// the file does not already exist.
pub fn default_policy_template() -> &'static str {
    r#"{
  // Path Access Policy — Blockcell
  // See docs/path_access_policy.md for full documentation.
  version: 1,

  // Fallback when no rule matches: "allow" | "confirm" | "deny"
  default_policy: "confirm",

  // Re-use session approval for the whole directory (reduces repeated prompts)
  cache_confirmed_dirs: true,

  // Always deny access to built-in sensitive paths (~/.ssh, /etc, etc.)
  builtin_protected_paths: true,

  rules: [
    // ── Deny sensitive credential directories (highest priority) ─────────
    {
      name: "deny-secrets",
      action: "deny",
      ops: ["read", "write", "list", "exec"],
      paths: [
        "~/.ssh",
        "~/.aws",
        "~/.gnupg",
        "~/.kube",
        "~/.config/gcloud",
        "/etc",
        "/System"
      ]
    },

    // ── Allow common development directories without confirmation ─────────
    // Uncomment and adjust to match your own workspace roots:
    // {
    //   name: "allow-dev-roots",
    //   action: "allow",
    //   ops: ["read", "list", "write"],
    //   paths: [
    //     "~/dev",
    //     "~/projects",
    //     "~/Desktop",
    //     "~/Documents"
    //   ]
    // },

    // ── Require confirmation for exec in home directory ───────────────────
    {
      name: "confirm-home-exec",
      action: "confirm",
      ops: ["exec"],
      paths: ["~"]
    }
  ]
}
"#
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn home() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/user"))
    }

    fn make_policy(rules: Vec<PathRule>, default: PolicyAction) -> PathPolicy {
        PathPolicy {
            config: PathPolicyFileConfig {
                version: 1,
                default_policy: default,
                cache_confirmed_dirs: true,
                builtin_protected_paths: false, // disable built-ins so unit tests are deterministic
                reject_symlinks: false,
                rules,
            },
            from_file: false,
        }
    }

    /// Same as [`make_policy`] but with `reject_symlinks` enabled.
    fn make_policy_reject_symlinks(
        rules: Vec<PathRule>,
        default: PolicyAction,
        reject_symlinks: bool,
    ) -> PathPolicy {
        PathPolicy {
            config: PathPolicyFileConfig {
                version: 1,
                default_policy: default,
                cache_confirmed_dirs: true,
                builtin_protected_paths: false,
                reject_symlinks,
                rules,
            },
            from_file: false,
        }
    }

    #[test]
    fn test_allow_rule_matches() {
        let dev_path = home().join("dev");
        let policy = make_policy(
            vec![PathRule {
                name: "allow-dev".to_string(),
                action: PolicyAction::Allow,
                ops: vec![PathOp::Read, PathOp::Write],
                paths: vec!["~/dev".to_string()],
            }],
            PolicyAction::Confirm,
        );
        let target = dev_path.join("project").join("main.rs");
        assert_eq!(policy.evaluate(&target, PathOp::Read), PolicyAction::Allow);
        assert_eq!(policy.evaluate(&target, PathOp::Write), PolicyAction::Allow);
        // exec not covered by this rule → default (confirm)
        assert_eq!(
            policy.evaluate(&target, PathOp::Exec),
            PolicyAction::Confirm
        );
    }

    #[test]
    fn test_deny_rule_matches() {
        let policy = make_policy(
            vec![PathRule {
                name: "deny-ssh".to_string(),
                action: PolicyAction::Deny,
                ops: vec![PathOp::Read, PathOp::Write, PathOp::List, PathOp::Exec],
                paths: vec!["~/.ssh".to_string()],
            }],
            PolicyAction::Confirm,
        );
        let ssh_key = home().join(".ssh").join("id_rsa");
        assert_eq!(policy.evaluate(&ssh_key, PathOp::Read), PolicyAction::Deny);
    }

    #[test]
    fn test_read_write_deny_rule_also_covers_exec() {
        // exec 兼具读写能力：仅声明 read/write 的 deny 规则必须拦截 exec 路径。
        let policy = make_policy(
            vec![PathRule {
                name: "deny-secrets".to_string(),
                action: PolicyAction::Deny,
                ops: vec![PathOp::Read, PathOp::Write],
                paths: vec!["~/.secrets".to_string()],
            }],
            PolicyAction::Confirm,
        );
        let target = home().join(".secrets").join("key.pem");
        assert_eq!(policy.evaluate(&target, PathOp::Exec), PolicyAction::Deny);

        // 反向不成立：confirm/allow 规则不扩展匹配 exec。
        let policy = make_policy(
            vec![PathRule {
                name: "confirm-work".to_string(),
                action: PolicyAction::Confirm,
                ops: vec![PathOp::Write],
                paths: vec!["~/work".to_string()],
            }],
            PolicyAction::Allow,
        );
        assert_eq!(
            policy.evaluate(&home().join("work").join("f"), PathOp::Exec),
            PolicyAction::Allow
        );
    }

    #[test]
    fn test_more_specific_allow_overrides_deny() {
        let policy = make_policy(
            vec![
                PathRule {
                    name: "deny-home".to_string(),
                    action: PolicyAction::Deny,
                    ops: vec![PathOp::Write],
                    paths: vec!["~".to_string()],
                },
                PathRule {
                    name: "allow-dev".to_string(),
                    action: PolicyAction::Allow,
                    ops: vec![PathOp::Write],
                    paths: vec!["~/dev".to_string()],
                },
            ],
            PolicyAction::Confirm,
        );
        let dev_file = home().join("dev").join("code.rs");
        // ~/dev is more specific than ~ → allow wins
        assert_eq!(
            policy.evaluate(&dev_file, PathOp::Write),
            PolicyAction::Allow
        );
        // ~/Documents is only covered by deny-home
        let doc_file = home().join("Documents").join("notes.txt");
        assert_eq!(
            policy.evaluate(&doc_file, PathOp::Write),
            PolicyAction::Deny
        );
    }

    #[test]
    fn test_default_policy_applied_when_no_rules_match() {
        let policy = make_policy(vec![], PolicyAction::Confirm);
        let random_path = PathBuf::from("/tmp/some/file.txt");
        assert_eq!(
            policy.evaluate(&random_path, PathOp::Read),
            PolicyAction::Confirm
        );
    }

    #[test]
    fn test_builtin_sensitive_paths_deny() {
        let policy = PathPolicy {
            config: PathPolicyFileConfig {
                builtin_protected_paths: true,
                rules: vec![],
                default_policy: PolicyAction::Confirm,
                ..Default::default()
            },
            from_file: false,
        };
        let ssh_key = home().join(".ssh").join("id_rsa");
        assert_eq!(policy.evaluate(&ssh_key, PathOp::Read), PolicyAction::Deny);
    }

    #[test]
    fn test_builtin_system_paths_read_allowed_write_denied() {
        let mk = |rules| PathPolicy {
            config: PathPolicyFileConfig {
                builtin_protected_paths: true,
                rules,
                default_policy: PolicyAction::Allow,
                ..Default::default()
            },
            from_file: false,
        };
        let policy = mk(vec![]);
        let conf = PathBuf::from("/etc/nginx/conf.d/site.conf");
        assert_eq!(policy.evaluate(&conf, PathOp::Read), PolicyAction::Allow);
        assert_eq!(
            policy.evaluate(&PathBuf::from("/etc/nginx"), PathOp::List),
            PolicyAction::Allow
        );
        assert_eq!(policy.evaluate(&conf, PathOp::Write), PolicyAction::Deny);
        assert_eq!(
            policy.evaluate(&PathBuf::from("/etc/shadow"), PathOp::Read),
            PolicyAction::Deny
        );
        assert_eq!(
            policy.evaluate(&PathBuf::from("/etc/sudoers.d/x"), PathOp::Exec),
            PolicyAction::Deny
        );

        let allowed = mk(vec![PathRule {
            name: "nginx".into(),
            paths: vec!["/etc/nginx".into()],
            ops: vec![PathOp::Write],
            action: PolicyAction::Allow,
        }]);
        assert_eq!(allowed.evaluate(&conf, PathOp::Write), PolicyAction::Allow);
        assert_eq!(
            allowed.evaluate(&PathBuf::from("/etc/hosts"), PathOp::Write),
            PolicyAction::Deny
        );
    }

    #[test]
    fn test_policy_from_template_parses() {
        let config: PathPolicyFileConfig =
            json5::from_str(default_policy_template()).expect("template should parse");
        assert_eq!(config.version, 1);
        assert!(!config.rules.is_empty());
    }

    #[test]
    fn test_path_op_from_tool_name() {
        assert_eq!(PathOp::from_tool_name("read_file"), PathOp::Read);
        assert_eq!(PathOp::from_tool_name("write_file"), PathOp::Write);
        assert_eq!(PathOp::from_tool_name("list_dir"), PathOp::List);
        assert_eq!(PathOp::from_tool_name("exec"), PathOp::Exec);
        assert_eq!(PathOp::from_tool_name("edit_file"), PathOp::Write);
    }

    /// Create a unique temporary directory without pulling in an extra crate.
    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "bc_pathpolicy_{tag}_{}_{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn test_symlinked_dir_cannot_escape_deny_for_nonexistent_target() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("symlink_deny");
        let secret = root.join("secret");
        std::fs::create_dir_all(&secret).unwrap();
        let link = root.join("link");
        symlink(&secret, &link).unwrap();

        // Deny rule is written against the real (canonical) secret directory.
        let canon_secret = std::fs::canonicalize(&secret).unwrap();
        let policy = make_policy(
            vec![PathRule {
                name: "deny-secret".to_string(),
                action: PolicyAction::Deny,
                ops: vec![PathOp::Read, PathOp::Write],
                paths: vec![canon_secret.to_string_lossy().to_string()],
            }],
            PolicyAction::Allow,
        );

        // A not-yet-existing file reached through the symlinked directory must
        // still resolve to the real secret dir and be denied.
        let via_link = link.join("new_file.txt");
        assert_eq!(
            policy.evaluate(&via_link, PathOp::Write),
            PolicyAction::Deny
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_nonexistent_target_under_real_dir_matches_prefix() {
        let root = unique_temp_dir("real_prefix");
        let allowed = root.join("allowed");
        std::fs::create_dir_all(&allowed).unwrap();

        let canon_allowed = std::fs::canonicalize(&allowed).unwrap();
        let policy = make_policy(
            vec![PathRule {
                name: "allow-dir".to_string(),
                action: PolicyAction::Allow,
                ops: vec![PathOp::Write],
                paths: vec![canon_allowed.to_string_lossy().to_string()],
            }],
            PolicyAction::Confirm,
        );

        // Deeply nested non-existing target stays matched to its real prefix.
        let target = allowed.join("sub").join("deeper").join("file.txt");
        assert_eq!(policy.evaluate(&target, PathOp::Write), PolicyAction::Allow);

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── evaluate_deny_only (upstream shared.rs:384 / 427 / 432) ──────────────

    /// Build a policy with an `allow` root and a `deny` subtree under it, both
    /// written against canonical paths so the test works on macOS too (where
    /// `/tmp` is itself a symlink to `/private/tmp`).
    fn deny_only_policy() -> (PathPolicy, PathBuf, PathBuf) {
        let root = unique_temp_dir("deny_only");
        let allowed = root.join("allowed");
        let secret = root.join("secret");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&secret).unwrap();
        let canon = |p: &Path| {
            std::fs::canonicalize(p)
                .unwrap()
                .to_string_lossy()
                .to_string()
        };
        let policy = make_policy(
            vec![
                PathRule {
                    name: "allow-root".to_string(),
                    action: PolicyAction::Allow,
                    ops: vec![PathOp::Read, PathOp::List],
                    paths: vec![canon(&root)],
                },
                PathRule {
                    name: "deny-secret".to_string(),
                    action: PolicyAction::Deny,
                    ops: vec![PathOp::Read, PathOp::List],
                    paths: vec![canon(&secret)],
                },
            ],
            PolicyAction::Confirm,
        );
        (policy, root.join("allowed"), root.join("secret"))
    }

    #[test]
    fn test_evaluate_deny_only_gates_walk_root_only() {
        let (policy, allowed, secret) = deny_only_policy();

        // The walk root itself is allowed through — the deny list must not
        // reject the whole call.
        assert!(!policy.evaluate_deny_only(&allowed, PathOp::List));
        // ...but a denied child inside that same root is.
        assert!(policy.evaluate_deny_only(&secret, PathOp::List));
        assert!(policy.evaluate_deny_only(&secret.join("id_rsa"), PathOp::List));
        // An allowed sibling under the walk root is not.
        assert!(!policy.evaluate_deny_only(&allowed.join("main.rs"), PathOp::List));

        // Per-entry filtering still applies the full policy: the deny rule wins
        // inside the denied subtree, the allow rule wins elsewhere.
        assert_eq!(
            policy.evaluate(&secret.join("id_rsa"), PathOp::List),
            PolicyAction::Deny
        );
        assert_eq!(
            policy.evaluate(&allowed.join("main.rs"), PathOp::List),
            PolicyAction::Allow
        );
    }

    #[test]
    fn test_evaluate_deny_only_false_without_deny_rules() {
        let root = unique_temp_dir("deny_only_none");
        std::fs::create_dir_all(&root).unwrap();
        let canon = root.canonicalize().unwrap();

        // Allow-only rules: nothing is ever denied, so every walk root passes.
        let allow_only = make_policy(
            vec![PathRule {
                name: "allow-all".to_string(),
                action: PolicyAction::Allow,
                ops: vec![PathOp::Read, PathOp::List, PathOp::Write],
                paths: vec![canon.to_string_lossy().to_string()],
            }],
            PolicyAction::Deny,
        );
        assert!(!allow_only.evaluate_deny_only(&canon, PathOp::List));
        assert!(!allow_only.evaluate_deny_only(&canon.join("a.rs"), PathOp::List));

        // No rules at all: same answer, even though the default policy denies.
        let empty = make_policy(vec![], PolicyAction::Deny);
        assert!(!empty.evaluate_deny_only(&canon, PathOp::List));
        assert!(!empty.evaluate_deny_only(&canon.join("a.rs"), PathOp::List));
        assert!(!empty.evaluate_deny_only(&canon.join("a.rs"), PathOp::Read));
        assert!(!empty.evaluate_deny_only(&canon.join("a.rs"), PathOp::Write));

        // A confirm rule is not a deny rule.
        let confirm = make_policy(
            vec![PathRule {
                name: "confirm-all".to_string(),
                action: PolicyAction::Confirm,
                ops: vec![PathOp::List],
                paths: vec![canon.to_string_lossy().to_string()],
            }],
            PolicyAction::Deny,
        );
        assert!(!confirm.evaluate_deny_only(&canon, PathOp::List));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_evaluate_deny_only_respects_ops() {
        // A deny rule scoped to `write` only must not gate a read/list walk.
        let root = unique_temp_dir("deny_only_ops");
        std::fs::create_dir_all(&root).unwrap();
        let canon = root.canonicalize().unwrap();
        let policy = make_policy(
            vec![PathRule {
                name: "deny-write".to_string(),
                action: PolicyAction::Deny,
                ops: vec![PathOp::Write],
                paths: vec![canon.to_string_lossy().to_string()],
            }],
            PolicyAction::Allow,
        );
        assert!(!policy.evaluate_deny_only(&canon, PathOp::List));
        assert!(!policy.evaluate_deny_only(&canon, PathOp::Read));
        // exec 兼具读写：deny-write 仍然拦截 exec 提取出的路径（与 evaluate 一致）。
        assert!(policy.evaluate_deny_only(&canon, PathOp::Exec));

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── reject_symlinks (upstream shared.rs:669) ─────────────────────────────

    #[cfg(unix)]
    #[test]
    fn test_reject_symlinks_flag_gates_symlinked_path() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("reject_symlinks");
        let allowed = root.join("allowed");
        std::fs::create_dir_all(&allowed).unwrap();
        let link = root.join("link");
        symlink(&allowed, &link).unwrap();
        let canon_allowed = std::fs::canonicalize(&allowed).unwrap();

        let rules = |name: &str| {
            vec![PathRule {
                name: name.to_string(),
                action: PolicyAction::Allow,
                ops: vec![PathOp::Read, PathOp::Write, PathOp::List],
                paths: vec![canon_allowed.to_string_lossy().to_string()],
            }]
        };

        // reject_symlinks = true → the symlinked path is denied.
        let strict = make_policy_reject_symlinks(rules("allow-all"), PolicyAction::Confirm, true);
        assert_eq!(strict.evaluate(&link, PathOp::Read), PolicyAction::Deny);
        assert_eq!(strict.evaluate(&link, PathOp::Write), PolicyAction::Deny);
        // The real directory behind the link is still allowed.
        assert_eq!(strict.evaluate(&allowed, PathOp::Read), PolicyAction::Allow);
        assert_eq!(
            strict.evaluate(&allowed.join("f.txt"), PathOp::Write),
            PolicyAction::Allow
        );

        // reject_symlinks = false (the default) → today's canonicalizing
        // behaviour: the link is followed to the allowed target.
        let lenient = make_policy_reject_symlinks(rules("allow-all"), PolicyAction::Confirm, false);
        assert_eq!(lenient.evaluate(&link, PathOp::Read), PolicyAction::Allow);
        assert_eq!(lenient.evaluate(&link, PathOp::Write), PolicyAction::Allow);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn test_reject_symlink_checks_the_original_not_the_canonical_path() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("reject_symlink_original");
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let link = root.join("link");
        symlink(&target, &link).unwrap();

        // The original path IS a symlink...
        assert!(PathPolicy::reject_symlink(&link));
        // ...and its canonical form never is, which is exactly why the check
        // has to run against the original (upstream shared.rs:664-668).
        let canonical = std::fs::canonicalize(&link).unwrap();
        assert!(!PathPolicy::reject_symlink(&canonical));
        assert_eq!(canonical, std::fs::canonicalize(&target).unwrap());
        // A plain directory and a non-existent path are not symlinks.
        assert!(!PathPolicy::reject_symlink(&target));
        assert!(!PathPolicy::reject_symlink(&root.join("does-not-exist")));

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── validate_path_rules (upstream config/src/validate/semantic.rs:285) ───

    fn rule_with_paths(name: &str, paths: &[&str]) -> PathRule {
        PathRule {
            name: name.to_string(),
            action: PolicyAction::Allow,
            ops: vec![PathOp::Read],
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_validate_path_rules_warns_on_relative_glob() {
        let config = PathPolicyFileConfig {
            rules: vec![rule_with_paths("dev", &["src/**"])],
            ..Default::default()
        };
        let warnings = validate_path_rules(&config);
        assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
        // The warning names the offending entry so it can be found.
        assert!(warnings[0].contains("src/**"), "got: {}", warnings[0]);
    }

    #[test]
    fn test_validate_path_rules_accepts_absolute_and_tilde_paths() {
        let config = PathPolicyFileConfig {
            rules: vec![rule_with_paths(
                "dev",
                &["/var/x", "~/y", "~", "**/secrets/**", "/home/a/**"],
            )],
            ..Default::default()
        };
        assert!(validate_path_rules(&config).is_empty());

        // Empty config and empty rule paths are also clean.
        assert!(validate_path_rules(&PathPolicyFileConfig::default()).is_empty());
        let no_paths = PathPolicyFileConfig {
            rules: vec![rule_with_paths("dev", &[])],
            ..Default::default()
        };
        assert!(validate_path_rules(&no_paths).is_empty());
    }

    #[test]
    fn test_validate_path_rules_is_warnings_only_never_errors() {
        // A config full of bad globs still produces only warnings, so
        // `PathPolicy::load` keeps loading it instead of falling back to safe
        // defaults (which would be a far bigger behaviour change).
        let config = PathPolicyFileConfig {
            rules: vec![rule_with_paths("bad", &["src/**", "relative/dir", ""])],
            ..Default::default()
        };
        let warnings = validate_path_rules(&config);
        assert_eq!(warnings.len(), 3, "warnings: {warnings:?}");
    }
}
