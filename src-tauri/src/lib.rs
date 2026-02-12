mod dut;
mod scpi;
mod vsg;
mod waveform;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use dut::DutClient;
use vsg::VsgInstrument;
use waveform::WaveformInfo;

struct AppState {
    vsg: Option<VsgInstrument>,
    dut: Option<DutClient>,
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
fn connect_dut(ip: String, state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;
    app_state.dut = None;

    let dut = DutClient::connect(&ip, 5)?;
    app_state.dut = Some(dut);
    Ok(())
}

#[tauri::command]
fn disconnect_dut(state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;
    app_state.dut = None;
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

#[derive(Clone, serde::Serialize)]
struct SweepProgress {
    current_power: f64,
    step_index: usize,
    total_steps: usize,
}

#[tauri::command]
fn cancel_sweep(sweep_cancel: State<Arc<AtomicBool>>) {
    sweep_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn power_sweep(
    cf: f64,
    bw_mhz: f64,
    cable_loss: f64,
    start_power: f64,
    end_power: f64,
    step: f64,
    app: AppHandle,
    state: State<Mutex<AppState>>,
    sweep_cancel: State<Arc<AtomicBool>>,
) -> Result<(), String> {
    // Reset cancel flag
    sweep_cancel.store(false, Ordering::SeqCst);
    let cancel_flag = Arc::clone(&sweep_cancel);

    let mut app_state = state.lock().map_err(|e| format!("Lock failed: {}", e))?;

    if app_state.vsg.is_none() {
        return Err("Not connected to instrument".into());
    }
    let wfm_data = app_state
        .wfm_data
        .clone()
        .ok_or("No waveform file loaded")?;

    let fs = bw_mhz * 2.0 * 1e6;

    // Destructure to allow simultaneous mutable borrows of vsg and dut
    let AppState { ref mut vsg, ref mut dut, .. } = *app_state;
    let vsg = vsg.as_mut().unwrap();

    // One-time setup: configure, download, create sequence, enable output
    vsg.prepare_sweep(&wfm_data, "waveform", cf, fs, start_power + cable_loss, 1000)?;

    // DUT parameters: carrier frequency and BW in MHz (integers for ATE command)
    let cf_mhz = (cf / 1e6).round() as u32;
    let bw = bw_mhz.round() as u32;

    if let Some(ref mut dut) = dut {
            dut.close_rx(cf_mhz)?;
        }

    // Calculate wait time for 1000 repetitions
    let sample_count = wfm_data.len() / 2;
    let wfm_duration = sample_count as f64 / fs;
    let wait_secs = wfm_duration as u64 + 100;
    let wait_duration = std::time::Duration::from_micros(wait_secs);

    // Build list of power steps
    let mut powers = Vec::new();
    let mut p = start_power;
    while p <= end_power + 1e-9 {
        powers.push(p);
        p += step;
    }
    let total_steps = powers.len();

    for (i, &power) in powers.iter().enumerate() {
        if cancel_flag.load(Ordering::SeqCst) {
            break;
        }

        // Open DUT RX before triggering
        if let Some(ref mut dut) = dut {
            dut.open_rx(cf_mhz, bw)?;
        }

        vsg.set_power(power + cable_loss)?;
        vsg.trigger()?;
        std::thread::sleep(wait_duration);

        // Close DUT RX after playback completes
        if let Some(ref mut dut) = dut {
            dut.read_mib(cf_mhz)?;
            dut.close_rx(cf_mhz)?;
        }

        let _ = app.emit(
            "sweep-progress",
            SweepProgress {
                current_power: power,
                step_index: i + 1,
                total_steps,
            },
        );
    }

    vsg.stop()?;
    let _ = app.emit("sweep-done", ());

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(AppState {
            vsg: None,
            dut: None,
            wfm_data: None,
        }))
        .manage(Arc::new(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            connect_instrument,
            disconnect_instrument,
            connect_dut,
            disconnect_dut,
            load_waveform,
            export_waveform,
            play_waveform,
            stop_waveform,
            power_sweep,
            cancel_sweep,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
