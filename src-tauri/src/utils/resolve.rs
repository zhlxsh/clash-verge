use super::{init, server};
use crate::{core::Profiles, log_if_err, states};
use tauri::{App, AppHandle, Manager};

/// handle something when start app
pub fn resolve_setup(app: &App) {
  resolve_window(app);

  // setup a simple http server for singleton
  server::embed_server(&app.handle());

  // init app config
  init::init_app(app.package_info());

  // init states
  let clash_state = app.state::<states::ClashState>();
  let verge_state = app.state::<states::VergeState>();
  let profiles_state = app.state::<states::ProfilesState>();

  let mut clash = clash_state.0.lock().unwrap();
  let mut verge = verge_state.0.lock().unwrap();
  let mut profiles = profiles_state.0.lock().unwrap();

  log_if_err!(clash.run_sidecar());

  *profiles = Profiles::read_file();

  clash.set_window(app.get_window("main"));
  log_if_err!(clash.activate(&profiles, true));

  verge.init_sysproxy(clash.info.port.clone());
  // enable tun mode
  if verge.config.enable_tun_mode.clone().unwrap_or(false)
    && verge.cur_sysproxy.is_some()
    && verge.cur_sysproxy.as_ref().unwrap().enable
  {
    log::info!("enable tun mode");
    clash.tun_mode(true).unwrap();
  }

  log_if_err!(verge.init_launch());
}

/// reset system proxy
pub fn resolve_reset(app_handle: &AppHandle) {
  let verge_state = app_handle.state::<states::VergeState>();
  let mut verge = verge_state.0.lock().unwrap();

  verge.reset_sysproxy();
}

/// customize the window theme
fn resolve_window(app: &App) {
  let window = app.get_window("main").unwrap();

  #[cfg(target_os = "windows")]
  {
    use window_shadows::set_shadow;
    use window_vibrancy::apply_blur;

    window.set_decorations(false).unwrap();
    set_shadow(&window, true).unwrap();
    apply_blur(&window, None).unwrap();
  }

  #[cfg(target_os = "macos")]
  {
    use tauri::LogicalSize;
    use tauri::Size::Logical;
    window.set_decorations(true).unwrap();
    window
      .set_size(Logical(LogicalSize {
        width: 800.0,
        height: 610.0,
      }))
      .unwrap();
    // use tauri_plugin_vibrancy::MacOSVibrancy;
    // #[allow(deprecated)]
    // window.apply_vibrancy(MacOSVibrancy::AppearanceBased);
  }
}
