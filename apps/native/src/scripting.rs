//! Native scripting-runtime setup and callback adapters.
use crate::HostEvent;
#[cfg(any(feature = "lua", feature = "rhai"))]
use crate::{NativeApp, ViewerConfig};
#[cfg(any(feature = "lua", feature = "rhai"))]
use engine::Camera;
use engine::Executor;
#[cfg(any(feature = "lua", feature = "rhai"))]
use std::{cell::RefCell, rc::Rc};
use winit::event_loop::EventLoopProxy;

pub(super) struct ScriptState {
  pub(super) default_language: &'static str,
  #[cfg(feature = "lua")]
  pub(super) lua:              Option<Rc<script_lua::LuaRuntime>>,
  #[cfg(feature = "rhai")]
  pub(super) rhai:             Option<Rc<RefCell<script_rhai::RhaiRuntime>>>,
}

impl ScriptState {
  fn eval(&mut self, language: &str, source: &str) -> String {
    #[cfg(not(any(feature = "lua", feature = "rhai")))]
    let _ = source;
    match language {
      #[cfg(feature = "lua")]
      "lua" => self
        .lua
        .as_ref()
        .map(|runtime| {
          runtime
            .eval(source)
            .map(|_| "ok".into())
            .unwrap_or_else(|error| error.to_string())
        })
        .unwrap_or("Lua is not enabled".into()),
      #[cfg(feature = "rhai")]
      "rhai" => self
        .rhai
        .as_ref()
        .map(|runtime| {
          runtime
            .borrow_mut()
            .eval(source)
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|error| error.to_string())
        })
        .unwrap_or("Rhai is not enabled".into()),
      _ => "unknown language".into(),
    }
  }
}

pub(super) fn start_repl(
  state: ScriptState,
  proxy: EventLoopProxy<HostEvent>,
) -> Executor<ScriptState> {
  let default_language = state.default_language.to_owned();
  let wake = proxy.clone();
  let executor = Executor::new_main(state).with_wakeup(move || {
    let _ = wake.send_event(HostEvent::Wake);
  });
  let submitter = executor.submitter();
  std::thread::spawn(move || {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut language = default_language;
    for line in stdin.lock().lines() {
      let Ok(line) = line else { break };
      let source = line.trim();
      if source == ":quit" {
        let _ = proxy.send_event(HostEvent::Exit);
        break;
      }
      if source == ":lua" || source == ":rhai" {
        language = source[1..].into();
        println!("{language}");
        continue;
      }
      if source == ":main" {
        println!("main executor");
        continue;
      }
      if source.is_empty() {
        continue;
      }
      let language = language.clone();
      let source = source.to_owned();
      match submitter.call(move |state| state.eval(&language, &source)) {
        Ok(task) => match pollster::block_on(task) {
          Ok(output) => println!("{output}"),
          Err(error) => eprintln!("task failed: {error:?}"),
        },
        Err(error) => {
          eprintln!("executor closed: {error:?}");
          break;
        }
      }
    }
  });
  executor
}

#[cfg(feature = "lua")]
pub fn run_lua(path: &str) -> Result<(), String> {
  use script_lua::LuaRuntime;
  let runtime = Rc::new(LuaRuntime::new().map_err(|e| e.to_string())?);
  runtime
    .eval(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
    .map_err(|e| e.to_string())?;
  let mut app = NativeApp::new();
  app.script_state = Some(ScriptState {
    default_language: "lua",
    lua: Some(runtime.clone()),
    #[cfg(feature = "rhai")]
    rhai: None,
  });
  for scripted in runtime.viewers() {
    let mut config = ViewerConfig::new(
      scripted.viewer.clone(),
      Rc::new(RefCell::new(Camera::default())),
    );
    let viewer = scripted.clone();
    let runtime_ref = runtime.clone();
    config.on_input = Some(Box::new(move |event| {
      if let Err(error) = viewer.dispatch_input(runtime_ref.lua(), event) {
        eprintln!("Lua callback: {error}");
      }
    }));
    let viewer = scripted.clone();
    config.on_frame = Some(Box::new(move |dt| {
      if let Err(error) = viewer.dispatch_frame(dt) {
        eprintln!("Lua frame: {error}");
      }
    }));
    app.add_viewer(config);
  }
  app.show()
}

#[cfg(feature = "rhai")]
pub fn run_rhai(path: &str) -> Result<(), String> {
  use script_rhai::RhaiRuntime;
  let runtime = Rc::new(RefCell::new(RhaiRuntime::new()));
  let _ = runtime
    .borrow_mut()
    .eval(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
    .map_err(|e| e.to_string())?;
  let mut app = NativeApp::new();
  app.script_state = Some(ScriptState {
    default_language: "rhai",
    #[cfg(feature = "lua")]
    lua: None,
    rhai: Some(runtime.clone()),
  });
  for scripted in runtime.borrow().viewers() {
    let mut config = ViewerConfig::new(
      scripted.viewer.clone(),
      Rc::new(RefCell::new(Camera::default())),
    );
    let viewer = scripted.clone();
    let runtime_ref = runtime.clone();
    config.on_input = Some(Box::new(move |event| {
      let runtime = runtime_ref.borrow();
      if let Err(error) = viewer.dispatch_input(runtime.engine(), runtime.ast(), event) {
        eprintln!("Rhai callback: {error}");
      }
    }));
    let viewer = scripted.clone();
    let runtime_ref = runtime.clone();
    config.on_frame = Some(Box::new(move |dt| {
      let runtime = runtime_ref.borrow();
      if let Err(error) = viewer.dispatch_frame(runtime.engine(), runtime.ast(), dt) {
        eprintln!("Rhai frame: {error}");
      }
    }));
    app.add_viewer(config);
  }
  app.show()
}

/// Run Lua-owned windows with the first viewer explicitly shared into Rhai.
#[cfg(all(feature = "lua", feature = "rhai"))]
pub fn run_both(lua_path: &str, rhai_path: &str) -> Result<(), String> {
  let lua = Rc::new(script_lua::LuaRuntime::new().map_err(|e| e.to_string())?);
  lua
    .eval(&std::fs::read_to_string(lua_path).map_err(|e| e.to_string())?)
    .map_err(|e| e.to_string())?;
  let scripted_viewers = lua.viewers();
  let mut rhai_runtime = script_rhai::RhaiRuntime::new();
  if let Some(first) = scripted_viewers.first() {
    rhai_runtime.expose_viewer("viewer", first.viewer.clone());
  }
  let _ = rhai_runtime
    .eval(&std::fs::read_to_string(rhai_path).map_err(|e| e.to_string())?)
    .map_err(|e| e.to_string())?;
  let rhai = Rc::new(RefCell::new(rhai_runtime));
  let mut app = NativeApp::new();
  app.script_state = Some(ScriptState {
    default_language: "lua",
    lua:              Some(lua.clone()),
    rhai:             Some(rhai),
  });
  for scripted in scripted_viewers {
    let mut config = ViewerConfig::new(
      scripted.viewer.clone(),
      Rc::new(RefCell::new(Camera::default())),
    );
    let viewer = scripted.clone();
    let runtime = lua.clone();
    config.on_input = Some(Box::new(move |event| {
      if let Err(error) = viewer.dispatch_input(runtime.lua(), event) {
        eprintln!("Lua callback: {error}");
      }
    }));
    let viewer = scripted.clone();
    config.on_frame = Some(Box::new(move |dt| {
      if let Err(error) = viewer.dispatch_frame(dt) {
        eprintln!("Lua frame: {error}");
      }
    }));
    app.add_viewer(config);
  }
  app.show()
}
