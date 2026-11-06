use tracing::info;

pub async fn run_discovery_responder(port: u16, server_port: u16) {
    if let Ok(socket) = tokio::net::UdpSocket::bind(format!("0.0.0.0:{port}")).await {
        info!("Discovery UDP responder active on port {port}");
        let mut buf = [0u8; 128];
        let reply = format!("LAUD_HERE:{server_port}");
        while let Ok((len, addr)) = socket.recv_from(&mut buf).await {
            if &buf[..len] == b"LAUD_DISCOVER" {
                let _ = socket.send_to(reply.as_bytes(), addr).await;
            }
        }
    }
}
