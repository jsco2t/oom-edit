use super::*;

fn assert_fresh_source(incremental: &Highlighter, viewports: &[Range<usize>]) {
    let fresh = Highlighter::new(incremental.text());
    assert_eq!(incremental.line_starts, fresh.line_starts);
    assert_eq!(
        incremental.spell_reference_labels,
        fresh.spell_reference_labels
    );
    assert_eq!(
        incremental.spell_front_matter_span,
        fresh.spell_front_matter_span
    );
    assert_eq!(
        incremental.spell_block_exclusion_ranges(0..incremental.text().len()),
        fresh.spell_block_exclusion_ranges(0..fresh.text().len())
    );
    let injection_key = |injection: &Injection| {
        (
            injection.start,
            injection.end,
            injection.language_name,
            injection.kind,
        )
    };
    assert_eq!(
        incremental
            .injections
            .iter()
            .map(injection_key)
            .collect::<Vec<_>>(),
        fresh
            .injections
            .iter()
            .map(injection_key)
            .collect::<Vec<_>>()
    );
    for viewport in viewports {
        assert_eq!(
            incremental.highlight_lines(viewport.clone()),
            fresh.highlight_lines(viewport.clone()),
            "viewport {viewport:?}"
        );
    }
}

#[test]
fn prefix_tree_coordinates_include_only_the_following_ascii_anchor() {
    let mut text = "First café\n\nSecond\n=\n".to_string();
    let mut starts = line_start_indices(&text);
    for start in [0, text.find("Second").unwrap() + 3] {
        let edit = TextEdit {
            range: start..start,
            new_text_len: 3,
            new_text: "New".into(),
        };
        assert!(edit_is_provably_block_neutral(&text, &edit));
        let logical = input_edit_indexed(&starts, &edit).unwrap();
        let anchored = block_edit_indexed(&starts, &edit, true).unwrap();
        assert_eq!(anchored.start_byte, logical.start_byte);
        assert_eq!(anchored.start_position, logical.start_position);
        assert_eq!(anchored.old_end_byte, start + 1);
        assert_eq!(anchored.new_end_byte, start + 4);
        assert_eq!(anchored.old_end_position.row, logical.start_position.row);
        assert_eq!(anchored.old_end_position.column, 1);
        assert_eq!(anchored.new_end_position.row, logical.start_position.row);
        assert_eq!(anchored.new_end_position.column, 4);
        let canonical = block_edit_indexed(&starts, &edit, false).unwrap();
        assert_eq!(canonical.old_end_byte, start);
        assert_eq!(canonical.new_end_byte, start + 3);
        apply_edit_to_line_starts(&mut starts, &edit);
        apply_edit_to_string(&mut text, &edit);
        assert_eq!(starts, line_start_indices(&text));
    }
    assert_eq!(text, "NewFirst café\n\nNewSecond\n=\n");
}

#[test]
fn structural_and_adjacent_prefix_edits_keep_the_parse_path() {
    for (original, start, prefix) in [
        ("First\nSecond\n=\n", 6, "New"),
        ("Title\n=\n", 0, "# "),
        ("Title\n=\n", 0, "\n"),
        ("[ref]: /target\n", 0, "New"),
        ("\n", 0, "New"),
        ("", 0, "New"),
        ("éclair\n", 0, "New"),
    ] {
        let mut highlighter = Highlighter::new(original);
        highlighter.apply_edit(&[TextEdit {
            range: start..start,
            new_text_len: prefix.len(),
            new_text: prefix.into(),
        }]);
        assert_fresh_source(&highlighter, std::slice::from_ref(&(0..usize::MAX)));
        assert_eq!(highlighter.last_parse_path, ParsePath::Incremental);
    }
}

