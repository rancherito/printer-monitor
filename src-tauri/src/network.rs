use std::net::TcpStream;
use std::time::Duration;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkConfig {
    pub ip: String,
    pub mask: String,
    pub gateway: String,
}

#[tauri::command]
pub fn get_network_config() -> Result<NetworkConfig, String> {
    // Devuelve config básica usando la IP local detectada
    let ip = local_ip_address::local_ip()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    Ok(NetworkConfig {
        ip,
        mask: "255.255.255.0".to_string(),
        gateway: "192.168.1.1".to_string(),
    })
}

#[tauri::command]
pub async fn scan_tcp_ip_printers(subnet: String) -> Result<Vec<String>, String> {
    crate::guards::guard_valid_ip(
        &(subnet.split('.').take(3).collect::<Vec<_>>().join(".") + ".1"),
    )
    .map_err(String::from)?;

    let base: Vec<&str> = subnet.split('.').take(3).collect();
    let base = base.join(".");
    let mut handles = Vec::new();

    for i in 1u8..=254 {
        let ip = format!("{base}.{i}");
        handles.push(tokio::task::spawn_blocking(move || {
            TcpStream::connect_timeout(
                &format!("{ip}:9100").parse().unwrap(),
                Duration::from_millis(300),
            )
            .ok()
            .map(|_| ip)
        }));
    }

    let mut found = Vec::new();
    for h in handles {
        if let Ok(Some(ip)) = h.await {
            found.push(ip);
        }
    }
    Ok(found)
}
