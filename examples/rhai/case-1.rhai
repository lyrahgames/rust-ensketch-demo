let context = GpuContext();
let scene = HostScene("assets/models/pyramid.obj");
let camera = Camera(); camera.frame(scene);
let viewer = ObjViewer(context); viewer.set_scene(scene);
viewer.set_title("Rhai case 1");
viewer.set_background_color(0.08, 0.09, 0.12, 1.0);
viewer.on_mouse_move(|e| {
    if e.left_pressed { camera.orbit(e.dx, e.dy); }
    if e.middle_pressed || e.right_pressed { camera.pan(e.dx, e.dy); }
});
viewer.on_mouse_wheel(|e| { camera.zoom(e.wheel * 0.2); });
viewer.on_frame(|v,dt| { v.render(camera); });
