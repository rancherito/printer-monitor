import { Injectable, computed, inject, resource, signal } from '@angular/core';
import { TauriService } from '../services/tauri.service';
import { LogService } from '../services/log.service';
import { PrintedFile, SystemInfo } from '../models';

@Injectable({ providedIn: 'root' })
export class SystemResourceService {
  private readonly tauri = inject(TauriService);
  private readonly log = inject(LogService);

  private readonly refresh$ = signal(0);

  readonly systemInfo = resource<SystemInfo, number>({
    params: () => this.refresh$(),
    loader: () => this.tauri.getSystemInfo(),
  });

  readonly outputDir = resource<string, number>({
    params: () => this.refresh$(),
    loader: () => this.tauri.getOutputDir(),
  });

  readonly printedFiles = resource<PrintedFile[], number>({
    params: () => this.refresh$(),
    loader: () => this.tauri.listPrintedFiles(),
  });

  readonly localIp = computed(() => this.systemInfo.value()?.local_ip ?? '—');
  readonly port = computed(() => this.systemInfo.value()?.port ?? 8001);
  readonly serverAddress = computed(() => `${this.localIp()}:${this.port()}`);
  readonly isDev = computed(() => this.systemInfo.value()?.is_dev ?? false);
  readonly autostartEnabled = computed(() => this.systemInfo.value()?.autostart_enabled ?? false);
  readonly serialPorts = computed(() => this.systemInfo.value()?.serial_ports ?? []);
  readonly printers = computed(() => this.systemInfo.value()?.printers ?? []);

  refreshAll(): void {
    this.refresh$.update((n) => n + 1);
  }

  async toggleAutostart(): Promise<void> {
    const current = this.autostartEnabled();
    try {
      await this.tauri.setAutostartEnabled(!current);
      this.refreshAll();
      this.log.log('info', `Inicio automático ${!current ? 'activado' : 'desactivado'}`);
    } catch (e) {
      this.log.log('error', 'Error al cambiar inicio automático', String(e));
    }
  }

  async setServerPort(port: number): Promise<void> {
    try {
      await this.tauri.setServerPort(port);
      this.refreshAll();
      this.log.log('success', `Puerto del servidor cambiado a ${port} (requiere reinicio)`);
    } catch (e) {
      this.log.log('error', 'Error al cambiar puerto del servidor', String(e));
    }
  }

  async saveOutputDir(dir: string): Promise<void> {
    try {
      await this.tauri.setOutputDir(dir);
      this.refreshAll();
      this.log.log('success', `Directorio de salida guardado: ${dir}`);
    } catch (e) {
      this.log.log('error', 'Error al guardar directorio de salida', String(e));
    }
  }

  async openOutputDir(): Promise<void> {
    try {
      await this.tauri.openOutputDir();
    } catch {
      /* ignore */
    }
  }
}
