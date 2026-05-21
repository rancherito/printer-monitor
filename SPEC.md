# Printer Monitor — Especificación v2

Aplicación de escritorio (Tauri 2 + Angular) para registrar y gestionar impresoras térmicas en puntos de venta. Una sola pantalla. Sin tabs.

---

## Objetivo

El usuario puede:
1. Ver la IP local de la máquina y el estado del autostart.
2. Ver las impresoras del sistema operativo (CUPS / Windows Spooler).
3. Registrar impresoras adicionales por TCP/IP o USB directo.
4. Imprimir una página de prueba en cualquier impresora.
5. Renombrar o eliminar las impresoras registradas por la app.

---

## Stack

| Capa | Tecnología |
|------|-----------|
| Frontend | Angular 21, Tailwind CSS v4, signals, OnPush |
| Desktop | Tauri 2, Rust |
| BD local | SQLite via rusqlite |
| Impresión OS | CUPS (macOS/Linux), Windows Spooler |
| Impresión directa | ESC/POS via TCP socket o puerto serie/USB |

Plataformas: **macOS**, **Windows** (x64) y **Linux (Ubuntu 22.04+)**.

---

## Patrón Estrategia — Backend de SO

Toda operación que depende del sistema operativo se encapsula detrás de un **trait `PrinterStrategy`**. En tiempo de ejecución, Tauri selecciona la implementación concreta según el SO detectado. Angular no conoce ni le importa cuál estrategia está activa.

```
             ┌─────────────────────┐
             │  PrinterStrategy    │  ← trait (interfaz Rust)
             │─────────────────────│
             │ + list_printers()   │
             │ + install_network() │
             │ + install_usb()     │
             │ + remove_printer()  │
             │ + rename_printer()  │
             │ + print_test()      │
             │ + clear_queue()     │
             └──────────┬──────────┘
                        │
          ┌─────────────┼──────────────┐
          ▼             ▼              ▼
   MacStrategy   WindowsStrategy  LinuxStrategy
   (lpadmin /    (Win Spooler /   (lpadmin /
    CUPS)         PowerShell)      CUPS)
```

### Selección de estrategia

```rust
fn get_strategy() -> Box<dyn PrinterStrategy> {
    #[cfg(target_os = "macos")]  return Box::new(MacStrategy);
    #[cfg(target_os = "windows")] return Box::new(WindowsStrategy);
    #[cfg(target_os = "linux")]  return Box::new(LinuxStrategy);
}
```

Cada comando Tauri llama a `get_strategy().método(...)`. La lógica específica de cada SO vive **únicamente** dentro de su implementación de estrategia.

### Matriz de soporte por plataforma

