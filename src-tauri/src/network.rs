#![allow(unused_imports)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkDevice {
    pub ip: String,
    pub hostname: Option<String>,
    pub is_reachable: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkConfig {
    pub ip: String,
    pub mask: String,
    pub gateway: String,
    pub interface: String,
}

// ─── Escaneo de red ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn scan_network() -> Vec<NetworkDevice> {
    use std::net::IpAddr;
    use std::process::Command;
    use std::sync::{Arc, Mutex};
    use std::thread;

    let local_ip = local_ip_address::local_ip().ok();
    let base = match local_ip {
        Some(IpAddr::V4(v4)) => {
            let o = v4.octets();
            format!("{}.{}.{}.", o[0], o[1], o[2])
        }
        _ => return vec![],
    };

    let devices = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for i in 1u8..=254 {
        let base = base.clone();
        let devices = Arc::clone(&devices);
        let handle = thread::spawn(move || {
            let ip = format!("{}{}", base, i);

            #[cfg(any(target_os = "macos", target_os = "linux"))]
            let reachable = Command::new("ping")
                .args(["-c", "1", "-W", "1", &ip])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            #[cfg(target_os = "windows")]
            let reachable = crate::hidden_cmd("ping")
                .args(["-n", "1", "-w", "500", &ip])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if reachable {
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                let hostname = Command::new("host")
                    .arg(&ip)
                    .output()
                    .ok()
                    .and_then(|o| {
                        let s = String::from_utf8_lossy(&o.stdout).to_string();
                        s.split("pointer ")
                            .nth(1)
                            .map(|h| h.trim().trim_end_matches('.').to_string())
                    });

                #[cfg(target_os = "windows")]
                let hostname: Option<String> = None;

                devices.lock().unwrap().push(NetworkDevice { ip, hostname, is_reachable: true });
            }
        });
        handles.push(handle);
    }
    for h in handles {
        let _ = h.join();
    }
    let mut result = devices.lock().unwrap().clone();
    result.sort_by(|a, b| {
        let al: u8 = a.ip.split('.').last().and_then(|n| n.parse().ok()).unwrap_or(0);
        let bl: u8 = b.ip.split('.').last().and_then(|n| n.parse().ok()).unwrap_or(0);
        al.cmp(&bl)
    });
    result
}

// ─── Escáner TCP/IP de impresoras ─────────────────────────────────────────────

#[tauri::command]
pub fn scan_tcp_ip_printers(ip: String, mask: String) -> Vec<String> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let ip_parts: Vec<u8> = ip.split('.').filter_map(|s| s.parse().ok()).collect();
    let mask_parts: Vec<u8> = mask.split('.').filter_map(|s| s.parse().ok()).collect();
    if ip_parts.len() != 4 || mask_parts.len() != 4 {
        return vec![];
    }

    let base = if mask_parts[3] == 0 {
        format!("{}.{}.{}.", ip_parts[0], ip_parts[1], ip_parts[2])
    } else {
        return if test_printer_port(&ip) { vec![ip] } else { vec![] };
    };

    let found = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for i in 1u8..=254 {
        let base = base.clone();
        let found = Arc::clone(&found);
        let handle = thread::spawn(move || {
            let ip = format!("{}{}", base, i);
            if test_printer_port(&ip) {
                found.lock().unwrap().push(ip);
            }
        });
        handles.push(handle);
    }
    for h in handles {
        let _ = h.join();
    }
    let mut result = found.lock().unwrap().clone();
    result.sort_by(|a, b| {
        let al: u8 = a.split('.').last().and_then(|n| n.parse().ok()).unwrap_or(0);
        let bl: u8 = b.split('.').last().and_then(|n| n.parse().ok()).unwrap_or(0);
        al.cmp(&bl)
    });
    result
}

fn test_printer_port(ip: &str) -> bool {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    let addr = format!("{}:9100", ip);
    addr.parse::<SocketAddr>()
        .map(|sa| TcpStream::connect_timeout(&sa, Duration::from_millis(500)).is_ok())
        .unwrap_or(false)
}

// ─── Configuración de red ─────────────────────────────────────────────────────

