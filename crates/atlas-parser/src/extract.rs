//! Extracts imports from a syntax tree.

use atlas_model::{DependencyEdgeType, ImportRecord, Language, ParseStatus, ParsedFile};
use tree_sitter::{Node, Parser};

const LFS_POINTER_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/";

/// Parses one source file.
///
/// The tree is walked with a cursor instead of recursion, because a hostile file
/// can nest deeply enough to overflow the stack of a recursive walk.
pub fn parse_source(source: &[u8], language: Language) -> ParsedFile {
    let mut parsed = ParsedFile {
        status: ParseStatus::Parsed,
        loc: count_loc(source),
        imports: Vec::new(),
        nonliteral_dynamic_imports: 0,
        require_calls: 0,
    };
    if source.starts_with(LFS_POINTER_PREFIX) {
        parsed.status = ParseStatus::LfsPointer;
        return parsed;
    }

    let grammar = match language {
        Language::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
    };
    let mut parser = Parser::new();
    let tree = parser
        .set_language(&grammar.into())
        .ok()
        .and_then(|()| parser.parse(source, None));
    let Some(tree) = tree else {
        parsed.status = ParseStatus::Failed;
        return parsed;
    };
    if tree.root_node().has_error() {
        parsed.status = ParseStatus::Partial;
    }

    let mut cursor = tree.walk();
    'walk: loop {
        visit(cursor.node(), source, &mut parsed);
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
    parsed
}

fn count_loc(source: &[u8]) -> u32 {
    source
        .split(|&byte| byte == b'\n')
        .filter(|line| line.iter().any(|byte| !byte.is_ascii_whitespace()))
        .count() as u32
}

fn visit(node: Node<'_>, source: &[u8], parsed: &mut ParsedFile) {
    match node.kind() {
        "import_statement" => {
            let specifier = node.child_by_field_name("source").or_else(|| {
                let mut cursor = node.walk();
                let clause = node
                    .children(&mut cursor)
                    .find(|child| child.kind() == "import_require_clause");
                clause.and_then(|clause| clause.child_by_field_name("source"))
            });
            if let Some(specifier) = specifier.and_then(|s| string_value(s, source)) {
                let kind = if is_type_only_import(node) {
                    DependencyEdgeType::TypeOnlyImport
                } else {
                    DependencyEdgeType::StaticImport
                };
                push(parsed, specifier, kind, node);
            }
        }
        "export_statement" => {
            if let Some(specifier) = node
                .child_by_field_name("source")
                .and_then(|s| string_value(s, source))
            {
                let kind = if has_type_keyword(node) {
                    DependencyEdgeType::TypeOnlyImport
                } else {
                    DependencyEdgeType::ReExport
                };
                push(parsed, specifier, kind, node);
            }
        }
        "call_expression" => visit_call(node, source, parsed),
        _ => {}
    }
}

fn visit_call(node: Node<'_>, source: &[u8], parsed: &mut ParsedFile) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    if function.kind() == "import" {
        let argument = node
            .child_by_field_name("arguments")
            .and_then(|arguments| arguments.named_child(0));
        match argument.and_then(|argument| string_value(argument, source)) {
            Some(specifier) => push(parsed, specifier, DependencyEdgeType::DynamicImport, node),
            None => parsed.nonliteral_dynamic_imports += 1,
        }
    } else if function.kind() == "identifier"
        && function
            .utf8_text(source)
            .is_ok_and(|name| name == "require")
    {
        parsed.require_calls += 1;
    }
}

fn push(parsed: &mut ParsedFile, specifier: String, kind: DependencyEdgeType, node: Node<'_>) {
    parsed.imports.push(ImportRecord {
        specifier,
        kind,
        line: node.start_position().row as u32 + 1,
    });
}

/// The text of a plain string, or of a template literal with no substitutions.
fn string_value(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    match node.kind() {
        "string" => {}
        "template_string" => {
            let mut children = node.named_children(&mut cursor);
            if children.any(|child| child.kind() == "template_substitution") {
                return None;
            }
        }
        _ => return None,
    }
    let bytes = source.get(node.start_byte() + 1..node.end_byte().checked_sub(1)?)?;
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

fn has_type_keyword(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == "type")
}

/// `import type ...`, or `import { type A, type B }` with nothing else.
fn is_type_only_import(node: Node<'_>) -> bool {
    if has_type_keyword(node) {
        return true;
    }
    let mut cursor = node.walk();
    let Some(clause) = node
        .children(&mut cursor)
        .find(|child| child.kind() == "import_clause")
    else {
        return false;
    };
    let mut clause_cursor = clause.walk();
    let parts: Vec<_> = clause.named_children(&mut clause_cursor).collect();
    let [named] = parts.as_slice() else {
        return false;
    };
    if named.kind() != "named_imports" {
        return false;
    }
    let mut named_cursor = named.walk();
    let specifiers: Vec<_> = named
        .named_children(&mut named_cursor)
        .filter(|child| child.kind() == "import_specifier")
        .collect();
    !specifiers.is_empty() && specifiers.into_iter().all(has_type_keyword)
}

