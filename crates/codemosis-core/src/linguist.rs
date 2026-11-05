//! Every language GitHub knows.
//!
//! The catalogue is GitHub Linguist's `languages.yml` (vendored in
//! `data/linguist/`, MIT licensed), compiled into a static table. Detection
//! follows Linguist's order: exact filename, then the interpreter named by a
//! shebang, then the extension, then heuristics for extensions several
//! languages share.
//!
//! Detection reports how sure it is. A language settled by a filename, a
//! shebang, an extension only one language uses, or a heuristic that found
//! positive evidence is *certain* or *heuristic*. An extension shared by
//! several languages with no evidence either way is a *guess*, and no
//! language-specific rule ever runs on a guess.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Programming,
    Markup,
    Data,
    Prose,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Programming => "programming",
            Kind::Markup => "markup",
            Kind::Data => "data",
            Kind::Prose => "prose",
        }
    }
}

/// One entry of Linguist's catalogue.
#[derive(Debug)]
pub struct LinguistLanguage {
    pub name: &'static str,
    pub kind: Kind,
    pub color: Option<&'static str>,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub interpreters: &'static [&'static str],
    pub aliases: &'static [&'static str],
    pub group: Option<&'static str>,
    pub ace_mode: Option<&'static str>,
    pub codemirror_mode: Option<&'static str>,
    pub tm_scope: Option<&'static str>,
    pub language_id: i64,
}

include!(concat!(env!("OUT_DIR"), "/linguist_data.rs"));

/// The whole catalogue.
pub fn all() -> &'static [LinguistLanguage] {
    LANGUAGES
}

