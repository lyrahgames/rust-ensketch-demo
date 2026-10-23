from pathlib import Path
import graphics as gfx

MODELS = Path(__file__).resolve().parents[2] / "assets" / "models"

def make_viewer(context, scene, camera, title, size=(640, 480), color=(0.08, 0.09, 0.12, 1.0)):
    viewer = gfx.ObjViewer(context)
    viewer.set_scene(scene)
    viewer.set_title(title)
    viewer.set_size(*size)
    viewer.set_background_color(*color)

    def move(event):
        if event.left_pressed:
            camera.orbit(event.dx, event.dy)
        if event.middle_pressed or event.right_pressed:
            camera.pan(event.dx, event.dy)

    def wheel(event):
        camera.zoom(event.wheel * 0.2)

    def frame(dt):
        viewer.render(camera)

    viewer.on_mouse_move(move)
    viewer.on_mouse_wheel(wheel)
    viewer.on_frame(frame)
    return viewer
