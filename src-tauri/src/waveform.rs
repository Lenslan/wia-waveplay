use std::path::Path;

use matfile::{MatFile, NumericData};

const GRAN: usize = 2;
const MIN_LEN: usize = 60;
const BW_MHZ: usize = 20;
const FRAME_INTERVAL_US: usize = 30;

#[derive(serde::Serialize, Clone)]
pub struct WaveformInfo {
    pub file_name: String,
    pub file_size: usize,
    pub sample_count: usize,
}

/// Load a waveform file. Dispatches by extension: .mat or .WAVEFORM.
pub fn load_waveform_file(file_path: &str) -> Result<(Vec<u8>, WaveformInfo), String> {
    let path = Path::new(file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "mat" => load_mat_file(path),
        "waveform" => load_waveform_raw(path),
        _ => Err(format!(
            "Unsupported file format: .{}. Supported: .mat, .WAVEFORM",
            ext
        )),
    }
}

/// Load a .mat file containing complex IQ data and convert to waveform bytes.
///
/// Mirrors the Python implementation in reference/gen_waveform.py:
///   import_mat() -> gen_wfm() -> interleaved big-endian int16 IQ bytes
fn load_mat_file(path: &Path) -> Result<(Vec<u8>, WaveformInfo), String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mat = MatFile::parse(file).map_err(|e| format!("Failed to parse .mat file: {}", e))?;

    // Find the data variable: skip MATLAB metadata variables (__header__, __version__, etc.)
    // and pick the first numeric array with more than 1 element.
    let array = mat
        .arrays()
        .iter()
        .find(|a| {
            let name = a.name();
            !(name.starts_with("__") && name.ends_with("__"))
                && a.size().iter().product::<usize>() > 1
        })
        .ok_or("No suitable data array found in .mat file")?;

    let dims = array.size();
    let (raw_real, raw_imag) = extract_f64_data(array.data())?;

    // Handle multi-dimensional arrays: take only the first row (path1).
    // MATLAB stores data column-major, so for an M×N matrix the first row
    // is at indices 0, M, 2M, 3M, …
    let (mut real, mut imag) = if dims.len() >= 2 && dims[0] > 1 {
        let num_rows = dims[0];
        let total_cols: usize = dims[1..].iter().product();
        let real: Vec<f64> = (0..total_cols).map(|c| raw_real[c * num_rows]).collect();
        let imag: Vec<f64> = (0..total_cols).map(|c| raw_imag[c * num_rows]).collect();
        (real, imag)
    } else {
        (raw_real, raw_imag)
    };

    // Append zeros for frame interval (matches Python: frame_interval_us * BW_Mhz * 2)
    let zero_count = FRAME_INTERVAL_US * BW_MHZ * 2;
    real.resize(real.len() + zero_count, 0.0);
    imag.resize(imag.len() + zero_count, 0.0);

    // Pad for granularity
    if real.len() % GRAN != 0 {
        real.push(0.0);
        imag.push(0.0);
    }

    if real.len() < MIN_LEN {
        return Err(format!(
            "Waveform length {} must be at least {}",
            real.len(),
            MIN_LEN
        ));
    }

    let sample_count = real.len();
    let wfm_bytes = gen_wfm(&real, &imag);

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let info = WaveformInfo {
        file_name,
        file_size: wfm_bytes.len(),
        sample_count,
    };

    Ok((wfm_bytes, info))
}

/// Extract real and imaginary parts as Vec<f64> from NumericData.
fn extract_f64_data(data: &NumericData) -> Result<(Vec<f64>, Vec<f64>), String> {
    match data {
        NumericData::Double { real, imag } => {
            let imag_vec = imag
                .as_ref()
                .cloned()
                .unwrap_or_else(|| vec![0.0; real.len()]);
            Ok((real.clone(), imag_vec))
        }
        NumericData::Single { real, imag } => {
            let real_f64: Vec<f64> = real.iter().map(|&v| v as f64).collect();
            let imag_f64: Vec<f64> = imag
                .as_ref()
                .map(|v| v.iter().map(|&x| x as f64).collect())
                .unwrap_or_else(|| vec![0.0; real.len()]);
            Ok((real_f64, imag_f64))
        }
        _ => Err("Unsupported data type in .mat file. Expected double or single precision float.".into()),
    }
}

/// Convert real/imag float arrays to interleaved big-endian int16 IQ bytes.
///
/// Mirrors Python gen_waveform.py: trans_wfm() + trans_wfm_iq() + interleave.
fn gen_wfm(real: &[f64], imag: &[f64]) -> Vec<u8> {
    // Determine auto-scaling based on max absolute value (mirrors trans_wfm)
    let max_val = real
        .iter()
        .chain(imag.iter())
        .map(|v| v.abs())
        .fold(0.0f64, f64::max);

    let scale = if max_val < 1.0 {
        2047.0
    } else if max_val < 10.0 {
        443.0
    } else {
        1.0
    };

    // Combined factor: scale * (32767 / 2047), mirrors trans_wfm_iq
    let factor = scale * 32767.0 / 2047.0;

    // Interleave I/Q as big-endian int16
    let mut result = Vec::with_capacity(real.len() * 4);
    for i in 0..real.len() {
        let i_val = (real[i] * factor).round().clamp(-32768.0, 32767.0) as i16;
        let q_val = (imag[i] * factor).round().clamp(-32768.0, 32767.0) as i16;
        result.extend_from_slice(&i_val.to_be_bytes());
        result.extend_from_slice(&q_val.to_be_bytes());
    }

    result
}

/// Load a pre-formatted .WAVEFORM file (raw big-endian interleaved int16 IQ).
fn load_waveform_raw(path: &Path) -> Result<(Vec<u8>, WaveformInfo), String> {
    let data =
        std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    if data.len() < 4 {
        return Err(
            "Waveform file is too small (must contain at least one IQ sample pair)".into(),
        );
    }

    if data.len() % 4 != 0 {
        return Err(format!(
            "Invalid waveform file: size {} is not a multiple of 4 bytes",
            data.len()
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let info = WaveformInfo {
        file_name,
        file_size: data.len(),
        sample_count: data.len() / 4,
    };

    Ok((data, info))
}
