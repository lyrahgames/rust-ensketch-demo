"""Run after building the Maturin extension with `--features lua,rhai`."""
import graphics as gfx

context = gfx.GpuContext()
viewer = gfx.ObjViewer(context)
viewer.eval_lua("viewer:set_background_color(0.2, 0.3, 0.4, 1)")
assert abs(float(viewer.eval_rhai("viewer.background_color()[0]")) - 0.2) < 0.0001
viewer.eval_rhai("viewer.set_background_color(0.5, 0.3, 0.4, 1.0);")
assert abs(viewer.background_color()[0] - 0.5) < 0.0001
print("Python-hosted Lua/Rhai shared viewer: ok")
