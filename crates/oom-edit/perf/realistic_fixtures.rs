//! Deterministic Markdown shapes for cold public-pane performance measurements.

#[allow(dead_code)]
#[path = "../../oom-edit-core/perf/fixtures.rs"]
mod existing;

pub const VERSION: &str = "oom-edit-realistic-v1";

pub const CLASSES: &[&str] = &[
    "prose",
    "mixed",
    "mixed-reference",
    "rust-fence",
    "many-fences",
    "lists",
    "tables",
];

const PROSE_LINE: &str = "A paragraph with *emphasis*, **strong text**, an [ordinary link](https://example.invalid/path), and `inline code` to exercise Markdown provenance across wrapping.\n";

pub fn generate(class: &str, bytes: usize) -> String {
    assert!(bytes >= 4096, "fixture must be at least 4 KiB");
    match class {
        "prose" => existing::seeded_markdown_fixture(bytes, 0x0a11_ce03),
        "mixed" => {
            let blocks = [
                mixed_unit("rust", "fn answer() -> u32 { 42 }"),
                mixed_unit("go", "package main\nfunc answer() int { return 42 }"),
                mixed_unit("python", "def answer():\n    return 42"),
                mixed_unit("sh", "printf 'ready\\n'"),
            ];
            repeat_blocks("---\ntitle: Performance fixture\n---\n\n", &blocks, bytes)
        }
        "mixed-reference" => {
            let blocks = [
                mixed_unit("rust", "fn answer() -> u32 { 42 }"),
                mixed_unit("go", "package main\nfunc answer() int { return 42 }"),
                mixed_unit("python", "def answer():\n    return 42"),
                mixed_unit("sh", "printf 'ready\\n'"),
            ]
            .map(|block| block.replace("[a link](https://example.invalid)", "[shared]"));
            repeat_blocks(
                "---\ntitle: Performance fixture\n---\n\n[shared]: https://example.invalid/path\n\n",
                &blocks,
                bytes,
            )
        }
        "many-fences" => repeat_blocks("# Many code blocks\n\n", &many_fence_blocks(), bytes),
        "lists" => repeat_blocks("# List fixture\n\n", &["- alpha **bold** item\n- beta [linked](https://example.invalid) item\n- gamma `code` item\n- delta ordinary item\n\n"], bytes),
        "tables" => repeat_blocks("# Table fixture\n\n", &["| One | Two | Three |\n| :--- | :---: | ---: |\n| alpha | beta | gamma |\n| delta | epsilon | zeta |\n\n"], bytes),
        "rust-fence" => one_rust_fence(bytes),
        _ => panic!("unknown fixture class: {class}"),
    }
}

fn repeat_blocks(prefix: &str, blocks: &[impl AsRef<str>], bytes: usize) -> String {
    let mut text = String::with_capacity(bytes);
    text.push_str(prefix);
    for block in blocks.iter().cycle() {
        let block = block.as_ref();
        if text.len() + block.len() > bytes {
            break;
        }
        text.push_str(block);
    }
    text.extend(std::iter::repeat_n('\n', bytes - text.len()));
    assert_eq!(text.len(), bytes);
    text
}

fn mixed_unit(language: &str, code: &str) -> String {
    let mut unit = String::new();
    unit.push_str("## A section with *emphasis* and [a link](https://example.invalid)\n\n");
    unit.push_str(&PROSE_LINE.repeat(9));
    unit.push('\n');
    for index in 0..7 {
        unit.push_str(&format!("- A task {index} with **strong** text, `code`, and a [link](https://example.invalid/next)\n"));
    }
    unit.push('\n');
    unit.push_str("| Name | State | Comment |\n| :--- | :---: | ---: |\n| alpha | ready | **bold** |\n| beta | waiting | `code` |\n\n");
    unit.push_str(&format!("```{language}\n{code}\n```\n\n"));
    unit.push_str(&PROSE_LINE.repeat(6));
    unit.push('\n');
    unit
}

fn many_fence_blocks() -> Vec<String> {
    [
        ("rust", "let answer = 42; println!(\"{answer}\");\n"),
        ("python", "value = 42\nprint(value)\n"),
        ("sh", "printf 'ready\\n'\nvalue=42\n"),
        ("go", "value := 42\nprintln(value)\n"),
        ("text", "plain text with several words\n"),
    ]
    .into_iter()
    .map(|(lang, line)| {
        let mut block = format!("```{lang}\n");
        block.push_str(&line.repeat(7));
        block.push_str("```\n\n");
        block
    })
    .collect()
}

fn one_rust_fence(bytes: usize) -> String {
    let mut text = String::with_capacity(bytes);
    text.push_str("# One Rust fence\n\n```rust\nfn main() {\n");
    let line = "    let answer = 42; println!(\"{answer}\");\n";
    let suffix = "}\n```\n";
    while text.len() + line.len() + suffix.len() <= bytes {
        text.push_str(line);
    }
    let padding = bytes - text.len() - suffix.len();
    text.extend(std::iter::repeat_n(' ', padding));
    text.push_str(suffix);
    assert_eq!(text.len(), bytes);
    text
}

pub fn identity(class: &str, text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{VERSION}:{class}:{}:{hash:016x}", text.len())
}
