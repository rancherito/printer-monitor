import { Component, input, output, ChangeDetectionStrategy } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { PrinterInfo, BluetoothDevice, SerialPort } from '../../tauri.service';

type UsbPrintSize = 'thermal_50mm' | 'thermal_80mm';

@Component({
  selector: 'app-dashboard-tab',
  standalone: true,
  imports: [NgIconComponent, CardComponent, BtnComponent],
  templateUrl: './dashboard-tab.component.html',
  styleUrl: './dashboard-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DashboardTabComponent {
  readonly loading = input.required<boolean>();
  readonly localIp = input.required<string>();
  readonly port = input.required<number | '—'>();
  readonly frontendUrl = input.required<string>();
  readonly printers = input.required<PrinterInfo[]>();
  readonly bluetoothDevices = input.required<BluetoothDevice[]>();
  readonly bluetoothLoaded = input.required<boolean>();
  readonly loadingBluetooth = input.required<boolean>();
  readonly serialPorts = input.required<SerialPort[]>();

  /** Clave `portName::size` de la impresora USB en curso, o `null` si no hay trabajo activo. */
  readonly usbPrintingFor = input.required<string | null>();
  /** Resultado del último trabajo USB: `null` si no hay ninguno todavía. */
  readonly usbPrintResult = input.required<{ ok: boolean; message: string } | null>();

  readonly loadBluetoothClick = output<void>();
  readonly printUsbTestClick = output<{ portName: string; size: UsbPrintSize }>();

  isUsbPrinting(portName: string, size: UsbPrintSize): boolean {
    return this.usbPrintingFor() === `${portName}::${size}`;
  }
}
