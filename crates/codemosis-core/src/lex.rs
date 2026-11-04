//! The lexical tier: knowing what is code, what is a comment and what is a
//! string, for languages without a full grammar.
//!
//! A language's lexical profile is derived from the editor modes Linguist
//! records for it (`ace_mode`, `codemirror_mode`, `tm_scope`), which group
//! hundreds of languages into a few families of comment and string syntax.
//! A language with no known family gets no profile, and rules that need to
//! tell code from comments do not run on it: unsure means no.

use crate::linguist::{self, LinguistLanguage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegKind {
    Code,
    Comment,
    String,
}

impl SegKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SegKind::Code => "code",
            SegKind::Comment => "comment",
            SegKind::String => "string",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
    pub kind: SegKind,
}

#[derive(Clone, Copy, Debug)]
pub struct Quote {
    pub open: &'static str,
    pub close: &'static str,
    /// Backslash escapes the next character.
    pub escapes: bool,
    /// May span lines.
    pub multiline: bool,
}

const fn q(open: &'static str, close: &'static str, escapes: bool, multiline: bool) -> Quote {
    Quote {
        open,
        close,
        escapes,
        multiline,
    }
}

/// Comment and string syntax shared by a family of languages.
#[derive(Debug)]
pub struct Profile {
    pub family: &'static str,
    pub line_comments: &'static [&'static str],
    /// Line comments that only count at the start of a word (after
    /// whitespace or at the start of a line), like `#` in shell.
    pub word_start_comments: bool,
    pub block_comments: &'static [(&'static str, &'static str)],
    pub quotes: &'static [Quote],
}

const DQ: Quote = q("\"", "\"", true, false);
const SQ: Quote = q("'", "'", true, false);

pub static C: Profile = Profile {
    family: "c",
    line_comments: &["//"],
    word_start_comments: false,
    block_comments: &[("/*", "*/")],
    quotes: &[DQ, SQ],
};
pub static C_TEMPLATE: Profile = Profile {
    family: "c-template",
    line_comments: &["//"],
    word_start_comments: false,
    block_comments: &[("/*", "*/")],
    quotes: &[DQ, SQ, q("`", "`", true, true)],
};
pub static C_HASH: Profile = Profile {
    family: "c-hash",
    line_comments: &["//", "#"],
    word_start_comments: false,
    block_comments: &[("/*", "*/")],
    quotes: &[DQ, SQ],
};
pub static HASH: Profile = Profile {
    family: "hash",
    line_comments: &["#"],
    word_start_comments: true,
    block_comments: &[],
    quotes: &[DQ, SQ],
};
pub static PYTHON_LIKE: Profile = Profile {
    family: "python",
    line_comments: &["#"],
    word_start_comments: true,
    block_comments: &[],
    quotes: &[q("\"\"\"", "\"\"\"", true, true), q("'''", "'''", true, true), DQ, SQ],
};
pub static SHELL: Profile = Profile {
    family: "shell",
    line_comments: &["#"],
    word_start_comments: true,
    block_comments: &[],
    quotes: &[q("\"", "\"", true, true), q("'", "'", false, true)],
};
pub static POWERSHELL: Profile = Profile {
    family: "powershell",
    line_comments: &["#"],
    word_start_comments: true,
    block_comments: &[("<#", "#>")],
    quotes: &[q("\"", "\"", false, true), q("'", "'", false, true)],
};
pub static SQL: Profile = Profile {
    family: "sql",
    line_comments: &["--"],
    word_start_comments: false,
    block_comments: &[("/*", "*/")],
    quotes: &[q("'", "'", false, true), q("\"", "\"", false, false)],
};
pub static LUA: Profile = Profile {
    family: "lua",
    line_comments: &["--"],
    word_start_comments: false,
    block_comments: &[("--[[", "]]")],
    quotes: &[DQ, SQ, q("[[", "]]", false, true)],
};
pub static DASH: Profile = Profile {
    family: "dash",
    line_comments: &["--"],
    word_start_comments: false,
    block_comments: &[("{-", "-}")],
    quotes: &[DQ],
};
pub static LISP: Profile = Profile {
    family: "lisp",
    line_comments: &[";"],
    word_start_comments: false,
    block_comments: &[("#|", "|#")],
    quotes: &[q("\"", "\"", true, true)],
};
pub static SEMICOLON: Profile = Profile {
    family: "semicolon",
    line_comments: &[";"],
    word_start_comments: false,
    block_comments: &[],
    quotes: &[DQ],
};
pub static PERCENT: Profile = Profile {
    family: "percent",
    line_comments: &["%"],
    word_start_comments: false,
    block_comments: &[],
    quotes: &[DQ],
};
pub static XML: Profile = Profile {
    family: "xml",
    line_comments: &[],
    word_start_comments: false,
    block_comments: &[("<!--", "-->")],
    quotes: &[],
};
pub static ML: Profile = Profile {
    family: "ml",
    line_comments: &[],
    word_start_comments: false,
    block_comments: &[("(*", "*)")],
    quotes: &[DQ],
};
pub static FSHARP: Profile = Profile {
    family: "fsharp",
    line_comments: &["//"],
    word_start_comments: false,
    block_comments: &[("(*", "*)")],
    quotes: &[DQ],
};
pub static PASCAL: Profile = Profile {
    family: "pascal",
    line_comments: &["//"],
    word_start_comments: false,
    block_comments: &[("{", "}"), ("(*", "*)")],
    quotes: &[q("'", "'", false, false)],
};
pub static FORTRAN: Profile = Profile {
    family: "fortran",
    line_comments: &["!"],
    word_start_comments: false,
    block_comments: &[],
    quotes: &[q("\"", "\"", false, false), q("'", "'", false, false)],
};
pub static BASIC: Profile = Profile {
    family: "basic",
    line_comments: &["'", "REM ", "rem "],
    word_start_comments: true,
    block_comments: &[],
    quotes: &[q("\"", "\"", false, false)],
};
pub static BATCH: Profile = Profile {
    family: "batch",
    line_comments: &["REM ", "rem ", "::"],
    word_start_comments: true,
    block_comments: &[],
    quotes: &[q("\"", "\"", false, false)],
};

