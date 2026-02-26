# Printer Monitor — Especificación Técnica

## Descripción General

Aplicación de escritorio para macOS, Windows y Linux que actúa como **monitor y proxy de impresión local**. Expone una URL HTTP en la red local para que otros dispositivos puedan enviar trabajos de impresión a las impresoras instaladas en el equipo host.

---

## Stack Tecnológico

| Capa | Tecnología |
|---|---|
| Shell | [Tauri 2.x](https://tauri.app) (Rust) |
| Frontend | Angular 21, Signals, OnPush, Tailwind CSS v4 |
| Persistencia | SQLite via `rusqlite` (bundled) |
| Construcción | Bun / npm · `ng build` + `tauri build` |
| Plataformas objetivo | macOS (aarch64), Windows (x86_64), Linux |

---

## Arquitectura

```
┌─────────────────────────────────────────────────────┐
│  Ventana Tauri (WebView)                            │
│  ┌───────────────────────────────────────────────┐  │
│  │  Angular 21 Frontend (puerto 9002/9003)       │  │
│  │  · Signals + OnPush                           │  │
│  │  · TauriService ──invoke()──▶ Rust commands  │  │
│  └───────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────┐  │
│  │  Rust Backend (lib.rs)                        │  │
│  │  · Impresoras (CUPS / WMI)                    │  │
│  │  · Generación PDF en memoria                  │  │
│  │  · Escaneo de red (ping paralelo)             │  │
│  │  · Bluetooth (system_profiler / bluetoothctl) │  │
│  │  · Autostart (tauri-plugin-autostart)         │  │
│  │  · Configuración SQLite                       │  │
│  └───────────────────────────────────────────────┘  │
│  [$APP_DATA/settings.db]  SQLite                    │
└─────────────────────────────────────────────────────┘
        │ HTTP accesible desde la red local
        ▼
   http://<IP_LOCAL>:<PUERTO>/
   (otros dispositivos de la subred)
```

---

## Gestión de Puertos

| Modo | Puerto por defecto | Persistido en SQLite |
|---|---|---|
| Desarrollo (`debug_assertions`) | 9002 | ❌ Siempre fijo |
| Producción (release) | 9003 | ✅ Clave `port_prod` |

**Comportamiento si el puerto está ocupado:** se busca automáticamente el siguiente puerto TCP libre (`find_free_port`) y, en producción, se persiste el nuevo valor.

El servidor de desarrollo de Angular se configura con `ng serve --port 9002` y Tauri apunta a `http://localhost:9002` como `devUrl`.

---

## Base de Datos

**Ruta:** `$APP_DATA/settings.db` (resuelto por Tauri según plataforma)

| Tabla | Columnas |
|---|---|
| `settings` | `key TEXT PRIMARY KEY`, `value TEXT NOT NULL` |

**Claves reservadas:**

| Clave | Tipo | Descripción |
|---|---|---|
| `port_prod` | `u16` | Puerto HTTP de producción (defecto: 9003) |

> `port_dev` **no se almacena** — es siempre 9002 y se resuelve en tiempo de ejecución.

---

## Funcionalidades

### 1. Información del Servidor
- **IP Local** — detectada con `local-ip-address`
- **Puerto activo** — según modo (dev/prod) con fallback automático
- **URL de Acceso** — `http://<IP>:<PUERTO>` visible y clicable para acceso desde otros dispositivos
- **Badge de modo** — indicador visual DEV (ámbar) / PROD (verde) en el header

### 2. Gestión de Impresoras

#### Listado
- macOS / Linux: `lpstat -p` (CUPS) para nombre y estado; `lpstat -d` para la predeterminada
- Windows: `wmic printer get Name,Default,PrinterStatus /format:csv`
- Estados mapeados: Disponible · Imprimiendo · Deshabilitada · Calentando · Desconocido

#### Renombrado
- **macOS / Linux (CUPS):** `lpadmin -p <queue_name> -D "<nuevo_nombre>"` — cambia la descripción visible en el sistema (diálogos de impresión y Configuración del Sistema)
- **Windows:** PowerShell `Rename-Printer -Name "<viejo>" -NewName "<nuevo>"`
- La UI permite editar el nombre inline (Enter para confirmar, Escape para cancelar)
- Tras renombrar exitosamente se recarga la lista de impresoras

#### Impresión de prueba
Genera un PDF mínimo en memoria (sin dependencias externas) y lo envía a la impresora:

| Formato | Dimensiones (pt) | Uso típico |
|---|---|---|
| A4 | 595 × 842 | Impresoras de oficina |
| Térmica 50 mm | 142 × 200 | Tickets pequeños |
| Térmica 80 mm | 227 × 200 | Tickets estándar POS |

- macOS / Linux: `lp -d <impresora> <archivo.pdf>`
- Windows: `cmd /C print /D:<impresora> <archivo.pdf>`

### 3. Escaneo de Red
- Detecta la subred local a partir de la IP del equipo
- Lanza 254 hilos en paralelo con `ping -c 1 -W 1` (macOS/Linux) o `ping -n 1 -w 500` (Windows)
- Resolución inversa de hostname con `host <ip>` (macOS/Linux)
- Resultados ordenados por último octeto de la IP
- Operación bajo demanda (botón "Escanear red") — puede tardar 5-30 s

### 4. Dispositivos Bluetooth
- **macOS:** `system_profiler SPBluetoothDataType -json` — lista dispositivos conectados y no conectados
- **Windows:** PowerShell `Get-PnpDevice -Class Bluetooth`
- **Linux:** `bluetoothctl devices` + `bluetoothctl info <addr>`
- Muestra: nombre, dirección MAC, estado Conectado / Emparejado
- Operación bajo demanda (botón "Cargar Bluetooth")

### 5. Inicio Automático
- Controlado por `tauri-plugin-autostart` v2
- macOS: `LaunchAgent` (sin privilegios de administrador)
- Toggle inmediato desde el header con feedback visual
- Estado sincronizado con `get_system_info` al arrancar

### 6. Configuración de Puertos
- **Puerto desarrollo:** mostrado como informativo (fijo 9002, no editable)
- **Puerto producción:** editable y persistido en SQLite
- Validación: 1–65535; error inline si el valor es inválido

---

## Comandos Tauri (IPC)

| Comando Rust | Descripción | Devuelve |
|---|---|---|
| `get_system_info` | Info completa al arranque | `SystemInfo` |
| `get_settings` | Configuración de puertos y extras | `AppSettings` |
| `set_setting(key, value)` | Persiste un par en SQLite | `Result<()>` |
| `get_printers` | Lista impresoras del SO | `Vec<PrinterInfo>` |
| `rename_printer(name, new_name)` | Renombra en el SO | `Result<String>` |
| `print_test(name, size)` | Genera e imprime PDF de prueba | `Result<String>` |
| `get_local_ip` | IP local del equipo | `String` |
| `get_app_port` | Puerto activo según modo | `u16` |
| `get_autostart_enabled` | Estado de inicio automático | `Result<bool>` |
| `set_autostart_enabled(enabled)` | Activa/desactiva autostart | `Result<()>` |
| `scan_network` | Escaneo de subred local | `Vec<NetworkDevice>` |
| `get_bluetooth_devices` | Dispositivos BT emparejados | `Vec<BluetoothDevice>` |

---

## Modelos de Datos

```typescript
interface SystemInfo {
  local_ip: string;
  port: number;
  is_dev: boolean;
  printers: PrinterInfo[];
  autostart_enabled: boolean;
  network_devices: NetworkDevice[];   // siempre [] en get_system_info
  bluetooth_devices: BluetoothDevice[]; // siempre [] en get_system_info
}

interface PrinterInfo {
  name: string;
  is_default: boolean;
  status: string;
}

interface AppSettings {
  port_dev: number;    // siempre 9002
  port_prod: number;   // de BD, default 9003
  active_port: number; // resuelto en runtime
  is_dev: boolean;
  extra: Record<string, string>;
}

interface NetworkDevice {
  ip: string;
  hostname: string | null;
  is_reachable: boolean;
}

interface BluetoothDevice {
  name: string;
  address: string;  // MAC
  is_connected: boolean;
}
```

---

## Permisos y Capacidades (Tauri)

Definidos en `src-tauri/capabilities/default.json`:

| Permiso | Uso |
|---|---|
| `autostart:allow-enable` | Activar inicio automático |
| `autostart:allow-disable` | Desactivar inicio automático |
| `autostart:allow-is-enabled` | Leer estado de autostart |
| `shell:allow-*` | Ejecución de comandos del SO |

---

## Limitaciones Conocidas

- **Renombrado en macOS:** `lpadmin -D` cambia la *descripción* (display name en diálogos), no el identificador interno de la cola CUPS. Puede requerir que el usuario cierre y reabra el diálogo de impresión para ver el cambio.
- **Escaneo de red:** basado en ICMP ping; hosts con firewall que bloqueen ping no serán detectados. La duración depende de la latencia de la subred.
- **Bluetooth en macOS:** requiere macOS 12+ para `system_profiler SPBluetoothDataType -json`.
- **Impresión de prueba (Windows):** el comando `print` de `cmd` soporta impresoras de red limitadas; se recomienda verificar con una impresora local.
- **Puerto de desarrollo:** es siempre 9002 en runtime; si está ocupado se usa el siguiente libre pero el valor no se persiste (se repite la búsqueda en cada arranque).

---

## Scripts Disponibles

```bash
npm run start           # ng serve --port 9002 (frontend dev)
npm run build           # ng build (producción)
npm run tauri:dev       # tauri dev (app completa en modo dev)
npm run tauri:build     # tauri build (todas las plataformas)
npm run tauri:build:mac # tauri build --target aarch64-apple-darwin
```

---

## Estructura de Ficheros Relevantes

```
printer-monitor/
├── SPEC.md                          ← este archivo
├── src/
│   └── app/
│       ├── app.ts                   ← componente principal (signals, métodos)
│       ├── app.html                 ← template Angular
│       ├── app.scss
│       └── tauri.service.ts         ← wrapper de comandos Tauri IPC
└── src-tauri/
    ├── Cargo.toml                   ← dependencias Rust
    ├── tauri.conf.json              ← configuración Tauri (puertos, bundle)
    ├── capabilities/default.json    ← permisos de la app
    └── src/
        ├── lib.rs                   ← todos los comandos Tauri
        └── main.rs
```
