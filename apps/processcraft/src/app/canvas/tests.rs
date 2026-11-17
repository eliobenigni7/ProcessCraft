//! Unit tests for the canvas gestures, geometry and menus.
use super::*;

fn b(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn zoom_keeps_anchor_fixed() {
    let anchor = Vec2::new(200.0, 100.0);
    let before = (10.0 + 200.0 / 1.0, 20.0 + 100.0 / 1.0);
    let (camera, zoom) = zoom_at((10.0, 20.0), 1.0, 2.0, anchor);
    assert_eq!(zoom, 2.0);
    let after = (camera.0 + 200.0 / 2.0, camera.1 + 100.0 / 2.0);
    assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
    assert_eq!(zoom_at((0.0, 0.0), 1.0, 100.0, anchor).1, MAX_ZOOM);
}

#[test]
fn stale_texture_is_scaled_and_offset() {
    let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(100.0, 50.0));
    let view = TextureView {
        camera: (0.0, 0.0),
        zoom: 1.0,
        size: Vec2::new(100.0, 50.0),
        request: ((0.0, 0.0), 1.0),
    };
    assert_eq!(texture_rect(rect, view, (0.0, 0.0), 1.0), rect);
    let panned = texture_rect(rect, view, (5.0, 0.0), 2.0);
    assert_eq!(panned.min, Pos2::new(0.0, 10.0));
    assert_eq!(panned.size(), Vec2::new(200.0, 100.0));
}

#[test]
fn overscanned_texture_lands_on_the_canvas_and_stays_within_raster_limits() {
    let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(1600.0, 900.0));
    let view = raster_view((100.0, 50.0), 2.0, rect.size(), 1.0);
    assert_eq!(view.size, Vec2::new(3200.0, 1800.0));
    assert_eq!(view.camera, (100.0 - 400.0, 50.0 - 225.0));
    // The canvas sits in the middle of the texture, pixel for pixel.
    let shown = texture_rect(rect, view, (100.0, 50.0), 2.0);
    assert_eq!(shown.min, rect.min - Vec2::new(800.0, 450.0));
    assert_eq!(shown.size(), view.size);
    // Drag previews sample the texture with UVs from the same origin and size.
    let uv = |x: f64| (x - view.camera.0) as f32 * view.zoom / view.size.x;
    assert_eq!(uv(100.0), 0.25);
    // High DPI or huge canvases get less overscan, within 16384 px and 64 MiB.
    for (size, ppp) in [
        (Vec2::new(1600.0, 900.0), 2.0),
        (Vec2::new(3840.0, 2160.0), 1.0),
    ] {
        let view = raster_view((0.0, 0.0), 1.0, size, ppp);
        let (w, h) = (view.size.x * ppp, view.size.y * ppp);
        assert!(
            view.size.x > size.x
                && w <= 16384.0
                && w.ceil() * h.ceil() * 4.0 <= 64.0 * 1024.0 * 1024.0
        );
    }
    let huge = raster_view((0.0, 0.0), 1.0, Vec2::new(9000.0, 9000.0), 1.0);
    assert_eq!(huge.size, Vec2::new(9000.0, 9000.0));
}

#[test]
fn pan_and_zoom_reuse_the_texture_until_coverage_zoom_or_settling_ask_for_a_raster() {
    let size = Vec2::new(1600.0, 900.0);
    let view = Some(raster_view((0.0, 0.0), 1.0, size, 1.0));
    assert!(needs_raster(None, (0.0, 0.0), 1.0, size, false));
    // The exact view, settled: nothing to do.
    assert!(!needs_raster(view, (0.0, 0.0), 1.0, size, true));
    // Pans inside the overscan margin and small zooms move the texture on the GPU.
    assert!(!needs_raster(view, (800.0, -450.0), 1.0, size, false));
    assert!(!needs_raster(view, (-400.0, 0.0), 0.8, size, false));
    assert!(!needs_raster(view, (100.0, 100.0), 1.25, size, false));
    // Past the margin, or past the zoom band: a new raster now.
    assert!(needs_raster(view, (801.0, 0.0), 1.0, size, false));
    assert!(needs_raster(view, (0.0, 0.0), 1.3, size, false));
    assert!(needs_raster(view, (0.0, 0.0), 0.75, size, false));
    // Settled on another camera: re-raster at the exact scale.
    assert!(needs_raster(view, (10.0, 0.0), 1.0, size, true));
    assert!(needs_raster(view, (0.0, 0.0), 1.1, size, true));
}

#[test]
fn resize_moves_only_the_dragged_corner_and_keeps_a_minimum() {
    let start = b(0.0, 0.0, 100.0, 80.0);
    let min = (20.0, 20.0);
    let free = |h, d| resize_bounds(&start, h, d, min, false, false, None);
    assert_eq!(free((1, 1), (20.0, 10.0)), b(0.0, 0.0, 120.0, 90.0));
    assert_eq!(free((-1, -1), (10.0, 10.0)), b(10.0, 10.0, 90.0, 70.0));
    assert_eq!(free((1, -1), (-500.0, 0.0)), b(0.0, 0.0, 20.0, 80.0));
    assert_eq!(free((-1, 1), (0.0, -500.0)), b(0.0, 0.0, 100.0, 20.0));
    // Edge handles change one axis only; results are whole units.
    assert_eq!(free((1, 0), (30.4, 50.0)), b(0.0, 0.0, 130.0, 80.0));
    assert_eq!(free((0, -1), (9.0, -10.6)), b(0.0, -11.0, 100.0, 91.0));
    // Locked proportions (or Shift) scale from the opposite corner.
    let event = b(0.0, 0.0, 36.0, 36.0);
    assert_eq!(
        resize_bounds(
            &event,
            (-1, -1),
            (-12.0, -4.0),
            (24.0, 24.0),
            true,
            false,
            None
        ),
        b(-12.0, -12.0, 48.0, 48.0)
    );
    assert_eq!(
        resize_bounds(&start, (1, 1), (50.0, 0.0), min, true, false, None),
        b(0.0, 0.0, 150.0, 120.0)
    );
    // Alt resizes around the centre.
    assert_eq!(
        resize_bounds(&start, (1, 0), (10.0, 0.0), min, false, true, None),
        b(-10.0, 0.0, 120.0, 80.0)
    );
    // Contained shapes (plus padding) stop the shrink where the core would refuse it.
    let keep = b(30.0, 10.0, 50.0, 40.0);
    assert_eq!(
        resize_bounds(
            &start,
            (-1, 1),
            (60.0, -60.0),
            min,
            false,
            false,
            Some(&keep)
        ),
        b(30.0, 0.0, 70.0, 50.0)
    );
}

