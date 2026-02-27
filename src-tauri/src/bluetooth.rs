#![allow(unused_imports)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BluetoothDevice {
    pub name: String,
    pub address: String,
    pub is_connected: bool,
}

#[tauri::command]
pub fn get_bluetooth_devices() -> Vec<BluetoothDevice> {
    use std::process::Command;
    let mut devices: Vec<BluetoothDevice> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("system_profiler")
            .args(["SPBluetoothDataType", "-json"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(bt_arr) = json.get("SPBluetoothDataType").and_then(|v| v.as_array()) {
                    for bt_entry in bt_arr {
                        for (key, connected) in
                            [("device_connected", true), ("device_not_connected", false)]
                        {
                            if let Some(list) = bt_entry.get(key).and_then(|v| v.as_array()) {
                                for dev in list {
                                    if let Some(obj) = dev.as_object() {
                                        for (name, info) in obj {
                                            let address = info
                                                .get("device_address")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("—")
                                                .to_string();
                                            devices.push(BluetoothDevice {
                                                name: name.clone(),
                                                address,
                                                is_connected: connected,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = crate::hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "Get-PnpDevice -Class Bluetooth | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Csv -NoTypeInformation",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            for line in text.lines().skip(1) {
                let cols: Vec<&str> = line.splitn(3, ',').collect();
                if cols.len() >= 3 {
                    let name = cols[0].trim_matches('"').to_string();
                    let address = cols[1].trim_matches('"').to_string();
                    let status = cols[2].trim_matches('"');
                    let is_connected = status.eq_ignore_ascii_case("OK");
                    if !name.is_empty() {
                        devices.push(BluetoothDevice { name, address, is_connected });
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("bluetoothctl").args(["devices"]).output() {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            for line in text.lines() {
                let parts: Vec<&str> = line.splitn(3, ' ').collect();
                if parts.len() == 3 && parts[0] == "Device" {
                    let address = parts[1].to_string();
                    let name = parts[2].to_string();
                    let is_connected = Command::new("bluetoothctl")
                        .args(["info", &address])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Connected: yes"))
                        .unwrap_or(false);
                    devices.push(BluetoothDevice { name, address, is_connected });
                }
            }
        }
    }

    devices
}
