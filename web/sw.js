// Cache only the public shell. API responses and credentials are never cached.
const CACHE = 'kanso-shell-0.1.0';
const FILES = ['/', '/app.js', '/style.css', '/icon.svg', '/manifest.webmanifest', '/licenses.html'];
self.addEventListener('install', event => event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(FILES))));
self.addEventListener('activate', event => event.waitUntil(caches.keys().then(keys => Promise.all(keys.filter(k => k.startsWith('kanso-shell-') && k !== CACHE).map(k => caches.delete(k))))));
self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || url.origin !== self.location.origin || !FILES.includes(url.pathname) || url.search) return;
  event.respondWith(fetch(event.request).catch(() => caches.match(url.pathname)));
});
