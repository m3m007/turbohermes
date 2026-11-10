//! 工具与安全相关配置类型。
//!
//! 包含 WebSearchConfig、ExecConfig、WebToolsConfig、ToolsConfig、
//! PathAccessConfig、SecurityConfig。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchConfig {
    /// Brave Search API key (optional, for brave_search)
    #[serde(default)]
    pub api_key: String,
    /// Baidu AI Search API key (optional, for baidu_search)
    /// Get from https://qianfan.baidubce.com — set env BAIDU_API_KEY or this field
    #[serde(default)]
    pub baidu_api_key: String,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            baidu_api_key: String::new(),
            max_results: default_max_results(),
        }
    }
}

fn default_max_results() -> u32 {
    5
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ApprovalPolicy {
    #[default]
    Ask,
    WorkspaceAuto,
}

impl ApprovalPolicy {
    pub fn parse(value: Option<&str>) -> crate::Result<Self> {
        match value.unwrap_or("ask") {
            "ask" => Ok(Self::Ask),
            "workspace-auto" => Ok(Self::WorkspaceAuto),
            other => Err(crate::Error::Validation(format!(
                "Invalid approval_policy '{other}'; expected ask or workspace-auto"
            ))),
        }
    }
}

/// Default posture for tool execution, separate from *when* a human is asked.
///
/// Ported from Moltis `SecurityLevel` (`crates/tools/src/approval.rs:58-80` at
/// `8a12400dc10551dfb9a6b568414b91a343ef2d5d`, MIT). Moltis separates "when do I
/// ask" (`ApprovalMode`, which BlockCell models as [`ApprovalPolicy`]) from
/// "what is the default posture" (this). Stored as a string so the security
/// pattern definitions stay in `blockcell-tools`, which cannot depend on this
/// crate; `blockcell_tools::approval::SecurityLevel::parse` accepts the same
/// values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityLevel {
    Deny,
    #[default]
    Allowlist,
    Full,
}