pub static APPLESCRIPT: Profile = Profile {
    family: "applescript",
    line_comments: &["--", "#"],
    word_start_comments: false,
    block_comments: &[("(*", "*)")],
    quotes: &[DQ],
};
pub static AUTOHOTKEY: Profile = Profile {
    family: "autohotkey",
    line_comments: &[";"],
    word_start_comments: true,
    block_comments: &[("/*", "*/")],
    quotes: &[q("\"", "\"", false, false)],
};

pub static ALL_PROFILES: &[&Profile] = &[
    &APPLESCRIPT,
    &AUTOHOTKEY,
    &C,
    &C_TEMPLATE,
    &C_HASH,
    &HASH,
    &PYTHON_LIKE,
    &SHELL,
    &POWERSHELL,
    &SQL,
    &LUA,
    &DASH,
    &LISP,
    &SEMICOLON,
    &PERCENT,
    &XML,
    &ML,
    &FSHARP,
    &PASCAL,
    &FORTRAN,
    &BASIC,
    &BATCH,
];

fn by_mode(mode: &str) -> Option<&'static Profile> {
    Some(match mode {
        "c_cpp" | "java" | "csharp" | "kotlin" | "scala" | "swift" | "dart" | "groovy" | "objectivec" | "d"
        | "haxe" | "actionscript" | "apex" | "glsl" | "protobuf" | "verilog" | "vala" | "rust" | "scad" | "zig"
        | "clike" | "jsoniq" | "solidity" | "odin" | "gcode" | "hjson" | "json5" | "nix" | "prisma"
        | "graphqlschema" => &C,
        "javascript" | "typescript" | "tsx" | "jsx" | "golang" | "go" => &C_TEMPLATE,
        "php" | "php_laravel_blade" => &C_HASH,
        // TOML has triple-quoted multi-line strings, like Python.
        "python" | "cython" | "nim" | "coffee" | "coffeescript" | "toml" => &PYTHON_LIKE,
        "sh" | "shell" => &SHELL,
        "powershell" => &POWERSHELL,
        "ruby" | "perl" | "r" | "julia" | "yaml" | "makefile" | "dockerfile" | "gitignore" | "properties"
        | "crystal" | "elixir" | "tcl" | "raku" | "gdscript" | "cmake" | "puppet" | "nginx" | "apache_conf" => &HASH,
        "sql" | "mysql" | "pgsql" | "sqlserver" | "plsql" => &SQL,
        "lua" => &LUA,
        "haskell" | "elm" | "purescript" | "ada" | "vhdl" | "eiffel" => &DASH,
        "lisp" | "clojure" | "scheme" | "commonlisp" => &LISP,
        "assembly_x86" | "ini" => &SEMICOLON,
        "erlang" | "prolog" | "matlab" | "octave" | "latex" | "tex" | "stex" => &PERCENT,
        "html" | "xml" | "svg" | "htmlmixed" | "htmlembedded" => &XML,
        "ocaml" | "mllike" | "sml" => &ML,
        "fsharp" => &FSHARP,
        "pascal" => &PASCAL,
        "fortran" => &FORTRAN,
        "vbscript" | "vb" => &BASIC,
        "batchfile" => &BATCH,
        "applescript" => &APPLESCRIPT,
        "autohotkey" => &AUTOHOTKEY,
        _ => return None,
    })
}