| Operación | macOS | Windows | Linux (Ubuntu) |
|-----------|:-----:|:-------:|:--------------:|
| Listar impresoras OS | ✅ lpstat | ✅ Get-Printer | ✅ lpstat |
| Instalar TCP/IP | ✅ lpadmin | ✅ Add-Printer | ✅ lpadmin |
| Instalar USB | ✅ lpadmin (usb://) | ✅ Add-Printer (USB001) | ✅ lpadmin (usb://) |
| Prueba de impresión | ✅ lp | ✅ Out-Printer | ✅ lp |
| Renombrar | ✅ lpadmin -D | ✅ Rename-Printer | ✅ lpadmin -D |
| Limpiar cola | ✅ cancel -a | ✅ Remove-PrintJob | ✅ cancel -a |
| Autostart | ✅ LaunchAgent | ✅ Run key | ✅ systemd / XDG |

---

## Guards

Los guards validan precondiciones antes de ejecutar operaciones críticas. Se implementan tanto en **Rust** (antes de llamar a la estrategia) como en **Angular** (antes de emitir un invoke).

### Guards en Rust

Cada guard es una función `fn guard_*(args) -> Result<(), GuardError>` que se llama al inicio del comando Tauri correspondiente.

| Guard | Comando(s) que lo usa | Condición que valida |
|-------|-----------------------|----------------------|
| `guard_non_empty_name` | `rename_printer`, `add_network_printer`, `add_usb_printer` | El alias/nombre no es vacío ni solo espacios |
| `guard_valid_ip` | `add_network_printer`, `scan_tcp_ip_printers`, `print_test_tcp` | La cadena es una IPv4 válida (regex `\d{1,3}(\.\d{1,3}){3}`) |
| `guard_port_reachable` | `add_network_printer`, `print_test_tcp` | TCP connect a IP:9100 con timeout 3s tiene éxito |
| `guard_printer_exists_os` | `rename_printer`, `print_test`, `clear_print_queue` | El `queue_name` existe en la lista de impresoras del SO |
| `guard_alias_unique` | `add_network_printer`, `add_usb_printer` | El alias no existe ya en `custom_printers` de SQLite |
| `guard_usb_port_exists` | `add_usb_printer` | El puerto serie indicado existe en `get_serial_ports()` |
| `guard_no_active_jobs` | `clear_print_queue` | Advertencia (no bloqueante): informa si hay trabajos activos |

```rust
// Ejemplo de uso en comando:
#[tauri::command]
pub fn add_network_printer(ip: String, name: String) -> Result<String, String> {
    guard_non_empty_name(&name)?;
    guard_valid_ip(&ip)?;
    guard_port_reachable(&ip, 9100)?;
    guard_alias_unique(&name)?;
    get_strategy().install_network(&ip, &name)
}
```

### Guards en Angular (al nivel de servicio)

Son funciones puras que se ejecutan en `printers.service.ts` antes de llamar a `TauriService`. Retornan `string | null` (mensaje de error o null si pasa).

| Guard | Cuándo se ejecuta | Qué valida |
|-------|--------------------|-----------|
| `guardNonEmpty(value)` | Confirmar alias en diálogos | Alias no vacío |
| `guardValidIp(ip)` | Confirmar TCP/IP | Formato IPv4 básico |
| `guardPortSelected` | Confirmar USB | Puerto USB seleccionado |
| `guardIpSelected` | Confirmar TCP/IP | IP seleccionada de la lista de escaneo |

Cuando un guard falla, el servicio escribe en la señal `tcpResult` / `usbResult` el mensaje de error y **no** llama a Tauri. El template muestra el error inline en el diálogo.

---

## Pantalla principal

Layout `h-screen flex flex-col`:

### Header (fijo)
- Ícono de impresora + título "Printer Monitor"
- Badge DEV/PROD
- Badge con la IP local (ej. `192.168.1.10`)
- Botón de autostart (toggle on/off)
- Botón de refresh

### Contenido (flex row, ocupa el espacio restante)

#### Panel izquierdo — Impresoras del SO (flex-1)
- Lista de impresoras detectadas por el SO (CUPS / Windows Spooler)
- Para cada impresora:
  - Nombre
  - Badge de estado
  - Botón 58mm (prueba de impresión)
  - Botón 80mm (prueba de impresión)
  - Botón limpiar cola
  - Botón renombrar (inline: input + confirmar + cancelar)
- Skeleton loaders mientras carga

#### Panel derecho — Impresoras de la app (w-72 fijo)
- Lista de impresoras registradas manualmente por la app (guardadas en SQLite)
- Para cada impresora:
  - Nombre (alias)
  - Badge de tipo: `TCP/IP` (azul) o `USB` (violeta)
  - Dirección en `font-mono` (IP:9100 o ruta USB)
  - Botón 58mm y 80mm (prueba)
  - Botón eliminar (rojo)
- Footer del panel con dos botones:
  - [+ TCP/IP] — abre diálogo para agregar por red
  - [+ USB] — abre diálogo para agregar por puerto serie

---

## Diálogos (native `<dialog>`)

### Diálogo TCP/IP
1. Escanear subred (botón que llama a `scan_tcp_ip_printers`)
2. Seleccionar IP de la lista de resultados
3. Ingresar nombre/alias
4. Confirmar → pasa guards Angular → invoke → guards Rust → estrategia instala

### Diálogo USB
1. Lista de puertos USB detectados como impresoras (de `get_serial_ports`)
2. Seleccionar puerto
3. Ingresar nombre/alias
4. Confirmar → pasa guards Angular → invoke → guards Rust → registra en SQLite

---

## Modelo de datos

### PrinterInfo (Tauri → Angular)
```
name: string            // nombre visible
queue_name: string      // nombre interno de cola CUPS / Windows
is_default: boolean
status: string          // "Disponible" | "Imprimiendo" | ...
source: "os" | "app"    // origen: sistema operativo o registrada por la app
connection_type: "os" | "network" | "usb_direct"
address: string | null  // IP:9100 o /dev/tty... para app printers
```

### custom_printers (SQLite)
```sql
CREATE TABLE custom_printers (
  alias           TEXT PRIMARY KEY,
  connection_type TEXT NOT NULL,  -- "network" | "usb_direct"
  address         TEXT NOT NULL   -- IP:9100 | /dev/cu.usbXXX
);
```

---

## Comandos Tauri (invoke)

| Comando | Guard Rust | Descripción |
|---------|-----------|-------------|
| `get_system_info` | — | IP local, puerto, is_dev, impresoras, puertos serie, autostart |
| `get_printers` | — | Lista de impresoras (OS + app) |
| `rename_printer` | `non_empty_name`, `printer_exists_os` | Renombrar impresora CUPS/Spooler |
| `print_test` | `printer_exists_os` | Prueba en impresora OS (por queue_name) |
| `print_test_tcp` | `valid_ip`, `port_reachable` | Prueba directa a IP:9100 (sin registrar) |
| `add_network_printer` | `non_empty_name`, `valid_ip`, `port_reachable`, `alias_unique` | Registrar impresora TCP/IP |
| `add_usb_printer` | `non_empty_name`, `usb_port_exists`, `alias_unique` | Registrar impresora USB |
| `clear_print_queue` | `printer_exists_os` | Limpiar cola de impresión |
| `remove_custom_printer` | — | Eliminar impresora de SQLite |
| `scan_tcp_ip_printers` | `valid_ip` | Escanear red en busca de impresoras en puerto 9100 |
| `get_network_config` | — | IP, máscara, gateway de la interfaz activa |
| `get_serial_ports` | — | Puertos serie/USB disponibles |
| `get_autostart_enabled` | — | Estado del autostart |
| `set_autostart_enabled` | — | Activar/desactivar autostart |

---

## Estructura de módulos Rust

```
src-tauri/src/
├── lib.rs                  // setup Tauri, registro de comandos
├── main.rs
├── strategy/
│   ├── mod.rs              // trait PrinterStrategy + fn get_strategy()
│   ├── mac.rs              // MacStrategy  (cfg macos)
│   ├── windows.rs          // WindowsStrategy (cfg windows)
│   └── linux.rs            // LinuxStrategy (cfg linux)
├── guards.rs               // todas las funciones guard_*
├── printers.rs             // comandos Tauri (delegan a strategy + guards)
├── serial.rs               // get_serial_ports
├── network.rs              // scan_tcp_ip_printers, get_network_config
├── system.rs               // get_system_info, get_local_ip, autostart, watcher
├── settings.rs             // SQLite helpers, open_db_global
└── api_server.rs           // servidor HTTP interno puerto 8001
```

---

## Servidor HTTP interno (api_server)

Puerto fijo `8001`. Recibe trabajos de impresión de otros procesos locales (ej. sistemas de POS).

```
POST /api/print
Body: { printer: string, pdf_b64: string, width: "58mm" | "80mm" }
```

Usa `print_pdf_job()` internamente — no es un comando Tauri, es una función Rust pura.

---

## Estructura Angular

```
src/app/
├── app.ts                      // bootstrap, solo monta HomeComponent
├── app.config.ts               // providers globales (TauriService, icons)
├── btn.component.ts            // botón reutilizable
├── card.component.ts           // card reutilizable
├── services/
│   └── tauri.service.ts        // wrapper tipado sobre invoke()
└── home/
    ├── home.component.ts
    ├── home.component.html
    ├── home.component.scss
    ├── system.service.ts       // IP, autostart, refresh, Tauri event listener
    └── printers.service.ts     // estado y acciones de impresoras + diálogos
```

---

## Características NO incluidas en v2

- Dashboard de sistema (CPU, RAM, disco)
- Configuración de red (cambiar IP estática / DHCP)
- Bluetooth
- Escáner general de red (ping sweep)
- Impresión de tickets con imagen/ESC-POS desde Angular
- Renombrar impresoras de la app (solo se pueden eliminar y volver a agregar)