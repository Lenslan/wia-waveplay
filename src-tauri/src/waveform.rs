use std::path::Path;

#[derive(serde::Serialize, Clone)]
pub struct WaveformInfo {
    pub file_name: String,
    pub file_size: usize,
    /// Number of IQ sample pairs (each pair = 4 bytes: 2x int16)
    pub sample_count: usize,
}

/// Load a .WAVEFORM file (pre-formatted big-endian interleaved int16 IQ data).
pub fn load_waveform_file(file_path: &str) -> Result<(Vec<u8>, WaveformInfo), String> {
    let path = Path::new(file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_uppercase();

    if ext != "WAVEFORM" {
        return Err(format!(
            "Unsupported file format: .{}. Only .WAVEFORM files are supported.",
            ext
        ));
    }

    let data =
        std::fs::read(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    if data.len() < 4 {
        return Err("Waveform file is too small (must contain at least one IQ sample pair)".into());
    }

    if data.len() % 4 != 0 {
        return Err(format!(
            "Invalid waveform file: size {} is not a multiple of 4 bytes (each IQ pair = 2x int16)",
            data.len()
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let file_size = data.len();
    let sample_count = file_size / 4;

    let info = WaveformInfo {
        file_name,
        file_size,
        sample_count,
    };

    Ok((data, info))
}
