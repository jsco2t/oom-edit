//! Deterministic seeded fixtures shared by debug smoke tests and release benches.

const SOURCE_LINE: &str =
    "source viewport line with markdown *content* and unicode λ 0123456789 repeated prose keeps the deterministic one-mebibyte fixture near ten thousand physical lines while exercising syntax and wrapping behavior.\n\n";

/// Build an exact-size Markdown document with deterministic seed-derived variation.
pub(crate) fn seeded_markdown_fixture(bytes: usize, seed: u64) -> String {
    let digit = char::from(b'0' + (seed % 10) as u8);
    let line = SOURCE_LINE.replacen('0', &digit.to_string(), 1);
    let mut text = String::with_capacity(bytes);
    while text.len() < bytes {
        let remaining = bytes - text.len();
        if line.len() > remaining {
            text.push_str(&"x".repeat(remaining));
        } else {
            text.push_str(&line);
        }
    }
    assert_eq!(text.len(), bytes);
    text
}

/// Build a deterministic rendered-layout fixture with exactly `line_count` lines.
pub(crate) fn seeded_rendered_fixture(line_count: usize, seed: u64) -> String {
    let seeded_word = if seed & 1 == 0 { "prose" } else { "words" };
    let text = (0..line_count)
        .map(|line| match line % 5 {
            0 => format!("## Heading {line}"),
            1 => {
                format!("Paragraph {line} has enough {seeded_word} to exercise wrapping.")
            }
            2 => format!("- list item {line}"),
            3 => format!("> quote {line}"),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(text.matches('\n').count() + 1, line_count);
    text
}
