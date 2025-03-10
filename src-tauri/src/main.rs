#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

mod cmds;
mod core;
mod states;
mod utils;

use crate::utils::{resolve, server};
use tauri::{
  api, CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
};

fn main() -> std::io::Result<()> {
  if server::check_singleton().is_err() {
    println!("app exists");
    return Ok(());
  }

  let tray_menu = SystemTrayMenu::new()
    .add_item(CustomMenuItem::new("open_window", "Show"))
    .add_item(CustomMenuItem::new("restart_clash", "Restart Clash"))
    .add_native_item(SystemTrayMenuItem::Separator)
    .add_item(CustomMenuItem::new("quit", "Quit").accelerator("CmdOrControl+Q"));

  #[allow(unused_mut)]
  let mut builder = tauri::Builder::default()
    .manage(states::VergeState::default())
    .manage(states::ClashState::default())
    .manage(states::ProfilesState::default())
    .setup(|app| Ok(resolve::resolve_setup(app)))
    .system_tray(SystemTray::new().with_menu(tray_menu))
    .on_system_tray_event(move |app_handle, event| match event {
      SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
        "open_window" => {
          let window = app_handle.get_window("main").unwrap();
          window.unminimize().unwrap();
          window.show().unwrap();
          window.set_focus().unwrap();
        }
        "restart_clash" => {
          let clash_state = app_handle.state::<states::ClashState>();
          let profiles_state = app_handle.state::<states::ProfilesState>();
          let mut clash = clash_state.0.lock().unwrap();
          let mut profiles = profiles_state.0.lock().unwrap();

          crate::log_if_err!(clash.restart_sidecar(&mut profiles));
        }
        "quit" => {
          resolve::resolve_reset(app_handle);
          api::process::kill_children();
          std::process::exit(0);
        }
        _ => {}
      },
      SystemTrayEvent::LeftClick { .. } => {
        if cfg![target_os = "windows"] {
          let window = app_handle.get_window("main").unwrap();
          window.unminimize().unwrap();
          window.show().unwrap();
          window.set_focus().unwrap();
        }
      }
      _ => {}
    })
    .invoke_handler(tauri::generate_handler![
      // common
      cmds::restart_sidecar,
      cmds::get_sys_proxy,
      cmds::get_cur_proxy,
      cmds::kill_sidecars,
      cmds::open_app_dir,
      cmds::open_logs_dir,
      // clash
      cmds::get_clash_info,
      cmds::patch_clash_config,
      // verge
      cmds::get_verge_config,
      cmds::patch_verge_config,
      // profile
      cmds::view_profile,
      cmds::patch_profile,
      cmds::create_profile,
      cmds::import_profile,
      cmds::update_profile,
      cmds::delete_profile,
      cmds::select_profile,
      cmds::get_profiles,
      cmds::sync_profiles,
      cmds::enhance_profiles,
      cmds::change_profile_chain
    ]);

  #[cfg(target_os = "macos")]
  {
    use tauri::{Menu, MenuItem, Submenu};

    let submenu_file = Submenu::new(
      "File",
      Menu::new()
        .add_native_item(MenuItem::Undo)
        .add_native_item(MenuItem::Redo)
        .add_native_item(MenuItem::Copy)
        .add_native_item(MenuItem::Paste)
        .add_native_item(MenuItem::Cut)
        .add_native_item(MenuItem::SelectAll),
    );
    builder = builder.menu(Menu::new().add_submenu(submenu_file));
  }

  builder
    .build(tauri::generate_context!())
    .expect("error while running tauri application")
    .run(|app_handle, e| match e {
      tauri::RunEvent::CloseRequested { label, api, .. } => {
        let app_handle = app_handle.clone();
        api.prevent_close();
        app_handle.get_window(&label).unwrap().hide().unwrap();
      }
      tauri::RunEvent::ExitRequested { .. } => {
        resolve::resolve_reset(app_handle);
        api::process::kill_children();
      }
      _ => {}
    });

  Ok(())
}
