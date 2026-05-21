import { ChangeDetectionStrategy, Component } from '@angular/core';
import { HomeComponent } from './home/home.component';

@Component({
  selector: 'app-root',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HomeComponent],
  template: '<app-home />',
})
export class App {}
