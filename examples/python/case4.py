import graphics as gfx
from common import MODELS, make_viewer

context = gfx.GpuContext()
scene = gfx.HostScene.load_obj(str(MODELS / "pyramid.obj"))
device_scene = scene.upload(context)
camera = gfx.Camera(); camera.frame(scene)
viewer_a = make_viewer(context, device_scene, camera, "Python case 4 A", (640, 480))
viewer_b = make_viewer(context, device_scene, camera, "Python case 4 B", (900, 400), (0.17, 0.08, 0.12, 1.0))
gfx.show()
