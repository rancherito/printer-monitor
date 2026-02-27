import { Component, inject, ChangeDetectionStrategy } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { NetworkConfigComponent } from './network-config.component';
import { NetworkService } from './network.service';

@Component({
  selector: 'app-network-tab',
  standalone: true,
  imports: [NgIconComponent, CardComponent, BtnComponent, NetworkConfigComponent],
  templateUrl: './network-tab.component.html',
  styleUrl: './network-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NetworkTabComponent {
  protected readonly network = inject(NetworkService);
}
