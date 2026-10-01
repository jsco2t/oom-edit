//! Diagnostic phase probe for the exact large-note acceptance fixture.

use std::error::Error;
use std::fs;
use std::hint::black_box;
use std::time::Instant;

use oom_edit_core::{EditorSession, KeyCode, KeyCodeKind, KeyInput, Mode, Modifiers};

fn key(kind: KeyCodeKind) -> KeyInput {
    KeyInput {
        code: KeyCode { kind },
        mods: Modifiers::default(),
    }
}

fn line_offset(text: &str, line: usize) -> usize {
    text.split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>()
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        2,
        "usage: performance_acceptance_1mb_core FIXTURE LINE"
    );
    let text = fs::read_to_string(&args[0])?;
    assert_eq!(text.len(), 1_048_722);
    let line = args[1].parse::<usize>()?;
    let started = Instant::now();
    let mut session = EditorSession::from_text(&text);
    let source_init_ns = started.elapsed().as_nanos();
    let started = Instant::now();
    black_box(session.rendered_layout_mut(100));
    let full_layout_ns = started.elapsed().as_nanos();
    black_box(session.jump_to_offset(line_offset(&text, line))?);
    assert_eq!(session.mode(), Mode::Normal);
    let started = Instant::now();
    black_box(session.handle_key(key(KeyCodeKind::Char('v'))));
    let select_entry_ns = started.elapsed().as_nanos();
    assert_eq!(session.mode(), Mode::Select);
    println!("CORE\t{line}\tsource-init\t{source_init_ns}");
    println!("CORE\t{line}\tfull-layout\t{full_layout_ns}");
    println!("CORE\t{line}\tselect-entry\t{select_entry_ns}");
    for index in 0..15 {
        let started = Instant::now();
        black_box(session.handle_key(key(KeyCodeKind::Char('j'))));
        let handler_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        black_box(session.rendered_selection());
        let selection_ns = started.elapsed().as_nanos();
        println!("CORE\t{line}\tmotion\t{index}\t{handler_ns}\t{selection_ns}");
    }
    Ok(())
}
