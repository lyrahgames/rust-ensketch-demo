import graphics as gfx
from common import MODELS, make_viewer

context = gfx.GpuContext()
scene_a = gfx.Scene.load_obj(str(MODELS / "pyramid.obj"))
scene_b = gfx.Scene.load_obj(str(MODELS / "cube.obj"))
camera_a = gfx.Camera(); camera_a.frame(scene_a)
camera_b = gfx.Camera(); camera_b.frame(scene_b)
viewer_a = make_viewer(context, scene_a, camera_a, "Python case 2 A")
viewer_b = make_viewer(context, scene_b, camera_b, "Python case 2 B", color=(0.17, 0.08, 0.12, 1.0))
gfx.show()
