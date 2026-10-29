#[path = "../src/overlay_geometry.rs"]
mod overlay_geometry;

use limit_core::settings::OverlayPosition;
use overlay_geometry::{placement, DragState, WorkArea, OVERLAY_SIZE};

fn screen(x: i32, scale: f64) -> WorkArea {
    WorkArea {
        x,
        y: 0,
        width: 1920,
        height: 1040,
        scale,
    }
}

#[test]
fn restores_saved_position_instead_of_system_displacement() {
    let saved = OverlayPosition { x: 1500, y: 700 };
    let (position, size) = placement(Some(saved), &[screen(0, 1.0)], 0).unwrap();
    assert_eq!(position, saved);
    assert_eq!(size, (176, 176));
}

#[test]
fn disconnected_monitor_fallback_does_not_replace_preferred_position() {
    let saved = Some(OverlayPosition { x: -1500, y: 400 });
    let primary = screen(0, 1.0);
    assert_eq!(
        placement(saved, &[primary], 0).unwrap().0,
        OverlayPosition { x: 1712, y: 40 }
    );
    assert_eq!(
        placement(saved, &[primary, screen(-1920, 1.0)], 0)
            .unwrap()
            .0,
        saved.unwrap()
    );
}

#[test]
fn restores_full_logical_size_at_destination_dpi() {
    let saved = OverlayPosition { x: 2200, y: 200 };
    let (position, size) = placement(Some(saved), &[screen(0, 1.0), screen(1920, 1.5)], 0).unwrap();
    assert_eq!(position, saved);
    assert_eq!(size, (264, 264));
    assert_eq!(OVERLAY_SIZE, 176.0);
}

#[test]
fn mixed_dpi_edge_uses_largest_intersection() {
    let saved = OverlayPosition { x: 1870, y: 200 };
    assert_eq!(
        placement(Some(saved), &[screen(0, 1.0), screen(1920, 1.5)], 0).unwrap(),
        (saved, (264, 264))
    );
}

#[test]
fn keeps_user_placement_across_monitor_edges() {
    let saved = OverlayPosition { x: -100, y: 300 };
    assert_eq!(
        placement(Some(saved), &[screen(0, 1.0)], 0).unwrap().0,
        saved
    );
}

#[test]
fn fallback_fits_small_work_area_and_uses_its_dpi() {
    let work = WorkArea {
        x: -200,
        y: -100,
        width: 200,
        height: 200,
        scale: 1.0,
    };
    assert_eq!(
        placement(None, &[work], 0).unwrap(),
        (OverlayPosition { x: -200, y: -76 }, (176, 176))
    );
}

#[test]
fn missing_display_does_not_invent_a_new_position() {
    assert_eq!(
        placement(Some(OverlayPosition { x: 50, y: 60 }), &[], 0),
        None
    );
}

#[test]
fn only_completed_user_drags_can_save_position() {
    let mut drag = DragState::default();
    assert!(!drag.active());
    assert_eq!(drag.finish(), None);
    drag.begin();
    assert!(drag.active());
    assert!(drag.finish().is_some());
    assert_eq!(drag.finish(), None);
}

#[test]
fn display_change_during_drag_cannot_save_system_position() {
    let mut drag = DragState::default();
    drag.begin();
    drag.interrupt();
    assert_eq!(drag.finish(), None);
}

#[test]
fn recovery_waits_until_completed_drag_has_been_saved() {
    let mut drag = DragState::default();
    drag.begin();
    let revision = drag.finish().unwrap();
    assert!(drag.busy());
    assert!(drag.is_current(revision));
    drag.saved();
    assert!(!drag.busy());
}

#[test]
fn older_save_cannot_override_newer_drag_or_resume_recovery_early() {
    let mut drag = DragState::default();
    drag.begin();
    let first = drag.finish().unwrap();
    drag.begin();
    let second = drag.finish().unwrap();
    assert!(!drag.is_current(first));
    assert!(drag.is_current(second));
    drag.saved();
    assert!(drag.busy());
    drag.saved();
    assert!(!drag.busy());
}
