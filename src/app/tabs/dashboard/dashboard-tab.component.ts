import { Component, ChangeDetectionStrategy, inject } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { SystemService } from './system.service';
import { PrintersService } from '../printers/printers.service';
import { BluetoothService } from '../bluetooth-tab/bluetooth.service';

type UsbPrintSize = 'thermal_58mm' | 'thermal_80mm';

@Component({
  selector: 'app-dashboard-tab',
  imports: [NgIconComponent, CardComponent, BtnComponent],
  templateUrl: './dashboard-tab.component.html',
  styleUrl: './dashboard-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DashboardTabComponent {
  protected readonly system = inject(SystemService);
  protected readonly printers = inject(PrintersService);
  protected readonly bluetooth = inject(BluetoothService);

  isUsbPrinting(portName: string, size: UsbPrintSize): boolean {
    return this.system.usbPrintingFor() === `${portName}::${size}`;
  }

  /** Devuelve true si port_name es una ruta real de dispositivo (/dev/… o COM…). */
  isDevPath(portName: string): boolean {
    return portName.startsWith('/dev/') || /^COM\d+$/i.test(portName) || portName.startsWith('\\\\.\\');
  }
}
