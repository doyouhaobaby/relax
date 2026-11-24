// Run with RELAX_OPT=none to exercise slot address definitions before optimization.
#[test]
fn guarded_or_candidates_share_guard_binding_storage() {
    struct Point(f32, f32);
    enum Event {
        Press,
        Move,
        Release,
    }

    #[inline(never)]
    fn sum(point: &Point) -> f32 {
        point.0 + point.1
    }

    #[inline(never)]
    fn handle(event: &Event, point: Option<Point>) -> f32 {
        match event {
            Event::Press => -1.0,
            Event::Move | Event::Release if let Some(point) = point => sum(&point),
            _ => 0.0,
        }
    }

    // The first guard is skipped when the second alternative matches.
    assert_eq!(handle(&Event::Release, Some(Point(2.0, 3.0))), 5.0);
    assert_eq!(handle(&Event::Move, Some(Point(3.0, 4.0))), 7.0);
    assert_eq!(handle(&Event::Release, None), 0.0);
    assert_eq!(handle(&Event::Press, Some(Point(2.0, 3.0))), -1.0);
}
