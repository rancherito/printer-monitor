import { Component, inject, ChangeDetectionStrategy } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { IpInputComponent } from '../../ip-input.component';
import { NetworkService } from './network.service';

@Component({
  selector: 'app-network-config',
  imports: [NgIconComponent, CardComponent, BtnComponent, IpInputComponent],
  template: `
    <section aria-labelledby="network-config-title">
      <h2 class="section-title" id="network-config-title">
        Configuración de red del equipo
        @if (!network.loadingNetworkConfig() && !network.networkConfig()) {
          <button appBtn variant="ghost" size="sm" (click)="network.loadNetworkConfig()" class="ml-auto">
            <ng-icon name="matRefresh" size="14"></ng-icon>
            Cargar
          </button>
        }
      </h2>

      @if (network.loadingNetworkConfig()) {
        <app-card padding="md" class="flex items-center gap-3 text-sm text-slate-500">
          <span class="w-4 h-4 border-2 border-blue-500 border-r-transparent rounded-full animate-spin shrink-0"></span>
          Obteniendo configuración de red...
        </app-card>
      } @else if (network.networkConfig()) {
        <app-card padding="md" class="flex flex-col gap-4">
          @if (network.networkConfigResult()) {
            <div class="flex items-start gap-2 p-2 rounded-md text-xs border"
                 [class.bg-green-50]="network.networkConfigResult()!.ok"
                 [class.border-green-200]="network.networkConfigResult()!.ok"
                 [class.text-green-700]="network.networkConfigResult()!.ok"
                 [class.bg-red-50]="!network.networkConfigResult()!.ok"
                 [class.border-red-200]="!network.networkConfigResult()!.ok"
                 [class.text-red-600]="!network.networkConfigResult()!.ok">
              <ng-icon [name]="network.networkConfigResult()!.ok ? 'matCheck' : 'matClose'" size="14" class="shrink-0"></ng-icon>
              {{ network.networkConfigResult()!.message }}
            </div>
          }

          @if (network.editingNetworkConfig()) {
            <div class="flex flex-col gap-3">
              <div class="flex flex-col gap-1.5">
                <label class="label-xs">Dirección IP</label>
                <app-ip-input [value]="network.tempIp()" (valueChange)="network.tempIp.set($event)" />
              </div>
              <div class="flex flex-col gap-1.5">
                <label class="label-xs">Máscara de subred</label>
                <app-ip-input [value]="network.tempMask()" (valueChange)="network.tempMask.set($event)" />
              </div>
              <div class="flex flex-col gap-1.5">
                <label class="label-xs">Gateway</label>
                <app-ip-input [value]="network.tempGateway()" (valueChange)="network.tempGateway.set($event)" />
              </div>
            </div>
            <div class="flex flex-wrap gap-2">
              <button appBtn variant="primary" size="sm" (click)="network.saveNetworkConfig()" [disabled]="network.savingNetworkConfig()">
                @if (network.savingNetworkConfig()) {
                  <span class="w-3 h-3 border-2 border-white border-r-transparent rounded-full animate-spin"></span>
                } @else {
                  <ng-icon name="matCheck" size="14"></ng-icon>
                }
                Guardar cambios
              </button>
              <button appBtn variant="ghost" size="sm" (click)="network.cancelEditNetworkConfig()">
                <ng-icon name="matClose" size="14"></ng-icon>
                Cancelar
              </button>
            </div>
          } @else {
            <div class="flex flex-col gap-3">
              <div>
                <p class="label-xs">Dirección IP</p>
                <p class="value-lg">{{ network.networkConfig()!.ip }}</p>
              </div>
              <div>
                <p class="label-xs">Máscara de subred</p>
                <p class="value-lg">{{ network.networkConfig()!.mask }}</p>
              </div>
              <div>
                <p class="label-xs">Gateway</p>
                <p class="value-lg">{{ network.networkConfig()!.gateway }}</p>
              </div>
              <div class="text-xs text-slate-400 flex items-center">
                Interfaz: <span class="font-mono ml-1">{{ network.networkConfig()!.interface }}</span>
              </div>
            </div>
            <div class="flex gap-2">
              <button appBtn variant="secondary" size="sm" (click)="network.startEditNetworkConfig()">
                <ng-icon name="matEdit" size="14"></ng-icon>
                Editar configuración
              </button>
              <button appBtn variant="ghost" size="sm" (click)="network.restoreNetworkDhcp()">
                <ng-icon name="matRefresh" size="14"></ng-icon>
                Restaurar DHCP
              </button>
            </div>
          }
        </app-card>
      } @else {
        <app-card padding="md">
          <p class="text-sm text-slate-400">Pulsa "Cargar" para ver la configuración de red.</p>
        </app-card>
      }
    </section>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NetworkConfigComponent {
  protected readonly network = inject(NetworkService);
}
