'use client';

import { useEffect } from 'react';

export function ServiceWorkerRegister() {
  useEffect(() => {
    if (typeof window === 'undefined') return;
    if (!('serviceWorker' in navigator)) return;

    navigator.serviceWorker
      .register('/sw.js')
      .then((registration) => {
        console.log('[Xergon] Service Worker registered:', registration.scope);
      })
      .catch((error) => {
        console.warn('[Xergon] Service Worker registration failed:', error);
      });
  }, []);

  return null;
}
