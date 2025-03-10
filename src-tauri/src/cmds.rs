use crate::{
  core::{ClashInfo, PrfItem, PrfOption, Profiles, VergeConfig},
  states::{ClashState, ProfilesState, VergeState},
  utils::{dirs, sysopt::SysProxyConfig},
};
use crate::{ret_err, wrap_err};
use anyhow::Result;
use serde_yaml::Mapping;
use std::{path::PathBuf, process::Command};
use tauri::{api, Manager, State};

/// get all profiles from `profiles.yaml`
#[tauri::command]
pub fn get_profiles<'a>(profiles_state: State<'_, ProfilesState>) -> Result<Profiles, String> {
  let profiles = profiles_state.0.lock().unwrap();
  Ok(profiles.clone())
}

/// synchronize data irregularly
#[tauri::command]
pub fn sync_profiles(profiles_state: State<'_, ProfilesState>) -> Result<(), String> {
  let mut profiles = profiles_state.0.lock().unwrap();
  wrap_err!(profiles.sync_file())
}

/// import the profile from url
/// and save to `profiles.yaml`
#[tauri::command]
pub async fn import_profile(
  url: String,
  option: Option<PrfOption>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let item = wrap_err!(PrfItem::from_url(&url, None, None, option).await)?;

  let mut profiles = profiles_state.0.lock().unwrap();
  wrap_err!(profiles.append_item(item))
}

/// new a profile
/// append a temp profile item file to the `profiles` dir
/// view the temp profile file by using vscode or other editor
#[tauri::command]
pub async fn create_profile(
  item: PrfItem, // partial
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let item = wrap_err!(PrfItem::from(item).await)?;
  let mut profiles = profiles_state.0.lock().unwrap();

  wrap_err!(profiles.append_item(item))
}

/// Update the profile
#[tauri::command]
pub async fn update_profile(
  index: String,
  option: Option<PrfOption>,
  clash_state: State<'_, ClashState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let (url, opt) = {
    // must release the lock here
    let profiles = profiles_state.0.lock().unwrap();
    let item = wrap_err!(profiles.get_item(&index))?;

    // check the profile type
    if let Some(typ) = item.itype.as_ref() {
      if *typ != "remote" {
        ret_err!(format!("could not update the `{typ}` profile"));
      }
    }

    if item.url.is_none() {
      ret_err!("failed to get the item url");
    }

    (item.url.clone().unwrap(), item.option.clone())
  };

  let fetch_opt = PrfOption::merge(opt, option);
  let item = wrap_err!(PrfItem::from_url(&url, None, None, fetch_opt).await)?;

  let mut profiles = profiles_state.0.lock().unwrap();
  wrap_err!(profiles.update_item(index.clone(), item))?;

  // reactivate the profile
  if Some(index) == profiles.get_current() {
    let clash = clash_state.0.lock().unwrap();
    wrap_err!(clash.activate(&profiles, false))?;
  }

  Ok(())
}

/// change the current profile
#[tauri::command]
pub fn select_profile(
  index: String,
  clash_state: State<'_, ClashState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let mut profiles = profiles_state.0.lock().unwrap();
  wrap_err!(profiles.put_current(index))?;

  let clash = clash_state.0.lock().unwrap();
  wrap_err!(clash.activate(&profiles, false))
}

/// change the profile chain
#[tauri::command]
pub fn change_profile_chain(
  chain: Option<Vec<String>>,
  app_handle: tauri::AppHandle,
  clash_state: State<'_, ClashState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let mut clash = clash_state.0.lock().unwrap();
  let mut profiles = profiles_state.0.lock().unwrap();

  profiles.put_chain(chain);
  clash.set_window(app_handle.get_window("main"));

  wrap_err!(clash.activate_enhanced(&profiles, false))
}

/// manually exec enhanced profile
#[tauri::command]
pub fn enhance_profiles(
  app_handle: tauri::AppHandle,
  clash_state: State<'_, ClashState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let mut clash = clash_state.0.lock().unwrap();
  let profiles = profiles_state.0.lock().unwrap();

  clash.set_window(app_handle.get_window("main"));

  wrap_err!(clash.activate_enhanced(&profiles, false))
}

/// delete profile item
#[tauri::command]
pub fn delete_profile(
  index: String,
  clash_state: State<'_, ClashState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let mut profiles = profiles_state.0.lock().unwrap();

  if wrap_err!(profiles.delete_item(index))? {
    let clash = clash_state.0.lock().unwrap();
    wrap_err!(clash.activate(&profiles, false))?;
  }

  Ok(())
}

/// patch the profile config
#[tauri::command]
pub fn patch_profile(
  index: String,
  profile: PrfItem,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let mut profiles = profiles_state.0.lock().unwrap();
  wrap_err!(profiles.patch_item(index, profile))
}

/// run vscode command to edit the profile
#[tauri::command]
pub fn view_profile(index: String, profiles_state: State<'_, ProfilesState>) -> Result<(), String> {
  let profiles = profiles_state.0.lock().unwrap();
  let item = wrap_err!(profiles.get_item(&index))?;

  let file = item.file.clone();
  if file.is_none() {
    ret_err!("the file is null");
  }

  let path = dirs::app_profiles_dir().join(file.unwrap());
  if !path.exists() {
    ret_err!("the file not found");
  }

  // use vscode first
  if let Ok(code) = which::which("code") {
    #[cfg(target_os = "windows")]
    {
      use std::os::windows::process::CommandExt;

      if let Err(err) = Command::new(code)
        .creation_flags(0x08000000)
        .arg(path)
        .spawn()
      {
        log::error!("{err}");
        return Err("failed to open file by VScode".into());
      }
    }

    #[cfg(not(target_os = "windows"))]
    if let Err(err) = Command::new(code).arg(path).spawn() {
      log::error!("{err}");
      return Err("failed to open file by VScode".into());
    }

    return Ok(());
  }

  open_path_cmd(path, "failed to open file by `open`")
}

