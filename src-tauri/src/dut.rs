use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// DUT (Device Under Test) client.
///
/// Communicates with the board's ATE daemon over TCP using JSON commands,
/// following the protocol in `reference/board_connect`.
pub struct DutClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

#[derive(Serialize)]
enum DutCommand {
    ATECmd { cmd: String, args: Vec<String> },
    ReadMib(String),
}

#[derive(Deserialize)]
struct ResponseHeader {
    is_error: bool,
    #[allow(dead_code)]
    file_size: u64,
}

impl DutClient {
    /// Connect to the DUT board at `ip` on port 9600.
    /// Sends ATEInit after connection.
    pub fn connect(ip: &str, timeout_secs: u64) -> Result<Self, String> {
        let addr = format!("{}:9600", ip);
        let socket_addr: std::net::SocketAddr = addr
            .parse()
            .map_err(|e| format!("Invalid DUT address '{}': {}", addr, e))?;

        let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(timeout_secs))
            .map_err(|e| format!("DUT connection to {} failed: {}", addr, e))?;

        stream
            .set_read_timeout(Some(Duration::from_secs(timeout_secs)))
            .map_err(|e| format!("DUT set read timeout failed: {}", e))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(timeout_secs)))
            .map_err(|e| format!("DUT set write timeout failed: {}", e))?;

        let reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| format!("DUT clone stream failed: {}", e))?,
        );

        let client = Self { stream, reader };
        // client.ate_init()?;
        Ok(client)
    }

    fn send_cmd(&mut self, cmd: DutCommand) -> Result<(), String> {
        let json = serde_json::to_string(&cmd).map_err(|e| format!("DUT serialize failed: {}", e))?;
        self.stream
            .write_all(json.as_bytes())
            .map_err(|e| format!("DUT write failed: {}", e))?;
        self.stream
            .write_all(b"\n")
            .map_err(|e| format!("DUT write newline failed: {}", e))?;
        self.stream
            .flush()
            .map_err(|e| format!("DUT flush failed: {}", e))
    }

    fn read_resp(&mut self) -> Result<(), String> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| format!("DUT read failed: {}", e))?;
        let resp: ResponseHeader =
            serde_json::from_str(&line).map_err(|e| format!("DUT response parse failed: {}", e))?;
        if resp.is_error {
            Err("DUT returned error".into())
        } else {
            Ok(())
        }
    }

    /// Read response and return the raw header line (for MIB parsing).
    fn read_resp_raw(&mut self) -> Result<String, String> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| format!("DUT read failed: {}", e))?;
        let resp: ResponseHeader =
            serde_json::from_str(&line).map_err(|e| format!("DUT response parse failed: {}", e))?;
        if resp.is_error {
            Err("DUT returned error".into())
        } else {
            let size = resp.file_size as usize;
            let mut text = vec![0u8;size];
            self.reader.read_exact(&mut text)
                .map_err(|e| format!("Can not extract string from dut mib:{e}"))?;
            String::from_utf8_lossy(&text)
                .parse()
                .map_err(|e| format!("Can not parse mib text to string:{e}"))
        }
    }

    /// Open RX on the DUT.
    ///
    /// - `cf_mhz`: carrier frequency in MHz (e.g. 2412, 5180)
    /// - `bw_mhz`: bandwidth in MHz (e.g. 20, 40, 80)
    pub fn open_rx(&mut self, cf_mhz: u32, bw_mhz: u32) -> Result<(), String> {
        let iface = if cf_mhz >= 5000 { "wlan0" } else { "wlan1" };
        let bw_code = match bw_mhz {
            40 => 2,
            80 => 3,
            160 => 4,
            _ => 1, // 20 MHz or default
        };
        let arg_str = format!(
            "{} fastconfig -f {} -c {} -w {} -u {} -r",
            iface, cf_mhz, cf_mhz, bw_code, bw_code
        );
        let args: Vec<String> = arg_str.split(' ').map(|s| s.to_string()).collect();
        let cmd = DutCommand::ATECmd {
            cmd: "ate_cmd".into(),
            args,
        };
        self.send_cmd(cmd)?;
        self.read_resp()
    }

    /// Close RX on the DUT.
    ///
    /// - `cf_mhz`: carrier frequency in MHz, used to determine the interface
    pub fn close_rx(&mut self, cf_mhz: u32) -> Result<(), String> {
        let iface = if cf_mhz >= 5000 { "wlan0" } else { "wlan1" };
        let arg_str = format!("{} fastconfig -k", iface);
        let args: Vec<String> = arg_str.split(' ').map(|s| s.to_string()).collect();
        let cmd = DutCommand::ATECmd {
            cmd: "ate_cmd".into(),
            args,
        };
        self.send_cmd(cmd)?;
        self.read_resp()
    }

    pub fn read_mib(&mut self, cf_mhz: u32) -> Result<String, String> {
        let iface = if cf_mhz >= 5000 { "wlan0" } else { "wlan1" };
        let cmd = DutCommand::ReadMib (iface.into());
        self.send_cmd(cmd)?;
        self.read_resp_raw()
    }

    /// MIB result extracted from `fastconfig -R` output.
    ///
    /// Example input:
    /// ```text
    /// [ 5360.257334] [***debug***] user->rec_rx_count = 1000
    /// ...
    /// receive 20M OK = 0, receive 40M OK = 1000, receive 80M OK = 0, receive 160M OK = 0
    /// ```
    pub fn parse_mib_resp(output: &str, bw_mhz: u32) -> MibResult {
        // Extract rec_rx_count: match "user->rec_rx_count = <number>"
        let rec_rx_count = output
            .lines()
            .find_map(|line| {
                let idx = line.find("user->rec_rx_count")?;
                let after_eq = line[idx..].split('=').nth(1)?;
                after_eq.trim().parse::<u32>().ok()
            });

        // Extract per-BW OK count from "receive <BW>M OK = <number>"
        // Build the key for the target bandwidth, e.g. "receive 20M OK"
        let bw_key = format!("receive {}M OK", bw_mhz);
        let rx_ok_count = output
            .lines()
            .find_map(|line| {
                let idx = line.find(&bw_key)?;
                // From the key position, find the '=' and parse the number after it
                let after_key = &line[idx + bw_key.len()..];
                let after_eq = after_key.split('=').nth(1)?;
                // Take only digits (stop at ',' or end of string)
                let num_str = after_eq.trim().split(',').next()?.trim();
                num_str.parse::<u32>().ok()
            });

        MibResult {
            rec_rx_count,
            rx_ok_count,
        }
    }
}

