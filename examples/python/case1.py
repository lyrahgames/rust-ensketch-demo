import graphics as gfx
from common import MODELS, make_viewer

context = gfx.GpuContext()
scene = gfx.HostScene.load_obj(str(MODELS / "pyramid.obj"))
camera = gfx.Camera()
camera.frame(scene)
viewer = make_viewer(context, scene, camera, "Python case 1")
gfx.show()