/// restart the sidecar
#[tauri::command]
pub fn restart_sidecar(
  clash_state: State<'_, ClashState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let mut clash = clash_state.0.lock().unwrap();
  let mut profiles = profiles_state.0.lock().unwrap();

  wrap_err!(clash.restart_sidecar(&mut profiles))
}

/// get the clash core info from the state
/// the caller can also get the infomation by clash's api
#[tauri::command]
pub fn get_clash_info(clash_state: State<'_, ClashState>) -> Result<ClashInfo, String> {
  let clash = clash_state.0.lock().unwrap();
  Ok(clash.info.clone())
}

/// update the clash core config
/// after putting the change to the clash core
/// then we should save the latest config
#[tauri::command]
pub fn patch_clash_config(
  payload: Mapping,
  clash_state: State<'_, ClashState>,
  verge_state: State<'_, VergeState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let mut clash = clash_state.0.lock().unwrap();
  let mut verge = verge_state.0.lock().unwrap();
  let mut profiles = profiles_state.0.lock().unwrap();
  wrap_err!(clash.patch_config(payload, &mut verge, &mut profiles))
}

/// get the system proxy
#[tauri::command]
pub fn get_sys_proxy() -> Result<SysProxyConfig, String> {
  wrap_err!(SysProxyConfig::get_sys())
}

/// get the current proxy config
/// which may not the same as system proxy
#[tauri::command]
pub fn get_cur_proxy(verge_state: State<'_, VergeState>) -> Result<Option<SysProxyConfig>, String> {
  let verge = verge_state.0.lock().unwrap();
  Ok(verge.cur_sysproxy.clone())
}

/// get the verge config
#[tauri::command]
pub fn get_verge_config(verge_state: State<'_, VergeState>) -> Result<VergeConfig, String> {
  let verge = verge_state.0.lock().unwrap();
  let mut config = verge.config.clone();

  if config.system_proxy_bypass.is_none() && verge.cur_sysproxy.is_some() {
    config.system_proxy_bypass = Some(verge.cur_sysproxy.clone().unwrap().bypass)
  }

  Ok(config)
}

/// patch the verge config
/// this command only save the config and not responsible for other things
#[tauri::command]
pub fn patch_verge_config(
  payload: VergeConfig,
  clash_state: State<'_, ClashState>,
  verge_state: State<'_, VergeState>,
  profiles_state: State<'_, ProfilesState>,
) -> Result<(), String> {
  let tun_mode = payload.enable_tun_mode.clone();

  // change tun mode
  if tun_mode.is_some() {
    let mut clash = clash_state.0.lock().unwrap();
    let profiles = profiles_state.0.lock().unwrap();

    wrap_err!(clash.tun_mode(tun_mode.unwrap()))?;
    clash.update_config();
    wrap_err!(clash.activate(&profiles, false))?;
  }

  let mut verge = verge_state.0.lock().unwrap();
  wrap_err!(verge.patch_config(payload))?;

  Ok(())
}

/// kill all sidecars when update app
#[tauri::command]
pub fn kill_sidecars() {
  api::process::kill_children();
}

/// open app config dir
#[tauri::command]
pub fn open_app_dir() -> Result<(), String> {
  let app_dir = dirs::app_home_dir();
  open_path_cmd(app_dir, "failed to open app dir")
}

/// open logs dir
#[tauri::command]
pub fn open_logs_dir() -> Result<(), String> {
  let log_dir = dirs::app_logs_dir();
  open_path_cmd(log_dir, "failed to open logs dir")
}

/// use the os default open command to open file or dir
fn open_path_cmd(path: PathBuf, err_str: &str) -> Result<(), String> {
  let result;

  #[cfg(target_os = "windows")]
  {
    use std::os::windows::process::CommandExt;

    result = Command::new("explorer")
      .creation_flags(0x08000000)
      .arg(&path)
      .spawn();
  }

  #[cfg(target_os = "macos")]
  {
    result = Command::new("open").arg(&path).spawn();
  }

  #[cfg(target_os = "linux")]
  {
    result = Command::new("xdg-open").arg(&path).spawn();
  }

  match result {
    Ok(child) => match child.wait_with_output() {
      Ok(out) => {
        // 退出码不为0 不一定没有调用成功
        // 因此仅做warn log且不返回错误
        if let Some(code) = out.status.code() {
          if code != 0 {
            log::warn!("failed to open {:?} (code {})", &path, code);
            log::warn!(
              "open cmd stdout: {}, stderr: {}",
              String::from_utf8_lossy(&out.stdout),
              String::from_utf8_lossy(&out.stderr),
            );
          }
        }
      }
      Err(err) => {
        log::error!("failed to open {:?} for {err}", &path);
        return Err(err_str.into());
      }
    },
    Err(err) => {
      log::error!("failed to open {:?} for {err}", &path);
      return Err(err_str.into());
    }
  }

  return Ok(());
}
