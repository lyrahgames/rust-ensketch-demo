context = GpuContext.new()
scene = HostScene.load_obj("assets/models/pyramid.obj")
device_scene = scene:upload(context)
camera = Camera.new(); camera:frame(scene)
viewer_a = ObjViewer.new(context); viewer_a:set_scene(device_scene); viewer_a:set_title("Lua case 4 A")
viewer_b = ObjViewer.new(context); viewer_b:set_scene(device_scene); viewer_b:set_title("Lua case 4 B")
viewer_b:set_size(900, 400)
viewer_b:set_background_color(0.17, 0.08, 0.12, 1)
local function move(e)
  if e.left_pressed then camera:orbit(e.dx, e.dy) end
  if e.middle_pressed or e.right_pressed then camera:pan(e.dx, e.dy) end
end
local function wheel(e) camera:zoom(e.wheel * 0.2) end
viewer_a:on_mouse_move(move); viewer_b:on_mouse_move(move)
viewer_a:on_mouse_wheel(wheel); viewer_b:on_mouse_wheel(wheel)
viewer_a:on_frame(function(dt) viewer_a:render(camera) end)
viewer_b:on_frame(function(dt) viewer_b:render(camera) end)
