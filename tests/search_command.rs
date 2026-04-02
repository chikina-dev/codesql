mod support;

use support::{
    TestWorkspace, assert_success, init_codesql, save_codesql, search_codesql, stderr_string,
    stdout_string,
};

#[test]
fn search_filters_results_with_path_like() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "const NOTE: &str = \"TODO\";\n");
    workspace.write_text_file("docs/todo.txt", "TODO\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE path LIKE 'src/%' AND contains(content, 'TODO') LIMIT 10",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("src/lib.rs"), "stdout was:\n{stdout}");
    assert!(!stdout.contains("docs/todo.txt"), "stdout was:\n{stdout}");
}

#[test]
fn search_filters_results_with_rust_language_metadata() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub struct Example;\n");
    workspace.write_text_file("web/app.ts", "export const example = 1;\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE language = 'rust' LIMIT 20",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("src/lib.rs"), "stdout was:\n{stdout}");
    assert!(!stdout.contains("web/app.ts"), "stdout was:\n{stdout}");
}

#[test]
fn search_classifies_typescript_and_javascript_files() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("web/app.ts", "export const typed = 1 as const;\n");
    workspace.write_text_file("web/runtime.js", "module.exports = { ready: true };\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    let typescript_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE language = 'typescript' LIMIT 20",
    );
    let javascript_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE language = 'javascript' LIMIT 20",
    );

    // Then
    assert_success(&typescript_output);
    assert_success(&javascript_output);
    let typescript_stdout = stdout_string(&typescript_output);
    let javascript_stdout = stdout_string(&javascript_output);
    assert!(
        typescript_stdout.contains("web/app.ts"),
        "stdout was:\n{typescript_stdout}"
    );
    assert!(
        !typescript_stdout.contains("web/runtime.js"),
        "stdout was:\n{typescript_stdout}"
    );
    assert!(
        javascript_stdout.contains("web/runtime.js"),
        "stdout was:\n{javascript_stdout}"
    );
    assert!(
        !javascript_stdout.contains("web/app.ts"),
        "stdout was:\n{javascript_stdout}"
    );
}

#[test]
fn search_returns_line_number_and_line_for_content_matches() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file(
        "src/lib.rs",
        "pub fn example() {\n    let a = 1;\n    // TODO: tighten verifier\n}\n",
    );
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    let output = search_codesql(
        &workspace,
        "SELECT path, line_no, line FROM files WHERE path LIKE 'src/%' AND contains(content, 'TODO') LIMIT 10",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("src/lib.rs"), "stdout was:\n{stdout}");
    assert!(
        stdout.contains("TODO: tighten verifier"),
        "stdout was:\n{stdout}"
    );
    assert!(stdout.contains("3"), "stdout was:\n{stdout}");
}

#[test]
fn search_rejects_invalid_queries() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn example() {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    let output = search_codesql(&workspace, "SELECT FROM files");

    // Then
    assert!(
        !output.status.success(),
        "expected failure.\nstdout:\n{}\nstderr:\n{}",
        stdout_string(&output),
        stderr_string(&output)
    );
    let stderr = stderr_string(&output);
    assert!(
        stderr.contains("query") || stderr.contains("parse"),
        "stderr was:\n{stderr}"
    );
}

#[test]
fn search_supports_comparison_operators() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "one\n");
    workspace.write_text_file("b.txt", "two\n");
    workspace.write_text_file("c.txt", "three\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    // path > 'a.txt' and path <= 'b.txt'
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE path > 'a.txt' AND path <= 'b.txt'",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(!stdout.contains("a.txt"));
    assert!(stdout.contains("b.txt"));
    assert!(!stdout.contains("c.txt"));
}

#[test]
fn search_supports_in_clause() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/main.rs", "fn main() {}\n");
    workspace.write_text_file("src/lib.rs", "pub fn lib() {}\n");
    workspace.write_text_file("src/test.ts", "const x = 1;\n");
    workspace.write_text_file("docs/index.md", "# Docs\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE ext IN ('rs', 'ts')",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("src/main.rs"));
    assert!(stdout.contains("src/lib.rs"));
    assert!(stdout.contains("src/test.ts"));
    assert!(!stdout.contains("docs/index.md"));
}

#[test]
fn search_supports_regex_content_function() {
    // Given
    let workspace = TestWorkspace::new();
    // Intentionally mixed casing or special pattern
    workspace.write_text_file("src/lib.rs", "pub fn example() {\n    let a = 1;\n    // FIXME: optimize this part\n}\n");
    workspace.write_text_file("src/main.rs", "fn main() {\n    // TODO: implement\n}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    // Regex matches FIX.* or TODO.*
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE regex(content, '(FIXME|TODO):.*')",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("src/lib.rs"));
    assert!(stdout.contains("src/main.rs"));
}

#[test]
fn search_supports_glob_path_function() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/core/utils.rs", "pub fn utils() {}\n");
    workspace.write_text_file("src/commands/save.rs", "pub fn save() {}\n");
    workspace.write_text_file("tests/save_command.rs", "fn test_save() {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    // Glob for 'src/**/*.rs'
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE glob(path, 'src/**/*.rs')",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("src/core/utils.rs"));
    assert!(stdout.contains("src/commands/save.rs"));
    assert!(!stdout.contains("tests/save_command.rs"));
}

#[test]
fn search_supports_order_by() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "one\n");
    workspace.write_text_file("b.txt", "two\n");
    workspace.write_text_file("c.txt", "three\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // When
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files ORDER BY path DESC LIMIT 2",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output).trim().to_owned();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    // descending order -> "c.txt" then "b.txt"
    assert_eq!(lines[0], "c.txt");
    assert_eq!(lines[1], "b.txt");
}

