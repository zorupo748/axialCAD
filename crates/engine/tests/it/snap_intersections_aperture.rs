//! Finite and apparent intersection behavior around the cursor aperture.

use cadcraft_doc::{Common, Drawing, Ellipse, EntityKind, Line, Space};
use cadcraft_engine::snap::{self, SnapHit, mode};
use cadcraft_geom::{Vec2, Vec3};

fn add_line(d: &mut Drawing, a: Vec2, b: Vec2) {
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: a.to3(0.0), b: b.to3(0.0) })).unwrap();
}

fn add_ellipse(d: &mut Drawing, center: Vec2, major: Vec2, ratio: f64) {
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::Ellipse(Ellipse { center: center.to3(0.0), major: major.to3(0.0), ratio, start: 0.0, end: std::f64::consts::TAU }),
    )
    .unwrap();
}

fn hit(d: &Drawing, cursor: Vec2, aperture: f64, osmode: u32) -> Option<SnapHit> {
    snap::osnap(d, &Space::Model, cursor, aperture, osmode, None, false)
}

#[test]
fn finite_intersection_on_the_aperture_boundary_is_included() {
    let mut d = Drawing::new_imperial();
    add_line(&mut d, Vec2::new(-1.0, 0.4), Vec2::new(1.0, 0.4));
    add_line(&mut d, Vec2::new(0.3, -1.0), Vec2::new(0.3, 1.0));

    let h = hit(&d, Vec2::ZERO, 0.5, mode::INT).expect("intersection on aperture boundary");
    assert_eq!(h.mode, mode::INT);
    assert!(h.point.near(Vec2::new(0.3, 0.4), 1e-12), "{h:?}");
}

#[test]
fn finite_intersection_just_outside_the_aperture_is_rejected() {
    let mut d = Drawing::new_imperial();
    let x = 0.300001;
    add_line(&mut d, Vec2::new(-1.0, 0.4), Vec2::new(1.0, 0.4));
    add_line(&mut d, Vec2::new(x, -1.0), Vec2::new(x, 1.0));

    assert!(hit(&d, Vec2::ZERO, 0.5, mode::INT).is_none());
}

#[test]
fn finite_line_parameter_tolerance_is_preserved() {
    let mut d = Drawing::new_imperial();
    let x = 1.0 + 5e-10;
    add_line(&mut d, Vec2::ZERO, Vec2::new(1.0, 0.0));
    add_line(&mut d, Vec2::new(x, -1.0), Vec2::new(x, 1.0));

    let h = hit(&d, Vec2::new(x, 0.0), 1e-9, mode::INT).expect("intersection within the existing line parameter tolerance");
    assert_eq!(h.mode, mode::INT);
    assert!(h.point.near(Vec2::new(x, 0.0), 1e-15), "{h:?}");
}

#[test]
fn tessellated_line_parameter_tolerance_is_preserved() {
    let mut d = Drawing::new_imperial();
    add_ellipse(&mut d, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.5);
    let x = 10.0 + 5e-12;
    add_line(&mut d, Vec2::new(x, -1.0), Vec2::new(x, 1.0));

    let h = hit(&d, Vec2::new(x, 0.0), 1e-10, mode::INT).expect("finite intersection within a tessellated chord's parameter tolerance");
    assert_eq!(h.mode, mode::INT);
    assert!(h.point.near(Vec2::new(x, 0.0), 1e-10), "{h:?}");
}

