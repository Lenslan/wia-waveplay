use std::io::{BufRead, BufReader, Write};
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

    pub fn read_mib(&mut self, cf_mhz: u32) -> Result<(), String> {
        let iface = if cf_mhz >= 5000 { "wlan0" } else { "wlan1" };
        let arg_str = format!("{} fastconfig -R", iface);
        let args: Vec<String> = arg_str.split(' ').map(|s| s.to_string()).collect();
        let cmd = DutCommand::ATECmd {
            cmd: "ate_cmd".into(),
            args,
        };
        self.send_cmd(cmd)?;
        self.read_resp()
    }
}
