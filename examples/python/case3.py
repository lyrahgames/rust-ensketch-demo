import graphics as gfx
from common import MODELS, make_viewer

context = gfx.GpuContext()
scene = gfx.Scene.load_obj(str(MODELS / "pyramid.obj"))
camera_a = gfx.Camera(); camera_a.frame(scene)
camera_b = gfx.Camera(); camera_b.frame(scene)
viewer_a = make_viewer(context, scene, camera_a, "Python case 3 A")
viewer_b = make_viewer(context, scene, camera_b, "Python case 3 B", color=(0.17, 0.08, 0.12, 1.0))
gfx.show()
