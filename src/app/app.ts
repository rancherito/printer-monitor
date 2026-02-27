import { Component, OnInit, OnDestroy, signal, ChangeDetectionStrategy, inject } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { BtnComponent } from './btn.component';
import { DashboardTabComponent } from './tabs/dashboard/dashboard-tab.component';
import { PrintersTabComponent } from './tabs/printers/printers-tab.component';
import { NetworkTabComponent } from './tabs/network/network-tab.component';
import { BluetoothTabComponent } from './tabs/bluetooth-tab/bluetooth-tab.component';
import { SystemService } from './tabs/dashboard/system.service';

type TabId = 'dashboard' | 'printers' | 'network' | 'bluetooth';

@Component({
  selector: 'app-root',
  imports: [NgIconComponent, BtnComponent, DashboardTabComponent, PrintersTabComponent, NetworkTabComponent, BluetoothTabComponent],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App implements OnInit, OnDestroy {
  protected readonly system = inject(SystemService);

  protected readonly tabs: ReadonlyArray<{ id: TabId; label: string; icon: string }> = [
    { id: 'dashboard', label: 'Dashboard', icon: 'matDashboard' },
    { id: 'printers', label: 'Impresoras', icon: 'matPrint' },
    { id: 'network', label: 'Red', icon: 'matLan' },
    { id: 'bluetooth', label: 'Bluetooth', icon: 'matBluetooth' },
  ];

  protected readonly activeTab = signal<TabId>('dashboard');

  async ngOnInit(): Promise<void> {
    await this.system.init();
  }

  ngOnDestroy(): void {
    this.system.destroy();
  }

  setActiveTab(tab: TabId): void {
    this.activeTab.set(tab);
  }
}
