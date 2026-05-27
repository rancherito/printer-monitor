import { Injectable, computed, signal } from '@angular/core';
import { LogEntry } from '../models';

@Injectable({ providedIn: 'root' })
export class LogService {
  private counter = 0;

  readonly entries = signal<LogEntry[]>([]);
  readonly unreadCount = computed(() => this.entries().filter(e => !e.read).length);

  log(level: LogEntry['level'], message: string, detail?: string): void {
    this.entries.update(ls =>
      [{ id: ++this.counter, level, message, detail, timestamp: new Date(), read: false }, ...ls].slice(0, 500)
    );
  }

  markAllRead(): void {
    this.entries.update(ls => ls.map(e => ({ ...e, read: true })));
  }

  clear(): void {
    this.entries.set([]);
  }
}
