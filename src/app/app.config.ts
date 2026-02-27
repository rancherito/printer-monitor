import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideIcons } from '@ng-icons/core';
import {
  matDashboard, matPrint, matLan, matBluetooth, matRefresh,
  matUsb, matEdit, matClose, matCheck, matSearch, matPowerSettingsNew,
  matRouter, matWifi, matAdd, matSettings, matLayersClear, matKeyboard,
} from '@ng-icons/material-icons/baseline';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideIcons({
      matDashboard, matPrint, matLan, matBluetooth, matRefresh,
      matUsb, matEdit, matClose, matCheck, matSearch, matPowerSettingsNew,
      matRouter, matWifi, matAdd, matSettings, matLayersClear, matKeyboard,
    }),
  ],
};

