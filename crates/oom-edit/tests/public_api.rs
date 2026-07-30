use oom_edit::{Args, ParseOutcome};

#[test]
fn oom_edit_binary_facade_exports_only_args_parse_outcome_and_run() {
    let _: fn(&Args) -> Result<(), Box<dyn std::error::Error>> = oom_edit::run;
    let _ = std::any::TypeId::of::<ParseOutcome>();

    let lib = include_str!("../src/lib.rs");
    let public_uses: Vec<_> = lib
        .lines()
        .filter(|line| line.starts_with("pub use "))
        .collect();
    assert_eq!(public_uses, ["pub use args::{Args, ParseOutcome};"]);
}
