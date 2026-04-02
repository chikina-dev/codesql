use std::path::Path;

use crate::constants::{
    ANALYZER_PLAINTEXT, ANALYZER_RUST, ANALYZER_TYPESCRIPT_JAVASCRIPT, LANGUAGE_JAVASCRIPT,
    LANGUAGE_PLAINTEXT, LANGUAGE_RUST, LANGUAGE_TYPESCRIPT,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub kind: String,
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisResult {
    pub analyzer_name: &'static str,
    pub language: &'static str,
    pub symbols: Vec<Symbol>,
}

pub trait Analyzer {
    fn name(&self) -> &'static str;
    fn supports(&self, path: &Path) -> bool;
    fn analyze(&self, path: &Path, content: Option<&str>) -> AnalysisResult;
}

pub struct AnalyzerRegistry {
    analyzers: Vec<Box<dyn Analyzer + Send + Sync>>,
}

impl AnalyzerRegistry {
    pub fn new() -> Self {
        Self {
            analyzers: vec![
                Box::new(RustAnalyzer),
                Box::new(TypeScriptJavaScriptAnalyzer),
                Box::new(PlainTextAnalyzer),
            ],
        }
    }

    pub fn analyze(&self, path: &Path, content: Option<&str>) -> AnalysisResult {
        for analyzer in &self.analyzers {
            if analyzer.supports(path) {
                return analyzer.analyze(path, content);
            }
        }
        PlainTextAnalyzer.analyze(path, content)
    }

    pub fn manifest_entries(&self) -> Vec<ManifestEntry> {
        self.analyzers
            .iter()
            .map(|analyzer| ManifestEntry {
                name: analyzer.name(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    pub name: &'static str,
}

struct PlainTextAnalyzer;

impl Analyzer for PlainTextAnalyzer {
    fn name(&self) -> &'static str {
        ANALYZER_PLAINTEXT
    }

    fn supports(&self, _path: &Path) -> bool {
        true
    }

    fn analyze(&self, _path: &Path, _content: Option<&str>) -> AnalysisResult {
        AnalysisResult {
            analyzer_name: ANALYZER_PLAINTEXT,
            language: LANGUAGE_PLAINTEXT,
            symbols: Vec::new(),
        }
    }
}

struct RustAnalyzer;

impl Analyzer for RustAnalyzer {
    fn name(&self) -> &'static str {
        ANALYZER_RUST
    }

    fn supports(&self, path: &Path) -> bool {
        extension(path) == Some("rs")
    }

    fn analyze(&self, _path: &Path, content: Option<&str>) -> AnalysisResult {
        let mut symbols = Vec::new();
        if let Some(text) = content {
            symbols = extract_rust_symbols(text);
        }
        
        AnalysisResult {
            analyzer_name: ANALYZER_RUST,
            language: LANGUAGE_RUST,
            symbols,
        }
    }
}

fn extract_rust_symbols(text: &str) -> Vec<Symbol> {
    use std::sync::LazyLock;
    static RE_RUST: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?m)^[ \t]*(?:pub(?:\([^)]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+(?:\u{0022}[^\u{0022}]+\u{0022}\s+)?)?(fn|struct|enum|trait|type|mod)\s+([a-zA-Z0-9_]+)").unwrap()
    });
    static RE_MACRO: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?m)^[ \t]*macro_rules!\s+([a-zA-Z0-9_]+)").unwrap()
    });

    let mut symbols = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    
    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = RE_RUST.captures(line) {
            let kind = match &caps[1] {
                "fn" => "Function",
                "struct" => "Struct",
                "enum" => "Enum",
                "trait" => "Trait",
                "type" => "Type",
                "mod" => "Module",
                _ => continue,
            };
            symbols.push(Symbol {
                kind: kind.to_string(),
                name: caps[2].to_string(),
                line: i + 1,
            });
        } else if let Some(caps) = RE_MACRO.captures(line) {
            symbols.push(Symbol {
                kind: "Macro".to_string(),
                name: caps[1].to_string(),
                line: i + 1,
            });
        }
    }
    symbols
}

struct TypeScriptJavaScriptAnalyzer;

impl Analyzer for TypeScriptJavaScriptAnalyzer {
    fn name(&self) -> &'static str {
        ANALYZER_TYPESCRIPT_JAVASCRIPT
    }

    fn supports(&self, path: &Path) -> bool {
        matches!(
            extension(path),
            Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
        )
    }

    fn analyze(&self, path: &Path, content: Option<&str>) -> AnalysisResult {
        let language = match extension(path) {
            Some("ts" | "tsx") => LANGUAGE_TYPESCRIPT,
            Some("js" | "jsx" | "mjs" | "cjs") => LANGUAGE_JAVASCRIPT,
            _ => LANGUAGE_PLAINTEXT,
        };
        
        let mut symbols = Vec::new();
        if let Some(text) = content {
            symbols = extract_ts_js_symbols(text);
        }
        
        AnalysisResult {
            analyzer_name: ANALYZER_TYPESCRIPT_JAVASCRIPT,
            language,
            symbols,
        }
    }
}

