use atlas_model::RepoPath;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::GitError;

/// Which file changes are left out of metrics, and related diff settings.
pub struct ExclusionRules {
    globs: GlobSet,
    max_file_bytes: u64,
    rename_similarity_percent: u16,
}

impl ExclusionRules {
    /// Compiles the rules.
    pub fn new(
        globs: &[String],
        max_file_bytes: u64,
        rename_similarity_percent: u16,
    ) -> Result<Self, GitError> {
        let mut builder = GlobSetBuilder::new();
        for glob in globs {
            let compiled = GlobBuilder::new(glob)
                .literal_separator(true)
                .build()
                .map_err(|error| GitError::InvalidGlob(error.to_string()))?;
            builder.add(compiled);
        }
        let globs = builder
            .build()
            .map_err(|error| GitError::InvalidGlob(error.to_string()))?;
        Ok(Self {
            globs,
            max_file_bytes,
            rename_similarity_percent,
        })
    }

    /// Whether `path` matches an exclusion glob.
    pub fn is_excluded(&self, path: &RepoPath) -> bool {
        self.globs.is_match(path.as_str())
    }

    pub(crate) fn max_file_bytes(&self) -> u64 {
        self.max_file_bytes
    }

    pub(crate) fn rename_similarity_percent(&self) -> u16 {
        self.rename_similarity_percent
    }

    pub(crate) fn diff_max_size(&self) -> i64 {
        i64::try_from(self.max_file_bytes).unwrap_or(i64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use atlas_model::AnalysisConfig;

    use super::ExclusionRules;

    fn matches(path: &str) -> bool {
        let rules = ExclusionRules::new(&AnalysisConfig::default().exclude_globs, 1, 50).unwrap();
        rules.is_excluded(&path.parse().unwrap())
    }

    #[test]
    fn default_rules_cover_top_level_and_nested_directories() {
        for path in [
            "node_modules/pkg/index.js",
            "packages/a/node_modules/b/c.ts",
            "dist/app.js",
            "vendor/x.ts",
            "src/types.d.ts",
            "public/lib.min.js",
        ] {
            assert!(matches(path), "{path}");
        }
    }

    #[test]
    fn default_rules_leave_lookalikes_alone() {
        for path in ["src/distance.ts", "src/vendors/x.ts", "src/app.ts"] {
            assert!(!matches(path), "{path}");
        }
    }

    #[test]
    fn invalid_globs_are_errors() {
        assert!(ExclusionRules::new(&["a[".to_owned()], 1, 50).is_err());
    }
}
