use crate::config::{detect_language, Language};
use crate::types::{SymbolEntry, SymbolKind};
use regex::Regex;
use std::path::Path;

type Rule = (Regex, SymbolKind);

fn rules_for(lang: &Language) -> Vec<Rule> {
    match lang {
        Language::Rust => rust_rules(),
        Language::Go => go_rules(),
        Language::Python => python_rules(),
        Language::TypeScript => typescript_rules(),
        Language::CSharp => csharp_rules(),
    }
}

fn compile(patterns: &[(&str, SymbolKind)]) -> Vec<Rule> {
    patterns
        .iter()
        .map(|(pat, kind)| (Regex::new(pat).expect("invalid regex"), kind.clone()))
        .collect()
}

fn rust_rules() -> Vec<Rule> {
    compile(&[
        (r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?fn\s+(\w+)"#, SymbolKind::Function),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+(\w+)", SymbolKind::Trait),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)", SymbolKind::Enum),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)", SymbolKind::Module),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+(\w+)", SymbolKind::Other),
        (r"^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+(\w+)", SymbolKind::Variable),
        (r"^\s*macro_rules!\s*(\w+)", SymbolKind::Other),
    ])
}

fn go_rules() -> Vec<Rule> {
    compile(&[
        (r"^\s*type\s+(\w+)\s+struct", SymbolKind::Struct),
        (r"^\s*type\s+(\w+)\s+interface", SymbolKind::Trait),
        (r"^\s*func\s+\([^)]*\)\s+(\w+)", SymbolKind::Function),
        (r"^\s*func\s+(\w+)", SymbolKind::Function),
        (r"^\s*type\s+(\w+)", SymbolKind::Other),
    ])
}

fn python_rules() -> Vec<Rule> {
    compile(&[
        (r"^\s*(?:async\s+)?def\s+(\w+)", SymbolKind::Function),
        (r"^\s*class\s+(\w+)", SymbolKind::Struct),
    ])
}

fn typescript_rules() -> Vec<Rule> {
    compile(&[
        (r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+(?:\*\s+)?(\w+)", SymbolKind::Function),
        (r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:export\s+)?(?:default\s+)?interface\s+(\w+)", SymbolKind::Trait),
        (r"^\s*(?:export\s+)?type\s+(\w+)", SymbolKind::Other),
        (r"^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)", SymbolKind::Variable),
    ])
}

fn csharp_rules() -> Vec<Rule> {
    compile(&[
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*class\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*interface\s+(\w+)", SymbolKind::Trait),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*struct\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*enum\s+(\w+)", SymbolKind::Enum),
        (r"^\s*(?:\[.*\]\s*)*(?:(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+)*record\s+(\w+)", SymbolKind::Struct),
        (r"^\s*(?:\[.*\]\s*)*(?:public|private|protected|internal|static|virtual|override|abstract|sealed|readonly|async|unsafe|partial|new|extern)\s+(?:\w+(?:<[^>]*>)?\s+)*(\w+)\s*\(", SymbolKind::Function),
    ])
}