#[test]
fn fit_to_content_grows_a_long_name_and_keeps_the_minimum() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    let before = shape_bounds(&app, &task);
    let rename = |app: &mut ProcessCraft, name: &str| {
        let document_id = app.ctl.session.snapshot().project.documents[0].id;
        assert!(app.ctl.mutate(vec![Mutation::Rename {
            document_id,
            element_id: task.clone(),
            name: Some(name.into()),
        }]));
    };
    rename(&mut app, &"Review the supplier invoice ".repeat(6));
    app.fit_to_content(&task);
    let grown = shape_bounds(&app, &task);
    assert!(grown.height > before.height, "{grown:?}");
    assert_eq!(
        (grown.x, grown.y, grown.width),
        (before.x, before.y, before.width)
    );
    assert_eq!(grown.height, grown.height.round());
    rename(&mut app, "Pay");
    app.fit_to_content(&task);
    assert_eq!(shape_bounds(&app, &task).height, 40.0);
}

#[test]
fn marquee_touches_right_to_left_and_shift_adds_to_the_selection() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::StartEvent);
    let start = app.selected.clone().unwrap();
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    let context = egui::Context::default();
    let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| app.canvas_ui(ui),
        );
        output.textures_delta.clear();
    };
    run(
        &mut app,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::SHIFT)],
    );
    app.select(Some(start.clone()));
    let screen = to_screen(
        app.canvas.rect.unwrap(),
        app.camera,
        app.zoom,
        &shape_bounds(&app, &task),
    );
    // From below right of the task back into its middle: it only touches the task.
    let (from, to) = (screen.right_bottom() + Vec2::splat(20.0), screen.center());
    let shift_button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::SHIFT,
    };
    run(&mut app, vec![egui::Event::PointerMoved(from)]);
    run(&mut app, vec![shift_button(from, true)]);
    for step in 1..=5 {
        let at = from + (to - from) * (step as f32 / 5.0);
        run(&mut app, vec![egui::Event::PointerMoved(at)]);
    }
    run(&mut app, vec![shift_button(to, false)]);
    assert!(app.multi.contains(&start), "{:?}", app.multi);
    assert!(app.multi.contains(&task), "{:?}", app.multi);
    // Left to right needs the whole shape; an edge counts when all its points are inside.
    let snap = app.ctl.session.snapshot();
    let diagram = &snap.project.documents[0].diagrams[0];
    let t = shape_bounds(&app, &task);
    let center = (t.x + t.width / 2.0, t.y + t.height / 2.0);
    assert!(marquee(diagram, center, (t.x + t.width + 5.0, t.y + t.height + 5.0)).is_empty());
    let all = marquee(diagram, (-1e4, -1e4), (1e4, 1e4));
    assert_eq!(all.len(), diagram.shapes.len() + diagram.edges.len());
    // Shift during a move keeps the dominant axis.
    assert_eq!(lock_axis(Vec2::new(30.0, -8.0)), Vec2::new(30.0, 0.0));
    assert_eq!(lock_axis(Vec2::new(3.0, -8.0)), Vec2::new(0.0, -8.0));
}

#[test]
fn bendpoint_and_segment_drags_edit_the_route() {
    let p = |x: f64, y: f64| Point { x, y };
    // Source shape left of the route, target shape at its end.
    let ends = [
        Some(b(-40.0, -20.0, 40.0, 40.0)),
        Some(b(100.0, 20.0, 40.0, 40.0)),
    ];
    let edit = |route: &[Point], part, dx, dy| edit_route(route, part, dx, dy, &ends, false);
    let route = vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 40.0), p(100.0, 40.0)];
    // A corner drags its two segments along: the route stays orthogonal and the source end
    // slides along its border.
    assert_eq!(
        edit(&route, EdgePart::Point(1), 5.0, 5.0),
        vec![p(0.0, 5.0), p(55.0, 5.0), p(55.0, 40.0), p(100.0, 40.0)]
    );
    // A vertical segment moves sideways only; a horizontal one up or down only.
    let moved = edit(&route, EdgePart::Segment(1), 10.0, 99.0);
    assert_eq!((moved[1].x, moved[2].x, moved[1].y), (60.0, 60.0, 0.0));
    let moved = edit(&route, EdgePart::Segment(2), 99.0, -10.0);
    assert_eq!((moved[2].y, moved[3].y, moved[3].x), (30.0, 30.0, 100.0));
    // The first segment slides its end along the source border, never past the shape.
    let moved = edit(&route, EdgePart::Segment(0), 0.0, 50.0);
    assert_eq!((moved[0].clone(), moved[1].y), (p(0.0, 20.0), 20.0));
    // The grid rounds the moved corner.
    assert_eq!(
        edit_route(&route, EdgePart::Point(1), 3.0, 4.0, &ends, true),
        route
    );
    // A diagonal segment gets a new bendpoint in its middle.
    let diagonal = vec![p(0.0, 0.0), p(100.0, 100.0)];
    assert_eq!(
        edit(&diagonal, EdgePart::Segment(0), 0.0, 10.0),
        vec![p(0.0, 0.0), p(50.0, 60.0), p(100.0, 100.0)]
    );
    // An end docks on the side nearest to the pointer: on the side facing its neighbour the
    // neighbour follows; on the far side a stub is routed around the shape.
    let dock = |at, tolerance| {
        let own = (ends[1].as_ref().unwrap(), Some(NodeKind::Task));
        redock_end(&route, 3, own, ends[0].as_ref(), at, tolerance)
    };
    assert_eq!(
        dock((100.0, 30.0), 0.0),
        vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 30.0), p(100.0, 30.0)]
    );
    assert_eq!(dock((97.0, 37.0), 5.0), route, "snaps to the side middle");
    assert_eq!(
        dock((130.0, 58.0), 0.0),
        vec![
            p(0.0, 0.0),
            p(50.0, 0.0),
            p(50.0, 40.0),
            p(50.0, 80.0),
            p(130.0, 80.0),
            p(130.0, 60.0)
        ]
    );
    // The source end of a two-point route moved to its top side: stubs off both shapes.
    let straight = vec![p(0.0, 0.0), p(100.0, 0.0)];
    let own = (ends[0].as_ref().unwrap(), None);
    let docked = redock_end(&straight, 0, own, ends[1].as_ref(), (-10.0, -25.0), 0.0);
    assert_eq!(docked.first(), Some(&p(-10.0, -20.0)), "{docked:?}");
    assert_eq!(docked.last(), straight.last());
    assert!(
        docked
            .windows(2)
            .all(|w| w[0].x == w[1].x || w[0].y == w[1].y),
        "{docked:?}"
    );
}

