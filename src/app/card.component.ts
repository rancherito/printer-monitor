import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'cd-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: '<ng-content />',
  styles: [`
    :host {
      display: block;
      background: var(--bg-surface);
      border: 1px solid var(--border);
      border-radius: var(--radius, 8px);
      padding: 10px 12px;
    }
  `],
})
export class CardComponent {}
