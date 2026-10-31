//! Resolves import specifiers to files in one tree.

use std::collections::HashSet;

use atlas_model::{ResolutionKind, UnresolvedReason};

use crate::tsconfig::ParsedTsconfig;

/// Extensions tried for a specifier that has none, in TypeScript's order.
const IMPLICIT_EXTENSIONS: [&str; 2] = [".ts", ".tsx"];

/// ESM TypeScript imports `./a.js` for a file named `a.ts`.
const EXTENSION_MAP: [(&str, &[&str]); 4] = [
    (".js", &[".ts", ".tsx"]),
    (".jsx", &[".tsx"]),
    (".mjs", &[".mts"]),
    (".cjs", &[".cts"]),
];

/// What an import refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A source file in this tree.
    Local {
        /// The file.
        path: String,
        /// How it was found.
        kind: ResolutionKind,
    },
    /// A package outside this repository.
    External,
    /// Neither; no edge is created, so that no edge is misleading.
    Unresolved(UnresolvedReason),
}

/// Resolves imports against the files of one commit.
#[derive(Debug, Clone, Copy)]
pub struct Resolver<'a> {
    /// Source files that can be graph nodes.
    pub sources: &'a HashSet<String>,
    /// Every path in the tree, to explain targets that are not source files.
    pub all_files: &'a HashSet<String>,
    /// Aliases, if the commit has a usable `tsconfig.json`.
    pub tsconfig: Option<&'a ParsedTsconfig>,
}

impl Resolver<'_> {
    /// Resolves `specifier` as imported by the file at `importer`.
    pub fn resolve(&self, importer: &str, specifier: &str) -> Resolution {
        if specifier.is_empty() || specifier.starts_with('#') {
            return Resolution::Unresolved(UnresolvedReason::UnsupportedSpecifier);
        }
        if specifier.starts_with('/') {
            return Resolution::Unresolved(UnresolvedReason::UnsupportedSpecifier);
        }
        if is_relative(specifier) {
            let directory = importer
                .rsplit_once('/')
                .map_or("", |(directory, _)| directory);
            return match normalize(directory, specifier) {
                Some(base) => {
                    self.local(&base, ResolutionKind::Relative, UnresolvedReason::NotFound)
                }
                None => Resolution::Unresolved(UnresolvedReason::EscapesRepository),
            };
        }
        if let Some(tsconfig) = self.tsconfig {
            if let Some(resolution) = self.resolve_alias(tsconfig, specifier) {
                return resolution;
            }
            if let Some(base_url) = &tsconfig.base_url
                && let Some(base) = normalize("", &format!("{base_url}/{specifier}"))
                && let Some(path) = self.find_source(&base)
            {
                return Resolution::Local {
                    path,
                    kind: ResolutionKind::BaseUrl,
                };
            }
        }
        if specifier.starts_with("@/") || specifier.starts_with("~/") {
            return Resolution::Unresolved(UnresolvedReason::UnknownAlias);
        }
        Resolution::External
    }

    fn resolve_alias(&self, tsconfig: &ParsedTsconfig, specifier: &str) -> Option<Resolution> {
        let base_url = tsconfig.base_url.as_deref().unwrap_or(".");
        for pattern in &tsconfig.patterns {
            let wildcard = match &pattern.suffix {
                None if specifier == pattern.prefix => "",
                None => continue,
                Some(suffix) => {
                    let Some(rest) = specifier.strip_prefix(&pattern.prefix) else {
                        continue;
                    };
                    let Some(rest) = rest.strip_suffix(suffix.as_str()) else {
                        continue;
                    };
                    rest
                }
            };
            let candidates: Vec<String> = pattern
                .targets
                .iter()
                .map(|target| format!("{base_url}/{}", target.replacen('*', wildcard, 1)))
                .filter_map(|joined| normalize("", &joined))
                .collect();
            if let Some(path) = candidates.iter().find_map(|base| self.find_source(base)) {
                return Some(Resolution::Local {
                    path,
                    kind: ResolutionKind::Alias,
                });
            }
            let reason = if candidates.iter().any(|base| self.all_files.contains(base)) {
                UnresolvedReason::NonSourceTarget
            } else {
                UnresolvedReason::AliasTargetMissing
            };
            return Some(Resolution::Unresolved(reason));
        }
        None
    }

    fn local(&self, base: &str, kind: ResolutionKind, missing: UnresolvedReason) -> Resolution {
        match self.find_source(base) {
            Some(path) => Resolution::Local { path, kind },
            None if self.all_files.contains(base) => {
                Resolution::Unresolved(UnresolvedReason::NonSourceTarget)
            }
            None => Resolution::Unresolved(missing),
        }
    }

    fn find_source(&self, base: &str) -> Option<String> {
        let exists = |candidate: &String| self.sources.contains(candidate);

        if self.sources.contains(base) {
            return Some(base.to_owned());
        }
        for (written, actual) in EXTENSION_MAP {
            if let Some(stem) = base.strip_suffix(written) {
                let mapped = actual.iter().map(|extension| format!("{stem}{extension}"));
                if let Some(found) = mapped.into_iter().find(exists) {
                    return Some(found);
                }
            }
        }
        let with_extension = IMPLICIT_EXTENSIONS.iter().map(|e| format!("{base}{e}"));
        let index = IMPLICIT_EXTENSIONS.iter().map(|e| {
            let separator = if base.is_empty() { "" } else { "/" };
            format!("{base}{separator}index{e}")
        });
        with_extension.chain(index).find(exists)
    }
}

