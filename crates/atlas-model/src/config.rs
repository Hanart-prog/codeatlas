//! Application configuration.
//!
//! Configuration comes from the user, never from the repository being analyzed:
//! a repository is untrusted, so it must not be able to change its own limits.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AtlasError;

/// All settings, loaded from an optional TOML file over these defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AtlasConfig {
    /// Rules that change analysis results.
    pub analysis: AnalysisConfig,
    /// Resource limits for untrusted input.
    pub limits: Limits,
    /// Local HTTP server settings.
    pub server: ServerConfig,
    /// Local data settings.
    pub storage: StorageConfig,
}

/// Rules that decide what is analyzed and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisConfig {
    /// Commits touching more files than this are excluded from co-change.
    pub bulk_commit_file_threshold: u32,
    /// Minimum shared commits before a co-change pair is reported.
    pub min_shared_commits: u32,
    /// Minimum similarity, as a percentage, for Git to report a rename.
    pub rename_similarity_percent: u16,
    /// Globs of vendored or generated paths to exclude; exclusions are counted.
    pub exclude_globs: Vec<String>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            bulk_commit_file_threshold: 50,
            min_shared_commits: 3,
            rename_similarity_percent: 50,
            exclude_globs: [
                "**/node_modules/**",
                "**/vendor/**",
                "**/dist/**",
                "**/build/**",
                "**/*.min.js",
                "**/*.d.ts",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }
}

/// Upper bounds on work done for untrusted repositories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Largest single blob to parse, in bytes.
    pub max_file_bytes: u64,
    /// Largest total of parsed blob bytes for one snapshot.
    pub max_total_indexed_bytes: u64,
    /// Largest number of graph nodes to build.
    pub max_graph_nodes: u32,
    /// Largest number of graph edges to build.
    pub max_graph_edges: u32,
    /// Longest a query may run, in milliseconds.
    pub query_timeout_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
            max_total_indexed_bytes: 2 * 1024 * 1024 * 1024,
            max_graph_nodes: 50_000,
            max_graph_edges: 500_000,
            query_timeout_ms: 30_000,
        }
    }
}

/// Local HTTP server settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to bind. Must be a loopback address.
    pub host: String,
    /// Port to bind; 0 picks a free port.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 4317,
        }
    }
}

/// Local data settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    /// Where to keep the database. Defaults to the OS cache directory.
    pub data_dir: Option<PathBuf>,
}

impl AtlasConfig {
    /// Parses and validates TOML. Unknown keys are errors, so a typo cannot
    /// silently leave a limit at its default.
    pub fn from_toml_str(source: &str) -> Result<Self, AtlasError> {
        let config: Self = toml::from_str(source).map_err(|e| AtlasError::Config(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Checks invariants that serde cannot express.
    pub fn validate(&self) -> Result<(), AtlasError> {
        let positive = [
            ("limits.max_file_bytes", self.limits.max_file_bytes),
            (
                "limits.max_total_indexed_bytes",
                self.limits.max_total_indexed_bytes,
            ),
            (
                "limits.max_graph_nodes",
                u64::from(self.limits.max_graph_nodes),
            ),
            (
                "limits.max_graph_edges",
                u64::from(self.limits.max_graph_edges),
            ),
            ("limits.query_timeout_ms", self.limits.query_timeout_ms),
            (
                "analysis.bulk_commit_file_threshold",
                u64::from(self.analysis.bulk_commit_file_threshold),
            ),
            (
                "analysis.min_shared_commits",
                u64::from(self.analysis.min_shared_commits),
            ),
        ];
        for (name, value) in positive {
            if value == 0 {
                return Err(AtlasError::Config(format!("{name} must be positive")));
            }
        }
        if !(1..=100).contains(&self.analysis.rename_similarity_percent) {
            return Err(AtlasError::Config(
                "analysis.rename_similarity_percent must be between 1 and 100".to_owned(),
            ));
        }
        Ok(())
    }

    /// Hash of the settings that can change analysis results, recorded with each
    /// analysis run so a changed setting invalidates cached results. Server and
    /// storage settings are excluded because they cannot affect results.
    pub fn analysis_hash(&self) -> String {
        let canonical = serde_json::to_vec(&(&self.analysis, &self.limits))
            .expect("analysis settings contain only JSON-representable values");
        Sha256::digest(canonical)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_specification() {
        let config = AtlasConfig::default();
        assert_eq!(config.analysis.bulk_commit_file_threshold, 50);
        assert_eq!(config.analysis.min_shared_commits, 3);
        assert_eq!(config.server.host, "127.0.0.1");
        config.validate().unwrap();
    }

    #[test]
    fn partial_files_keep_defaults_for_the_rest() {
        let config = AtlasConfig::from_toml_str("[server]\nport = 9000\n").unwrap();
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.limits, Limits::default());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = AtlasConfig::from_toml_str("[limits]\nmax_file_byte = 1\n").unwrap_err();
        assert!(err.to_string().contains("max_file_byte"), "{err}");
    }

    #[test]
    fn zero_limits_are_rejected() {
        let err = AtlasConfig::from_toml_str("[limits]\nmax_graph_nodes = 0\n").unwrap_err();
        assert!(err.to_string().contains("max_graph_nodes"), "{err}");
    }

    #[test]
    fn analysis_hash_ignores_server_and_storage_settings() {
        let base = AtlasConfig::default();
        let mut other = base.clone();
        other.server.port = 1;
        other.storage.data_dir = Some(PathBuf::from("/elsewhere"));
        assert_eq!(base.analysis_hash(), other.analysis_hash());
    }

    #[test]
    fn analysis_hash_changes_with_rules_and_limits() {
        let base = AtlasConfig::default();
        let mut rule = base.clone();
        rule.analysis.bulk_commit_file_threshold += 1;
        let mut limit = base.clone();
        limit.limits.max_file_bytes += 1;
        assert_ne!(base.analysis_hash(), rule.analysis_hash());
        assert_ne!(base.analysis_hash(), limit.analysis_hash());
        assert_eq!(base.analysis_hash().len(), 64);
    }
}