#[test]
fn search_reads_across_multiple_segments() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "TODO 1\n");
    init_codesql(&workspace);
    save_codesql(&workspace); // Gen 1 has TODO 1

    workspace.write_text_file("b.txt", "TODO 2\n");
    save_codesql(&workspace); // Gen 2 has TODO 2

    let output = search_codesql(&workspace, "SELECT path FROM files WHERE contains(content, 'TODO')");
    
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("a.txt"));
    assert!(stdout.contains("b.txt"));
}

#[test]
fn search_respects_tombstones_for_updated_files() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "OLD_WORD\n");
    init_codesql(&workspace);
    save_codesql(&workspace); // Gen 1 has OLD_WORD

    workspace.write_text_file("a.txt", "NEW_WORD\n");
    save_codesql(&workspace); // Gen 2 has NEW_WORD and tombstone for Old A

    let output_old = search_codesql(&workspace, "SELECT path FROM files WHERE contains(content, 'OLD_WORD')");
    let stdout_old = stdout_string(&output_old);
    // OLD_WORD shouldn't appear
    assert!(!stdout_old.contains("a.txt"));

    let output_new = search_codesql(&workspace, "SELECT path FROM files WHERE contains(content, 'NEW_WORD')");
    let stdout_new = stdout_string(&output_new);
    // NEW_WORD should appear
    assert!(stdout_new.contains("a.txt"));
}

#[test]
fn search_supports_has_symbol_function() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/main.rs", "fn my_custom_function() {}\n");
    workspace.write_text_file("src/lib.rs", "pub struct CustomStruct;\n");
    workspace.write_text_file("web/app.ts", "export class TsClass {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    let output_fn = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE has_symbol('Function', 'my_custom_function')",
    );
    assert_success(&output_fn);
    assert!(stdout_string(&output_fn).contains("src/main.rs"));

    let output_struct = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE has_symbol('Struct', 'CustomStruct')",
    );
    assert_success(&output_struct);
    assert!(stdout_string(&output_struct).contains("src/lib.rs"));

    let output_class = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE has_symbol('Class', regex('^TsC.*'))",
    );
    assert_success(&output_class);
    assert!(stdout_string(&output_class).contains("web/app.ts"));
}

#[test]
fn search_supports_has_symbol_with_glob_and_regex_wrappers() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/main.rs", "fn my_custom_function() {}\n");
    workspace.write_text_file("src/lib.rs", "pub struct CustomStruct;\n");
    workspace.write_text_file("web/app.ts", "export class TsClass {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // regex(kind), glob(name)
    let output_1 = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE has_symbol(regex('^Struct|Class$'), glob('Custom*'))",
    );
    assert_success(&output_1);
    let stdout_1 = stdout_string(&output_1);
    assert!(stdout_1.contains("src/lib.rs"));
    assert!(!stdout_1.contains("web/app.ts"));

    // exact(kind), regex(name)
    let output_2 = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE has_symbol('Class', regex('^Ts.*'))",
    );
    assert_success(&output_2);
    let stdout_2 = stdout_string(&output_2);
    assert!(stdout_2.contains("web/app.ts"));
}

#[test]
fn search_evaluates_has_symbol_correctly_inside_or_conditions_with_content() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/main.rs", "fn my_custom_function() {}\n");
    workspace.write_text_file("src/lib.rs", "pub struct CustomStruct;\n // contains keyword: DANGER\n");
    workspace.write_text_file("src/other.rs", "// contains keyword: DANGER\n");
    workspace.write_text_file("src/safe.rs", "fn safe_function() {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    // Mixed OR condition: triggers memory evaluation (evaluate()) for has_symbol
    // Should match `my_custom_function` (main.rs) OR DANGER (lib.rs, other.rs)
    // Should NOT match safe.rs (has no DANGER, and function name is not my_custom_function)
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE has_symbol('Function', 'my_custom_function') OR contains(content, 'DANGER')",
    );
    assert_success(&output);
    let stdout = stdout_string(&output);
    
    assert!(stdout.contains("src/main.rs")); // Matched by has_symbol
    assert!(stdout.contains("src/lib.rs"));   // Matched by contains
    assert!(stdout.contains("src/other.rs")); // Matched by contains
    assert!(!stdout.contains("src/safe.rs")); // Should NOT match!
}
