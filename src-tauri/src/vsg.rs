use crate::scpi::ScpiClient;

/// Controller for Keysight EXG/MXG/PSG/M938x Vector Signal Generators.
///
/// Implements SCPI-based instrument control for waveform download and playback.
/// Reference: pyarbtools VSG class in reference/pyarbtools/instruments.py
pub struct VsgInstrument {
    client: ScpiClient,
    pub inst_id: String,
}

impl VsgInstrument {
    /// Connect to a VSG at the given IP address (port 5025).
    /// If `reset` is true, sends *RST and waits for completion.
    pub fn connect(ip: &str, timeout_secs: u64, reset: bool) -> Result<Self, String> {
        let mut client = ScpiClient::connect(ip, 5025, timeout_secs)?;

        if reset {
            client.write_cmd("*rst")?;
            client.query("*opc?")?;
        }

        let inst_id = client.query("*idn?")?;

        Ok(Self { client, inst_id })
    }

    /// Configure the VSG with carrier frequency, sample rate, and output power.
    ///
    /// - `cf`: carrier frequency in Hz
    /// - `fs`: ARB sample clock rate in Hz
    /// - `amp`: output power in dBm
    pub fn configure(&mut self, cf: f64, fs: f64, amp: f64) -> Result<(), String> {
        self.client
            .write_cmd(&format!("frequency {}", cf))?;
        self.client
            .write_cmd(&format!("radio:arb:sclock:rate {}", fs))?;
        self.client
            .write_cmd(&format!("power {}", amp))?;
        self.client.err_check()
    }

    /// Download a pre-formatted waveform (big-endian interleaved int16 IQ) to the instrument.
    ///
    /// `wfm_data` should be raw bytes from a .WAVEFORM file.
    pub fn download_wfm(&mut self, wfm_data: &[u8], wfm_id: &str) -> Result<(), String> {
        // Stop output before downloading
        self.client.write_cmd("output:modulation 0")?;
        self.client.write_cmd("radio:arb:state 0")?;

        // Download waveform binary data using IEEE 488.2 block format
        let cmd = format!("mmemory:data \"WFM1:{}\",", wfm_id);
        self.client.write_binary_block(&cmd, wfm_data)?;

        // Select the uploaded waveform
        self.client
            .write_cmd(&format!("radio:arb:waveform \"WFM1:{}\"", wfm_id))?;

        self.client.err_check()
    }

    /// Activate arb playback: select waveform, enable RF output, modulation, and arb state.
    /// Plays the waveform continuously (infinite loop).
    pub fn play(&mut self, wfm_id: &str) -> Result<(), String> {
        self.client
            .write_cmd(&format!("radio:arb:waveform \"WFM1:{}\"", wfm_id))?;
        self.client.write_cmd("output 1")?;
        self.client.write_cmd("output:modulation 1")?;
        self.client.write_cmd("radio:arb:state 1")?;
        self.client.err_check()
    }

    /// Activate arb playback with a finite repeat count.
    ///
    /// `count` is the number of times to play the waveform.
    ///
    /// TODO: Implement finite repeat count via SCPI commands.
    /// Possible SCPI commands to investigate:
    ///   - `radio:arb:trigger:type:continuous` vs `single`
    ///   - `radio:arb:count <n>`
    ///   - `radio:arb:retrigger:count <n>`
    ///   - Refer to Keysight X-Series Signal Generators Programming Guide
    ///     for the exact commands supported by the target instrument model.
    pub fn play_with_repeat(&mut self, wfm_id: &str, _count: u32) -> Result<(), String> {
        // TODO: Send SCPI commands to configure finite repeat count, e.g.:
        // self.client.write_cmd("radio:arb:trigger:type single")?;
        // self.client.write_cmd(&format!("radio:arb:count {}", _count))?;

        // Fallback: play continuously until the finite-repeat SCPI is implemented
        self.play(wfm_id)
    }

    /// Stop playback: disable RF output, modulation, and arb state.
    pub fn stop(&mut self) -> Result<(), String> {
        self.client.write_cmd("output 0")?;
        self.client.write_cmd("output:modulation 0")?;
        self.client.write_cmd("radio:arb:state 0")?;
        Ok(())
    }
}
