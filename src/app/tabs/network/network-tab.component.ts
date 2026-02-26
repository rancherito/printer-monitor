import { Component, input, output, ChangeDetectionStrategy } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { NetworkDevice, AppSettings } from '../../tauri.service';

@Component({
  selector: 'app-network-tab',
  standalone: true,
  imports: [NgIconComponent, CardComponent, BtnComponent],
  templateUrl: './network-tab.component.html',
  styleUrl: './network-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NetworkTabComponent {
  readonly scanningNetwork = input.required<boolean>();
  readonly networkDevices = input.required<NetworkDevice[]>();
  readonly networkError = input.required<string | null>();
  readonly settings = input.required<AppSettings | null>();
  readonly portDev = input.required<number>();
  readonly portProd = input.required<number>();
  readonly savingPort = input.required<boolean>();
  readonly portSaveResult = input.required<{ ok: boolean; message: string } | null>();

  readonly scanNetworkClick = output<void>();
  readonly savePortClick = output<{ key: 'port_dev' | 'port_prod'; value: string }>();
}