#[test]
fn large_tessellated_chord_parameter_slack_is_not_pruned() {
    let mut d = Drawing::new_imperial();
    let radius = 1e12;
    let step = std::f64::consts::TAU / cadcraft_geom::arc_segments(radius, std::f64::consts::TAU, radius * 1e-3) as f64;
    let next = Vec2::new(-radius + radius * step.cos(), radius * 0.5 * step.sin());
    let t = 1.0 / next.x;
    assert!(t.abs() < 1e-9);
    let cursor = Vec2::new(1.0, t * next.y);
    add_ellipse(&mut d, Vec2::new(-radius, 0.0), Vec2::new(radius, 0.0), 0.5);
    add_line(&mut d, Vec2::new(1.0, cursor.y - 100.0), Vec2::new(1.0, cursor.y + 100.0));

    let h = hit(&d, cursor, 0.5, mode::INT).expect("finite intersection within a long tessellated chord's parameter tolerance");
    assert_eq!(h.mode, mode::INT);
    assert!(h.point.near(cursor, 1e-6), "{h:?}");
}

#[test]
fn tessellated_ellipse_intersection_near_cursor_is_kept() {
    let mut d = Drawing::new_imperial();
    add_ellipse(&mut d, Vec2::ZERO, Vec2::new(10.0, 0.0), 0.5);
    add_line(&mut d, Vec2::new(9.0, 0.0), Vec2::new(11.0, 0.0));

    let h = hit(&d, Vec2::new(10.0, 0.0), 0.05, mode::INT).expect("ellipse and line intersection");
    assert_eq!(h.mode, mode::INT);
    assert!(h.point.near(Vec2::new(10.0, 0.0), 1e-9), "{h:?}");
}

#[test]
fn ellipse_bounds_covering_cursor_do_not_create_a_curve_hit() {
    let mut d = Drawing::new_imperial();
    for i in 0..12usize {
        let radius = 50.0 + i as f64 * 10.0;
        let major = Vec2::from_angle(i as f64 * 0.17) * radius;
        add_ellipse(&mut d, Vec2::ZERO, major, 0.75 + (i % 5) as f64 * 0.04);
    }

    assert!(hit(&d, Vec2::ZERO, 0.1, mode::INT).is_none());
}

#[test]
fn apparent_intersection_keeps_segments_outside_the_finite_aperture() {
    let mut d = Drawing::new_imperial();
    add_line(&mut d, Vec2::new(0.15, 0.0), Vec2::new(0.18, 0.0));
    add_line(&mut d, Vec2::new(0.0, 0.15), Vec2::new(0.0, 0.18));

    let h = hit(&d, Vec2::ZERO, 0.1, mode::APP).expect("apparent intersection of extended lines");
    assert_eq!(h.mode, mode::APP);
    assert!(h.point.near(Vec2::ZERO, 1e-12), "{h:?}");
}

#[test]
fn real_intersection_keeps_precedence_when_app_is_also_enabled() {
    let mut d = Drawing::new_imperial();
    add_line(&mut d, Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0));
    add_line(&mut d, Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0));
    add_line(&mut d, Vec2::new(0.15, 0.04), Vec2::new(0.18, 0.04));
    add_line(&mut d, Vec2::new(0.04, 0.15), Vec2::new(0.04, 0.18));

    let h = hit(&d, Vec2::ZERO, 0.1, mode::INT | mode::APP).expect("real and apparent intersections");
    assert_eq!(h.mode, mode::INT);
    assert!(h.point.near(Vec2::ZERO, 1e-12), "{h:?}");
}

#[test]
fn line_and_circle_intersections_remain_available() {
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), EntityKind::Circle(cadcraft_doc::Circle { center: Vec3::ZERO, radius: 2.0 })).unwrap();
    add_line(&mut d, Vec2::new(-3.0, 0.0), Vec2::new(3.0, 0.0));
    add_line(&mut d, Vec2::new(0.0, -3.0), Vec2::new(0.0, 3.0));

    let h = hit(&d, Vec2::ZERO, 0.1, mode::INT).expect("crossing lines inside a circle drawing");
    assert_eq!(h.mode, mode::INT);
    assert!(h.point.near(Vec2::ZERO, 1e-12), "{h:?}");
}

#[test]
fn non_finite_query_inputs_return_no_hit() {
    let mut d = Drawing::new_imperial();
    add_line(&mut d, Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0));
    add_line(&mut d, Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0));

    assert!(hit(&d, Vec2::ZERO, f64::NAN, mode::INT).is_none());
    assert!(hit(&d, Vec2::new(f64::INFINITY, 0.0), 0.1, mode::INT).is_none());
}

