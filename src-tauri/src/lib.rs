mod bluetooth;
mod network;
mod printers;
mod serial;
mod settings;
mod system;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

/// Crea un `Command` sin ventana de consola en Windows (CREATE_NO_WINDOW).
/// En otros SO equivale a `std::process::Command::new`.
#[cfg(target_os = "windows")]
pub(crate) fn hidden_cmd(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = std::process::Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?
            }

            {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let show_item = MenuItemBuilder::with_id("show", "Abrir").build(app)?;
                let quit_item = MenuItemBuilder::with_id("quit", "Salir").build(app)?;
                let menu = MenuBuilder::new(app).item(&show_item).item(&quit_item).build()?;

                TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .tooltip("Centro de Ayuda CODICORE")
                    .menu(&menu)
                    .on_menu_event(|app_h, event| match event.id().as_ref() {
                        "show" => {
                            if let Some(w) = app_h.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => app_h.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app_h = tray.app_handle();
                            if let Some(w) = app_h.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                    })
                    .build(app)?;
            }

            system::start_printer_watcher(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            printers::get_printers,
            printers::rename_printer,
            printers::print_test,
            printers::add_network_printer,
            serial::get_serial_ports,
            settings::get_settings,
            settings::set_setting,
            settings::get_app_port,
            system::get_local_ip,
            system::get_system_info,
            system::get_autostart_enabled,
            system::set_autostart_enabled,
            network::scan_network,
            network::scan_tcp_ip_printers,
            network::get_network_config,
            network::set_network_config,
            network::restore_network_dhcp,
            bluetooth::get_bluetooth_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
