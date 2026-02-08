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

    let vsg = VsgInstrument::connect(&ip, 3, true)?;
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
fn load_waveform(file_path: String, bw_mhz: usize, frame_interval_us: usize, state: State<Mutex<AppState>>) -> Result<WaveformInfo, String> {
    let (data, info) = waveform::load_waveform_file(&file_path, bw_mhz, frame_interval_us)?;

    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;
    app_state.wfm_data = Some(data);

    Ok(info)
}

#[tauri::command]
fn export_waveform(file_path: String, state: State<Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;

    let wfm_data = app_state
        .wfm_data
        .as_ref()
        .ok_or("No waveform data to export")?;

    std::fs::write(&file_path, wfm_data)
        .map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
fn play_waveform(
    cf: f64,
    bw_mhz: f64,
    amp: f64,
    repeat_count: u32,
    state: State<Mutex<AppState>>,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;

    if app_state.vsg.is_none() {
        return Err("Not connected to instrument".into());
    }
    let wfm_data = app_state
        .wfm_data
        .clone()
        .ok_or("No waveform file loaded")?;

    let fs = bw_mhz * 2.0 * 1e6;
    let vsg = app_state.vsg.as_mut().unwrap();
    vsg.configure(cf, fs, amp)?;
    vsg.download_wfm(&wfm_data, "waveform")?;

    if repeat_count > 0 {
        vsg.play_with_repeat("waveform", repeat_count)?;
    } else {
        vsg.play("waveform")?;
    }

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
            export_waveform,
            play_waveform,
            stop_waveform,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