#[test]
fn adding_after_a_selected_node_appends_and_connects_by_id() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::StartEvent);
    let start = app.selected.clone().unwrap();
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    let snap = app.ctl.session.snapshot();
    let document = &snap.project.documents[0];
    let diagram = &document.diagrams[0];
    let bounds = |id: &ElementSelector| {
        diagram
            .shapes
            .iter()
            .find(|s| &s.element == id)
            .unwrap()
            .bounds
            .clone()
    };
    assert!(bounds(&task).x >= bounds(&start).x + bounds(&start).width);
    assert_eq!(diagram.edges.len(), 1);
    app.select(None);
    app.add(NodeKind::EndEvent);
    let end = app.selected.clone().unwrap();
    assert_eq!(
        app.ctl.session.snapshot().project.documents[0].diagrams[0]
            .edges
            .len(),
        1
    );
    assert!(app.connect_elements(task, end));
    let snap = app.ctl.session.snapshot();
    let edges = &snap.project.documents[0].diagrams[0].edges;
    assert_eq!(edges.len(), 2);
    assert!(
        edges[1]
            .waypoints
            .windows(2)
            .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
    );
}

#[test]
fn empty_drag_pans_and_ctrl_plus_zooms_the_canvas() {
    let mut app = ProcessCraft::default();
    let context = egui::Context::default();
    let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| {
                app.canvas_shortcuts(ui.ctx());
                app.canvas_ui(ui)
            },
        );
        output.textures_delta.clear();
    };
    run(&mut app, vec![]);
    let (camera, zoom) = (app.camera, app.zoom);
    let (from, to) = (Pos2::new(400., 300.), Pos2::new(500., 300.));
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    run(
        &mut app,
        vec![egui::Event::PointerMoved(from), button(from, true)],
    );
    run(&mut app, vec![egui::Event::PointerMoved(to)]);
    run(&mut app, vec![button(to, false)]);
    assert!(app.camera.0 < camera.0);
    assert_eq!(app.zoom, zoom);
    run(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Plus,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }],
    );
    assert!((app.zoom - zoom * ZOOM_STEP).abs() < 1e-5);
    assert_eq!(context.zoom_factor(), 1.0);
}

/// Runs canvas frames on one egui context; waits for hit regions from the background render.
fn canvas_runner(
    app: &mut ProcessCraft,
) -> impl FnMut(&mut ProcessCraft, Vec<egui::Event>) + use<> {
    let context = egui::Context::default();
    let run = move |app: &mut ProcessCraft, events: Vec<egui::Event>| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| app.canvas_ui(ui),
        );
        output.textures_delta.clear();
    };
    for _ in 0..200 {
        run(app, vec![]);
        if !app.hits.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!app.hits.is_empty(), "render produced no hit regions");
    run
}
fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}
/// Screen rectangle of `id`'s shape and its context pad entry `action`.
fn on_screen(app: &ProcessCraft, id: &ElementSelector, action: PadAction) -> (Rect, Pos2) {
    let snap = app.ctl.session.snapshot();
    let document = &snap.project.documents[0];
    let shape = document.diagrams[0]
        .shapes
        .iter()
        .find(|s| &s.element == id)
        .unwrap();
    let screen = to_screen(
        app.canvas.rect.unwrap(),
        app.camera,
        app.zoom,
        &shape.bounds,
    );
    let entry = context_pad(document, id, screen, app.canvas.rect.unwrap())
        .into_iter()
        .find(|(a, _)| *a == action)
        .unwrap();
    (screen, entry.1.center())
}

#[test]
fn dragging_the_connect_handle_onto_a_task_creates_a_flow() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::StartEvent);
    let start = app.selected.clone().unwrap();
    app.select(None);
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    app.select(Some(start.clone()));
    let mut run = canvas_runner(&mut app);
    let drag = |app: &mut ProcessCraft,
                run: &mut dyn FnMut(&mut ProcessCraft, Vec<egui::Event>),
                from: Pos2,
                to: Pos2| {
        run(app, vec![egui::Event::PointerMoved(from)]);
        run(app, vec![button(from, true)]);
        for step in 1..=5 {
            run(
                app,
                vec![egui::Event::PointerMoved(
                    from + (to - from) * (step as f32 / 5.0),
                )],
            );
        }
        run(app, vec![button(to, false)]);
        run(app, vec![]);
    };
    let edges = |app: &ProcessCraft| {
        app.ctl.session.snapshot().project.documents[0].diagrams[0]
            .edges
            .len()
    };
    let edges_before = edges(&app);
    let (_, handle) = on_screen(&app, &start, PadAction::Connect);
    let (target, _) = on_screen(&app, &task, PadAction::Delete);
    drag(&mut app, &mut run, handle, target.center());
    let snap = app.ctl.session.snapshot();
    assert_eq!(edges(&app), edges_before + 1);
    assert!(snap.project.documents[0].processes[0].flows.iter().any(|f| {
        matches!(&f.source, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == start)
            && matches!(&f.target, Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) if *id == task)
    }));
    // Back into the start event is refused with the rule's reason and changes nothing.
    app.select(Some(task.clone()));
    run(&mut app, vec![]);
    let (_, handle) = on_screen(&app, &task, PadAction::Connect);
    let (start_rect, _) = on_screen(&app, &start, PadAction::Delete);
    drag(&mut app, &mut run, handle, start_rect.center());
    assert_eq!(edges(&app), edges_before + 1);
    assert!(app.ctl.error.contains("Start events"), "{}", app.ctl.error);
}

