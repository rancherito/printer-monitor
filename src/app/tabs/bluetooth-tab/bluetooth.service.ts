import { Injectable, inject, signal } from '@angular/core';
import { TauriService, BluetoothDevice } from '../../services/tauri.service';

@Injectable({ providedIn: 'root' })
export class BluetoothService {
  private readonly tauri = inject(TauriService);

  readonly loadingBluetooth = signal(false);
  readonly bluetoothDevices = signal<BluetoothDevice[]>([]);
  readonly bluetoothError = signal<string | null>(null);
  readonly bluetoothLoaded = signal(false);

  async loadBluetooth(): Promise<void> {
    this.loadingBluetooth.set(true);
    this.bluetoothError.set(null);
    this.bluetoothLoaded.set(false);
    try {
      const devices = await this.tauri.getBluetoothDevices();
      this.bluetoothDevices.set(devices);
      this.bluetoothLoaded.set(true);
    } catch (e) {
      this.bluetoothError.set(String(e));
    } finally {
      this.loadingBluetooth.set(false);
    }
  }
}
