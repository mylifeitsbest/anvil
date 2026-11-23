//! Минимальный разбор Markdown из hover: текст и блоки кода.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text(String),
    Code { lang: String, text: String },
    Rule,
}

fn clean_inline(s: &str) -> String {
    let mut out = s.replace("**", "").replace("__", "").replace('`', "");
    // [текст](ссылка) → текст
    while let (Some(a), Some(m)) = (out.find('['), out.find("](")) {
        if m <= a {
            break;
        }
        let Some(close) = out[m..].find(')').map(|x| m + x) else { break };
        let text = out[a + 1..m].to_string();
        out.replace_range(a..=close, &text);
    }
    out.replace("\\_", "_").replace("\\*", "*").replace("&nbsp;", " ")
}

pub fn parse(md: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut code: Option<(String, String)> = None;
    let flush = |text: &mut String, out: &mut Vec<Block>| {
        let t = text.trim().to_string();
        if !t.is_empty() {
            out.push(Block::Text(t));
        }
        text.clear();
    };
    for line in md.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            match code.take() {
                Some((lang, body)) => out.push(Block::Code { lang, text: body.trim_end().to_string() }),
                None => {
                    flush(&mut text, &mut out);
                    code = Some((rest.trim().to_string(), String::new()));
                }
            }
            continue;
        }
        if let Some((_, body)) = code.as_mut() {
            body.push_str(line);
            body.push('\n');
            continue;
        }
        if trimmed == "---" || trimmed == "***" {
            flush(&mut text, &mut out);
            out.push(Block::Rule);
            continue;
        }
        let l = trimmed.trim_start_matches('#').trim_start();
        if l.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
        } else {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push(' ');
            }
            text.push_str(&clean_inline(l));
        }
    }
    if let Some((lang, body)) = code {
        out.push(Block::Code { lang, text: body.trim_end().to_string() });
    }
    flush(&mut text, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_and_text() {
        let md = "```rust\nfn len(&self) -> usize\n```\n---\nReturns the **length**.\nSee [docs](http://x).";
        let b = parse(md);
        assert_eq!(b[0], Block::Code { lang: "rust".into(), text: "fn len(&self) -> usize".into() });
        assert_eq!(b[1], Block::Rule);
        assert_eq!(b[2], Block::Text("Returns the length. See docs.".into()));
    }
}
