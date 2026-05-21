export interface PrinterInfo {
  name: string;
  queue_name: string;
  is_default: boolean;
  status: string;
  source: 'os' | 'app';
  connection_type: 'os' | 'network' | 'usb_direct' | 'usb_system' | 'usb_app';
  address: string | null;
}

export interface SystemInfo {
  local_ip: string;
  port: number;
  is_dev: boolean;
  printers: PrinterInfo[];
  serial_ports: string[];
  autostart_enabled: boolean;
}

export interface NetworkConfig {
  ip: string;
  mask: string;
  gateway: string;
}
