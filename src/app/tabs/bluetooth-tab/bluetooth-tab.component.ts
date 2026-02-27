import { Component, ChangeDetectionStrategy, inject } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { BluetoothService } from './bluetooth.service';

@Component({
  selector: 'app-bluetooth-tab',
  imports: [NgIconComponent, CardComponent, BtnComponent],
  templateUrl: './bluetooth-tab.component.html',
  styleUrl: './bluetooth-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BluetoothTabComponent {
  protected readonly bluetooth = inject(BluetoothService);
}
