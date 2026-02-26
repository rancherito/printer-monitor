import { Component, input, output, ChangeDetectionStrategy } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { BluetoothDevice } from '../../tauri.service';

@Component({
  selector: 'app-bluetooth-tab',
  standalone: true,
  imports: [NgIconComponent, CardComponent, BtnComponent],
  templateUrl: './bluetooth-tab.component.html',
  styleUrl: './bluetooth-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BluetoothTabComponent {
  readonly bluetoothDevices = input.required<BluetoothDevice[]>();
  readonly bluetoothLoaded = input.required<boolean>();
  readonly loadingBluetooth = input.required<boolean>();
  readonly bluetoothError = input.required<string | null>();

  readonly loadBluetoothClick = output<void>();
}