#[cfg(test)]
mod tests {
    use atlas_model::DependencyEdgeType::{DynamicImport, ReExport, StaticImport, TypeOnlyImport};
    use atlas_model::{DependencyEdgeType, Language, ParseStatus};
    use proptest::prelude::*;

    use super::parse_source;

    fn imports(source: &str) -> Vec<(String, DependencyEdgeType)> {
        parse_source(source.as_bytes(), Language::Typescript)
            .imports
            .into_iter()
            .map(|import| (import.specifier, import.kind))
            .collect()
    }

    fn one(source: &str) -> (String, DependencyEdgeType) {
        let found = imports(source);
        assert_eq!(found.len(), 1, "{source}: {found:?}");
        found.into_iter().next().unwrap()
    }

    #[test]
    fn static_import_forms() {
        for source in [
            "import a from './m';",
            "import { a, b as c } from './m';",
            "import * as ns from './m';",
            "import a, { b } from \"./m\";",
            "import './m';",
            "import a = require('./m');",
            "import { type A, b } from './m';",
        ] {
            assert_eq!(one(source), ("./m".to_owned(), StaticImport), "{source}");
        }
    }

    #[test]
    fn type_only_forms() {
        for source in [
            "import type { A } from './m';",
            "import type A from './m';",
            "import { type A, type B } from './m';",
            "export type { A } from './m';",
        ] {
            assert_eq!(one(source), ("./m".to_owned(), TypeOnlyImport), "{source}");
        }
    }

    #[test]
    fn re_export_forms() {
        for source in [
            "export * from './m';",
            "export * as ns from './m';",
            "export { a, b as c } from './m';",
        ] {
            assert_eq!(one(source), ("./m".to_owned(), ReExport), "{source}");
        }
    }

    #[test]
    fn local_exports_are_not_imports() {
        assert!(imports("export const a = 1; export { a as b }; export default 2;").is_empty());
    }

    #[test]
    fn literal_dynamic_imports_are_edges() {
        for source in [
            "const m = import('./m');",
            "async function f() { await import(`./m`); }",
        ] {
            assert_eq!(one(source), ("./m".to_owned(), DynamicImport), "{source}");
        }
    }

    #[test]
    fn nonliteral_dynamic_imports_are_counted_not_guessed() {
        let parsed = parse_source(b"import(name); import(`./${x}`);", Language::Typescript);
        assert!(parsed.imports.is_empty());
        assert_eq!(parsed.nonliteral_dynamic_imports, 2);
    }

    #[test]
    fn require_calls_are_counted_not_followed() {
        let parsed = parse_source(b"const a = require('./m');", Language::Typescript);
        assert!(parsed.imports.is_empty());
        assert_eq!(parsed.require_calls, 1);
    }

    #[test]
    fn text_that_only_looks_like_an_import_is_ignored() {
        let source =
            "// import a from './x';\nconst s = \"import b from './y'\";\n/* import './z' */";
        assert!(imports(source).is_empty());
    }

    #[test]
    fn imports_keep_their_source_lines() {
        let parsed = parse_source(
            b"\n\nimport './a';\n\nimport './b';\n",
            Language::Typescript,
        );
        let lines: Vec<_> = parsed.imports.iter().map(|i| i.line).collect();
        assert_eq!(lines, [3, 5]);
    }

    #[test]
    fn loc_counts_non_blank_lines() {
        assert_eq!(parse_source(b"a;\n\n  \nb;\n", Language::Typescript).loc, 2);
    }

    #[test]
    fn grammars_differ_for_angle_brackets() {
        let cast = b"import './a'; const x = <string>y;";
        assert_eq!(
            parse_source(cast, Language::Typescript).status,
            ParseStatus::Parsed
        );
        assert_eq!(
            parse_source(cast, Language::Tsx).status,
            ParseStatus::Partial
        );

        let jsx = b"import './a'; const x = <div>hi</div>;";
        assert_eq!(parse_source(jsx, Language::Tsx).status, ParseStatus::Parsed);
        assert_eq!(parse_source(jsx, Language::Tsx).imports.len(), 1);
    }

    #[test]
    fn syntax_errors_keep_the_imports_found() {
        let parsed = parse_source(b"import './a';\nfunction ( {{{\n", Language::Typescript);
        assert_eq!(parsed.status, ParseStatus::Partial);
        assert_eq!(parsed.imports[0].specifier, "./a");
    }

    #[test]
    fn lfs_pointers_are_not_parsed() {
        let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 9\n";
        let parsed = parse_source(pointer, Language::Typescript);
        assert_eq!(parsed.status, ParseStatus::LfsPointer);
        assert!(parsed.imports.is_empty());
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        let source = format!("const a = {}1{};", "[".repeat(50_000), "]".repeat(50_000));
        parse_source(source.as_bytes(), Language::Typescript);
    }

    proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..2000)) {
            parse_source(&bytes, Language::Typescript);
            parse_source(&bytes, Language::Tsx);
        }

        #[test]
        fn specifiers_round_trip(name in "[a-zA-Z0-9_./-]{1,40}") {
            let source = format!("import x from '{name}';");
            prop_assert_eq!(one(&source).0, name);
        }
    }
}
