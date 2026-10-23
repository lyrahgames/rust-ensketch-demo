context = GpuContext.new()
scene = HostScene.load_obj("assets/models/pyramid.obj")
camera = Camera.new()
camera:frame(scene)
viewer = ObjViewer.new(context)
viewer:set_scene(scene)
viewer:set_title("Lua case 1")
viewer:set_background_color(0.08, 0.09, 0.12, 1)
viewer:on_mouse_move(function(e)
  if e.left_pressed then camera:orbit(e.dx, e.dy) end
  if e.middle_pressed or e.right_pressed then camera:pan(e.dx, e.dy) end
end)
viewer:on_mouse_wheel(function(e) camera:zoom(e.wheel * 0.2) end)
viewer:on_frame(function(dt) viewer:render(camera) end)
