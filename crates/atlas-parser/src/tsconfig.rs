//! Reads aliases from a root `tsconfig.json`. The file is JSONC and untrusted.

use atlas_model::TsconfigStatus;
use serde_json::Value;

/// One `paths` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasPattern {
    pub(crate) prefix: String,
    /// Text after `*`, or `None` for an exact pattern.
    pub(crate) suffix: Option<String>,
    pub(crate) targets: Vec<String>,
}

/// Aliases from a `tsconfig.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedTsconfig {
    pub(crate) base_url: Option<String>,
    pub(crate) patterns: Vec<AliasPattern>,
    /// How the file was used.
    pub status: Option<TsconfigStatus>,
}

/// Parses a root `tsconfig.json`. Invalid files yield no aliases.
pub fn parse_tsconfig(bytes: &[u8]) -> ParsedTsconfig {
    let invalid = ParsedTsconfig {
        status: Some(TsconfigStatus::Invalid),
        ..ParsedTsconfig::default()
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return invalid;
    };
    let Ok(config) = serde_json::from_str::<Value>(&strip_jsonc(text)) else {
        return invalid;
    };

    let options = config.get("compilerOptions");
    let base_url = options
        .and_then(|options| options.get("baseUrl"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let mut patterns: Vec<AliasPattern> = options
        .and_then(|options| options.get("paths"))
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(pattern, targets)| {
            let targets = targets
                .as_array()?
                .iter()
                .filter_map(|target| target.as_str().map(str::to_owned))
                .collect();
            let (prefix, suffix) = match pattern.split_once('*') {
                Some((prefix, suffix)) => (prefix.to_owned(), Some(suffix.to_owned())),
                None => (pattern.clone(), None),
            };
            Some(AliasPattern {
                prefix,
                suffix,
                targets,
            })
        })
        .collect();
    // TypeScript picks the pattern with the longest matching prefix.
    patterns.sort_by(|a, b| {
        b.prefix
            .len()
            .cmp(&a.prefix.len())
            .then_with(|| a.prefix.cmp(&b.prefix))
    });

    let status = if config.get("extends").is_some() {
        TsconfigStatus::AppliedIgnoringExtends
    } else {
        TsconfigStatus::Applied
    };
    ParsedTsconfig {
        base_url,
        patterns,
        status: Some(status),
    }
}

/// Removes comments and trailing commas, leaving strings alone.
fn strip_jsonc(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            output.push(c);
            match c {
                '\\' => output.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                output.push(c);
            }
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut previous = ' ';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            ('}' | ']', _) => {
                let kept = output.trim_end().len();
                if output[..kept].ends_with(',') {
                    output.truncate(kept - 1);
                }
                output.push(c);
            }
            _ => output.push(c),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use atlas_model::TsconfigStatus;
    use proptest::prelude::*;

    use super::parse_tsconfig;

    #[test]
    fn reads_paths_and_base_url_from_jsonc() {
        let config = parse_tsconfig(
            br#"{
              // a comment with "quotes"
              "compilerOptions": {
                "baseUrl": ".", /* block */
                "paths": { "@/*": ["./src/*"], "exact": ["./lib/exact.ts",], },
              },
            }"#,
        );
        assert_eq!(config.status, Some(TsconfigStatus::Applied));
        assert_eq!(config.base_url.as_deref(), Some("."));
        assert_eq!(config.patterns.len(), 2);
    }

    #[test]
    fn comment_markers_inside_strings_survive() {
        let config = parse_tsconfig(br#"{"compilerOptions":{"baseUrl":"./a//b/*c"}}"#);
        assert_eq!(config.base_url.as_deref(), Some("./a//b/*c"));
    }

    #[test]
    fn extends_is_reported_not_followed() {
        let config = parse_tsconfig(br#"{"extends":"../secret.json"}"#);
        assert_eq!(config.status, Some(TsconfigStatus::AppliedIgnoringExtends));
    }

    #[test]
    fn invalid_files_yield_no_aliases() {
        for bytes in [&b"not json"[..], b"{", b"\xff\xfe"] {
            let config = parse_tsconfig(bytes);
            assert_eq!(config.status, Some(TsconfigStatus::Invalid));
            assert!(config.patterns.is_empty());
        }
    }

    proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..500)) {
            parse_tsconfig(&bytes);
        }
    }
}
