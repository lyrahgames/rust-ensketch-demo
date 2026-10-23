"""Run with the installed extension and an available X11/Wayland display."""
import graphics as gfx

context = gfx.GpuContext()
scene = gfx.Scene.from_bytes(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")
camera = gfx.Camera()
camera.frame(scene)
viewer_a = gfx.ObjViewer(context)
viewer_b = gfx.ObjViewer(context)
viewer_a.set_scene(scene)
viewer_b.set_scene(scene)
frames = [0, 0]
resizes = []

def frame_a(dt):
    frames[0] += 1
    viewer_a.render(camera)
    if frames[0] == 3:
        viewer_a.close()

def frame_b(dt):
    frames[1] += 1
    viewer_b.render(camera)
    if frames[1] == 1:
        viewer_b.set_size(700, 400)
    if frames[1] == 5:
        viewer_b.close()

viewer_a.on_frame(frame_a)
viewer_b.on_frame(frame_b)
viewer_b.on_resize(lambda event: resizes.append((event.width, event.height)))
gfx.show()
assert frames == [3, 5], frames
assert (700, 400) in resizes, resizes
print("Python windows, callbacks, and independent close: ok")
