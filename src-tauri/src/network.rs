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
pub async fn get_network_config() -> Result<NetworkConfig, String> {
    tokio::task::spawn_blocking(|| {
        // Devuelve config básica usando la IP local detectada
        let ip = local_ip_address::local_ip()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        Ok(NetworkConfig {
            ip,
            mask: "255.255.255.0".to_string(),
            gateway: "192.168.1.1".to_string(),
        })
    })
    .await
    .map_err(|e| format!("Join error: {e}"))?
}

#[tauri::command]
pub async fn scan_tcp_ip_printers(subnet: String) -> Result<Vec<String>, String> {
    crate::guards::guard_valid_ip(
        &(subnet.split('.').take(3).collect::<Vec<_>>().join(".") + ".1"),
    )
    .map_err(String::from)?;

    let base: Vec<&str> = subnet.split('.').take(3).collect();
    let base = base.join(".");

    // Lanzar los 254 sondeos en paralelo; el total = max(timeout) ≈ 300ms
    // en lugar de ~76s de la versión serial.
    let handles: Vec<_> = (1u8..=254)
        .map(|i| {
            let ip = format!("{base}.{i}");
            tokio::task::spawn_blocking(move || {
                TcpStream::connect_timeout(
                    &format!("{ip}:9100").parse().unwrap(),
                    Duration::from_millis(300),
                )
                .ok()
                .map(|_| ip)
            })
        })
        .collect();

    let results = futures::future::join_all(handles).await;
    let found: Vec<String> = results
        .into_iter()
        .filter_map(|r| r.ok().flatten())
        .collect();
    Ok(found)
}
