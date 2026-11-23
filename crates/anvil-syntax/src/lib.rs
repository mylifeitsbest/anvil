//! Подсветка синтаксиса и сведения о языках.
//!
//! * tree-sitter для Rust, Python, JS, TS/TSX, C, C++, Go, HTML, CSS, JSON, Markdown;
//! * свой лёгкий разборщик для TOML;
//! * ещё ~25 языков без грамматики, но с комментариями и запуском через тулчейны.
//!
//! Грамматики инициализируются лениво — при первом открытии файла этого языка.

use std::collections::HashMap;
use std::path::Path;

use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter as TsHighlighter};

/// Классы подсветки (совпадают с ключами `[syntax]` в настройках).
pub const CLASS_NAMES: &[&str] = &[
    "text",
    "keyword",
    "function",
    "type",
    "string",
    "number",
    "comment",
    "macro",
    "attribute",
    "punctuation",
    "operator",
    "variable",
    "parameter",
    "property",
    "constant",
    "builtin",
    "label",
    "tag",
    "escape",
    "module",
    "heading",
    "link",
    "emphasis",
];

fn class_of(name: &str) -> u8 {
    CLASS_NAMES.iter().position(|c| *c == name).unwrap_or(0) as u8
}

/// Имена захватов tree-sitter, которые мы узнаём, и их класс.
const CAPTURES: &[(&str, &str)] = &[
    ("attribute", "attribute"),
    ("comment", "comment"),
    ("constant", "constant"),
    ("constant.builtin", "builtin"),
    ("constant.character", "string"),
    ("constant.numeric", "number"),
    ("constructor", "type"),
    ("embedded", "text"),
    ("escape", "escape"),
    ("function", "function"),
    ("function.builtin", "function"),
    ("function.macro", "macro"),
    ("function.method", "function"),
    ("keyword", "keyword"),
    ("label", "label"),
    ("number", "number"),
    ("float", "number"),
    ("boolean", "builtin"),
    ("operator", "operator"),
    ("property", "property"),
    ("punctuation", "punctuation"),
    ("punctuation.bracket", "punctuation"),
    ("punctuation.delimiter", "punctuation"),
    ("punctuation.special", "punctuation"),
    ("string", "string"),
    ("string.escape", "escape"),
    ("string.special", "string"),
    ("character", "string"),
    ("tag", "tag"),
    ("tag.attribute", "attribute"),
    ("type", "type"),
    ("type.builtin", "type"),
    ("variable", "variable"),
    ("variable.builtin", "builtin"),
    ("variable.parameter", "parameter"),
    ("variable.member", "property"),
    ("module", "module"),
    ("namespace", "module"),
    ("text.title", "heading"),
    ("text.literal", "string"),
    ("text.uri", "link"),
    ("text.reference", "link"),
    ("text.emphasis", "emphasis"),
    ("text.strong", "heading"),
    ("markup.heading", "heading"),
    ("markup.raw", "string"),
    ("markup.link", "link"),
    ("markup.italic", "emphasis"),
    ("markup.bold", "heading"),
    ("markup.list", "punctuation"),
    ("markup.quote", "comment"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grammar {
    None,
    Toml,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    C,
    Cpp,
    Go,
    Html,
    Css,
    Json,
    Markdown,
}

/// Язык: как распознать файл и как с ним работать.
#[derive(Debug)]
pub struct Language {
    pub id: &'static str,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub file_names: &'static [&'static str],
    pub line_comment: Option<&'static str>,
    pub indent_after: &'static [char],
    grammar: Grammar,
}

impl Language {
    pub fn has_grammar(&self) -> bool {
        self.grammar != Grammar::None
    }
}

const BR: &[char] = &['{', '[', '('];
const PY: &[char] = &['{', '[', '(', ':'];

macro_rules! lang {
    ($id:literal, $name:literal, [$($ext:literal),*], [$($f:literal),*], $c:expr, $ind:expr, $g:ident) => {
        Language { id: $id, name: $name, extensions: &[$($ext),*], file_names: &[$($f),*], line_comment: $c, indent_after: $ind, grammar: Grammar::$g }
    };
}

pub static LANGUAGES: &[Language] = &[
    lang!("rust", "Rust", ["rs"], [], Some("//"), BR, Rust),
    lang!("python", "Python", ["py", "pyw", "pyi"], [], Some("#"), PY, Python),
    lang!("javascript", "JavaScript", ["js", "mjs", "cjs", "jsx"], [], Some("//"), BR, JavaScript),
    lang!("typescript", "TypeScript", ["ts", "mts", "cts"], [], Some("//"), BR, TypeScript),
    lang!("tsx", "TSX", ["tsx"], [], Some("//"), BR, Tsx),
    lang!("c", "C", ["c", "h"], [], Some("//"), BR, C),
    lang!("cpp", "C++", ["cpp", "cc", "cxx", "hpp", "hh", "hxx", "ino"], [], Some("//"), BR, Cpp),
    lang!("go", "Go", ["go"], [], Some("//"), BR, Go),
    lang!("html", "HTML", ["html", "htm", "xhtml", "vue", "svelte"], [], None, &[], Html),
    lang!("css", "CSS", ["css", "scss", "less"], [], None, BR, Css),
    lang!("json", "JSON", ["json", "jsonc", "json5"], [], None, BR, Json),
    lang!("markdown", "Markdown", ["md", "markdown", "mdx"], [], None, &[], Markdown),
    lang!("toml", "TOML", ["toml"], ["Cargo.lock"], Some("#"), BR, Toml),
    lang!("java", "Java", ["java"], [], Some("//"), BR, None),
    lang!("kotlin", "Kotlin", ["kt", "kts"], [], Some("//"), BR, None),
    lang!("csharp", "C#", ["cs", "csx"], [], Some("//"), BR, None),
    lang!("fsharp", "F#", ["fs", "fsx"], [], Some("//"), BR, None),
    lang!("zig", "Zig", ["zig"], [], Some("//"), BR, None),
    lang!("swift", "Swift", ["swift"], [], Some("//"), BR, None),
    lang!("dart", "Dart", ["dart"], [], Some("//"), BR, None),
    lang!("scala", "Scala", ["scala", "sc"], [], Some("//"), BR, None),
    lang!("d", "D", ["d"], [], Some("//"), BR, None),
    lang!("nim", "Nim", ["nim"], [], Some("#"), PY, None),
    lang!("crystal", "Crystal", ["cr"], [], Some("#"), BR, None),
    lang!("ruby", "Ruby", ["rb"], ["Gemfile", "Rakefile"], Some("#"), BR, None),
    lang!("php", "PHP", ["php"], [], Some("//"), BR, None),
    lang!("perl", "Perl", ["pl", "pm"], [], Some("#"), BR, None),
    lang!("lua", "Lua", ["lua"], [], Some("--"), BR, None),
    lang!("haskell", "Haskell", ["hs"], [], Some("--"), &[], None),
    lang!("ocaml", "OCaml", ["ml", "mli"], [], None, &[], None),
    lang!("elixir", "Elixir", ["ex", "exs"], [], Some("#"), BR, None),
    lang!("erlang", "Erlang", ["erl"], [], Some("%"), BR, None),
    lang!("julia", "Julia", ["jl"], [], Some("#"), BR, None),
    lang!("r", "R", ["r", "R"], [], Some("#"), BR, None),
    lang!("fortran", "Fortran", ["f90", "f95", "f03", "f08", "f"], [], Some("!"), &[], None),
    lang!("pascal", "Pascal", ["pas", "pp", "lpr"], [], Some("//"), &[], None),
    lang!("asm", "Assembly", ["asm", "s", "nasm"], [], Some(";"), &[], None),
    lang!("shell", "Shell", ["sh", "bash", "zsh"], [], Some("#"), BR, None),
    lang!("powershell", "PowerShell", ["ps1", "psm1"], [], Some("#"), BR, None),
    lang!("batch", "Batch", ["bat", "cmd"], [], Some("REM"), &[], None),
    lang!("sql", "SQL", ["sql"], [], Some("--"), BR, None),
    lang!("yaml", "YAML", ["yml", "yaml"], [], Some("#"), &[':'], None),
    lang!("make", "Makefile", ["mk"], ["Makefile", "makefile", "GNUmakefile"], Some("#"), &[], None),
    lang!("cmake", "CMake", ["cmake"], ["CMakeLists.txt"], Some("#"), BR, None),
    lang!("dockerfile", "Dockerfile", [], ["Dockerfile"], Some("#"), &[], None),
    lang!("text", "Текст", ["txt", "log"], [], None, &[], None),
];

pub fn by_id(id: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.id == id)
}

/// Язык по имени файла; если не распознан — «Текст».
pub fn detect(path: &Path) -> &'static Language {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    LANGUAGES
        .iter()
        .find(|l| l.file_names.iter().any(|f| *f == name))
        .or_else(|| LANGUAGES.iter().find(|l| l.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext))))
        .or_else(|| by_id("text"))
        .unwrap_or(&LANGUAGES[LANGUAGES.len() - 1])
}

