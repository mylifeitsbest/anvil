//! Поиск и замена (обычный текст или regex) по всему документу.

use regex::{Regex, RegexBuilder};
use ropey::Rope;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

pub fn build_regex(query: &str, opts: SearchOptions) -> Result<Regex, regex::Error> {
    let pat = if opts.regex { query.to_string() } else { regex::escape(query) };
    let pat = if opts.whole_word { format!(r"\b(?:{pat})\b") } else { pat };
    RegexBuilder::new(&pat).case_insensitive(!opts.case_sensitive).multi_line(true).size_limit(1 << 24).build()
}

/// Все совпадения как диапазоны символов. Пустые совпадения пропускаются.
pub fn find_all(rope: &Rope, query: &str, opts: SearchOptions, limit: usize) -> Result<Vec<(usize, usize)>, regex::Error> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = build_regex(query, opts)?;
    let text = rope.to_string();
    let mut out = Vec::new();
    for m in re.find_iter(&text) {
        if m.start() == m.end() {
            continue;
        }
        out.push((rope.byte_to_char(m.start()), rope.byte_to_char(m.end())));
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// Замены для «заменить всё»: в режиме regex поддерживаются `$1`, `${name}`.
pub fn replacements(rope: &Rope, query: &str, replacement: &str, opts: SearchOptions) -> Result<Vec<(usize, usize, String)>, regex::Error> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = build_regex(query, opts)?;
    let text = rope.to_string();
    let mut out = Vec::new();
    for caps in re.captures_iter(&text) {
        let Some(m) = caps.get(0) else { continue };
        if m.start() == m.end() {
            continue;
        }
        let mut dst = String::new();
        if opts.regex {
            caps.expand(replacement, &mut dst);
        } else {
            dst.push_str(replacement);
        }
        out.push((rope.byte_to_char(m.start()), rope.byte_to_char(m.end()), dst));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_case() {
        let r = Rope::from_str("Foo foo FOO");
        assert_eq!(find_all(&r, "foo", SearchOptions::default(), 10).map(|v| v.len()).ok(), Some(3));
        let cs = SearchOptions { case_sensitive: true, ..Default::default() };
        assert_eq!(find_all(&r, "foo", cs, 10).ok(), Some(vec![(4, 7)]));
    }

    #[test]
    fn regex_with_unicode_offsets() {
        let r = Rope::from_str("привет мир, привет");
        let o = SearchOptions { regex: true, ..Default::default() };
        assert_eq!(find_all(&r, "п\\w+", o, 10).ok(), Some(vec![(0, 6), (12, 18)]));
        let rep = replacements(&r, "(п)ривет", "${1}ока", o).unwrap_or_default();
        assert_eq!(rep[0].2, "пока");
    }

    #[test]
    fn whole_word() {
        let r = Rope::from_str("cat concat cat_");
        let o = SearchOptions { whole_word: true, ..Default::default() };
        assert_eq!(find_all(&r, "cat", o, 10).ok(), Some(vec![(0, 3)]));
    }
}
