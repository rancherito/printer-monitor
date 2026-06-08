import { ApplicationConfig, provideBrowserGlobalErrorListeners, provideZonelessChangeDetection } from '@angular/core';
import { provideIcons } from '@ng-icons/core';
import {
  matAdd, matBolt, matCheck, matCheckCircle, matClose,
  matDeleteOutline, matDescription, matEdit, matFolder,
  matFolderOpen, matHistory, matLan, matListAlt, matPrint, matRefresh,
  matSearch, matSettings, matUsb,
} from '@ng-icons/material-icons/baseline';

export const appConfig: ApplicationConfig = {
  providers: [
    provideZonelessChangeDetection(),
    provideBrowserGlobalErrorListeners(),
    provideIcons({
      matAdd, matBolt, matCheck, matCheckCircle, matClose,
      matDeleteOutline, matDescription, matEdit, matFolder,
      matFolderOpen, matHistory, matLan, matListAlt, matPrint, matRefresh,
      matSearch, matSettings, matUsb,
    }),
  ]
};
