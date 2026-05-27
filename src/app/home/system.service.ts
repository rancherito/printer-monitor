import { Injectable, inject, signal, computed } from '@angular/core';
import { TauriService } from '../services/tauri.service';
import { LogService } from '../services/log.service';
import { SystemInfo, PrintedFile } from '../models';

@Injectable({ providedIn: 'root' })
export class SystemService {
  private readonly tauri = new TauriService();
  private readonly log = inject(LogService);

  readonly systemInfo = signal<SystemInfo | null>(null);
  readonly loading = signal(false);
  readonly error = signal<string | null>(null);
  readonly pendingPort = signal<number | null>(null);

  readonly localIp = computed(() => this.systemInfo()?.local_ip ?? '—');
  readonly isDev = computed(() => this.systemInfo()?.is_dev ?? false);
  readonly autostartEnabled = computed(() => this.systemInfo()?.autostart_enabled ?? false);
  readonly serverPort = computed(() => this.systemInfo()?.port ?? 8001);
  readonly serverAddress = computed(() => `${this.localIp()}:${this.serverPort()}`);

  // ─── Configuración ─────────────────────────────────────────────────────────
  readonly outputDir = signal<string>('');
  readonly outputDirInput = signal<string>('');
  readonly printedFiles = signal<PrintedFile[]>([]);
  readonly filesLoading = signal(false);

  async loadSystemInfo(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const info = await this.tauri.getSystemInfo();
      this.systemInfo.set(info);
      this.log.log('info', `Sistema cargado — IP: ${info.local_ip}:${info.port}`);
    } catch (e) {
      this.error.set(String(e));
      this.log.log('error', 'Error al cargar información del sistema', String(e));
    } finally {
      this.loading.set(false);
    }
  }

  async loadOutputDir(): Promise<void> {
    try {
      const dir = await this.tauri.getOutputDir();
      this.outputDir.set(dir);
      this.outputDirInput.set(dir);
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async saveOutputDir(): Promise<void> {
    try {
      await this.tauri.setOutputDir(this.outputDirInput());
      this.outputDir.set(this.outputDirInput());
      this.log.log('success', `Directorio de salida guardado: ${this.outputDirInput()}`);
    } catch (e) {
      this.error.set(String(e));
      this.log.log('error', 'Error al guardar directorio de salida', String(e));
    }
  }

  async loadPrintedFiles(): Promise<void> {
    this.filesLoading.set(true);
    try {
      const files = await this.tauri.listPrintedFiles();
      this.printedFiles.set(files);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.filesLoading.set(false);
    }
  }

  async openOutputDir(): Promise<void> {
    try { await this.tauri.openOutputDir(); } catch { /* ignore */ }
  }

  async toggleAutostart(): Promise<void> {
    const current = this.autostartEnabled();
    try {
      await this.tauri.setAutostartEnabled(!current);
      this.systemInfo.update(s => s ? { ...s, autostart_enabled: !current } : s);
      this.log.log('info', `Inicio automático ${!current ? 'activado' : 'desactivado'}`);
    } catch (e) {
      this.error.set(String(e));
      this.log.log('error', 'Error al cambiar inicio automático', String(e));
    }
  }

  async setServerPort(port: number): Promise<void> {
    try {
      await this.tauri.setServerPort(port);
      this.pendingPort.set(port);
      this.log.log('success', `Puerto del servidor cambiado a ${port} (requiere reinicio)`);
    } catch (e) {
      this.error.set(String(e));
      this.log.log('error', 'Error al cambiar puerto del servidor', String(e));
    }
  }
}
