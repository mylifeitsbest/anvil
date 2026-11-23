//! Сниппеты LSP (`foo(${1:arg})$0`) превращаем в обычный текст и позицию курсора.

/// Возвращает текст без разметки и позицию курсора (в символах) — у первой вкладки или `$0`.
pub fn expand(s: &str) -> (String, Option<usize>) {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut n = 0usize;
    let mut first: Option<(u32, usize)> = None;
    let mut zero: Option<usize> = None;
    let note = |idx: u32, at: usize, first: &mut Option<(u32, usize)>, zero: &mut Option<usize>| {
        if idx == 0 {
            zero.get_or_insert(at);
        } else if first.is_none_or(|(i, _)| idx < i) {
            *first = Some((idx, at));
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            out.push(chars[i + 1]);
            n += 1;
            i += 2;
            continue;
        }
        if c == '$' && i + 1 < chars.len() {
            let nx = chars[i + 1];
            if nx.is_ascii_digit() {
                let mut j = i + 1;
                let mut idx = 0u32;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    idx = idx * 10 + chars[j].to_digit(10).unwrap_or(0);
                    j += 1;
                }
                note(idx, n, &mut first, &mut zero);
                i = j;
                continue;
            }
            if nx == '{' {
                let mut j = i + 2;
                let mut idx = 0u32;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    idx = idx * 10 + chars[j].to_digit(10).unwrap_or(0);
                    j += 1;
                }
                note(idx, n, &mut first, &mut zero);
                // ${1:текст}, ${1|a,b|} или ${1}
                let mut depth = 1;
                let mut body = String::new();
                if j < chars.len() && (chars[j] == ':' || chars[j] == '|') {
                    let choice = chars[j] == '|';
                    j += 1;
                    while j < chars.len() && depth > 0 {
                        match chars[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        if depth > 0 {
                            body.push(chars[j]);
                        }
                        j += 1;
                    }
                    if choice {
                        body = body.trim_end_matches('|').split(',').next().unwrap_or_default().to_string();
                    }
                    let (inner, _) = expand(&body);
                    n += inner.chars().count();
                    out.push_str(&inner);
                } else {
                    while j < chars.len() && chars[j] != '}' {
                        j += 1;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        out.push(c);
        n += 1;
        i += 1;
    }
    (out, first.map(|(_, at)| at).or(zero))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_and_cursor() {
        assert_eq!(expand("println!(\"$1\")$0"), ("println!(\"\")".into(), Some(10)));
        assert_eq!(expand("foo(${1:a}, ${2:b})$0"), ("foo(a, b)".into(), Some(4)));
        assert_eq!(expand("len()$0"), ("len()".into(), Some(5)));
        assert_eq!(expand("plain"), ("plain".into(), None));
        assert_eq!(expand("${1|one,two|}"), ("one".into(), Some(0)));
        assert_eq!(expand("a\\$b"), ("a$b".into(), None));
    }
}
