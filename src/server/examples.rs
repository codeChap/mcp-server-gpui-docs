use crate::index::{Corpus, Doc};

pub(super) const BODY_LIMIT: usize = 24_000;

pub(super) fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}…\n\n[truncated — call get with this id for the full file]",
        &s[..end]
    )
}

pub(super) fn example_payload(corpus: &Corpus, name: &str) -> Result<String, String> {
    let stem = example_stem(name);
    if stem.is_empty() {
        return Err("Empty example name".into());
    }
    let q = stem.to_lowercase();
    let matches: Vec<_> = corpus
        .examples()
        .into_iter()
        .filter(|d| d.id.to_lowercase().contains(&q) || d.title.to_lowercase().contains(&q))
        .collect();
    if matches.is_empty() {
        return Err(format!("No example matching {name:?}"));
    }
    let exact: Vec<_> = matches
        .iter()
        .copied()
        .filter(|d| {
            d.path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(&format!("{stem}.rs")))
        })
        .collect();
    match exact.as_slice() {
        [one] => Ok(format_example(one)),
        [] if matches.len() == 1 => Ok(format_example(matches[0])),
        [] => Err(ambiguous(&matches)),
        _ => Err(ambiguous(&exact)),
    }
}

fn ambiguous(docs: &[&Doc]) -> String {
    let list: Vec<_> = docs.iter().map(|d| d.id.as_str()).collect();
    format!(
        "Multiple examples:\n{}\nCall get with one id.",
        list.join("\n")
    )
}

fn example_stem(name: &str) -> String {
    let n = name.trim();
    let lower = n.to_lowercase();
    if lower.ends_with(".rs") {
        n[..n.len() - 3].to_string()
    } else {
        n.to_string()
    }
}

fn format_example(d: &Doc) -> String {
    format!(
        "# {}\n# {}\n\n{}",
        d.id,
        d.path.display(),
        clip(&d.body, BODY_LIMIT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Doc;
    use std::path::PathBuf;

    fn doc(id: &str, title: &str, body: &str) -> Doc {
        Doc::new(id, "zed-gpui", "example", title, PathBuf::from(id), body)
    }

    fn corpus(docs: Vec<Doc>) -> Corpus {
        Corpus {
            docs,
            missing: Vec::new(),
        }
    }

    #[test]
    fn clip_does_not_panic_on_multibyte() {
        let s = "é".repeat(200);
        let out = clip(&s, 10);
        assert!(out.contains('…'));
        assert!(out.is_char_boundary(out.find('…').unwrap()));
    }

    #[test]
    fn clip_short_unchanged() {
        assert_eq!(clip("hi", 10), "hi");
    }

    #[test]
    fn example_stem_strips_rs() {
        let c = corpus(vec![doc(
            "zed-gpui/crates/gpui/examples/hello_world.rs",
            "hello world",
            "fn main() {}",
        )]);
        let msg = example_payload(&c, "hello_world.rs").unwrap();
        assert!(msg.contains("hello_world.rs"));
        let msg = example_payload(&c, "hello_world").unwrap();
        assert!(msg.contains("hello_world.rs"));
    }

    #[test]
    fn example_ambiguous_without_exact_filename() {
        let c = corpus(vec![
            doc("a/examples/foo_hello.rs", "foo hello", "a"),
            doc("b/examples/bar_hello.rs", "bar hello", "b"),
        ]);
        let err = example_payload(&c, "hello").unwrap_err();
        assert!(err.starts_with("Multiple examples:"));
    }

    #[test]
    fn empty_example_name() {
        let c = corpus(vec![]);
        assert_eq!(example_payload(&c, "  ").unwrap_err(), "Empty example name");
    }
}
