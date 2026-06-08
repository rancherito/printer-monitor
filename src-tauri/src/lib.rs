mod strategy;
mod guards;
mod printers;
mod serial;
mod network;
mod system;
mod settings;
mod api_server;
mod escpos_print;
mod printer_cache;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Si ya hay una instancia corriendo → mostrar su ventana y salir
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
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

            // Si se lanzó via autostart ocultar la ventana al tray de inmediato
            let launched_hidden = std::env::args().any(|a| a == "--autostart");
            if launched_hidden {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            // Autoarranque: SOLO en release. Los builds de desarrollo (tauri dev)
            // no deben tocar el registro — si lo hacen, al reiniciar Windows el
            // binario dev arranca sin servidor Angular y muestra ERR_CONNECTION_REFUSED.
            tauri::async_runtime::spawn(async {
                if cfg!(debug_assertions) {
                    return; // dev build → no modificar el registro
                }
                if system::is_first_launch() {
                    // Primera ejecución del instalador: registrar autoarranque
                    let _ = system::set_autostart(true);
                    system::mark_initialized();
                } else if system::get_autostart_status() {
                    // Reinstalación / actualización: refrescar la ruta del exe
                    // para que apunte siempre al binario actual
                    let _ = system::set_autostart(true);
                }
            });

            // ── Bandeja del sistema ──────────────────────────────────────────
            let show_i = MenuItem::with_id(app, "show", "Abrir",  true, None::<&str>)?;
            let quit_i  = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let icon = tauri::include_image!("icons/32x32.png");

            TrayIconBuilder::<tauri::Wry>::new()
                .icon(icon)
                .tooltip("Centro de Ayuda Codicore")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Clic izquierdo sobre el ícono → mostrar ventana
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        // Cerrar ventana → ocultar a bandeja (la app sigue corriendo)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            system::get_system_info,
            system::get_autostart_enabled,
            system::set_autostart_enabled,
            system::get_server_port,
            system::set_server_port,
            system::get_output_dir,
            system::set_output_dir,
            system::list_printed_files,
            system::open_output_dir,
            printers::get_printers,
            printers::rename_printer,
            printers::print_test,
            printers::print_test_pdf_internal,
            printers::print_test_a4_pdf,
            printers::print_test_tcp,
            printers::test_usb_printer,
            printers::add_network_printer,
            printers::add_usb_printer,
            printers::clear_print_queue,
            printers::remove_custom_printer,
            network::get_network_config,
            network::scan_tcp_ip_printers,
            serial::get_serial_ports,
            serial::get_usb_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