/// Press at `from`, move in five steps, release at `to`.
fn drag_on(
    app: &mut ProcessCraft,
    run: &mut dyn FnMut(&mut ProcessCraft, Vec<egui::Event>),
    from: Pos2,
    to: Pos2,
) {
    run(app, vec![egui::Event::PointerMoved(from)]);
    run(app, vec![button(from, true)]);
    for step in 1..=5 {
        run(
            app,
            vec![egui::Event::PointerMoved(
                from + (to - from) * (step as f32 / 5.0),
            )],
        );
    }
    run(app, vec![button(to, false)]);
    run(app, vec![]);
}
fn shape_bounds(app: &ProcessCraft, id: &ElementSelector) -> Bounds {
    app.ctl.session.snapshot().project.documents[0].diagrams[0]
        .shapes
        .iter()
        .find(|s| &s.element == id)
        .unwrap()
        .bounds
        .clone()
}

/// A start event connected to a task, with the flow selected; returns the task id.
fn selected_flow_to_task(app: &mut ProcessCraft) -> ElementSelector {
    app.add(NodeKind::StartEvent);
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    let flow = app.ctl.session.snapshot().project.documents[0].processes[0].flows[0]
        .id
        .selector();
    app.select(Some(flow));
    task
}
fn route(app: &ProcessCraft) -> Vec<Point> {
    app.ctl.session.snapshot().project.documents[0].diagrams[0].edges[0]
        .waypoints
        .clone()
}
fn model_to_screen(app: &ProcessCraft, p: &Point) -> Pos2 {
    to_screen(
        app.canvas.rect.unwrap(),
        app.camera,
        app.zoom,
        &b(p.x, p.y, 0.0, 0.0),
    )
    .min
}
fn orthogonal(points: &[Point]) -> bool {
    points
        .windows(2)
        .all(|w| w[0].x == w[1].x || w[0].y == w[1].y)
}

#[test]
fn dragging_an_edge_end_docks_it_on_another_side_of_its_shape() {
    let mut app = ProcessCraft::default();
    let task = selected_flow_to_task(&mut app);
    let mut run = canvas_runner(&mut app);
    let t = shape_bounds(&app, &task);
    let before = route(&app);
    let undo = app.ctl.session.snapshot().undo_count;
    // Released just above the task's top side, three quarters along it.
    let from = model_to_screen(&app, before.last().unwrap());
    let target = model_to_screen(
        &app,
        &Point {
            x: t.x + t.width * 0.75,
            y: t.y,
        },
    ) - Vec2::new(0.0, 4.0);
    drag_on(&mut app, &mut run, from, target);
    let after = route(&app);
    let last = after.last().unwrap();
    assert_eq!(last.y, t.y, "{after:?} {}", app.ctl.error);
    assert!((last.x - (t.x + t.width * 0.75)).abs() <= 1.5, "{after:?}");
    assert!(orthogonal(&after), "{after:?}");
    assert_eq!(after[0], before[0]);
    assert_eq!(app.ctl.session.snapshot().undo_count, undo + 1);
}

#[test]
fn an_end_slid_along_its_side_keeps_its_place_when_the_shape_moves() {
    let mut app = ProcessCraft::default();
    let task = selected_flow_to_task(&mut app);
    let mut run = canvas_runner(&mut app);
    let t = shape_bounds(&app, &task);
    let end = route(&app).last().unwrap().clone();
    let from = model_to_screen(&app, &end);
    let to = model_to_screen(
        &app,
        &Point {
            x: end.x - 2.0,
            y: end.y + 12.0,
        },
    );
    drag_on(&mut app, &mut run, from, to);
    let docked = route(&app).last().unwrap().clone();
    let offset = docked.y - (t.y + t.height / 2.0);
    assert_eq!(docked.x, t.x, "{:?}", route(&app));
    assert!(offset >= 10.0, "{:?}", route(&app));
    // Move the task down: the end keeps its offset on the left side.
    app.select(Some(task.clone()));
    run(&mut app, vec![]);
    let (screen, _) = on_screen(&app, &task, PadAction::Delete);
    let grab = screen.center() + Vec2::new(10.0, -10.0);
    let down = Vec2::new(0.0, 40.0 * app.zoom);
    drag_on(&mut app, &mut run, grab, grab + down);
    let moved = shape_bounds(&app, &task);
    assert_ne!(moved, t, "{}", app.ctl.error);
    let after = route(&app);
    let last = after.last().unwrap();
    assert_eq!(last.x, moved.x, "{after:?}");
    let kept = last.y - (moved.y + moved.height / 2.0);
    assert!((kept - offset).abs() < 1e-6, "{after:?}");
    assert!(orthogonal(&after), "{after:?}");
}

#[test]
fn dragging_a_task_carries_its_boundary_event() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    app.add(NodeKind::BoundaryEvent);
    let event = app.selected.clone().unwrap();
    app.select(None);
    let (task_before, event_before) = (shape_bounds(&app, &task), shape_bounds(&app, &event));
    let mut run = canvas_runner(&mut app);
    let (screen, _) = on_screen(&app, &task, PadAction::Delete);
    let from = screen.center() - Vec2::new(0.0, screen.height() / 4.0);
    drag_on(&mut app, &mut run, from, from + Vec2::new(60.0, 40.0));
    let (task_after, event_after) = (shape_bounds(&app, &task), shape_bounds(&app, &event));
    assert_ne!(task_after, task_before, "{}", app.ctl.error);
    assert_eq!(event_after.x - event_before.x, task_after.x - task_before.x);
    assert_eq!(event_after.y - event_before.y, task_after.y - task_before.y);
}

