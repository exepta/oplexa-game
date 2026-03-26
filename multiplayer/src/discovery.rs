use serde::{Deserialize, Serialize};
use std::io::{self, ErrorKind};
use std::net::{SocketAddr, UdpSocket};

const DISCOVERY_QUERY: &[u8] = b"OPLEXA_DISCOVERY_V1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanServerInfo {
    pub server_name: String,
    pub motd: String,
    pub session_url: String,
    #[serde(default)]
    pub observed_addr: Option<String>,
}

pub struct LanDiscoveryClient {
    socket: UdpSocket,
    port: u16,
}

impl LanDiscoveryClient {
    pub fn bind(port: u16) -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_broadcast(true)?;
        socket.set_nonblocking(true)?;

        Ok(Self { socket, port })
    }

    pub fn broadcast_query(&self) -> io::Result<()> {
        self.socket
            .send_to(DISCOVERY_QUERY, SocketAddr::from(([255, 255, 255, 255], self.port)))?;
        Ok(())
    }

    pub fn poll(&self) -> io::Result<Vec<LanServerInfo>> {
        let mut buffer = [0_u8; 1024];
        let mut servers = Vec::new();

        loop {
            match self.socket.recv_from(&mut buffer) {
                Ok((bytes, addr)) => {
                    let mut info = serde_json::from_slice::<LanServerInfo>(&buffer[..bytes]).map_err(
                        |error| io::Error::new(ErrorKind::InvalidData, error),
                    )?;
                    info.observed_addr = Some(addr.ip().to_string());
                    servers.push(info);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        Ok(servers)
    }
}

pub struct LanDiscoveryServer {
    socket: UdpSocket,
    payload: Vec<u8>,
}

impl LanDiscoveryServer {
    pub fn bind(port: u16, info: LanServerInfo) -> io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port)))?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            payload: serde_json::to_vec(&info)
                .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?,
        })
    }

    pub fn poll(&self) -> io::Result<()> {
        let mut buffer = [0_u8; 128];

        loop {
            match self.socket.recv_from(&mut buffer) {
                Ok((bytes, addr)) => {
                    if &buffer[..bytes] == DISCOVERY_QUERY {
                        self.socket.send_to(&self.payload, addr)?;
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        Ok(())
    }
}