fn add_wcs_line(d: &mut Drawing, a: Vec3, b: Vec3) {
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a, b })).unwrap();
}

#[test]
fn projected_apparent_ray_uses_the_crossed_lines_depth_and_the_view_aperture() {
    let mut d = Drawing::new_imperial();
    let camera = cadcraft_geom::OrbitCamera { elevation: std::f64::consts::FRAC_PI_4, ..Default::default() };
    let expected = Vec3::new(0.0, 4.0, 8.0);
    add_wcs_line(&mut d, expected - Vec3::X * 5.0, expected + Vec3::X * 5.0);
    let start = Vec3::new(0.0, 0.0, 30.0);
    let toward = start - Vec3::Z;
    let crossing = camera.project_point(expected);
    let query = |offset, aperture, from, to| {
        snap::projected_apparent_line_intersection(&d, &Space::Model, &camera, crossing + Vec2::new(offset, 0.0), aperture, from, to)
    };
    assert!(query(0.126, 0.125, start, toward).is_none());
    let hit = query(0.124, 0.125, start, toward).unwrap();
    assert_eq!(hit.mode, mode::APP);
    let point = hit.point3.unwrap();
    assert!(point.near(expected, 1e-9));
    assert!(camera.project_point(point).near(camera.project_point(Vec3::new(0.0, 0.0, 12.0)), 1e-9));
    assert_eq!(hit.input(), cadcraft_engine::Input::Point3(point));
    let edge = crossing + Vec2::new(0.125, 0.0);
    let aperture = edge.dist(camera.project_point(point));
    assert!(query(0.125, aperture + 1e-12, start, toward).is_some());
    assert!(query(0.0, 0.125, start, start + Vec3::Z).is_none(), "crossing is behind the ray");
}

#[test]
fn projected_horizontal_ray_crosses_a_horizontal_target_in_an_oblique_view() {
    use cadcraft_geom::OrbitCamera;

    let camera = OrbitCamera { elevation: std::f64::consts::FRAC_PI_4, ..OrbitCamera::default() };
    let mut d = Drawing::new_imperial();
    let start = Vec3::new(-5.0, 0.0, 10.0);
    let expected = Vec3::new(0.0, 2.0, 8.0);
    add_wcs_line(&mut d, expected - Vec3::Y * 5.0, expected + Vec3::Y * 5.0);
    let crossing = camera.project_point(expected);
    let hit = snap::projected_apparent_line_intersection(&d, &Space::Model, &camera, crossing, 0.125, start, start + Vec3::X).unwrap();
    let endpoint = hit.point3.unwrap();
    assert!(endpoint.near(expected, 1e-9));
    assert!(camera.project_point(endpoint).near(camera.project_point(Vec3::new(0.0, 0.0, 10.0)), 1e-9));
}

#[test]
fn projected_horizontal_ray_crosses_a_perpendicular_target_in_plan() {
    use cadcraft_geom::OrbitCamera;

    let camera = OrbitCamera::default();
    let mut d = Drawing::new_imperial();
    let start = Vec3::new(-5.0, 0.0, 10.0);
    let expected = Vec3::new(0.0, 0.0, 10.0);
    add_wcs_line(&mut d, expected - Vec3::Y, expected + Vec3::Y);
    let crossing = camera.project_point(expected);
    let hit = snap::projected_apparent_line_intersection(&d, &Space::Model, &camera, crossing, 0.125, start, start + Vec3::X).unwrap();
    assert!(hit.point3.unwrap().near(expected, 1e-9));
}