#[test]
fn dropping_a_task_on_a_flow_inserts_it() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::StartEvent);
    let start = app.selected.clone().unwrap();
    app.add(NodeKind::Task);
    let first = app.selected.clone().unwrap();
    app.select(None);
    app.add(NodeKind::Task);
    let second = app.selected.clone().unwrap();
    app.select(None);
    let mut run = canvas_runner(&mut app);
    let snap = app.ctl.session.snapshot();
    let edge = snap.project.documents[0].diagrams[0].edges[0].clone();
    let (a, b) = (
        &edge.waypoints[0],
        &edge.waypoints[edge.waypoints.len() - 1],
    );
    let mid = Bounds {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
        width: 0.0,
        height: 0.0,
    };
    let target = to_screen(app.canvas.rect.unwrap(), app.camera, app.zoom, &mid).min;
    let (screen, _) = on_screen(&app, &second, PadAction::Delete);
    drag_on(&mut app, &mut run, screen.center(), target);
    let snap = app.ctl.session.snapshot();
    let flows = &snap.project.documents[0].processes[0].flows;
    let ends = |f: &processcraft_core::model::Flow| {
        let id = |r: &Option<processcraft_core::bpmn_activity::ModelReference>| match r {
            Some(processcraft_core::bpmn_activity::ModelReference::Local(id)) => id.clone(),
            _ => panic!("external end"),
        };
        (id(&f.source), id(&f.target))
    };
    assert_eq!(flows.len(), 2, "{}", app.ctl.error);
    assert!(
        flows
            .iter()
            .any(|f| ends(f) == (start.clone(), second.clone()))
    );
    assert!(
        flows
            .iter()
            .any(|f| ends(f) == (second.clone(), first.clone()))
    );
}

#[test]
fn element_keys_add_from_the_palette_and_append_from_the_pad() {
    let mut app = ProcessCraft::default();
    let context = egui::Context::default();
    let run = |app: &mut ProcessCraft, key| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                app.canvas_shortcuts(ui.ctx());
                app.canvas_ui(ui)
            },
        );
        output.textures_delta.clear();
    };
    let counts = |app: &ProcessCraft| {
        let snap = app.ctl.session.snapshot();
        let process = &snap.project.documents[0].processes[0];
        (process.nodes.len(), process.flows.len())
    };
    // Nothing selected: T adds a task, like clicking the palette.
    run(&mut app, egui::Key::T);
    assert_eq!(counts(&app), (1, 0), "{}", app.ctl.error);
    assert_eq!(app.multi.len(), 1, "the new task is selected");
    // One shape selected: 3 appends a connected end event, like the context pad.
    run(&mut app, egui::Key::Num3);
    assert_eq!(counts(&app), (2, 1), "{}", app.ctl.error);
    let snap = app.ctl.session.snapshot();
    assert!(
        snap.project.documents[0].processes[0]
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::EndEvent)
    );
    assert_eq!(element_key(NodeKind::UserTask), Some("U"));
}

#[test]
fn ctrl_a_selects_the_diagram_and_ctrl_d_duplicates_it() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::StartEvent);
    app.add(NodeKind::Task);
    let context = egui::Context::default();
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    };
    let run = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| {
                app.canvas_shortcuts(ui.ctx());
                app.canvas_ui(ui)
            },
        );
        output.textures_delta.clear();
    };
    run(&mut app, vec![key(egui::Key::A)]);
    // Two shapes and the connecting flow.
    assert_eq!(app.multi.len(), 3);
    let first_x = |app: &ProcessCraft| {
        app.ctl.session.snapshot().project.documents[0].diagrams[0].shapes[0]
            .bounds
            .x
    };
    // Arrow keys move the selection by 1 px.
    let x = first_x(&app);
    run(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(first_x(&app), x + 1.0, "{}", app.ctl.error);
    let before = app.ctl.session.snapshot();
    run(&mut app, vec![key(egui::Key::D)]);
    let after = app.ctl.session.snapshot();
    assert_eq!(after.undo_count, before.undo_count + 1);
    let process = &after.project.documents[0].processes[0];
    assert_eq!((process.nodes.len(), process.flows.len()), (4, 2));
    let diagram = &after.project.documents[0].diagrams[0];
    assert_eq!((diagram.shapes.len(), diagram.edges.len()), (4, 2));
    // The copies are selected, and offset by 20/20 from the originals.
    assert_eq!(app.multi.len(), 3);
    let shape = |id: &ElementSelector| diagram.shapes.iter().find(|s| &s.element == id);
    let copy = app.multi.iter().find_map(shape).unwrap();
    assert_eq!(copy.bounds.x, diagram.shapes[0].bounds.x + 20.0);
    assert_eq!(copy.bounds.y, diagram.shapes[0].bounds.y + 20.0);
}
#[test]
fn auto_scroll_only_near_the_border() {
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
    assert_eq!(auto_scroll_step(rect, Pos2::new(200.0, 150.0)), Vec2::ZERO);
    assert_eq!(
        auto_scroll_step(rect, Pos2::new(5.0, 295.0)),
        Vec2::new(-AUTO_SCROLL_STEP, AUTO_SCROLL_STEP)
    );
    assert_eq!(auto_scroll_step(rect, Pos2::new(-5.0, 150.0)), Vec2::ZERO);
}