/// Участок текста с классом подсветки (байтовые смещения).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub class: u8,
}

fn ts_config(g: Grammar) -> Option<HighlightConfiguration> {
    use tree_sitter::Language as L;
    let build = |lang: L, name: &str, queries: &[&str], inj: &str| -> Option<HighlightConfiguration> {
        // пробуем объединённые запросы, при ошибке — только первый
        let joined = queries.join("\n");
        HighlightConfiguration::new(lang.clone(), name, &joined, inj, "")
            .or_else(|_| HighlightConfiguration::new(lang, name, queries.first().copied().unwrap_or_default(), inj, ""))
            .ok()
    };
    let mut cfg = match g {
        Grammar::Rust => build(tree_sitter_rust::LANGUAGE.into(), "rust", &[tree_sitter_rust::HIGHLIGHTS_QUERY], "")?,
        Grammar::Python => build(tree_sitter_python::LANGUAGE.into(), "python", &[tree_sitter_python::HIGHLIGHTS_QUERY], "")?,
        Grammar::JavaScript => build(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            &[tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY],
            "",
        )?,
        Grammar::TypeScript => build(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            &[tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY],
            "",
        )?,
        Grammar::Tsx => build(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            &[tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY],
            "",
        )?,
        Grammar::C => build(tree_sitter_c::LANGUAGE.into(), "c", &[tree_sitter_c::HIGHLIGHT_QUERY], "")?,
        Grammar::Cpp => build(tree_sitter_cpp::LANGUAGE.into(), "cpp", &[tree_sitter_cpp::HIGHLIGHT_QUERY, tree_sitter_c::HIGHLIGHT_QUERY], "")?,
        Grammar::Go => build(tree_sitter_go::LANGUAGE.into(), "go", &[tree_sitter_go::HIGHLIGHTS_QUERY], "")?,
        Grammar::Html => build(tree_sitter_html::LANGUAGE.into(), "html", &[tree_sitter_html::HIGHLIGHTS_QUERY], "")?,
        Grammar::Css => build(tree_sitter_css::LANGUAGE.into(), "css", &[tree_sitter_css::HIGHLIGHTS_QUERY], "")?,
        Grammar::Json => build(tree_sitter_json::LANGUAGE.into(), "json", &[tree_sitter_json::HIGHLIGHTS_QUERY], "")?,
        Grammar::Markdown => build(tree_sitter_md::LANGUAGE.into(), "markdown", &[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK], "")?,
        Grammar::None | Grammar::Toml => return None,
    };
    let names: Vec<&str> = CAPTURES.iter().map(|(n, _)| *n).collect();
    cfg.configure(&names);
    Some(cfg)
}

