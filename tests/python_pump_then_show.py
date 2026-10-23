"""A manually started native session can be finished with blocking show()."""
import graphics as gfx

context = gfx.GpuContext()
scene = gfx.Scene.from_bytes(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")
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
assert gfx.process_events(0)
gfx.show()
assert frames == 3, frames
print("Python manual-to-blocking event loop: ok")