#[tauri::command]
pub fn get_network_config() -> Result<NetworkConfig, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let route_output = Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .map_err(|e| format!("Error al obtener interfaz: {}", e))?;
        let route_str = String::from_utf8_lossy(&route_output.stdout);
        let interface_name = route_str
            .lines()
            .find(|line| line.trim().starts_with("interface:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "en0".to_string());

        let services_output = Command::new("networksetup")
            .args(["-listallhardwareports"])
            .output()
            .ok();
        let service_name = services_output.as_ref().and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut current = String::new();
            for line in text.lines() {
                if line.starts_with("Hardware Port:") {
                    current = line.split(':').nth(1).unwrap_or("").trim().to_string();
                } else if line.starts_with("Device:") && line.contains(&interface_name) {
                    return Some(current.clone());
                }
            }
            None
        });

        if let Some(ref svc) = service_name {
            if let Ok(info_out) =
                Command::new("networksetup").args(["-getinfo", svc]).output()
            {
                let info = String::from_utf8_lossy(&info_out.stdout);
                let mut ip = String::new();
                let mut mask = String::new();
                let mut gateway = String::new();
                for line in info.lines() {
                    let line = line.trim();
                    if line.starts_with("IP address:") {
                        ip = line["IP address:".len()..].trim().to_string();
                    } else if line.starts_with("Subnet mask:") {
                        mask = line["Subnet mask:".len()..].trim().to_string();
                    } else if line.starts_with("Router:") {
                        gateway = line["Router:".len()..].trim().to_string();
                    }
                }
                if ip.is_empty() {
                    ip = crate::system::get_local_ip();
                }
                if mask.is_empty() {
                    mask = "255.255.255.0".to_string();
                }
                return Ok(NetworkConfig { ip, mask, gateway, interface: interface_name });
            }
        }

        // Fallback
        let ip = Command::new("ipconfig")
            .args(["getifaddr", &interface_name])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let gateway = route_str
            .lines()
            .find(|line| line.trim().starts_with("gateway:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        Ok(NetworkConfig {
            ip: if ip.is_empty() { crate::system::get_local_ip() } else { ip },
            mask: "255.255.255.0".to_string(),
            gateway,
            interface: interface_name,
        })
    }

    #[cfg(target_os = "windows")]
    {
        let output = crate::hidden_cmd("netsh")
            .args(["interface", "ip", "show", "config"])
            .output()
            .map_err(|e| format!("Error al obtener configuración: {}", e))?;
        let config_str = String::from_utf8_lossy(&output.stdout);
        let mut ip = String::new();
        let mut mask = String::new();
        let mut gateway = String::new();
        let interface = "Ethernet".to_string();
        for line in config_str.lines() {
            let line = line.trim();
            if line.starts_with("Dirección IP") || line.starts_with("IP Address") {
                if let Some(addr) = line.split(':').nth(1) {
                    ip = addr.trim().to_string();
                }
            } else if line.starts_with("Máscara de subred") || line.starts_with("Subnet Mask") {
                if let Some(m) = line.split(':').nth(1) {
                    mask = m.trim().to_string();
                }
            } else if line.starts_with("Puerta de enlace") || line.starts_with("Default Gateway")
            {
                if let Some(gw) = line.split(':').nth(1) {
                    gateway = gw.trim().to_string();
                }
            }
        }
        if ip.is_empty() { ip = crate::system::get_local_ip(); }
        if mask.is_empty() { mask = "255.255.255.0".to_string(); }
        if gateway.is_empty() { gateway = "192.168.1.1".to_string(); }
        Ok(NetworkConfig { ip, mask, gateway, interface })
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let route_output = Command::new("ip")
            .args(["route", "show", "default"])
            .output()
            .map_err(|e| format!("Error al obtener ruta: {}", e))?;
        let route_str = String::from_utf8_lossy(&route_output.stdout);
        let interface_name = route_str
            .split_whitespace()
            .skip_while(|&s| s != "dev")
            .nth(1)
            .unwrap_or("eth0")
            .to_string();
        let ip_output = Command::new("ip")
            .args(["addr", "show", &interface_name])
            .output()
            .map_err(|e| format!("Error al obtener IP: {}", e))?;
        let ip_str = String::from_utf8_lossy(&ip_output.stdout);
        let mut ip = String::new();
        let mut mask = String::new();
        for line in ip_str.lines() {
            if line.contains("inet ") && !line.contains("inet6") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(addr) = parts.get(1) {
                    if let Some((ip_part, mask_part)) = addr.split_once('/') {
                        ip = ip_part.to_string();
                        if let Ok(cidr) = mask_part.parse::<u32>() {
                            let bits = !0u32 << (32 - cidr);
                            mask = format!(
                                "{}.{}.{}.{}",
                                (bits >> 24) & 0xFF,
                                (bits >> 16) & 0xFF,
                                (bits >> 8) & 0xFF,
                                bits & 0xFF
                            );
                        }
                    }
                }
            }
        }
        let gateway = route_str
            .split_whitespace()
            .nth(2)
            .unwrap_or("192.168.1.1")
            .to_string();
        if ip.is_empty() { ip = crate::system::get_local_ip(); }
        if mask.is_empty() { mask = "255.255.255.0".to_string(); }
        Ok(NetworkConfig { ip, mask, gateway, interface: interface_name })
    }
}