fn by_scope(scope: &str) -> Option<&'static Profile> {
    let s = scope.trim_start_matches("source.").trim_start_matches("text.");
    let head = s.split('.').next().unwrap_or(s);
    Some(match head {
        "c" | "c++" | "cpp" | "java" | "cs" | "objc" | "objc++" | "swift" | "kotlin" | "scala" | "dart" | "rust"
        | "d" | "groovy" | "glsl" | "hlsl" | "cuda-c++" | "proto" | "solidity" | "zig" | "verilog" => &C,
        "js" | "ts" | "tsx" | "go" => &C_TEMPLATE,
        "php" => &C_HASH,
        "python" | "toml" | "gdscript" => &PYTHON_LIKE,
        "shell" => &SHELL,
        "powershell" => &POWERSHELL,
        "ruby" | "perl" | "r" | "julia" | "yaml" | "makefile" | "elixir" | "crystal" | "tcl" | "nim" => &HASH,
        "sql" => &SQL,
        "lua" => &LUA,
        "haskell" | "elm" | "ada" | "vhdl" => &DASH,
        "lisp" | "clojure" | "scheme" | "racket" => &LISP,
        "erlang" | "prolog" | "matlab" | "tex" | "latex" => &PERCENT,
        "xml" | "html" => &XML,
        "ocaml" | "sml" => &ML,
        "fsharp" => &FSHARP,
        "pascal" => &PASCAL,
        "fortran" => &FORTRAN,
        "vbnet" | "vb" | "asp" => &BASIC,
        "batchfile" => &BATCH,
        _ => return None,
    })
}

/// The lexical profile of a Linguist language, if its family is known.
pub fn profile_for(lang: &LinguistLanguage) -> Option<&'static Profile> {
    // Prose and plain data have no comments to tell apart.
    if matches!(lang.name, "Text" | "JSON" | "CSV" | "TSV" | "Markdown") {
        return None;
    }
    lang.ace_mode
        .filter(|m| *m != "text")
        .and_then(by_mode)
        .or_else(|| lang.codemirror_mode.and_then(by_mode))
        .or_else(|| lang.tm_scope.filter(|s| *s != "none").and_then(by_scope))
        .or_else(|| {
            lang.group
                .and_then(linguist::by_name)
                .filter(|g| g.name != lang.name)
                .and_then(profile_for)
        })
}

fn starts_word(text: &[u8], i: usize) -> bool {
    i == 0 || matches!(text[i - 1], b' ' | b'\t' | b'\n' | b'\r' | b';')
}