#[test]
fn context_pad_append_creates_a_connected_task_and_selects_it() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::StartEvent);
    let start = app.selected.clone().unwrap();
    let mut run = canvas_runner(&mut app);
    let (_, entry) = on_screen(&app, &start, PadAction::Append(NodeKind::Task));
    run(&mut app, vec![egui::Event::PointerMoved(entry)]);
    run(&mut app, vec![button(entry, true)]);
    run(&mut app, vec![button(entry, false)]);
    let task = app.selected.clone().unwrap();
    assert_ne!(task, start);
    let snap = app.ctl.session.snapshot();
    let document = &snap.project.documents[0];
    assert_eq!(
        processcraft_core::queries::find_node(document, &task).map(|n| n.kind),
        Some(NodeKind::Task)
    );
    assert_eq!(document.diagrams[0].edges.len(), 1);
    // An end event offers no append entries.
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::splat(36.0));
    app.add(NodeKind::EndEvent);
    let end = app.selected.clone().unwrap();
    let snap = app.ctl.session.snapshot();
    let pad = context_pad(&snap.project.documents[0], &end, screen, Rect::EVERYTHING);
    assert!(
        pad.iter()
            .all(|(a, _)| !matches!(a, PadAction::Append(k) if *k != NodeKind::TextAnnotation))
    );
}
#[test]
fn context_pad_adds_a_lane_below_a_pool() {
    let mut app = ProcessCraft::default();
    app.add_pool();
    let snap = app.ctl.session.snapshot();
    let document = &snap.project.documents[0];
    let pool = document.collaboration.as_ref().unwrap().participants[0]
        .id
        .selector();
    let before = document.diagrams[0]
        .shapes
        .iter()
        .find(|s| s.element == pool)
        .unwrap()
        .bounds
        .clone();
    app.select(Some(pool.clone()));
    let mut run = canvas_runner(&mut app);
    let (_, entry) = on_screen(&app, &pool, PadAction::AddLane(LaneLocation::Below));
    run(&mut app, vec![egui::Event::PointerMoved(entry)]);
    run(&mut app, vec![button(entry, true)]);
    run(&mut app, vec![button(entry, false)]);
    let snap = app.ctl.session.snapshot();
    let document = &snap.project.documents[0];
    let shapes = &document.diagrams[0].shapes;
    let after = &shapes.iter().find(|s| s.element == pool).unwrap().bounds;
    assert_eq!(after.height, before.height + 120.0, "{}", app.ctl.error);
    // The pool had no lanes: it gets one covering its body and the new 120 high lane.
    let lanes = &document.processes[0].lanes;
    assert_eq!(lanes.len(), 2);
    let mut heights: Vec<f64> = lanes
        .iter()
        .map(|l| {
            shapes
                .iter()
                .find(|s| s.element == l.id)
                .unwrap()
                .bounds
                .height
        })
        .collect();
    heights.sort_by(f64::total_cmp);
    assert_eq!(heights, vec![120.0, before.height]);
}
#[test]
fn r_opens_the_replace_menu_and_enter_turns_a_task_into_a_user_task() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    let context = egui::Context::default();
    let frame = |app: &mut ProcessCraft, events: Vec<egui::Event>| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| {
                app.canvas_shortcuts(ui.ctx());
                app.canvas_ui(ui)
            },
        );
        output.textures_delta.clear();
    };
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    // The context pad offers the wrench for the task.
    let snap = app.ctl.session.snapshot();
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 80.0));
    assert!(
        context_pad(&snap.project.documents[0], &task, screen, Rect::EVERYTHING)
            .iter()
            .any(|(a, _)| *a == PadAction::Replace)
    );
    frame(&mut app, vec![key(egui::Key::R)]);
    assert!(app.canvas.replace.is_some());
    frame(&mut app, vec![]);
    frame(&mut app, vec![egui::Event::Text("User".into())]);
    assert_eq!(app.canvas.replace.as_ref().unwrap().query, "User");
    frame(&mut app, vec![key(egui::Key::Enter)]);
    assert!(app.canvas.replace.is_none(), "{}", app.ctl.error);
    let snap = app.ctl.session.snapshot();
    let node =
        processcraft_core::queries::find_node(&snap.project.documents[0], &task).expect("same id");
    assert_eq!(node.kind, NodeKind::UserTask);
}
#[test]
fn vacant_bounds_avoids_shapes_but_not_containers() {
    let shape = |bounds: Bounds| Shape {
        di_metadata: None,
        is_message_visible: None,
        participant_band_kind: None,
        choreography_activity_shape: None,
        label_visibility: None,
        is_horizontal: None,
        is_marker_visible: None,
        is_expanded: None,
        id: "S".parse::<processcraft_core::ids::XmlId>().unwrap().into(),
        element: "E".parse().unwrap(),
        bounds,
        label: None,
        style: None,
    };
    let free = vacant_bounds(&[], (100.0, 100.0), 100.0, 80.0);
    assert_eq!(free, b(50.0, 60.0, 100.0, 80.0));
    let pool = shape(b(-1000.0, -1000.0, 3000.0, 3000.0));
    assert_eq!(
        vacant_bounds(std::slice::from_ref(&pool), (100.0, 100.0), 100.0, 80.0),
        free
    );
    let blocker = shape(b(80.0, 80.0, 100.0, 80.0));
    let moved = vacant_bounds(&[pool, blocker.clone()], (100.0, 100.0), 100.0, 80.0);
    assert_ne!(moved, free);
    let o = &blocker.bounds;
    assert!(
        moved.x + moved.width + 20.0 <= o.x
            || o.x + o.width + 20.0 <= moved.x
            || moved.y + moved.height + 20.0 <= o.y
            || o.y + o.height + 20.0 <= moved.y
    );
}

/// Frames running the editor shortcuts and the canvas on one egui context.
fn shortcut_runner() -> impl FnMut(&mut ProcessCraft, Vec<egui::Event>) {
    let context = egui::Context::default();
    move |app: &mut ProcessCraft, events: Vec<egui::Event>| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                events,
                ..Default::default()
            },
            |ui| {
                app.canvas_shortcuts(ui.ctx());
                app.canvas_ui(ui)
            },
        );
        output.textures_delta.clear();
    }
}
fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}
fn node_of(app: &ProcessCraft, id: &ElementSelector) -> processcraft_core::model::Node {
    processcraft_core::queries::find_node(&app.ctl.session.snapshot().project.documents[0], id)
        .unwrap()
        .clone()
}