#[tauri::command]
pub fn set_network_config(ip: String, mask: String, gateway: String) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let interface = Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .map_err(|e| format!("Error al obtener interfaz: {}", e))?;
        let interface_str = String::from_utf8_lossy(&interface.stdout);
        let interface_name = interface_str
            .lines()
            .find(|line| line.trim().starts_with("interface:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "en0".to_string());

        let services = Command::new("networksetup")
            .args(["-listallhardwareports"])
            .output()
            .map_err(|e| format!("Error al listar servicios: {}", e))?;
        let services_str = String::from_utf8_lossy(&services.stdout);
        let mut service_name = String::new();
        let mut found = false;
        for line in services_str.lines() {
            if line.starts_with("Hardware Port:") {
                service_name = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Device:") && line.contains(&interface_name) {
                found = true;
                break;
            }
        }
        if !found || service_name.is_empty() {
            return Err(format!(
                "No se pudo encontrar el servicio de red para la interfaz {}.",
                interface_name
            ));
        }

        let script = format!(
            "cd /tmp && networksetup -setmanual '{}' {} {} {}",
            service_name.replace("'", "\\'"),
            ip,
            mask,
            gateway
        );
        let output = Command::new("osascript")
            .args(["-e", &format!("do shell script \"{}\" with administrator privileges", script)])
            .output()
            .map_err(|e| format!("Error al configurar red: {}", e))?;

        if output.status.success() {
            Ok("Configuración de red actualizada correctamente".to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            if error.contains("User cancelled") || error.contains("cancelado") {
                Err("Operación cancelada por el usuario".to_string())
            } else if error.contains("shell-init") || error.contains("getcwd") {
                if !error.contains("networksetup") {
                    Ok("Configuración aplicada (puede requerir reconexión de red)".to_string())
                } else {
                    Err(format!("Error de networksetup para la interfaz '{}'", interface_name))
                }
            } else if !error.is_empty() {
                Err(format!("Error: {}", error.trim()))
            } else {
                Ok("Configuración de red actualizada correctamente".to_string())
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = crate::hidden_cmd("netsh")
            .args([
                "interface",
                "ip",
                "set",
                "address",
                "name=Ethernet",
                "source=static",
                &format!("addr={}", ip),
                &format!("mask={}", mask),
                &format!("gateway={}", gateway),
            ])
            .output()
            .map_err(|e| format!("Error al configurar red: {}", e))?;
        if output.status.success() {
            Ok("Configuración de red actualizada correctamente".to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("Error: {}. Ejecuta la aplicación como administrador.", error))
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let has_nmcli = Command::new("which")
            .arg("nmcli")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if has_nmcli {
            let connection = Command::new("nmcli")
                .args(["-t", "-f", "NAME", "connection", "show", "--active"])
                .output()
                .map_err(|e| format!("Error al obtener conexión: {}", e))?;
            let conn_name = String::from_utf8_lossy(&connection.stdout)
                .lines()
                .next()
                .unwrap_or("Wired connection 1")
                .to_string();
            let output = Command::new("pkexec")
                .args([
                    "nmcli",
                    "connection",
                    "modify",
                    &conn_name,
                    "ipv4.method",
                    "manual",
                    "ipv4.addresses",
                    &format!("{}/{}", ip, mask),
                    "ipv4.gateway",
                    &gateway,
                ])
                .output()
                .map_err(|e| format!("Error al configurar red: {}", e))?;
            if output.status.success() {
                let _ = Command::new("nmcli").args(["connection", "down", &conn_name]).output();
                let _ = Command::new("nmcli").args(["connection", "up", &conn_name]).output();
                Ok("Configuración de red actualizada correctamente".to_string())
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                Err(format!("Error al aplicar configuración: {}", error))
            }
        } else {
            Err("NetworkManager no está disponible. Configura la red manualmente.".to_string())
        }
    }
}

#[tauri::command]
pub fn restore_network_dhcp() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let route_output = Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .map_err(|e| format!("Error al obtener ruta: {}", e))?;
        let route_str = String::from_utf8_lossy(&route_output.stdout);
        let interface_name = route_str
            .lines()
            .find(|line| line.trim().starts_with("interface:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .ok_or_else(|| "No se pudo detectar la interfaz de red".to_string())?;

        let services = Command::new("networksetup")
            .args(["-listallhardwareports"])
            .output()
            .map_err(|e| format!("Error al listar puertos: {}", e))?;
        let services_str = String::from_utf8_lossy(&services.stdout);
        let mut service_name = String::new();
        let mut found = false;
        for line in services_str.lines() {
            if line.starts_with("Hardware Port:") {
                service_name = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Device:") && line.contains(&interface_name) {
                found = true;
                break;
            }
        }
        if !found || service_name.is_empty() {
            return Err(format!(
                "No se pudo encontrar el servicio para la interfaz '{}'",
                interface_name
            ));
        }

        let script = format!(
            "cd /tmp && networksetup -setdhcp '{}'",
            service_name.replace("'", "\\'")
        );
        let output = Command::new("osascript")
            .args(["-e", &format!("do shell script \"{}\" with administrator privileges", script)])
            .output()
            .map_err(|e| format!("Error al ejecutar comando: {}", e))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let has_shell_init = stderr.contains("shell-init:");
        let has_real_err =
            stderr.to_lowercase().contains("error") && !stderr.contains("shell-init:");

        if output.status.success() || (has_shell_init && !has_real_err) {
            Ok("Configuración de red restaurada a DHCP correctamente".to_string())
        } else if !stderr.is_empty() {
            Err(format!("Error: {}", stderr.trim()))
        } else {
            Ok("Configuración de red restaurada a DHCP correctamente".to_string())
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let ipconfig = Command::new("ipconfig")
            .output()
            .map_err(|e| format!("Error al ejecutar ipconfig: {}", e))?;
        let ipconfig_str = String::from_utf8_lossy(&ipconfig.stdout);
        let interface_name = ipconfig_str
            .lines()
            .find(|line| line.contains("Ethernet") || line.contains("Wi-Fi"))
            .and_then(|line| line.split(':').next())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Ethernet".to_string());
        let script =
            format!("netsh interface ip set address name=\"{}\" source=dhcp", interface_name);
        let output = Command::new("powershell")
            .args([
                "-Command",
                &format!("Start-Process cmd -ArgumentList '/c {}' -Verb RunAs -Wait", script),
            ])
            .output()
            .map_err(|e| format!("Error al ejecutar comando: {}", e))?;
        if output.status.success() {
            Ok("Configuración de red restaurada a DHCP correctamente".to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("Error: {}. Ejecuta la aplicación como administrador.", error))
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let has_nmcli = Command::new("which")
            .arg("nmcli")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if has_nmcli {
            let connection = Command::new("nmcli")
                .args(["-t", "-f", "NAME", "connection", "show", "--active"])
                .output()
                .map_err(|e| format!("Error al obtener conexión: {}", e))?;
            let conn_name = String::from_utf8_lossy(&connection.stdout)
                .lines()
                .next()
                .unwrap_or("Wired connection 1")
                .to_string();
            let output = Command::new("pkexec")
                .args(["nmcli", "connection", "modify", &conn_name, "ipv4.method", "auto"])
                .output()
                .map_err(|e| format!("Error al configurar red: {}", e))?;
            if output.status.success() {
                let _ = Command::new("nmcli").args(["connection", "down", &conn_name]).output();
                let _ = Command::new("nmcli").args(["connection", "up", &conn_name]).output();
                Ok("Configuración de red restaurada a DHCP correctamente".to_string())
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                Err(format!("Error al aplicar configuración: {}", error))
            }
        } else {
            Err("NetworkManager no está disponible. Configura la red manualmente.".to_string())
        }
    }
}
