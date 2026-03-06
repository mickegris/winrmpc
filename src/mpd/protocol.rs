//! Low-level async MPD protocol: TCP connection, line parsing, binary data.
//! The MPD protocol is line-based text ending with "OK\n" or "ACK [...]\n".
//! Binary responses (albumart/readpicture) use size/binary headers.

use crate::mpd::error::{MpdError, MpdResult};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub struct MpdConnection {
    reader: BufReader<tokio::io::ReadHalf<TcpStream>>,
    writer: tokio::io::WriteHalf<TcpStream>,
    pub protocol_version: String,
}

impl MpdConnection {
    pub async fn connect(addr: &str) -> MpdResult<Self> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| MpdError::Connection(format!("Failed to connect to {addr}: {e}")))?;

        let (rh, wh) = tokio::io::split(stream);
        let mut reader = BufReader::new(rh);

        // Read greeting: "OK MPD x.y.z\n"
        let mut greeting = String::new();
        reader.read_line(&mut greeting).await?;
        let protocol_version = greeting
            .strip_prefix("OK MPD ")
            .map(|s| s.trim().to_string())
            .ok_or_else(|| MpdError::Protocol(format!("Invalid greeting: {greeting}")))?;

        Ok(Self {
            reader,
            writer: wh,
            protocol_version,
        })
    }

    /// Send a command and read key-value response pairs
    pub async fn command(&mut self, cmd: &str) -> MpdResult<Vec<(String, String)>> {
        self.send(cmd).await?;
        self.read_pairs().await
    }

    /// Send a command that returns binary data (albumart, readpicture)
    pub async fn command_binary(&mut self, cmd: &str) -> MpdResult<Option<(Vec<u8>, usize)>> {
        self.send(cmd).await?;
        self.read_binary().await
    }

    /// Send a command list (atomic batch)
    pub async fn command_list(
        &mut self,
        cmds: &[&str],
    ) -> MpdResult<Vec<Vec<(String, String)>>> {
        let mut full = String::from("command_list_ok_begin\n");
        for c in cmds {
            full.push_str(c);
            full.push('\n');
        }
        full.push_str("command_list_end\n");
        self.writer.write_all(full.as_bytes()).await?;
        self.writer.flush().await?;

        let mut results = Vec::new();
        let mut current = Vec::new();
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).await?;
            let trimmed = line.trim_end();

            if trimmed == "OK" {
                results.push(current);
                break;
            } else if trimmed == "list_OK" {
                results.push(current);
                current = Vec::new();
            } else if trimmed.starts_with("ACK ") {
                return Err(Self::parse_ack(trimmed));
            } else if let Some((k, v)) = trimmed.split_once(": ") {
                current.push((k.to_string(), v.to_string()));
            }
        }
        Ok(results)
    }

    async fn send(&mut self, cmd: &str) -> MpdResult<()> {
        let msg = format!("{cmd}\n");
        self.writer.write_all(msg.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn read_pairs(&mut self) -> MpdResult<Vec<(String, String)>> {
        let mut pairs = Vec::new();
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).await?;
            let trimmed = line.trim_end();

            if trimmed == "OK" {
                break;
            } else if trimmed.starts_with("ACK ") {
                return Err(Self::parse_ack(trimmed));
            } else if let Some((k, v)) = trimmed.split_once(": ") {
                pairs.push((k.to_string(), v.to_string()));
            }
        }
        Ok(pairs)
    }

    async fn read_binary(&mut self) -> MpdResult<Option<(Vec<u8>, usize)>> {
        let mut total_size: usize = 0;
        let binary_size: usize;

        // Read header lines until we get "binary: N" or "OK"
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).await?;
            let trimmed = line.trim_end();

            if trimmed == "OK" {
                return Ok(None);
            } else if trimmed.starts_with("ACK ") {
                return Err(Self::parse_ack(trimmed));
            } else if let Some((k, v)) = trimmed.split_once(": ") {
                match k {
                    "size" => {
                        total_size = v
                            .parse()
                            .map_err(|e| MpdError::Parse(format!("size: {e}")))?;
                    }
                    "binary" => {
                        binary_size = v
                            .parse()
                            .map_err(|e| MpdError::Parse(format!("binary: {e}")))?;
                        break;
                    }
                    _ => {}
                }
            }
        }

        if binary_size == 0 {
            return Ok(None);
        }

        // Read exactly binary_size bytes of data
        let mut data = vec![0u8; binary_size];
        self.reader.read_exact(&mut data).await?;

        // Read trailing newline + OK line
        let mut nl = [0u8; 1];
        self.reader.read_exact(&mut nl).await?;
        let mut ok_line = String::new();
        self.reader.read_line(&mut ok_line).await?;

        Ok(Some((data, total_size)))
    }

    fn parse_ack(line: &str) -> MpdError {
        // Format: ACK [error@command_listNum] {current_command} message
        if let Some(rest) = line.strip_prefix("ACK [") {
            if let Some(bracket_end) = rest.find(']') {
                let error_part = &rest[..bracket_end];
                let message = rest[bracket_end + 1..].trim().to_string();
                if let Some((code_str, _)) = error_part.split_once('@') {
                    if let Ok(code) = code_str.parse() {
                        return MpdError::Server { code, message };
                    }
                }
                return MpdError::Server { code: 0, message };
            }
        }
        MpdError::Protocol(line.to_string())
    }
}

/// Convert pairs to a HashMap with Vec values (handles duplicate keys)
pub fn pairs_to_map(pairs: &[(String, String)]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (k, v) in pairs {
        map.entry(k.clone()).or_default().push(v.clone());
    }
    map
}

/// Split response pairs into groups, each starting with the given key
pub fn split_groups(
    pairs: &[(String, String)],
    group_key: &str,
) -> Vec<Vec<(String, String)>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    for (k, v) in pairs {
        if k == group_key && !current.is_empty() {
            groups.push(current);
            current = Vec::new();
        }
        current.push((k.clone(), v.clone()));
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}