#[test]
fn e_edits_names_and_annotation_text_inline() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut app = ProcessCraft::default();
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    let mut frame = shortcut_runner();
    // E opens the single-line name editor; Enter commits.
    frame(&mut app, vec![key(egui::Key::E, egui::Modifiers::NONE)]);
    assert!(
        app.canvas
            .label_edit
            .as_ref()
            .is_some_and(|e| !e.annotation)
    );
    frame(&mut app, vec![]);
    app.canvas.label_edit.as_mut().unwrap().text = "Review order".into();
    frame(&mut app, vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    assert!(app.canvas.label_edit.is_none());
    assert_eq!(node_of(&app, &task).name.as_deref(), Some("Review order"));
    // A text annotation edits its text in a multi-line editor; Escape cancels.
    app.select(None);
    app.add(NodeKind::TextAnnotation);
    let note = app.selected.clone().unwrap();
    let text = |app: &ProcessCraft| match node_of(app, &note).details {
        Some(NodeDetails::TextAnnotation { text: Some(t), .. }) => {
            t.plain_text().unwrap_or_default().to_owned()
        }
        _ => String::new(),
    };
    frame(&mut app, vec![key(egui::Key::E, egui::Modifiers::NONE)]);
    assert!(app.canvas.label_edit.as_ref().is_some_and(|e| e.annotation));
    frame(&mut app, vec![]);
    app.canvas.label_edit.as_mut().unwrap().text = "discarded".into();
    frame(
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.canvas.label_edit.is_none());
    assert_eq!(text(&app), "");
    // Ctrl+Enter commits the multi-line text.
    frame(&mut app, vec![key(egui::Key::E, egui::Modifiers::NONE)]);
    frame(&mut app, vec![]);
    app.canvas.label_edit.as_mut().unwrap().text = "Line one\nLine two".into();
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(&mut app, vec![key(egui::Key::Enter, ctrl)]);
    assert!(app.canvas.label_edit.is_none(), "{}", app.ctl.error);
    assert_eq!(text(&app), "Line one\nLine two");
}

#[test]
fn flow_pad_switches_a_gateway_flow_to_default() {
    use processcraft_core::bpmn_model::NodeDetails;
    let mut app = ProcessCraft::default();
    app.add(NodeKind::ExclusiveGateway);
    let gateway = app.selected.clone().unwrap();
    app.add(NodeKind::Task);
    let snap = app.ctl.session.snapshot();
    let document = &snap.project.documents[0];
    let flow = document.processes[0].flows[0].id.selector();
    let pad = flow_pad(document, &flow, Pos2::ZERO, Rect::EVERYTHING);
    let actions: Vec<PadAction> = pad.iter().map(|(a, _)| *a).collect();
    // Gateways offer default flows; conditional flows are offered from activities.
    assert!(actions.contains(&PadAction::Flow(FlowVariant::Default)));
    assert!(actions.contains(&PadAction::Delete));
    // The current type (plain) is not offered.
    assert!(!actions.contains(&PadAction::Flow(FlowVariant::Plain)));
    app.select(Some(flow.clone()));
    app.pad_action(flow.clone(), PadAction::Flow(FlowVariant::Default));
    assert!(
        matches!(node_of(&app, &gateway).details, Some(NodeDetails::Gateway { default_flow: Some(ref f), .. }) if *f == flow),
        "{}",
        app.ctl.error
    );
}

#[test]
fn click_to_connect_and_drop_on_empty_canvas_offers_append() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::StartEvent);
    let start = app.selected.clone().unwrap();
    app.select(None);
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    app.select(Some(start.clone()));
    let mut run = canvas_runner(&mut app);
    let flows = |app: &ProcessCraft| {
        app.ctl.session.snapshot().project.documents[0].processes[0]
            .flows
            .len()
    };
    // Click the connect entry, then click the target.
    let (_, entry) = on_screen(&app, &start, PadAction::Connect);
    run(&mut app, vec![egui::Event::PointerMoved(entry)]);
    run(&mut app, vec![button(entry, true)]);
    run(&mut app, vec![button(entry, false)]);
    assert!(app.canvas.connect_click.is_some());
    let (target, _) = on_screen(&app, &task, PadAction::Delete);
    run(&mut app, vec![egui::Event::PointerMoved(target.center())]);
    run(&mut app, vec![button(target.center(), true)]);
    run(&mut app, vec![button(target.center(), false)]);
    assert!(app.canvas.connect_click.is_none());
    assert_eq!(flows(&app), 1, "{}", app.ctl.error);
    // Dropping a connection on empty canvas opens the append popup; Escape closes it.
    app.select(Some(task.clone()));
    run(&mut app, vec![]);
    let (_, handle) = on_screen(&app, &task, PadAction::Connect);
    let empty = app.canvas.rect.unwrap().right_bottom() - Vec2::new(40.0, 40.0);
    drag_on(&mut app, &mut run, handle, empty);
    assert!(app.canvas.append_menu.is_some(), "{}", app.ctl.error);
    run(
        &mut app,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.canvas.append_menu.is_none());
    drag_on(&mut app, &mut run, handle, empty);
    let menu = app.canvas.append_menu.take().unwrap();
    assert!(
        app.append_choices(&menu.source)
            .iter()
            .any(|(_, e)| e.kind == NodeKind::EndEvent)
    );
    let before = app.ctl.session.snapshot().undo_count;
    assert!(app.create_connected(menu.source, NodeKind::EndEvent.into(), menu.at));
    assert_eq!(app.ctl.session.snapshot().undo_count, before + 1);
    assert_eq!(flows(&app), 2);
    let end = app.selected.clone().unwrap();
    assert_eq!(node_of(&app, &end).kind, NodeKind::EndEvent);
    // An end event cannot start a sequence flow: no append popup entries.
    assert!(app.append_choices(&end).is_empty());
}

