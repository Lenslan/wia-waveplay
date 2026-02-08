use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

pub struct ScpiClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl ScpiClient {
    pub fn connect(ip: &str, port: u16, timeout_secs: u64) -> Result<Self, String> {
        let addr = format!("{}:{}", ip, port);
        let socket_addr: std::net::SocketAddr = addr
            .parse()
            .map_err(|e| format!("Invalid address '{}': {}", addr, e))?;

        let stream =
            TcpStream::connect_timeout(&socket_addr, Duration::from_secs(timeout_secs))
                .map_err(|e| format!("Connection to {} failed: {}", addr, e))?;

        stream
            .set_read_timeout(Some(Duration::from_secs(timeout_secs)))
            .map_err(|e| format!("Failed to set read timeout: {}", e))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(timeout_secs)))
            .map_err(|e| format!("Failed to set write timeout: {}", e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| format!("Failed to set nodelay: {}", e))?;

        let reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| format!("Failed to clone stream: {}", e))?,
        );

        Ok(Self { stream, reader })
    }

    pub fn write_cmd(&mut self, cmd: &str) -> Result<(), String> {
        self.stream
            .write_all(format!("{}\n", cmd).as_bytes())
            .map_err(|e| format!("Write failed: {}", e))?;
        self.stream
            .flush()
            .map_err(|e| format!("Flush failed: {}", e))
    }

    pub fn read_response(&mut self) -> Result<String, String> {
        let mut response = String::new();
        self.reader
            .read_line(&mut response)
            .map_err(|e| format!("Read failed: {}", e))?;
        Ok(response.trim().to_string())
    }

    pub fn query(&mut self, cmd: &str) -> Result<String, String> {
        self.write_cmd(cmd)?;
        self.read_response()
    }

    /// Send a SCPI command followed by IEEE 488.2 definite length arbitrary block data.
    pub fn write_binary_block(&mut self, cmd: &str, data: &[u8]) -> Result<(), String> {
        let data_len_str = data.len().to_string();
        let num_digits = data_len_str.len();

        // Format: <cmd>#<num_digits><data_length><binary_data>\n
        let header = format!("{}#{}{}", cmd, num_digits, data_len_str);
        self.stream
            .write_all(header.as_bytes())
            .map_err(|e| format!("Write header failed: {}", e))?;
        self.stream
            .write_all(data)
            .map_err(|e| format!("Write binary data failed: {}", e))?;
        self.stream
            .write_all(b"\n")
            .map_err(|e| format!("Write terminator failed: {}", e))?;
        self.stream
            .flush()
            .map_err(|e| format!("Flush failed: {}", e))
    }

    pub fn err_check(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        loop {
            let resp = self.query("SYST:ERR?")?;
            let cleaned = resp.replace('+', "").replace('-', "");
            if cleaned.starts_with("0,") || cleaned.contains("No error") {
                break;
            }
            errors.push(resp);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("Instrument errors: {}", errors.join("; ")))
        }
    }
}