/// Подсветчик с ленивым кэшем грамматик.
pub struct Highlighter {
    ts: TsHighlighter,
    configs: HashMap<&'static str, Option<HighlightConfiguration>>,
    capture_class: Vec<u8>,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self { ts: TsHighlighter::new(), configs: HashMap::new(), capture_class: CAPTURES.iter().map(|(_, c)| class_of(c)).collect() }
    }
}

impl Highlighter {
    pub fn highlight(&mut self, lang: &Language, text: &str) -> Vec<Span> {
        match lang.grammar {
            Grammar::None => Vec::new(),
            Grammar::Toml => toml_spans(text),
            g => {
                let cfg = self.configs.entry(lang.id).or_insert_with(|| ts_config(g));
                let Some(cfg) = cfg.as_ref() else { return Vec::new() };
                let mut out = Vec::new();
                let Ok(events) = self.ts.highlight(cfg, text.as_bytes(), None, None, |_| None) else { return out };
                let mut stack: Vec<u8> = Vec::new();
                for ev in events {
                    match ev {
                        Ok(HighlightEvent::HighlightStart(h)) => stack.push(self.capture_class.get(h.0).copied().unwrap_or(0)),
                        Ok(HighlightEvent::HighlightEnd) => {
                            stack.pop();
                        }
                        Ok(HighlightEvent::Source { start, end }) => {
                            if let Some(&class) = stack.last()
                                && class != 0
                                && end > start
                            {
                                match out.last_mut() {
                                    Some(Span { end: e, class: c, .. }) if *e == start && *c == class => *e = end,
                                    _ => out.push(Span { start, end, class }),
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                out
            }
        }
    }
}

/// Простой построчный разбор TOML (грамматика не нужна, а настройки — в TOML).
pub fn toml_spans(text: &str) -> Vec<Span> {
    let (kw, fnc, ty, st, num, cm, p) =
        (class_of("keyword"), class_of("function"), class_of("type"), class_of("string"), class_of("number"), class_of("comment"), class_of("punctuation"));
    let mut out = Vec::new();
    let mut off = 0;
    for line in text.split_inclusive('\n') {
        let b = line.as_bytes();
        let n = line.trim_end_matches(['\n', '\r']).len();
        let mut i = 0;
        while i < n && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        let mut push = |s: usize, e: usize, c: u8| out.push(Span { start: off + s, end: off + e, class: c });
        if i < n && b[i] == b'[' {
            let e = line[i..n].find(']').map(|x| i + x + 1).unwrap_or(n);
            push(i, e, ty);
            i = e;
        } else if let Some(eq) = line[..n].find('=')
            && !line[i..eq].contains('"')
            && !line[i..eq].contains('#')
            && eq > i
        {
            push(i, eq, fnc);
            push(eq, eq + 1, p);
            i = eq + 1;
        }
        while i < n {
            let c = b[i];
            if c == b'#' {
                push(i, n, cm);
                break;
            } else if c == b'"' || c == b'\'' {
                let mut j = i + 1;
                while j < n && b[j] != c {
                    if b[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                let e = (j + 1).min(n);
                push(i, e, st);
                i = e;
            } else if c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
                let mut j = i + 1;
                while j < n && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'.' | b'_' | b':' | b'-' | b'+')) {
                    j += 1;
                }
                push(i, j, num);
                i = j;
            } else if b[i..n].starts_with(b"true") || b[i..n].starts_with(b"false") {
                let e = i + if b[i..n].starts_with(b"true") { 4 } else { 5 };
                push(i, e, kw);
                i = e;
            } else if c.is_ascii_alphabetic() {
                let mut j = i;
                while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'-') {
                    j += 1;
                }
                let k = line[j..n].trim_start();
                if k.starts_with('=') {
                    push(i, j, fnc);
                }
                i = j.max(i + 1);
            } else {
                if matches!(c, b'{' | b'}' | b'[' | b']' | b',' | b'=') {
                    push(i, i + 1, p);
                }
                // шагаем по символам, а не байтам, чтобы не попасть в середину UTF-8
                i += line[i..].chars().next().map_or(1, char::len_utf8);
            }
        }
        off += line.len();
    }
    out
}

/// Скобка, её глубина и индекс пары.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bracket {
    pub pos: usize,
    pub depth: u16,
    pub partner: Option<u32>,
}

/// Все скобки вне строк и комментариев — для раскраски по глубине и подсветки пары.
pub fn brackets(text: &str, spans: &[Span]) -> Vec<Bracket> {
    let (st, cm, esc) = (class_of("string"), class_of("comment"), class_of("escape"));
    let mut skip = spans.iter().filter(|s| s.class == st || s.class == cm || s.class == esc).peekable();
    let mut out: Vec<Bracket> = Vec::new();
    let mut stack: Vec<(u8, usize)> = Vec::new();
    for (i, c) in text.bytes().enumerate() {
        if !matches!(c, b'(' | b')' | b'[' | b']' | b'{' | b'}') {
            continue;
        }
        while skip.peek().is_some_and(|s| s.end <= i) {
            skip.next();
        }
        if skip.peek().is_some_and(|s| s.start <= i && i < s.end) {
            continue;
        }
        match c {
            b'(' | b'[' | b'{' => {
                out.push(Bracket { pos: i, depth: stack.len() as u16, partner: None });
                stack.push((c, out.len() - 1));
            }
            _ => {
                let open = match c {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if stack.last().is_some_and(|(o, _)| *o == open) {
                    let (_, idx) = stack.pop().unwrap_or((0, 0));
                    let me = out.len() as u32;
                    out[idx].partner = Some(me);
                    out.push(Bracket { pos: i, depth: stack.len() as u16, partner: Some(idx as u32) });
                } else {
                    out.push(Bracket { pos: i, depth: stack.len() as u16, partner: None });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_languages() {
        assert_eq!(detect(Path::new("src/main.rs")).id, "rust");
        assert_eq!(detect(Path::new("x/App.TSX")).id, "tsx");
        assert_eq!(detect(Path::new("Makefile")).id, "make");
        assert_eq!(detect(Path::new("weird.zzz")).id, "text");
        assert!(LANGUAGES.len() >= 40);
    }

    #[test]
    fn every_grammar_builds() {
        for l in LANGUAGES.iter().filter(|l| !matches!(l.grammar, Grammar::None | Grammar::Toml)) {
            assert!(ts_config(l.grammar).is_some(), "грамматика {} не собралась", l.id);
        }
    }

    #[test]
    fn rust_spans_have_keywords_and_strings() {
        let mut h = Highlighter::default();
        let src = "fn main() { let s = \"hi\"; } // done";
        let spans = h.highlight(by_id("rust").unwrap_or(&LANGUAGES[0]), src);
        let kinds: Vec<&str> = spans.iter().map(|s| CLASS_NAMES[s.class as usize]).collect();
        assert!(kinds.contains(&"keyword"), "{kinds:?}");
        assert!(kinds.contains(&"string"), "{kinds:?}");
        assert!(kinds.contains(&"comment"), "{kinds:?}");
    }

    #[test]
    fn toml_highlighting() {
        let s = toml_spans("[ui]\nname = \"x\" # c\nn = 1\n");
        let classes: Vec<&str> = s.iter().map(|s| CLASS_NAMES[s.class as usize]).collect();
        assert_eq!(classes, vec!["type", "function", "punctuation", "string", "comment", "function", "punctuation", "number"]);
    }

    #[test]
    fn brackets_skip_strings_and_pair_up() {
        let text = "f(a[\")\"], {b})";
        let spans = vec![Span { start: 4, end: 7, class: class_of("string") }];
        let b = brackets(text, &spans);
        assert_eq!(b.len(), 6);
        assert_eq!(b[0].partner, Some(5));
        assert_eq!(b[1].depth, 1);
    }
}