#[test]
fn palette_triggers_are_explicit_and_timers_start_empty() {
    use processcraft_core::{
        bpmn_model::{EventDefinition, NodeDetails},
        replace::Trigger,
    };
    let entry = |kind, trigger| PaletteEntry {
        kind,
        trigger: Some(trigger),
    };
    let definitions =
        |app: &ProcessCraft| match node_of(app, app.selected.as_ref().unwrap()).details {
            Some(NodeDetails::Event { definitions, .. }) => definitions,
            _ => panic!("not an event"),
        };
    let mut app = ProcessCraft::default();
    app.add_entry(entry(NodeKind::IntermediateCatchEvent, Trigger::Message));
    assert!(matches!(
        definitions(&app).as_slice(),
        [EventDefinition::Message {
            message_ref: None,
            ..
        }]
    ));
    app.select(None);
    app.add_entry(entry(NodeKind::IntermediateCatchEvent, Trigger::Timer));
    assert!(matches!(
        definitions(&app).as_slice(),
        [EventDefinition::Timer {
            kind: None,
            expression: None,
            ..
        }]
    ));
    // Appended after a task (one undo step) and as a boundary event of it.
    app.select(None);
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    let undo = app.ctl.session.snapshot().undo_count;
    app.add_entry(entry(NodeKind::IntermediateCatchEvent, Trigger::Message));
    assert_eq!(app.ctl.session.snapshot().undo_count, undo + 1);
    assert!(matches!(
        definitions(&app).as_slice(),
        [EventDefinition::Message { .. }]
    ));
    app.select(Some(task.clone()));
    app.add_entry(entry(NodeKind::BoundaryEvent, Trigger::Timer));
    assert_eq!(
        node_of(&app, app.selected.as_ref().unwrap()).kind,
        NodeKind::BoundaryEvent
    );
    assert!(matches!(
        definitions(&app).as_slice(),
        [EventDefinition::Timer {
            expression: None,
            ..
        }]
    ));
    // Dropped from a drag: the core create path gets the same trigger.
    app.select(None);
    assert!(app.drop_new(
        entry(NodeKind::IntermediateCatchEvent, Trigger::Message),
        Point {
            x: -600.0,
            y: -600.0
        }
    ));
    assert!(matches!(
        definitions(&app).as_slice(),
        [EventDefinition::Message { .. }]
    ));
    // The palette never offers a catch or boundary event without its trigger.
    assert!(
        super::super::shell::more_palette_entries()
            .iter()
            .all(|(_, e)| !matches!(
                e.kind,
                NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
            ) || e.trigger.is_some())
    );
}

#[test]
fn snapping_uses_siblings_not_shapes_of_other_scopes() {
    let mut app = ProcessCraft::default();
    app.add(NodeKind::Task);
    let task = app.selected.clone().unwrap();
    app.select(None);
    app.add(NodeKind::SubProcess);
    let sub = app.selected.clone().unwrap();
    let s = shape_bounds(&app, &sub);
    app.select(None);
    assert!(app.drop_new(
        NodeKind::Task.into(),
        Point {
            x: s.x + 80.0,
            y: s.y + 60.0
        }
    ));
    let inner = app.selected.clone().unwrap();
    let snap = app.ctl.session.snapshot();
    let d = &snap.project.documents[0];
    assert_eq!(
        processcraft_core::rules::locate(d, &inner).and_then(|l| l.scope),
        Some(sub.clone())
    );
    let diagram = &d.diagrams[0];
    let shape = diagram.shapes.iter().find(|s| s.element == task).unwrap();
    let a = shape.bounds.clone();
    let items = vec![(shape.id.selector(), a.clone())];
    // Mid 2 below the inner task's mid (another scope): no snap.
    let dx = s.x - 1000.0 - a.x;
    let dy = s.y + 62.0 - (a.y + a.height / 2.0);
    let at = |dx: f64, dy: f64| Bounds {
        x: a.x + dx,
        y: a.y + dy,
        ..a.clone()
    };
    let free = snap_drag(d, diagram, &items, &at(dx, dy), 7.0, false);
    assert_eq!((free.dy, free.guide_y), (0.0, None));
    // Mid 3 below the subprocess mid (a sibling): snaps.
    let dy = s.y + 103.0 - (a.y + a.height / 2.0);
    let snapped = snap_drag(d, diagram, &items, &at(dx, dy), 7.0, false);
    assert_eq!(snapped.dy, -3.0);
}

#[test]
fn choreography_scopes_are_cached_per_generation() {
    let mut app = ProcessCraft::default();
    let snap = app.ctl.session.snapshot();
    let first = app.choreography_scope_infos(&snap.project.documents[0], snap.generation);
    let again = app.choreography_scope_infos(&snap.project.documents[0], snap.generation);
    assert!(Arc::ptr_eq(&first, &again));
    app.add(NodeKind::Task);
    let snap = app.ctl.session.snapshot();
    let next = app.choreography_scope_infos(&snap.project.documents[0], snap.generation);
    assert!(!Arc::ptr_eq(&first, &next));
}

#[test]
fn pasted_bpmn_xml_is_offered_as_a_new_model() {
    let xml = include_str!("../../../../../tests/fixtures/bpmn/minimal.bpmn");
    assert!(looks_like_bpmn(xml));
    assert!(!looks_like_bpmn("Task\nReview order"));
    assert!(!looks_like_bpmn("<html><body>definitions</body></html>"));
    let mut app = ProcessCraft::default();
    let mut frame = shortcut_runner();
    frame(&mut app, vec![egui::Event::Paste("Task".into())]);
    assert!(app.canvas.paste_bpmn.is_none());
    assert!(
        app.ctl.error.contains("Nothing to paste"),
        "{}",
        app.ctl.error
    );
    frame(&mut app, vec![egui::Event::Paste(xml.into())]);
    assert!(app.canvas.paste_bpmn.is_some());
    // Importing goes through the file import preparation in the background.
    let ctx = egui::Context::default();
    let pasted = app.canvas.paste_bpmn.take().unwrap();
    app.import_bpmn_text(&ctx, pasted);
    let mut result = None;
    for _ in 0..500 {
        result = app.effects.poll();
        if result.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        matches!(result, Some(crate::effects::ResultValue::Import { .. })),
        "import did not prepare"
    );
}