/// A language by name or alias, ignoring case.
pub fn by_name(name: &str) -> Option<&'static LinguistLanguage> {
    let lower = name.to_lowercase();
    LANGUAGES
        .iter()
        .find(|l| l.name.eq_ignore_ascii_case(name))
        .or_else(|| LANGUAGES.iter().find(|l| l.aliases.iter().any(|a| *a == lower)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// Several languages were possible and nothing decided between them.
    Guess,
    /// A content heuristic found positive evidence.
    Heuristic,
    /// Filename, shebang, or an extension only one language uses.
    Certain,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Guess => "guess",
            Confidence::Heuristic => "heuristic",
            Confidence::Certain => "certain",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Detection {
    pub language: Option<&'static LinguistLanguage>,
    pub confidence: Confidence,
    /// How it was decided: "filename", "shebang", "extension", "heuristic", "ranking".
    pub method: &'static str,
    /// Every language that was possible.
    pub candidates: Vec<&'static LinguistLanguage>,
}

impl Detection {
    fn none() -> Detection {
        Detection {
            language: None,
            confidence: Confidence::Guess,
            method: "none",
            candidates: Vec::new(),
        }
    }

    fn single(lang: &'static LinguistLanguage, method: &'static str) -> Detection {
        Detection {
            language: Some(lang),
            confidence: Confidence::Certain,
            method,
            candidates: vec![lang],
        }
    }

    /// True when rules written for this language may run on the file.
    pub fn trusted(&self) -> bool {
        self.language.is_some() && self.confidence >= Confidence::Heuristic
    }
}

/// The interpreter named by a `#!` line: `#!/usr/bin/env python3.11` gives "python3.11".
pub fn shebang_interpreter(text: &str) -> Option<String> {
    let first = text.lines().next()?;
    let rest = first.strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    let mut prog = words.next()?.rsplit('/').next()?.to_string();
    if prog == "env" {
        // Skip env's options, such as `-S`.
        prog = words.find(|w| !w.starts_with('-') && !w.contains('='))?.to_string();
    }
    Some(prog)
}

fn by_interpreter(name: &str) -> Vec<&'static LinguistLanguage> {
    let exact: Vec<_> = LANGUAGES.iter().filter(|l| l.interpreters.contains(&name)).collect();
    if !exact.is_empty() {
        return exact;
    }
    // python3.11 → python3 → python
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    if trimmed != name && !trimmed.is_empty() {
        return LANGUAGES.iter().filter(|l| l.interpreters.contains(&trimmed)).collect();
    }
    Vec::new()
}

/// Languages ordered by how common they are on GitHub, used only to break
/// ties for display. A tie broken this way is still a guess.
const POPULARITY: &[&str] = &[
    "JavaScript",
    "Python",
    "TypeScript",
    "Java",
    "C#",
    "C++",
    "PHP",
    "Shell",
    "C",
    "Go",
    "Ruby",
    "Kotlin",
    "Rust",
    "Swift",
    "Dart",
    "Scala",
    "Objective-C",
    "Lua",
    "Perl",
    "R",
    "MATLAB",
    "Haskell",
    "Elixir",
    "Clojure",
    "Groovy",
    "PowerShell",
    "Julia",
    "SQL",
    "HTML",
    "CSS",
    "Markdown",
    "JSON",
    "YAML",
    "XML",
    "Text",
];

fn rank(l: &LinguistLanguage) -> (usize, u8, &'static str) {
    let pop = POPULARITY.iter().position(|p| *p == l.name).unwrap_or(POPULARITY.len());
    let kind = match l.kind {
        Kind::Programming => 0,
        Kind::Markup => 1,
        Kind::Data => 2,
        Kind::Prose => 3,
    };
    (pop, kind, l.name)
}

/// Detect the language of a file from its path and contents.
pub fn detect(path: &Path, text: &str) -> Detection {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // 1. Exact filename.
    let named: Vec<_> = LANGUAGES.iter().filter(|l| l.filenames.contains(&file_name)).collect();
    if named.len() == 1 {
        return Detection::single(named[0], "filename");
    }

    // 2. Shebang.
    if let Some(interp) = shebang_interpreter(text) {
        let langs = by_interpreter(&interp);
        if langs.len() == 1 {
            return Detection::single(langs[0], "shebang");
        }
    }

    // 3. Extension: the longest one any language lists (`.blade.php` beats `.php`).
    let lower = file_name.to_lowercase();
    let mut best_len = 0;
    let mut candidates: Vec<&'static LinguistLanguage> = named;
    if candidates.is_empty() {
        for l in LANGUAGES {
            for ext in l.extensions {
                let matches = !ext.is_empty() && (file_name.ends_with(ext) || lower.ends_with(&ext.to_lowercase()));
                if matches {
                    if ext.len() > best_len {
                        best_len = ext.len();
                        candidates.clear();
                    }
                    if ext.len() == best_len && !candidates.iter().any(|c| c.name == l.name) {
                        candidates.push(l);
                    }
                }
            }
        }
    }
    match candidates.len() {
        0 => Detection::none(),
        1 => Detection::single(candidates[0], "extension"),
        _ => {
            let ext = lower.rsplit('.').next().unwrap_or("").to_string();
            if let Some(name) = heuristic(&ext, &candidates, text) {
                if let Some(lang) = candidates.iter().find(|c| c.name == name) {
                    return Detection {
                        language: Some(lang),
                        confidence: Confidence::Heuristic,
                        method: "heuristic",
                        candidates,
                    };
                }
            }
            let mut sorted = candidates.clone();
            sorted.sort_by_key(|l| rank(l));
            Detection {
                language: Some(sorted[0]),
                confidence: Confidence::Guess,
                method: "ranking",
                candidates,
            }
        }
    }
}

fn has(text: &str, pattern: &str) -> bool {
    crate::regex_cache::get(pattern)
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

/// Content heuristics for shared extensions, after Linguist's heuristics.yml.
/// Each returns a language only on positive evidence.
fn heuristic(ext: &str, candidates: &[&'static LinguistLanguage], text: &str) -> Option<&'static str> {
    let names: Vec<&str> = candidates.iter().map(|c| c.name).collect();
    let pick = |n: &'static str| names.contains(&n).then_some(n);
    match ext {
        "h" => {
            if has(text, r"(?m)^\s*(@interface|@implementation|@protocol|#import\s)") {
                pick("Objective-C")
            } else if has(
                text,
                r"(?m)(^\s*(class|namespace|template\s*<)|std::|#include\s*<(iostream|string|vector|memory)>)",
            ) {
                pick("C++")
            } else if has(text, r"(?m)^\s*#\s*(include|define|ifndef|if|pragma)\b") {
                pick("C")
            } else {
                None
            }
        }
        "m" => {
            if has(text, r"(?m)^\s*(@interface|@implementation|@property|#import\s)") {
                pick("Objective-C")
            } else if has(text, r"(?m)^\s*(function\s|%|end\s*$|disp\()") {
                pick("MATLAB")
            } else {
                None
            }
        }
        "ts" => {
            if has(text, r"^\s*(<\?xml|<!DOCTYPE TS|<TS\b)") {
                pick("XML")
            } else {
                pick("TypeScript")
            }
        }
        "tsx" => {
            if has(text, r"^\s*(<\?xml|<tileset\b)") {
                pick("XML")
            } else {
                pick("TSX")
            }
        }
        "pl" => {
            if has(
                text,
                r"(?m)^\s*(use\s+(strict|warnings)|my\s+[$@%]|sub\s+\w+|package\s+\w)",
            ) {
                pick("Perl")
            } else if has(text, r"(?m)(^\s*:-|\w+\([^)]*\)\s*:-)") {
                pick("Prolog")
            } else {
                None
            }
        }
        "rs" => {
            if has(text, r"#pragma\s+rs") {
                pick("RenderScript")
            } else {
                pick("Rust")
            }
        }
        "md" => {
            if has(text, r"^\s*\(define_") {
                pick("GCC Machine Description")
            } else {
                pick("Markdown")
            }
        }
        "cs" => pick("C#"),
        "php" => {
            if has(text, r"<\?hh") {
                pick("Hack")
            } else {
                pick("PHP")
            }
        }
        "sql" => {
            if has(text, r"(?im)(^\s*GO\s*$|\bWAITFOR\b|\bBEGIN\s+TRAN|\bSET\s+NOCOUNT\b)") {
                pick("TSQL")
            } else if has(text, r"(?i)(\$\$|LANGUAGE\s+plpgsql|\bRAISE\s+NOTICE\b)") {
                pick("PLpgSQL")
            } else if has(
                text,
                r"(?i)create\s+(or\s+replace\s+)?(package|procedure|function|trigger)\b.*\b(is|as)\b",
            ) {
                pick("PLSQL")
            } else {
                pick("SQL")
            }
        }
        "inc" => {
            if has(text, r"<\?(php|=)?") {
                pick("PHP")
            } else {
                None
            }
        }
        "yml" | "yaml" => pick("YAML"),
        "json" => pick("JSON"),
        "txt" => {
            if has(text, r"^\[Adblock") {
                pick("Adblock Filter List")
            } else {
                pick("Text")
            }
        }
        "cls" => {
            if has(text, r"(?m)^\s*\\(documentclass|ProvidesClass)") {
                pick("TeX")
            } else if has(
                text,
                r"(?i)\b(public|private|global)\s+(with|without)?\s*(sharing\s+)?class\b",
            ) {
                pick("Apex")
            } else {
                None
            }
        }
        "v" => {
            if has(text, r"(?m)^\s*module\s+\w+\s*(#|\()") {
                pick("Verilog")
            } else if has(text, r"(?m)^\s*(Require|Theorem|Lemma|Proof|Definition)\b") {
                pick("Coq")
            } else if has(text, r"(?m)^\s*(fn\s+\w+\(|import\s+\w+$)") {
                pick("V")
            } else {
                None
            }
        }
        "r" => {
            if has(text, r"(<-|library\(|function\s*\()") {
                pick("R")
            } else {
                None
            }
        }
        "d" => {
            if has(text, r"(?m)^\s*(import\s+std\.|module\s+\w)") {
                pick("D")
            } else if has(text, r"(?m)^[^\s:]+\s*:.*\\?$") {
                pick("Makefile")
            } else {
                None
            }
        }
        "fs" => {
            if has(text, r"(?m)^\s*(let|open|module|type)\s") {
                pick("F#")
            } else if has(text, r"(?m)^\s*(#version|uniform|varying|void\s+main)") {
                pick("GLSL")
            } else {
                None
            }
        }
        "pro" => {
            if has(text, r"(?m)^\s*(QT|TARGET|SOURCES|HEADERS)\s*[+]?=") {
                pick("QMake")
            } else if has(text, r":-") {
                pick("Prolog")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether a file is someone else's or a machine's, and must never be edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Source,
    Generated(String),
    Vendored(String),
}

impl Role {
    pub fn is_source(&self) -> bool {
        matches!(self, Role::Source)
    }
}

const VENDORED_DIRS: &[&str] = &[
    "node_modules",
    "vendor",
    "third_party",
    "third-party",
    "thirdparty",
    "3rdparty",
    "bower_components",
    "Pods",
    "Carthage",
    ".yarn",
    "site-packages",
    "jspm_packages",
];

const GENERATED_FILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "npm-shrinkwrap.json",
    "Cargo.lock",
    "Gemfile.lock",
    "composer.lock",
    "poetry.lock",
    "Pipfile.lock",
    "go.sum",
    "flake.lock",
    "mix.lock",
    "pubspec.lock",
    "Package.resolved",
    "packages.lock.json",
    "uv.lock",
    "bun.lockb",
];

const GENERATED_SUFFIXES: &[&str] = &[
    ".min.js",
    ".min.css",
    ".min.mjs",
    ".js.map",
    ".css.map",
    ".pb.go",
    ".pb.cc",
    ".pb.h",
    "_pb2.py",
    "_pb2_grpc.py",
    ".pb.swift",
    ".designer.cs",
    ".Designer.cs",
    ".g.dart",
    ".freezed.dart",
    ".g.cs",
    "_generated.go",
    ".generated.ts",
];

/// Decide whether a file is source, generated or vendored.
pub fn role(path: &Path, text: &str) -> Role {
    for comp in path.components() {
        let c = comp.as_os_str().to_string_lossy();
        if VENDORED_DIRS.contains(&c.as_ref()) {
            return Role::Vendored(format!("inside {c}/"));
        }
        // Xcode documentation sets (jazzy, docc) are generated output.
        if c.ends_with(".docset") || c.ends_with(".doccarchive") {
            return Role::Generated(format!("inside the generated {c}"));
        }
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Well-known libraries copied into a project (after Linguist's vendor.yml).
    static VENDORED_FILE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let vendored_file = VENDORED_FILE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)^(jquery([.-][\w.-]*)?|typeahead([.-][\w.-]*)?|bootstrap(\.bundle)?|modernizr([.-][\w.-]*)?|underscore(-min)?|backbone(-min)?|require|prism|highlight(\.pack)?|lunr|handlebars([.-][\w.-]*)?|mustache|moment(-with-locales)?|angular|react(-dom)?(\.(production|development))?|vue|d3(\.v\d+)?|chart|popper|htmx|alpine)(\.min)?\.js$",
        )
        .expect("valid regex")
    });
    if vendored_file.is_match(&name) {
        return Role::Vendored(format!("{name} is a well-known third-party library"));
    }
    if GENERATED_FILES.contains(&name.as_str()) {
        return Role::Generated("lockfile".to_string());
    }
    if let Some(suffix) = GENERATED_SUFFIXES.iter().find(|s| name.ends_with(*s)) {
        return Role::Generated(format!("{suffix} files are generated"));
    }
    let head: String = text.lines().take(8).collect::<Vec<_>>().join("\n");
    let markers = [
        ("DO NOT EDIT", "marked DO NOT EDIT"),
        ("@generated", "marked @generated"),
        ("Code generated by", "says it is generated"),
        ("Autogenerated by", "says it is generated"),
        ("auto-generated", "says it is generated"),
        ("automatically generated", "says it is generated"),
        ("This file was generated", "says it is generated"),
    ];
    for (needle, why) in markers {
        if head.contains(needle) {
            return Role::Generated(why.to_string());
        }
    }
    // Minified: very long lines of code.
    let lines = text.lines().count().max(1);
    if text.len() > 5000 && text.len() / lines > 500 {
        return Role::Generated("minified (very long lines)".to_string());
    }
    Role::Source
}

#[cfg(test)]
mod vendored_tests {
    use super::*;

    #[test]
    fn well_known_libraries_and_docsets() {
        let r = |p: &str| role(Path::new(p), "x");
        assert!(matches!(r("docs/js/typeahead.jquery.js"), Role::Vendored(_)));
        assert!(matches!(r("static/jquery-3.7.1.min.js"), Role::Vendored(_)));
        assert!(matches!(
            r("Alamofire.docset/Contents/Resources/x.js"),
            Role::Generated(_)
        ));
        assert!(matches!(r("src/app.js"), Role::Source));
        assert!(matches!(r("src/required.js"), Role::Source));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn d(path: &str, text: &str) -> Detection {
        detect(&PathBuf::from(path), text)
    }

    #[test]
    fn the_whole_catalogue_is_present() {
        assert!(all().len() > 800, "{} languages", all().len());
        let py = by_name("python").unwrap();
        assert_eq!(py.kind, Kind::Programming);
        assert!(py.extensions.contains(&".py"));
        assert!(by_name("golang").is_some(), "aliases resolve");
    }

    #[test]
    fn filenames_shebangs_and_extensions() {
        assert_eq!(d("Dockerfile", "").language.unwrap().name, "Dockerfile");
        assert_eq!(d("Makefile", "").language.unwrap().name, "Makefile");
        let s = d("bin/tool", "#!/usr/bin/env python3.12\nprint(1)\n");
        assert_eq!(s.language.unwrap().name, "Python");
        assert_eq!(s.method, "shebang");
        assert_eq!(d("deploy", "#!/bin/bash\necho hi\n").language.unwrap().name, "Shell");
        assert_eq!(d("src/main.rs", "fn main() {}").language.unwrap().name, "Rust");
        assert_eq!(d("App.tsx", "").language.unwrap().name, "TSX");
        assert_eq!(d("view.blade.php", "").language.unwrap().name, "Blade");
    }

    #[test]
    fn shared_extensions_need_evidence() {
        let h = d("api.h", "#include <vector>\nclass Api {};\n");
        assert_eq!(h.language.unwrap().name, "C++");
        assert_eq!(h.confidence, Confidence::Heuristic);
        assert_eq!(
            d("api.h", "#ifndef API_H\nint f(void);\n#endif\n")
                .language
                .unwrap()
                .name,
            "C"
        );
        let unknown = d("notes.m", "hello\n");
        assert_eq!(unknown.confidence, Confidence::Guess);
        assert!(!unknown.trusted());
        assert_eq!(
            d("app.ts", "const x: number = 1;\n").language.unwrap().name,
            "TypeScript"
        );
        assert_eq!(
            d("tr.ts", "<?xml version=\"1.0\"?>\n<TS>").language.unwrap().name,
            "XML"
        );
    }

    #[test]
    fn every_grammar_owns_its_primary_extension() {
        for lang in crate::lang::all() {
            for name in lang.linguist_names() {
                let l = by_name(name).unwrap();
                let ext = l.extensions.first().unwrap();
                let det = d(&format!("file{ext}"), "");
                assert!(
                    det.trusted() && det.language.unwrap().name == *name,
                    "{name}: a file ending in {ext} is detected as {:?} ({:?})",
                    det.language.map(|l| l.name),
                    det.confidence
                );
            }
        }
    }

    #[test]
    fn generated_and_vendored_files_are_recognised() {
        let p = PathBuf::from;
        assert!(matches!(role(&p("package-lock.json"), ""), Role::Generated(_)));
        assert!(matches!(role(&p("web/node_modules/x/index.js"), ""), Role::Vendored(_)));
        assert!(matches!(role(&p("api.pb.go"), ""), Role::Generated(_)));
        assert!(matches!(
            role(&p("gen.go"), "// Code generated by stringer. DO NOT EDIT.\n"),
            Role::Generated(_)
        ));
        assert_eq!(role(&p("src/app.py"), "print(1)\n"), Role::Source);
    }
}
