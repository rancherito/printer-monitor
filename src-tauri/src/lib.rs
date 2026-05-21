mod strategy;
mod guards;
mod printers;
mod serial;
mod network;
mod system;
mod settings;
mod api_server;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            // Iniciar servidor HTTP interno en background
            tauri::async_runtime::spawn(api_server::start());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system::get_system_info,
            system::get_autostart_enabled,
            system::set_autostart_enabled,
            printers::get_printers,
            printers::rename_printer,
            printers::print_test,
            printers::print_test_tcp,
            printers::test_usb_printer,
            printers::add_network_printer,
            printers::add_usb_printer,
            printers::clear_print_queue,
            printers::remove_custom_printer,
            network::get_network_config,
            network::scan_tcp_ip_printers,
            serial::get_serial_ports,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
