context = DeviceContext.new()
scene = Scene.load_obj("assets/models/pyramid.obj")
camera_a = Camera.new(); camera_a:frame(scene)
camera_b = Camera.new(); camera_b:frame(scene)
viewer_a = ObjViewer.new(context); viewer_a:set_scene(scene); viewer_a:set_title("Lua case 3 A")
viewer_b = ObjViewer.new(context); viewer_b:set_scene(scene); viewer_b:set_title("Lua case 3 B")
viewer_b:set_background_color(0.17, 0.08, 0.12, 1)
viewer_a:on_mouse_move(function(e)
  if e.left_pressed then camera_a:orbit(e.dx, e.dy) end
  if e.middle_pressed or e.right_pressed then camera_a:pan(e.dx, e.dy) end
end)
viewer_b:on_mouse_move(function(e)
  if e.left_pressed then camera_b:orbit(e.dx, e.dy) end
  if e.middle_pressed or e.right_pressed then camera_b:pan(e.dx, e.dy) end
end)
viewer_a:on_mouse_wheel(function(e) camera_a:zoom(e.wheel * 0.2) end)
viewer_b:on_mouse_wheel(function(e) camera_b:zoom(e.wheel * 0.2) end)
viewer_a:on_frame(function(dt) viewer_a:render(camera_a) end)
viewer_b:on_frame(function(dt) viewer_b:render(camera_b) end)
