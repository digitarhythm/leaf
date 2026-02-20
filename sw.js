// sw.js - Dynamic Caching Strategy
const CACHE_NAME = 'leaf-cache-v6';

// 起動時に最低限必要なローカルアセット
const PRECACHE_ASSETS = [
  './',
  'index.html',
  'manifest.json',
  'icon.svg',
  'icon-192.png',
  'icon-512.png',
  'editor_interop.js',
  'auth.js',
  'db.js',
  'drive.js',
  'privacy.html',
  'terms.html',
  'licenses.html'
];

// キャッシュを許可するドメイン
const ALLOWED_DOMAINS = [
  'cdnjs.cloudflare.com',
  'cdn.jsdelivr.net',
  'fonts.googleapis.com',
  'fonts.gstatic.com'
];

self.addEventListener('install', (event) => {
  self.skipWaiting();
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      return cache.addAll(PRECACHE_ASSETS);
    })
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) => {
      return Promise.all(
        keys.filter((key) => key !== CACHE_NAME)
            .map((key) => caches.delete(key))
      );
    })
  );
  return self.clients.claim();
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  const isAllowedCdn = ALLOWED_DOMAINS.some(domain => url.hostname === domain);
  const isSameOrigin = url.origin === self.location.origin;

  // 許可されたドメイン以外、または non-GET リクエストはスルー
  if ((!isSameOrigin && !isAllowedCdn) || event.request.method !== 'GET') return;

  event.respondWith(
    caches.match(event.request, { ignoreSearch: true }).then((cachedResponse) => {
      if (cachedResponse) return cachedResponse;

      // キャッシュにない場合はネットワークから取得し、成功したら保存する
      return fetch(event.request).then((networkResponse) => {
        // 正常なレスポンスのみ保存
        if (!networkResponse || networkResponse.status !== 200) {
          return networkResponse;
        }

        const responseToCache = networkResponse.clone();
        caches.open(CACHE_NAME).then((cache) => {
          cache.put(event.request, responseToCache);
        });

        return networkResponse;
      }).catch(() => {
        // オフライン時のフォールバック
        if (event.request.mode === 'navigate') {
          return caches.match('index.html');
        }
        return new Response('Offline', { status: 503, statusText: 'Offline' });
      });
    })
  );
});
