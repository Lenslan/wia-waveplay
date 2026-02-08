mod scpi;
mod vsg;
mod waveform;

use std::sync::Mutex;
use tauri::State;
use vsg::VsgInstrument;
use waveform::WaveformInfo;

struct AppState {
    vsg: Option<VsgInstrument>,
    wfm_data: Option<Vec<u8>>,
}

#[tauri::command]
fn connect_instrument(ip: String, state: State<Mutex<AppState>>) -> Result<String, String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;

    // Disconnect existing connection first
    if let Some(ref mut vsg) = app_state.vsg {
        let _ = vsg.stop();
    }
    app_state.vsg = None;

    let vsg = VsgInstrument::connect(&ip, 10, true)?;
    let inst_id = vsg.inst_id.clone();
    app_state.vsg = Some(vsg);

    Ok(inst_id)
}

#[tauri::command]
fn disconnect_instrument(state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;

    if let Some(ref mut vsg) = app_state.vsg {
        let _ = vsg.stop();
    }
    app_state.vsg = None;

    Ok(())
}

#[tauri::command]
fn load_waveform(file_path: String, state: State<Mutex<AppState>>) -> Result<WaveformInfo, String> {
    let (data, info) = waveform::load_waveform_file(&file_path)?;

    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;
    app_state.wfm_data = Some(data);

    Ok(info)
}

#[tauri::command]
fn play_waveform(cf: f64, fs: f64, amp: f64, state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;

    if app_state.vsg.is_none() {
        return Err("Not connected to instrument".into());
    }
    let wfm_data = app_state
        .wfm_data
        .clone()
        .ok_or("No waveform file loaded")?;

    let vsg = app_state.vsg.as_mut().unwrap();
    vsg.configure(cf, fs, amp)?;
    vsg.download_wfm(&wfm_data, "waveform")?;
    vsg.play("waveform")?;

    Ok(())
}

#[tauri::command]
fn stop_waveform(state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;

    let vsg = app_state
        .vsg
        .as_mut()
        .ok_or("Not connected to instrument")?;
    vsg.stop()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(AppState {
            vsg: None,
            wfm_data: None,
        }))
        .invoke_handler(tauri::generate_handler![
            connect_instrument,
            disconnect_instrument,
            load_waveform,
            play_waveform,
            stop_waveform,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