fn extract_ts_js_symbols(text: &str) -> Vec<Symbol> {
    use std::sync::LazyLock;
    static RE_TS_DECL: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?m)^[ \t]*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?(?:async\s+)?(function|class|interface|type|enum)\s+([a-zA-Z0-9_$]+)").unwrap()
    });
    static RE_TS_VAR: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?m)^[ \t]*(?:export\s+)?(?:const|let|var)\s+([a-zA-Z0-9_$]+)\s*=\s*(?:await\s+)?(?:async\s+)?(?:function\b|class\b|\([^)]*\)\s*=>|[a-zA-Z0-9_$]+\s*=>)").unwrap()
    });
    static RE_TS_METHOD: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?m)^[ \t]*(?:(?:public|private|protected)\s+)?(?:static\s+)?(?:abstract\s+)?(?:override\s+)?(?:async\s+)?(?:get\s+|set\s+)?\*?([a-zA-Z0-9_$]+)\s*(?:<[^>]*>)?\s*\([^)]*\)").unwrap()
    });

    let mut symbols = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = RE_TS_DECL.captures(line) {
            let kind = match &caps[1] {
                "function" => "Function",
                "class" => "Class",
                "interface" => "Interface",
                "type" => "Type",
                "enum" => "Enum",
                _ => continue,
            };
            symbols.push(Symbol {
                kind: kind.to_string(),
                name: caps[2].to_string(),
                line: i + 1,
            });
        } else if let Some(caps) = RE_TS_VAR.captures(line) {
            symbols.push(Symbol {
                kind: "Function".to_string(),
                name: caps[1].to_string(),
                line: i + 1,
            });
        } else if let Some(caps) = RE_TS_METHOD.captures(line) {
            let name = &caps[1];
            // JS reserved control flow words that look like method calls: if ()
            let is_reserved = matches!(
                name,
                "if" | "for" | "while" | "switch" | "catch" | "return" | "throw" | "typeof" | "import" | "export" | "new"
            );
            if !is_reserved {
                symbols.push(Symbol {
                    kind: "Function".to_string(), // Class methods are treated broadly as Functions
                    name: name.to_string(),
                    line: i + 1,
                });
            }
        }
    }
    symbols
}

fn extension(path: &Path) -> Option<&str> {
    path.extension().and_then(|value| value.to_str())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::constants::{
        LANGUAGE_JAVASCRIPT, LANGUAGE_PLAINTEXT, LANGUAGE_RUST, LANGUAGE_TYPESCRIPT,
    };

    use super::AnalyzerRegistry;

    #[test]
    fn registry_classifies_supported_languages() {
        let registry = AnalyzerRegistry::new();

        assert_eq!(
            registry.analyze(Path::new("src/lib.rs"), None).language,
            LANGUAGE_RUST
        );
        assert_eq!(
            registry.analyze(Path::new("web/app.ts"), None).language,
            LANGUAGE_TYPESCRIPT
        );
        assert_eq!(
            registry.analyze(Path::new("web/runtime.js"), None).language,
            LANGUAGE_JAVASCRIPT
        );
        assert_eq!(
            registry.analyze(Path::new("docs/readme.md"), None).language,
            LANGUAGE_PLAINTEXT
        );
    }

    #[test]
    fn manifest_matches_enabled_analyzers() {
        let registry = AnalyzerRegistry::new();

        let names = registry
            .manifest_entries()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["Rust", "TypeScript/JavaScript", "PlainText"]);
    }

    #[test]
    fn extract_rust_symbols_handles_complex_cases() {
        let text = "
            pub(crate) unsafe fn my_unsafe_fn() {}
            impl MyTrait for MyStruct {
                async fn my_method() {}
                fn another_method(&self) {}
            }
            pub type HttpResult<T> = Result<T, HttpError>;
            macro_rules! my_macro { () => {} }
            pub mod inner_module {
                extern \"C\" fn foreign() {}
            }
        ";
        let symbols = super::extract_rust_symbols(text);
        assert!(symbols.iter().any(|s| s.name == "my_unsafe_fn" && s.kind == "Function"));
        assert!(symbols.iter().any(|s| s.name == "my_method" && s.kind == "Function"));
        assert!(symbols.iter().any(|s| s.name == "another_method" && s.kind == "Function"));
        assert!(symbols.iter().any(|s| s.name == "HttpResult" && s.kind == "Type"));
        assert!(symbols.iter().any(|s| s.name == "my_macro" && s.kind == "Macro"));
        assert!(symbols.iter().any(|s| s.name == "inner_module" && s.kind == "Module"));
        assert!(symbols.iter().any(|s| s.name == "foreign" && s.kind == "Function"));
    }

    #[test]
    fn extract_ts_js_symbols_handles_complex_cases() {
        let text = "
            export default class MyClass {
                public async myMethod() {}
                private get myProp() {}
            }
            let myLetFn = async () => {};
            var myVarFn = function() {};
            
            // Should not match control structures
            if (condition) {}
            for (let i = 0; i < 10; i++) {}
            catch (e) {}
        ";
        let symbols = super::extract_ts_js_symbols(text);
        assert!(symbols.iter().any(|s| s.name == "MyClass" && s.kind == "Class"));
        assert!(symbols.iter().any(|s| s.name == "myMethod" && s.kind == "Function"));
        assert!(symbols.iter().any(|s| s.name == "myProp" && s.kind == "Function"));
        assert!(symbols.iter().any(|s| s.name == "myLetFn" && s.kind == "Function"));
        assert!(symbols.iter().any(|s| s.name == "myVarFn" && s.kind == "Function"));

        // Make sure keywords aren't extracted as methods:
        assert!(!symbols.iter().any(|s| s.name == "if" || s.name == "for" || s.name == "catch"));
    }
}