#[test]
fn prefix_edits_preserve_ownership_without_document_scans() {
    for original in [
        "Title\n=\n",
        "Title\n---\n",
        "\nTitle\n=\n",
        "before\n\nTitle\n---\n",
        "Title *emphasis*\nsecond café line\n=\n",
        "Title **strong** [link](url)\n\nnext\n",
        "---\ntitle: metadata\n---\nTitle\n=\n",
        "+++\ntitle = 'metadata'\n+++\nTitle\n---\n",
    ] {
        let mut highlighter = Highlighter::new(original);
        let start = original.find("Title").unwrap();
        let counts = (
            highlighter.block_parse_count.get(),
            highlighter.reference_scan_count,
            highlighter.injection_scan_count,
        );
        for edit in [
            TextEdit {
                range: start..start,
                new_text_len: 3,
                new_text: "New".into(),
            },
            TextEdit {
                range: start + 3..start,
                new_text_len: 0,
                new_text: String::new(),
            },
            TextEdit {
                range: start..start,
                new_text_len: 3,
                new_text: "New".into(),
            },
        ] {
            highlighter.apply_edit(std::slice::from_ref(&edit));
            assert_fresh_source(&highlighter, std::slice::from_ref(&(0..usize::MAX)));
            assert_eq!(
                highlighter.last_parse_path,
                ParsePath::BlockNeutral,
                "{original:?}, {edit:?}"
            );
            assert_eq!(
                (
                    highlighter.block_parse_count.get(),
                    highlighter.reference_scan_count,
                    highlighter.injection_scan_count
                ),
                counts,
                "prefix edit must not scan the document: {original:?}"
            );
        }
    }
}

#[test]
fn large_prefix_edits_keep_remote_injection_cache_and_local_work() {
    for bytes in [64 * 1024 - 1, 64 * 1024, 64 * 1024 + 1, 1024 * 1024] {
        let row = "Alpha *word* café.\n\n";
        let tail = "\n\n```rust\nfn retained() {}\n```\n\n[ref]: /target\n\nLast.\n";
        let mut original = row.repeat((bytes - tail.len()) / row.len());
        original.push_str(&" ".repeat(bytes - tail.len() - original.len()));
        original.push_str(tail);
        assert_eq!(original.len(), bytes);
        let mut highlighter = Highlighter::new(&original);
        let lines = highlighter.line_starts.len();
        let remote = lines.saturating_sub(9)..lines;
        highlighter.highlight_lines(remote.clone());
        let cached_root = highlighter.source_parse_cache.borrow()[0]
            .tree
            .root_node()
            .id();
        let parses = highlighter.source_injection_parse_count.get();
        let counts = (
            highlighter.block_parse_count.get(),
            highlighter.reference_scan_count,
            highlighter.injection_scan_count,
        );
        let middle = (lines / 2) & !1;
        for line in [0, middle] {
            let start = highlighter.line_starts[line];
            assert!(highlighter.text()[start..].starts_with("Alpha"));
            for edit in [
                TextEdit {
                    range: start..start,
                    new_text_len: 3,
                    new_text: "New".into(),
                },
                TextEdit {
                    range: start + 3..start,
                    new_text_len: 0,
                    new_text: String::new(),
                },
                TextEdit {
                    range: start..start,
                    new_text_len: 3,
                    new_text: "New".into(),
                },
                TextEdit {
                    range: start..start + 3,
                    new_text_len: 0,
                    new_text: String::new(),
                },
            ] {
                highlighter.apply_edit(&[edit]);
                assert_fresh_source(&highlighter, &[0..6, middle..middle + 6, remote.clone()]);
                assert_eq!(highlighter.last_parse_path, ParsePath::BlockNeutral);
                assert_eq!(
                    (
                        highlighter.block_parse_count.get(),
                        highlighter.reference_scan_count,
                        highlighter.injection_scan_count
                    ),
                    counts
                );
                assert_eq!(highlighter.source_injection_parse_count.get(), parses);
                assert_eq!(
                    highlighter.source_parse_cache.borrow()[0]
                        .tree
                        .root_node()
                        .id(),
                    cached_root
                );
            }
        }
        assert_eq!(highlighter.text(), original);
    }
}

