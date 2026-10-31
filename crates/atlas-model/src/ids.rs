//! Validated identifiers. Git-derived strings are untrusted, so these types can
//! only be constructed through parsing.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::AtlasError;

/// Primary key of a row in `repositories`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(type = "number")]
pub struct RepositoryId(pub i64);

/// Primary key of a row in `analysis_runs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(type = "number")]
pub struct AnalysisRunId(pub i64);

/// A full Git object id: lowercase hex, 40 (SHA-1) or 64 (SHA-256) characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(try_from = "String", into = "String")]
#[ts(type = "string")]
pub struct Oid(String);

impl Oid {
    /// The normalized hex representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Oid {
    type Err = AtlasError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid_length = matches!(value.len(), 40 | 64);
        if valid_length && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err(AtlasError::InvalidInput(
                "object id must be 40 or 64 hexadecimal characters".to_owned(),
            ))
        }
    }
}

impl TryFrom<String> for Oid {
    type Error = AtlasError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Oid> for String {
    fn from(oid: Oid) -> Self {
        oid.0
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A path relative to a repository root, as Git records it: `/`-separated and
/// confined to the repository. A path is not a file identity, since files get
/// renamed; it only names a location within one tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(try_from = "String", into = "String")]
#[ts(type = "string")]
pub struct RepoPath(String);

impl RepoPath {
    /// The path as Git records it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RepoPath {
    type Err = AtlasError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let reject =
            |reason: &str| Err(AtlasError::InvalidInput(format!("invalid path: {reason}")));
        if value.is_empty() {
            return reject("empty");
        }
        if value.contains('\0') {
            return reject("contains NUL");
        }
        if value.starts_with('/') {
            return reject("absolute");
        }
        if value
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
        {
            return reject("empty, `.` or `..` segment");
        }
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for RepoPath {
    type Error = AtlasError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<RepoPath> for String {
    fn from(path: RepoPath) -> Self {
        path.0
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn oid_is_normalized_to_lowercase() {
        let upper = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        let oid: Oid = upper.parse().unwrap();
        assert_eq!(oid.as_str(), upper.to_ascii_lowercase());
    }

    #[test]
    fn oid_rejects_abbreviations_and_non_hex() {
        assert!("abc123".parse::<Oid>().is_err());
        assert!("g".repeat(40).parse::<Oid>().is_err());
        assert!("a".repeat(41).parse::<Oid>().is_err());
    }

    #[test]
    fn oid_validation_also_applies_when_deserializing() {
        assert!(serde_json::from_str::<Oid>("\"nope\"").is_err());
    }

    #[test]
    fn repo_path_keeps_spaces_and_unicode() {
        let path: RepoPath = "src/módulo uno/índice.ts".parse().unwrap();
        assert_eq!(path.as_str(), "src/módulo uno/índice.ts");
    }

    #[test]
    fn repo_path_rejects_escapes() {
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a/./b",
            "a//b",
            "a/",
            "a\0b",
        ] {
            assert!(bad.parse::<RepoPath>().is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn repo_path_validation_also_applies_when_deserializing() {
        assert!(serde_json::from_str::<RepoPath>("\"../secret\"").is_err());
    }

    proptest! {
        #[test]
        fn oid_round_trips(hex in "[0-9a-fA-F]{40}|[0-9a-fA-F]{64}") {
            let oid: Oid = hex.parse().unwrap();
            prop_assert_eq!(oid.as_str(), hex.to_ascii_lowercase());
            prop_assert_eq!(oid.to_string().parse::<Oid>().unwrap(), oid);
        }

        #[test]
        fn accepted_paths_stay_inside_the_repository(raw in "[a-z./\\\\ ]{0,24}") {
            if let Ok(path) = raw.parse::<RepoPath>() {
                prop_assert!(!path.as_str().starts_with('/'));
                prop_assert!(path.as_str().split('/').all(|s| !matches!(s, "" | "." | "..")));
            }
        }
    }
}
