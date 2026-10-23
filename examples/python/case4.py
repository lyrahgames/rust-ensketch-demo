import graphics as gfx
from common import MODELS, make_viewer

context = gfx.GpuContext()
scene = gfx.Scene.load_obj(str(MODELS / "pyramid.obj"))
device_mesh = gfx.DeviceTriangleMesh.from_scene(context, scene, 0)
camera = gfx.Camera(); camera.frame(scene)
viewer_a = make_viewer(context, device_mesh, camera, "Python case 4 A", (640, 480))
viewer_b = make_viewer(context, device_mesh, camera, "Python case 4 B", (900, 400), (0.17, 0.08, 0.12, 1.0))
gfx.show()
