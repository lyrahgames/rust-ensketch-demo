import init, { GpuContext, HostScene, Camera, ObjViewer } from '../../pkg/script_js.js';

export function pointerEvent(event, dx = 0, dy = 0) {
  return {
    kind: 'mouse_move', dx, dy, x: event.offsetX, y: event.offsetY, wheel: 0,
    left_pressed: Boolean(event.buttons & 1),
    middle_pressed: Boolean(event.buttons & 4),
    right_pressed: Boolean(event.buttons & 2),
  };
}

export function wheelEvent(event) {
  return { ...pointerEvent(event), kind: 'mouse_wheel', wheel: -event.deltaY / (event.deltaMode === 1 ? 1 : 40) };
}

export function buttonEvent(event) {
  return { ...pointerEvent(event), kind: 'mouse_button', button: event.button, pressed: event.type === 'pointerdown' };
}

export function keyEvent(event) {
  return { kind: 'key', key: event.code, pressed: event.type === 'keydown' };
}

export function resizeEvent(width, height) {
  return { kind: 'resize', width, height };
}

export function closeRequestEvent() {
  return { kind: 'close_request' };
}

export async function run(caseNumber) {
  await init();
  const context = await GpuContext.create();
  const pyramid = await HostScene.loadObjUrl(new URL('../../assets/models/pyramid.obj', import.meta.url).href);
  const cube = caseNumber === 2
    ? await HostScene.loadObjUrl(new URL('../../assets/models/cube.obj', import.meta.url).href)
    : null;
  const deviceScene = caseNumber === 4 ? pyramid.upload(context) : null;
  const canvases = [...document.querySelectorAll('canvas')];
  const listeners = new AbortController();
  const sharedCamera = new Camera();
  sharedCamera.frame(pyramid);
  const viewers = canvases.map((canvas, index) => {
    canvas.style.width = `${canvas.width}px`;
    canvas.style.height = 'auto';
    const viewer = new ObjViewer(context, canvas);
    const scene = caseNumber === 2 && index === 1 ? cube : pyramid;
    if (deviceScene) viewer.setDeviceScene(deviceScene);
    else viewer.setHostScene(scene);
    viewer.setBackgroundColor(...(index === 0 ? [0.08, 0.09, 0.12, 1] : [0.17, 0.08, 0.12, 1]));
    const camera = caseNumber === 4 || index === 0 ? sharedCamera : new Camera();
    if (camera !== sharedCamera) camera.frame(scene);
    let last = null;
    canvas.addEventListener('pointerdown', event => {
      const input = buttonEvent(event);
      if (input.pressed) { canvas.setPointerCapture(event.pointerId); last = [event.clientX, event.clientY]; }
    }, { signal: listeners.signal });
    canvas.addEventListener('pointerup', () => { last = null; }, { signal: listeners.signal });
    canvas.addEventListener('pointermove', event => {
      if (!last) return;
      const dx = event.clientX - last[0], dy = event.clientY - last[1];
      last = [event.clientX, event.clientY];
      const input = pointerEvent(event, dx, dy);
      if (input.left_pressed) camera.orbit(input.dx, input.dy);
      if (input.right_pressed || input.middle_pressed) camera.pan(input.dx, input.dy);
    }, { signal: listeners.signal });
    canvas.addEventListener('contextmenu', event => event.preventDefault(), { signal: listeners.signal });
    canvas.addEventListener('wheel', event => { event.preventDefault(); camera.zoom(wheelEvent(event).wheel * 0.2); }, { passive: false, signal: listeners.signal });
    const observer = new ResizeObserver(() => {
      const rect = canvas.getBoundingClientRect();
      const event = resizeEvent(
        Math.max(1, Math.round(rect.width * devicePixelRatio)),
        Math.max(1, Math.round(rect.height * devicePixelRatio)),
      );
      if (canvas.width !== event.width || canvas.height !== event.height) viewer.resize(event.width, event.height);
    });
    observer.observe(canvas);
    return { viewer, camera, scene, canvas, observer };
  });
  window.addEventListener('keydown', event => {
    const input = keyEvent(event);
    if (input.key === 'KeyR' && input.pressed) {
      for (const { camera, scene } of viewers) { camera.reset(); camera.frame(scene); }
    }
  }, { signal: listeners.signal });
  let running = true;
  let frameHandle = 0;
  function frame() {
    if (!running) return;
    for (const { viewer, camera } of viewers) viewer.render(camera);
    frameHandle = requestAnimationFrame(frame);
  }
  frameHandle = requestAnimationFrame(frame);
  window.addEventListener('pagehide', () => {
    running = false;
    cancelAnimationFrame(frameHandle);
    listeners.abort();
    for (const { viewer, observer } of viewers) { observer.disconnect(); viewer.close(); viewer.free(); }
    for (const camera of new Set(viewers.map(item => item.camera))) camera.free();
    if (deviceScene) deviceScene.free();
    if (cube) cube.free();
    pyramid.free();
    context.free();
  }, { once: true });
  window.addEventListener('pageshow', event => {
    if (event.persisted) location.reload();
  });
  document.querySelectorAll('input[type=color]').forEach((input, i) => {
    input.addEventListener('input', () => {
      const color = input.value;
      viewers[i].viewer.setBackgroundColor(parseInt(color.slice(1,3),16)/255, parseInt(color.slice(3,5),16)/255, parseInt(color.slice(5,7),16)/255, 1);
    }, { signal: listeners.signal });
  });
}