impl SecurityLevel {
    pub fn parse(value: Option<&str>) -> crate::Result<Self> {
        match value.unwrap_or("allowlist") {
            "allowlist" => Ok(Self::Allowlist),
            "deny" | "strict" => Ok(Self::Deny),
            "full" | "permissive" => Ok(Self::Full),
            other => Err(crate::Error::Validation(format!(
                "Invalid security_level '{other}'; expected allowlist, deny or full"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecConfig {
    #[serde(default = "default_exec_timeout")]
    pub timeout: u32,
    #[serde(default = "crate::default_true")]
    pub restrict_to_workspace: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_policy: Option<String>,
    /// Explicit user opt-in; tool/path policy and dangerous-command checks still apply.
    #[serde(default)]
    pub allow_unsandboxed_shell: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>,
    /// Default posture, from Moltis `SecurityLevel`. One of `allowlist`
    /// (default), `deny` or `full`. `deny` blocks exec outright unless a
    /// `SecurityLevel`-agnostic allowlist entry matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_level: Option<String>,
    /// Commands that may skip the approval prompt.
    ///
    /// Ported from Moltis `matches_allowlist`
    /// (`crates/tools/src/approval.rs:453-488`): an entry of `*` matches
    /// everything, an exact entry matches that binary name, and an entry
    /// ending in `*` matches that prefix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowlist: Vec<String>,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            timeout: default_exec_timeout(),
            restrict_to_workspace: true,
            sandbox_policy: None,
            allow_unsandboxed_shell: false,
            approval_policy: None,
            security_level: None,
            allowlist: Vec::new(),
        }
    }
}

fn default_exec_timeout() -> u32 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebToolsConfig {
    #[serde(default)]
    pub search: WebSearchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsConfig {
    #[serde(default)]
    pub web: WebToolsConfig,
    #[serde(default)]
    pub exec: ExecConfig,
    /// Tick interval in seconds for the agent runtime loop (alert checks, cron, evolution).
    /// Lower values enable faster alert response. Default: 30. Min: 10. Max: 300.
    #[serde(default = "default_tick_interval")]
    pub tick_interval_secs: u32,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            web: WebToolsConfig::default(),
            exec: ExecConfig::default(),
            tick_interval_secs: default_tick_interval(),
        }
    }
}

fn default_tick_interval() -> u32 {
    30
}

#[cfg(test)]
mod tests {
    use super::{ApprovalPolicy, ExecConfig, SecurityLevel};

    #[test]
    fn approval_policy_parses_supported_values() {
        assert_eq!(ApprovalPolicy::parse(None).unwrap(), ApprovalPolicy::Ask);
        assert_eq!(
            ApprovalPolicy::parse(Some("ask")).unwrap(),
            ApprovalPolicy::Ask
        );
        assert_eq!(
            ApprovalPolicy::parse(Some("workspace-auto")).unwrap(),
            ApprovalPolicy::WorkspaceAuto
        );
        assert!(ApprovalPolicy::parse(Some("bogus")).is_err());
    }

    #[test]
    fn exec_config_deserializes_camel_case_approval_policy() {
        let config: ExecConfig =
            serde_json::from_str(r#"{"approvalPolicy":"workspace-auto"}"#).unwrap();
        assert_eq!(config.approval_policy.as_deref(), Some("workspace-auto"));
    }

    #[test]
    fn security_level_parses_moltis_aliases() {
        assert_eq!(SecurityLevel::parse(None).unwrap(), SecurityLevel::Allowlist);
        assert_eq!(
            SecurityLevel::parse(Some("deny")).unwrap(),
            SecurityLevel::Deny
        );
        assert_eq!(
            SecurityLevel::parse(Some("strict")).unwrap(),
            SecurityLevel::Deny
        );
        assert_eq!(
            SecurityLevel::parse(Some("full")).unwrap(),
            SecurityLevel::Full
        );
        assert_eq!(
            SecurityLevel::parse(Some("permissive")).unwrap(),
            SecurityLevel::Full
        );
        assert!(SecurityLevel::parse(Some("bogus")).is_err());
    }

    #[test]
    fn exec_config_defaults_to_permissive_security_level_and_empty_allowlist() {
        let config: ExecConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(config.security_level, None);
        assert!(config.allowlist.is_empty());
    }

    #[test]
    fn exec_config_deserializes_security_level_and_allowlist() {
        let config: ExecConfig = serde_json::from_str(
            r#"{"securityLevel":"deny","allowlist":["git status","cargo test*"]}"#,
        )
        .unwrap();
        assert_eq!(config.security_level.as_deref(), Some("deny"));
        assert_eq!(
            config.allowlist,
            vec!["git status".to_string(), "cargo test*".to_string()]
        );
    }
}

/// Configuration for the path-access policy system.
/// Points to the separate `path_access.json5` rules file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathAccessConfig {
    /// Whether the path-access policy system is active.
    #[serde(default = "super::default_true")]
    pub enabled: bool,

    /// Path to the rules file. Supports `~/` expansion.
    #[serde(default = "default_path_access_policy_file")]
    pub policy_file: String,

    /// Behavior when the policy file is missing or unparseable.
    /// One of: `"fallback_to_safe_default"` | `"fail_closed"` | `"disabled"`
    #[serde(default = "default_missing_file_policy")]
    pub missing_file_policy: String,

    /// Reserved for future hot-reload support.
    #[serde(default)]
    pub reload_on_change: bool,
}

fn default_path_access_policy_file() -> String {
    "~/.blockcell/path_access.json5".to_string()
}

fn default_missing_file_policy() -> String {
    "fallback_to_safe_default".to_string()
}

impl Default for PathAccessConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            policy_file: default_path_access_policy_file(),
            missing_file_policy: default_missing_file_policy(),
            reload_on_change: false,
        }
    }
}

/// Top-level security settings for the agent runtime.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SecurityConfig {
    /// Explicit destination for unscoped background notifications. Without
    /// this setting only a local CLI session can establish the destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_session_key: Option<String>,
    /// Path-access policy rules.
    #[serde(default)]
    pub path_access: PathAccessConfig,
}