/// Split text into code, comments and strings.
pub fn segments(text: &str, p: &Profile) -> Vec<Segment> {
    let b = text.as_bytes();
    let mut out: Vec<Segment> = Vec::new();
    let mut code_start = 0;
    let mut i = 0;
    let push = |out: &mut Vec<Segment>, start: usize, end: usize, kind: SegKind| {
        if end > start {
            out.push(Segment { start, end, kind });
        }
    };
    'outer: while i < b.len() {
        for (open, close) in p.block_comments {
            if b[i..].starts_with(open.as_bytes()) {
                let end = text[i + open.len()..]
                    .find(close)
                    .map(|j| i + open.len() + j + close.len())
                    .unwrap_or(b.len());
                push(&mut out, code_start, i, SegKind::Code);
                push(&mut out, i, end, SegKind::Comment);
                i = end;
                code_start = i;
                continue 'outer;
            }
        }
        for lc in p.line_comments {
            if b[i..].starts_with(lc.as_bytes()) && (!p.word_start_comments || starts_word(b, i)) {
                let end = text[i..].find('\n').map(|j| i + j).unwrap_or(b.len());
                push(&mut out, code_start, i, SegKind::Code);
                push(&mut out, i, end, SegKind::Comment);
                i = end;
                code_start = i;
                continue 'outer;
            }
        }
        for qt in p.quotes {
            if b[i..].starts_with(qt.open.as_bytes()) {
                let mut j = i + qt.open.len();
                let end = loop {
                    if j >= b.len() {
                        break b.len();
                    }
                    if qt.escapes && b[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if b[j..].starts_with(qt.close.as_bytes()) {
                        break j + qt.close.len();
                    }
                    if !qt.multiline && b[j] == b'\n' {
                        break j;
                    }
                    j += 1;
                };
                let end = end.min(b.len());
                push(&mut out, code_start, i, SegKind::Code);
                push(&mut out, i, end, SegKind::String);
                i = end;
                code_start = i;
                continue 'outer;
            }
        }
        // Advance one character.
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    push(&mut out, code_start, b.len(), SegKind::Code);
    out
}

/// The kind of text at a byte offset.
pub fn kind_at(segments: &[Segment], offset: usize) -> SegKind {
    match segments.binary_search_by(|s| {
        if offset < s.start {
            std::cmp::Ordering::Greater
        } else if offset >= s.end {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(i) => segments[i].kind,
        Err(_) => SegKind::Code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str, p: &Profile) -> Vec<(SegKind, String)> {
        segments(text, p)
            .into_iter()
            .map(|s| (s.kind, text[s.start..s.end].to_string()))
            .collect()
    }

    #[test]
    fn c_family() {
        let k = kinds("a = \"x // y\"; // done\n/* b */ c", &C);
        assert_eq!(
            k,
            vec![
                (SegKind::Code, "a = ".into()),
                (SegKind::String, "\"x // y\"".into()),
                (SegKind::Code, "; ".into()),
                (SegKind::Comment, "// done".into()),
                (SegKind::Code, "\n".into()),
                (SegKind::Comment, "/* b */".into()),
                (SegKind::Code, " c".into()),
            ]
        );
    }

    #[test]
    fn shell_hash_needs_a_word_start() {
        let k = kinds("echo ${#arr} # count\nx='a # b'\n", &SHELL);
        assert!(k.contains(&(SegKind::Comment, "# count".into())));
        assert!(k.contains(&(SegKind::String, "'a # b'".into())));
        assert!(k
            .iter()
            .any(|(kind, t)| *kind == SegKind::Code && t.contains("${#arr}")));
    }

    #[test]
    fn python_triple_quotes() {
        let k = kinds("s = '''a\n# not a comment\n'''\n# real\n", &PYTHON_LIKE);
        assert!(k.contains(&(SegKind::String, "'''a\n# not a comment\n'''".into())));
        assert!(k.contains(&(SegKind::Comment, "# real".into())));
    }

    #[test]
    fn profiles_come_from_linguist() {
        let get = |n: &str| profile_for(linguist::by_name(n).unwrap()).map(|p| p.family);
        assert_eq!(get("Java"), Some("c"));
        assert_eq!(get("PowerShell"), Some("powershell"));
        assert_eq!(get("Shell"), Some("shell"));
        assert_eq!(get("Erlang"), Some("percent"));
        assert_eq!(get("Clojure"), Some("lisp"));
        assert_eq!(get("Markdown"), None);
        let covered = linguist::all().iter().filter(|l| profile_for(l).is_some()).count();
        assert!(covered > 300, "{covered} languages have a lexical profile");
    }

    #[test]
    fn toml_multiline_strings() {
        let text = "a = '''\n<<<<<<< x\n'''\nb = 1 # c\n";
        let toml = profile_for(linguist::by_name("TOML").unwrap()).unwrap();
        let s = segments(text, toml);
        assert_eq!(kind_at(&s, text.find("<<<").unwrap()), SegKind::String);
        assert_eq!(kind_at(&s, text.find("# c").unwrap()), SegKind::Comment);
    }

    #[test]
    fn kind_at_offsets() {
        let text = "x # c\ny";
        let s = segments(text, &HASH);
        assert_eq!(kind_at(&s, 0), SegKind::Code);
        assert_eq!(kind_at(&s, 3), SegKind::Comment);
        assert_eq!(kind_at(&s, 6), SegKind::Code);
    }
}