fn is_relative(specifier: &str) -> bool {
    matches!(specifier, "." | "..") || specifier.starts_with("./") || specifier.starts_with("../")
}

/// Joins and normalizes a path. `None` means it leaves the repository root.
fn normalize(directory: &str, relative: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in directory.split('/').chain(relative.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use atlas_model::ResolutionKind::{self, Alias, BaseUrl, Relative};
    use atlas_model::UnresolvedReason::{self, *};
    use proptest::prelude::*;

    use super::{Resolution, Resolver, normalize};
    use crate::tsconfig::parse_tsconfig;

    const SOURCES: &[&str] = &[
        "index.ts",
        "src/a.ts",
        "src/b.tsx",
        "src/dir/index.ts",
        "src/ui/index.tsx",
        "src/both.ts",
        "src/both/index.ts",
        "src/esm.mts",
        "src/common.cts",
        "src/lib/util.ts",
        "packages/core/main.ts",
    ];

    fn set(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| (*p).to_owned()).collect()
    }

    fn resolve(tsconfig: Option<&str>, importer: &str, specifier: &str) -> Resolution {
        let sources = set(SOURCES);
        let mut all_files = sources.clone();
        all_files.extend(["src/style.css", "src/data.json", "src/real.js"].map(str::to_owned));
        let parsed = tsconfig.map(|text| parse_tsconfig(text.as_bytes()));
        Resolver {
            sources: &sources,
            all_files: &all_files,
            tsconfig: parsed.as_ref(),
        }
        .resolve(importer, specifier)
    }

    fn local(path: &str, kind: ResolutionKind) -> Resolution {
        Resolution::Local {
            path: path.to_owned(),
            kind,
        }
    }

    fn unresolved(reason: UnresolvedReason) -> Resolution {
        Resolution::Unresolved(reason)
    }

    #[test]
    fn relative_candidates() {
        for (specifier, expected) in [
            ("./b", "src/b.tsx"),
            ("./a.ts", "src/a.ts"),
            ("./dir", "src/dir/index.ts"),
            ("./dir/", "src/dir/index.ts"),
            ("./ui", "src/ui/index.tsx"),
            ("../index", "index.ts"),
            ("..", "index.ts"),
            ("./lib/../a", "src/a.ts"),
        ] {
            assert_eq!(
                resolve(None, "src/a.ts", specifier),
                local(expected, Relative),
                "{specifier}"
            );
        }
    }

    #[test]
    fn a_file_wins_over_a_directory_index() {
        assert_eq!(
            resolve(None, "src/a.ts", "./both"),
            local("src/both.ts", Relative)
        );
    }

    #[test]
    fn esm_extensions_map_to_source_files() {
        for (specifier, expected) in [
            ("./a.js", "src/a.ts"),
            ("./b.js", "src/b.tsx"),
            ("./b.jsx", "src/b.tsx"),
            ("./esm.mjs", "src/esm.mts"),
            ("./common.cjs", "src/common.cts"),
        ] {
            assert_eq!(
                resolve(None, "src/a.ts", specifier),
                local(expected, Relative),
                "{specifier}"
            );
        }
    }

    #[test]
    fn missing_and_non_source_targets_are_explained() {
        assert_eq!(resolve(None, "src/a.ts", "./nope"), unresolved(NotFound));
        for specifier in ["./style.css", "./data.json", "./real.js"] {
            assert_eq!(
                resolve(None, "src/a.ts", specifier),
                unresolved(NonSourceTarget),
                "{specifier}"
            );
        }
    }

    #[test]
    fn paths_leaving_the_repository_are_rejected() {
        assert_eq!(
            resolve(None, "src/a.ts", "../../secret"),
            unresolved(EscapesRepository)
        );
        assert_eq!(
            resolve(None, "index.ts", "../x"),
            unresolved(EscapesRepository)
        );
    }

    #[test]
    fn packages_are_external() {
        for specifier in [
            "react",
            "@scope/pkg",
            "@scope/pkg/sub",
            "node:fs",
            "lodash/get",
        ] {
            assert_eq!(
                resolve(None, "src/a.ts", specifier),
                Resolution::External,
                "{specifier}"
            );
        }
    }

    #[test]
    fn unsupported_and_alias_like_forms_are_labelled() {
        assert_eq!(
            resolve(None, "src/a.ts", "#internal"),
            unresolved(UnsupportedSpecifier)
        );
        assert_eq!(
            resolve(None, "src/a.ts", ""),
            unresolved(UnsupportedSpecifier)
        );
        assert_eq!(
            resolve(None, "src/a.ts", "/abs/path"),
            unresolved(UnsupportedSpecifier)
        );
        assert_eq!(resolve(None, "src/a.ts", "@/a"), unresolved(UnknownAlias));
        assert_eq!(resolve(None, "src/a.ts", "~/a"), unresolved(UnknownAlias));
    }

    const CONFIG: &str = r#"{"compilerOptions":{"paths":{
        "@/*":["./src/*"],
        "@/lib/*":["./src/lib/*"],
        "@core":["./packages/core/main.ts"],
        "fallback/*":["./missing/*","./src/*"]
    }}}"#;

    #[test]
    fn tsconfig_paths_resolve() {
        for (specifier, expected) in [
            ("@/a", "src/a.ts"),
            ("@/dir", "src/dir/index.ts"),
            ("@/lib/util", "src/lib/util.ts"),
            ("@core", "packages/core/main.ts"),
            ("fallback/a", "src/a.ts"),
        ] {
            assert_eq!(
                resolve(Some(CONFIG), "src/x.ts", specifier),
                local(expected, Alias),
                "{specifier}"
            );
        }
    }

    #[test]
    fn a_matching_alias_with_no_file_is_not_a_package() {
        assert_eq!(
            resolve(Some(CONFIG), "src/x.ts", "@/nope"),
            unresolved(AliasTargetMissing)
        );
    }

    #[test]
    fn an_alias_to_a_non_source_file_says_so() {
        assert_eq!(
            resolve(Some(CONFIG), "src/x.ts", "@/style.css"),
            unresolved(NonSourceTarget)
        );
    }

    #[test]
    fn aliases_cannot_leave_the_repository() {
        let config = r#"{"compilerOptions":{"paths":{"evil/*":["../../*"]}}}"#;
        assert_eq!(
            resolve(Some(config), "src/x.ts", "evil/a"),
            unresolved(AliasTargetMissing)
        );
    }

    #[test]
    fn base_url_resolves_bare_paths_and_leaves_packages_external() {
        let config = r#"{"compilerOptions":{"baseUrl":"./src"}}"#;
        assert_eq!(
            resolve(Some(config), "src/x.ts", "lib/util"),
            local("src/lib/util.ts", BaseUrl)
        );
        assert_eq!(
            resolve(Some(config), "src/x.ts", "react"),
            Resolution::External
        );
    }

    proptest! {
        #[test]
        fn normalized_paths_stay_inside_the_repository(
            directory in "[a-z]{1,3}(/[a-z]{1,3}){0,3}",
            relative in "(\\.\\.?/|[a-z]{1,3}/){0,6}[a-z]{0,3}",
        ) {
            if let Some(path) = normalize(&directory, &relative) {
                prop_assert!(!path.split('/').any(|s| s == ".." || s == "."));
                prop_assert!(!path.starts_with('/'));
            }
        }

        #[test]
        fn local_results_always_exist(specifier in "(\\.\\.?/){0,3}[a-z/.]{0,12}") {
            if let Resolution::Local { path, .. } = resolve(Some(CONFIG), "src/lib/x.ts", &specifier) {
                prop_assert!(SOURCES.contains(&path.as_str()));
            }
        }
    }
}