pub fn extract_symbols(path: &Path) -> Vec<SymbolEntry> {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => return vec![],
    };

    let code = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let rules = rules_for(&lang);
    let mut symbols = Vec::new();

    for (line_num, line) in code.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        for (re, kind) in &rules {
            if let Some(caps) = re.captures(trimmed) {
                if let Some(name) = caps.get(1) {
                    symbols.push(SymbolEntry {
                        name: name.as_str().to_string(),
                        signature: trimmed.to_string(),
                        kind: kind.clone(),
                        line: (line_num + 1) as u32,
                        character: 0,
                    });
                }
                break;
            }
        }
    }

    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_and_extract(code: &str, filename: &str) -> Vec<SymbolEntry> {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(filename);
        fs::write(&path, code).unwrap();
        extract_symbols(&path)
    }

    #[test]
    fn rust_fn() {
        let syms = write_and_extract("fn hello() {}", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "hello");
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert_eq!(syms[0].line, 1);
    }

    #[test]
    fn rust_pub_fn() {
        let syms = write_and_extract("pub fn do_thing() {}", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "do_thing");
    }

    #[test]
    fn rust_pub_crate_fn() {
        let syms = write_and_extract("pub(crate) fn internal() {}", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "internal");
    }

    #[test]
    fn rust_async_fn() {
        let syms = write_and_extract("pub async fn fetch() {}", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "fetch");
    }

    #[test]
    fn rust_struct() {
        let syms = write_and_extract("struct Foo {}", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Foo");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn rust_enum() {
        let syms = write_and_extract("enum Color { Red, Green }", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Color");
        assert_eq!(syms[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn rust_trait() {
        let syms = write_and_extract("pub trait Display {}", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Display");
        assert_eq!(syms[0].kind, SymbolKind::Trait);
    }

    #[test]
    fn rust_mod() {
        let syms = write_and_extract("mod utils;", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "utils");
        assert_eq!(syms[0].kind, SymbolKind::Module);
    }

    #[test]
    fn rust_type_alias() {
        let syms = write_and_extract("pub type Result<T> = std::result::Result<T, Error>;", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Result");
        assert_eq!(syms[0].kind, SymbolKind::Other);
    }

    #[test]
    fn rust_const() {
        let syms = write_and_extract("const MAX_SIZE: usize = 1024;", "test.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "MAX_SIZE");
        assert_eq!(syms[0].kind, SymbolKind::Variable);
    }

    #[test]
    fn rust_multiple_symbols() {
        let code = r#"
fn foo() {}
struct Bar {}
enum Baz {}
pub trait Qux {}
mod utils;
"#;
        let syms = write_and_extract(code, "test.rs");
        assert_eq!(syms.len(), 5);
        assert_eq!(syms[0].name, "foo");
        assert_eq!(syms[1].name, "Bar");
        assert_eq!(syms[2].name, "Baz");
        assert_eq!(syms[3].name, "Qux");
        assert_eq!(syms[4].name, "utils");
    }

    #[test]
    fn go_func() {
        let syms = write_and_extract("func main() {}", "main.go");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "main");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn go_method() {
        let syms = write_and_extract("func (s *Service) Serve() {}", "main.go");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Serve");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn go_struct() {
        let syms = write_and_extract("type User struct { Name string }", "main.go");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "User");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn go_interface() {
        let syms = write_and_extract("type Reader interface { Read() }", "main.go");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Reader");
        assert_eq!(syms[0].kind, SymbolKind::Trait);
    }

    #[test]
    fn go_type_alias() {
        let syms = write_and_extract("type HandlerFunc func(ResponseWriter, *Request)", "main.go");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "HandlerFunc");
        assert_eq!(syms[0].kind, SymbolKind::Other);
    }

    #[test]
    fn python_def() {
        let syms = write_and_extract("def process():\n    pass", "test.py");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "process");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn python_async_def() {
        let syms = write_and_extract("async def fetch():\n    pass", "test.py");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "fetch");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn python_class() {
        let syms = write_and_extract("class MyClass:\n    pass", "test.py");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "MyClass");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn typescript_function() {
        let syms = write_and_extract("function greet(name: string): void {}", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "greet");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn typescript_export_function() {
        let syms = write_and_extract("export function format() {}", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "format");
    }

    #[test]
    fn typescript_async_function() {
        let syms = write_and_extract("export async function fetchData() {}", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "fetchData");
    }

    #[test]
    fn typescript_class() {
        let syms = write_and_extract("class Animal {}", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Animal");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn typescript_abstract_class() {
        let syms = write_and_extract("export abstract class Base {}", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Base");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn typescript_interface() {
        let syms = write_and_extract("interface User { name: string }", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "User");
        assert_eq!(syms[0].kind, SymbolKind::Trait);
    }

    #[test]
    fn typescript_type() {
        let syms = write_and_extract("type Callback = () => void", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Callback");
        assert_eq!(syms[0].kind, SymbolKind::Other);
    }

    #[test]
    fn typescript_export_const() {
        let syms = write_and_extract("export const PI = 3.14", "test.ts");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "PI");
        assert_eq!(syms[0].kind, SymbolKind::Variable);
    }

    #[test]
    fn csharp_class() {
        let syms = write_and_extract("public class MyClass { }", "test.cs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "MyClass");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn csharp_interface() {
        let syms = write_and_extract("public interface ILogger { }", "test.cs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "ILogger");
        assert_eq!(syms[0].kind, SymbolKind::Trait);
    }

    #[test]
    fn csharp_struct_type() {
        let syms = write_and_extract("public struct Point { }", "test.cs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Point");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn csharp_enum() {
        let syms = write_and_extract("enum Color { Red, Green }", "test.cs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Color");
        assert_eq!(syms[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn csharp_method() {
        let syms = write_and_extract("public void DoSomething() { }", "test.cs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "DoSomething");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn csharp_static_method() {
        let syms = write_and_extract("public static string Format() => \"\";", "test.cs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Format");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn csharp_async_method() {
        let syms = write_and_extract("public async Task<Result> ProcessAsync() { }", "test.cs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "ProcessAsync");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn unsupported_extension() {
        let syms = write_and_extract("fn main() {}", "test.txt");
        assert!(syms.is_empty());
    }

    #[test]
    fn non_existent_file() {
        let syms = extract_symbols(Path::new("/tmp/nonexistent_file_lcp_test.rs"));
        assert!(syms.is_empty());
    }
}
