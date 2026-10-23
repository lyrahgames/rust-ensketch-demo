"""Manual winit event pumping, with Python retaining its own outer loop."""
import graphics as gfx

context = gfx.GpuContext()
scene = gfx.HostScene.from_bytes(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")
camera = gfx.Camera()
camera.frame(scene)
viewer = gfx.ObjViewer(context)
viewer.set_scene(scene)
frames = 0


def on_frame(dt):
    global frames
    frames += 1
    viewer.render(camera)
    if frames == 3:
        viewer.close()


viewer.on_frame(on_frame)
for _ in range(200):
    if not gfx.process_events(20):
        break
else:
    raise AssertionError("manual event pump did not exit")

assert frames == 3, frames
print("Python manual event pump: ok")