/// Parsed MIB statistics from DUT `fastconfig -R` output.
#[derive(Clone, Debug)]
pub struct MibResult {
    /// Total received packet count (`user->rec_rx_count`).
    pub rec_rx_count: Option<u32>,
    /// Decoded OK count for the matching bandwidth (`receive <BW>M OK`).
    pub rx_ok_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MIB: &str = r#"
[ 5360.255098] [***debug***] v_mib_state = 0x0  user->mib = 0
[ 5360.255188] [***debug***] v_mib_state = 0x0  user->mib = 0
[ 5360.255256] [***debug***] v_mib_state = 0x0  user->mib = 0
[ 5360.255334] [***debug***] v_mib_state = 0x0  user->mib = 0
[ 5360.255485] [***debug***] v_mib_state = 0x0  user->fcs_err = 0
[ 5360.255662] [***debug***] v_mib_state = 0x0  user->phy_err = 0
[ 5360.257334] [***debug***] user->rec_rx_count = 1000
[ 5360.258854] [***debug***] rssi1 = -76
[ 5360.258899] [***debug***] rssi2 = -77
receive 20M OK = 0, receive 40M OK = 1000, receive 80M OK = 0, receive 160M OK = 0
rssi_1 = -76， rssi_2 = -77
"#;

    #[test]
    fn parse_rec_rx_count() {
        let result = DutClient::parse_mib_resp(SAMPLE_MIB, 40);
        assert_eq!(result.rec_rx_count, Some(1000));
    }

    #[test]
    fn parse_rx_ok_40m() {
        let result = DutClient::parse_mib_resp(SAMPLE_MIB, 40);
        assert_eq!(result.rx_ok_count, Some(1000));
    }

    #[test]
    fn parse_rx_ok_20m() {
        let result = DutClient::parse_mib_resp(SAMPLE_MIB, 20);
        assert_eq!(result.rx_ok_count, Some(0));
    }

    #[test]
    fn parse_rx_ok_80m() {
        let result = DutClient::parse_mib_resp(SAMPLE_MIB, 80);
        assert_eq!(result.rx_ok_count, Some(0));
    }

    #[test]
    fn parse_rx_ok_missing_bw() {
        let result = DutClient::parse_mib_resp(SAMPLE_MIB, 10);
        assert_eq!(result.rx_ok_count, None);
    }
}