#[test]
fn sequential_prefix_batch_preserves_points_and_both_heading_owners() {
    let original = "First café\n=\n\nSecond *word*\n---\n";
    let second = original.find("Second").unwrap();
    let mut highlighter = Highlighter::new(original);
    highlighter.apply_edit(&[
        TextEdit {
            range: 0..0,
            new_text_len: 3,
            new_text: "New".into(),
        },
        TextEdit {
            range: second + 3..second + 3,
            new_text_len: 3,
            new_text: "New".into(),
        },
    ]);
    assert_eq!(
        highlighter.text(),
        "NewFirst café\n=\n\nNewSecond *word*\n---\n"
    );
    assert_fresh_source(&highlighter, std::slice::from_ref(&(0..usize::MAX)));
    assert_eq!(highlighter.last_parse_path, ParsePath::BlockNeutral);
    highlighter.apply_edit(&[
        TextEdit {
            range: Range { start: 3, end: 0 },
            new_text_len: 0,
            new_text: String::new(),
        },
        TextEdit {
            range: second + 3..second,
            new_text_len: 0,
            new_text: String::new(),
        },
    ]);
    assert_eq!(highlighter.text(), original);
    assert_fresh_source(&highlighter, std::slice::from_ref(&(0..usize::MAX)));
}

#[test]
fn line_start_insertion_preserves_setext_heading_spans() {
    for original in [
        "Title\n=\n",
        "Title\n---\n",
        "\nTitle\n=\n",
        "before\n\nTitle\n---\n",
        "Title\nsecond line\n=\n",
        "Title café\n=\n",
    ] {
        let mut highlighter = Highlighter::new(original);
        let start = original.find("Title").unwrap();
        for edit in [
            TextEdit {
                range: start..start,
                new_text_len: 3,
                new_text: "New".into(),
            },
            TextEdit {
                range: start + 3..start,
                new_text_len: 0,
                new_text: String::new(),
            },
            TextEdit {
                range: start..start,
                new_text_len: 3,
                new_text: "New".into(),
            },
        ] {
            highlighter.apply_edit(std::slice::from_ref(&edit));
            assert_eq!(
                highlighter.highlight_lines(0..100),
                Highlighter::new(highlighter.text()).highlight_lines(0..100),
                "source {original:?}, edit {edit:?}"
            );
        }
    }
}

#[test]
fn saved_edit_sequence_matches_fresh_highlighting() {
    let original = "K`\"U\n\n%i\n=_$\nNE\n'=Q{$Xk\\Pt\n\n \n^??\n?.d:\";Ac(<\\O/D\n'5\nTC\n;PP\"q<\n$%\n*h\n%\n.5V\"Z`\n\nH(2*(\n.z:d.c:<U'\n\nH&5 \n&4X?wh=<\nZV:\n^\n\"45\n\n\nV~\n@an_\n`'\n \nR.`\".k6%\n=\nt\n\njK`*\n/&LUl\\%\n{=\n\njN\n[6\n\n\n&D}<\nwM:V=<phSz\nY<ISO.\nbD\n$*\n\n}-E]T=*[:_/AKwD[\nVC\n:P\ne`AaF//V]<Y!I<t%&{+;/[_\njd<";
    let operations: &[&[u8]] = &[
        &[243, 41, 220, 79],
        &[88, 105],
        &[60, 145],
        &[48],
        &[181, 124, 87],
        &[42, 242],
        &[155],
    ];
    let mut incremental = Highlighter::new(original);
    let mut current = original.to_owned();
    for (index, byte) in operations
        .iter()
        .flat_map(|group| group.iter())
        .copied()
        .enumerate()
    {
        let start = usize::from(byte) % (current.len() + 1);
        let end =
            (start + usize::from(byte.wrapping_add(10)) % current.len().max(1)).min(current.len());
        let text: String = (0..usize::from(byte.wrapping_add(1)) % 10)
            .map(|i| {
                let value = byte.wrapping_add(i as u8);
                if value.is_multiple_of(5) {
                    '\n'
                } else {
                    char::from(value % 95 + 32)
                }
            })
            .collect();
        let range = if byte % 2 == 0 {
            start..end
        } else {
            end..start
        };
        let edit = TextEdit {
            range,
            new_text_len: text.len(),
            new_text: text.clone(),
        };
        incremental.apply_edit(std::slice::from_ref(&edit));
        current.replace_range(start..end, &text);
        let fresh = Highlighter::new(&current);
        assert_eq!(incremental.highlight_lines(0..1000), fresh.highlight_lines(0..1000),
            "edit {index}, byte {byte}, edit {edit:?}, path {:?}, text {current:?}, incremental tree {}, fresh tree {}",
            incremental.last_parse_path, incremental.md_tree.root_node().to_sexp(), fresh.md_tree.root_node().to_sexp());
    }
}
