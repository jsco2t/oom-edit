use oom_edit::{ColorValue, OwnedStyle, PaneCell, PaneCursor, PaneCursorShape, PaneFrame};

#[test]
fn owned_frame_facade_has_no_terminal_types() {
    let frame = PaneFrame::blank(2, 1);
    let _: std::sync::Arc<Vec<PaneCell>> = frame.cells.clone();
    assert_eq!((frame.width, frame.height, frame.cells.len()), (2, 1, 2));
    assert_eq!(frame.cell(0, 0).unwrap().symbol, " ");
    assert!(frame.cell(2, 0).is_none());
    let _ = std::any::TypeId::of::<PaneCell>();
    let _ = std::any::TypeId::of::<PaneCursor>();
    let _ = std::any::TypeId::of::<PaneCursorShape>();
    let _ = std::any::TypeId::of::<OwnedStyle>();
    let _ = std::any::TypeId::of::<ColorValue>();
}