#[test]
fn projected_ray_ignores_parallel_and_collinear_targets_and_crosses_no_pairs_of_existing_lines() {
    use cadcraft_geom::OrbitCamera;

    let camera = OrbitCamera::default();
    let start = Vec3::new(-5.0, 0.0, 10.0);
    let cursor = camera.project_point(Vec3::new(0.0, 0.0, 10.0));
    for offset in [0.0, 0.05] {
        let mut d = Drawing::new_imperial();
        add_wcs_line(&mut d, Vec3::new(-1.0, offset, 10.0), Vec3::new(1.0, offset, 10.0));
        assert!(snap::projected_apparent_line_intersection(&d, &Space::Model, &camera, cursor, 0.125, start, start + Vec3::X).is_none());
    }

    let mut d = Drawing::new_imperial();
    add_wcs_line(&mut d, Vec3::new(-1.0, 0.0, 8.0), Vec3::new(1.0, 0.0, 8.0));
    add_wcs_line(&mut d, Vec3::new(0.0, -1.0, 12.0), Vec3::new(0.0, 1.0, 12.0));
    let away = start + Vec3::Y * 2.0;
    assert!(snap::projected_apparent_line_intersection(&d, &Space::Model, &camera, cursor, 0.125, away, away + Vec3::X).is_none());
}

#[test]
fn projected_apparent_ray_rejects_parallel_directions_and_crossings_beyond_the_target_segment() {
    let mut d = Drawing::new_imperial();
    let camera = cadcraft_geom::OrbitCamera { elevation: std::f64::consts::FRAC_PI_4, ..Default::default() };
    let start = Vec3::new(0.0, 0.0, 30.0);
    let crossing = camera.project_point(Vec3::new(0.0, 0.0, 12.0));
    add_wcs_line(&mut d, Vec3::new(0.001, 4.0, 8.0), Vec3::new(5.0, 4.0, 8.0));
    let query = |doc: &Drawing, direction: Vec3| {
        snap::projected_apparent_line_intersection(doc, &Space::Model, &camera, crossing, 0.125, start, start + direction)
    };
    assert!(query(&d, -Vec3::Z).is_none(), "APP targets the existing segment");
    add_wcs_line(&mut d, Vec3::new(-1.0, 4.0, 8.0), Vec3::new(1.0, 4.0, 8.0));
    assert!(query(&d, camera.view_dir() + Vec3::X * 1e-6).is_none(), "ray nearly parallel to the view");
    let mut d = Drawing::new_imperial();
    add_wcs_line(&mut d, Vec3::new(-1e-6, 4.0, 5.0), Vec3::new(1e-6, 4.0, 11.0));
    assert!(query(&d, -Vec3::Z).is_none(), "nearly parallel projected lines");
}

#[test]
#[ignore = "manual timing in an optimized build"]
fn projected_app_pointer_query_cost() {
    use cadcraft_geom::OrbitCamera;
    use std::hint::black_box;
    use std::time::Instant;

    let camera = OrbitCamera::default();
    const MOVES: u32 = 256;
    for count in [1_000, 10_000, 100_000] {
        for dense in [false, true] {
            let mut d = Drawing::new_imperial();
            for n in 0..count {
                let offset = if dense { f64::from(n) / f64::from(count) * 0.1 } else { f64::from(n / 2) };
                let z = 10.0 + f64::from(n % 16);
                let (a, b) = if n % 2 == 0 {
                    (Vec3::new(-1.0, offset, z), Vec3::new(1.0, offset, z))
                } else {
                    (Vec3::new(offset, -1.0, z), Vec3::new(offset, 1.0, z))
                };
                add_wcs_line(&mut d, a, b);
            }
            let start = Instant::now();
            for step in 0..MOVES {
                let cursor = Vec2::new(f64::from(step) / f64::from(MOVES) * 0.1, 0.0);
                let start = Vec3::new(cursor.x, -1.0, 0.0);
                black_box(snap::projected_apparent_line_intersection(black_box(&d), &Space::Model, &camera, cursor, 0.125, start, start + Vec3::Y));
            }
            eprintln!("{count} lines, dense={dense}: {:?}/pointer query (mean)", start.elapsed() / MOVES);
        }
    }
}
