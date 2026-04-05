'use client';

/**
 * Register the Xergon service worker.
 *
 * Safe to call multiple times -- checks for serviceWorker support
 * and only registers once.
 */
export function registerSW(): void {
  if (typeof window === 'undefined') return;
  if (!('serviceWorker' in navigator)) return;

  window.addEventListener('load', () => {
    navigator.serviceWorker
      .register('/sw.js')
      .then((registration) => {
        console.log('[Xergon] Service Worker registered:', registration.scope);
      })
      .catch((error) => {
        console.warn('[Xergon] Service Worker registration failed:', error);
      });
  });
}
