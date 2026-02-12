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
        if fs > 240.0 * 1e6 {
            return Err("Sample Rate Can not be set more than 240 MHz!".into())
        }
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
        self.client.write_cmd("radio:arb:trigger:type continuous")?;
        self.client
            .write_cmd(&format!("radio:arb:waveform \"WFM1:{}\"", wfm_id))?;
        self.client.write_cmd("output 1")?;
        self.client.write_cmd("output:modulation 1")?;
        self.client.write_cmd("radio:arb:state 1")?;
        self.client.err_check()
    }

    /// Activate arb playback with a finite repeat count.
    ///
    /// Creates a waveform sequence from the uploaded segment with the specified
    /// repeat count, then plays the sequence.
    ///
    /// SCPI flow (from Keysight N5182B Programming Guide):
    ///   1. Build sequence: `:SOURce:RADio:ARB:SEQuence "<seq>","<wfm>",<reps>,<markers>`
    ///   2. Select sequence:  `:SOURce:RADio:ARB:WAVeform "SEQ:<seq>"`
    ///   3. Enable output:    ARB state → modulation → RF output
    pub fn play_with_repeat(&mut self, wfm_id: &str, count: u32) -> Result<(), String> {
        let seq_id = format!("seq_{}", wfm_id);

        // // Create a waveform sequence referencing the uploaded segment.
        // // markers = 0 (no markers enabled)
        self.client.write_cmd(&format!(
            "radio:arb:sequence \"{}\",\"WFM1:{}\",{},0",
            seq_id, wfm_id, count
        ))?;

        // Select the sequence for playback
        self.client.write_cmd(&format!(
            "radio:arb:waveform \"SEQ:{}\"",
            seq_id
        ))?;
        self.client.write_cmd("radio:arb:trigger:source bus")?;
        self.client.write_cmd("radio:arb:trigger:type single")?;

        // Enable playback (order per Keysight documentation)
        self.client.write_cmd("radio:arb:state 1")?;
        self.client.write_cmd("output:modulation 1")?;
        self.client.write_cmd("output 1")?;

        self.client.write_cmd("*TRG")?;

        self.client.err_check()
    }

    /// Set output power without reconfiguring CF/FS.
    pub fn set_power(&mut self, amp: f64) -> Result<(), String> {
        self.client.write_cmd(&format!("power {}", amp))?;
        self.client.err_check()
    }

    /// One-time sweep setup: configure CF/FS/power, download wfm, create sequence,
    /// set trigger mode to bus/single, and enable output.
    pub fn prepare_sweep(
        &mut self,
        wfm_data: &[u8],
        wfm_id: &str,
        cf: f64,
        fs: f64,
        amp: f64,
        repeat_count: u32,
    ) -> Result<(), String> {
        self.configure(cf, fs, amp)?;
        self.download_wfm(wfm_data, wfm_id)?;

        let seq_id = format!("seq_{}", wfm_id);

        // Create sequence with specified repeat count
        self.client.write_cmd(&format!(
            "radio:arb:sequence \"{}\",\"WFM1:{}\",{},0",
            seq_id, wfm_id, repeat_count
        ))?;

        // Select the sequence
        self.client.write_cmd(&format!(
            "radio:arb:waveform \"SEQ:{}\"",
            seq_id
        ))?;

        // Set trigger to bus/single so we control each burst with *TRG
        self.client.write_cmd("radio:arb:trigger:source bus")?;
        self.client.write_cmd("radio:arb:trigger:type single")?;

        // Enable playback chain
        self.client.write_cmd("radio:arb:state 1")?;
        self.client.write_cmd("output:modulation 1")?;
        self.client.write_cmd("output 1")?;

        self.client.err_check()
    }

    /// Send *TRG to start the prepared sequence.
    pub fn trigger(&mut self) -> Result<(), String> {
        self.client.write_cmd("*TRG")?;
        self.client.err_check()
    }

    /// Stop playback: disable RF output, modulation, and arb state.
    pub fn stop(&mut self) -> Result<(), String> {
        self.client.write_cmd("output 0")?;
        self.client.write_cmd("output:modulation 0")?;
        self.client.write_cmd("radio:arb:state 0")?;
        Ok(())
    }
}
