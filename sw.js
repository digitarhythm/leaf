// sw.js - Maximum Offline Resilience
const CACHE_NAME = 'leaf-cache-v8';

// 起動時に確実に保存するアセット（Ace Editorの動的ロード分も含む）
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
  'licenses.html',
  // --- External Libraries (CDNs) ---
  'https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/gruvbox-dark.min.css',
  'https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js',
  // Ace Editor Core
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ace.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-gruvbox.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/mode-text.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/mode-javascript.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/mode-markdown.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ext-language_tools.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ext-modelist.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ext-searchbox.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/keybinding-vim.js',
  // Ace Workers (Dynamic load files)
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/worker-javascript.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/worker-json.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/worker-html.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/worker-css.js',
  // Marked & Mermaid
  'https://cdn.jsdelivr.net/npm/marked/marked.min.js',
  'https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js',
  // Fonts
  'https://fonts.googleapis.com/css2?family=Petit+Formal+Script&display=swap'
];

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
      return Promise.all(
        PRECACHE_ASSETS.map(url => {
          // プリキャッシュ時は確実に CORS モードで取得を試みる
          return fetch(url, { mode: 'cors' }).then(response => {
            if (response.ok) return cache.put(url, response);
            // CORSがダメな場合は不透明レスポンスとして再試行
            return fetch(url, { mode: 'no-cors' }).then(opaqueRes => cache.put(url, opaqueRes));
          }).catch(err => console.error("Precaching failed for:", url, err));
        })
      );
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
  if (event.request.method !== 'GET') return;

  const url = new URL(event.request.url);
  const isAllowedCdn = ALLOWED_DOMAINS.some(domain => url.hostname === domain);
  const isSameOrigin = url.origin === self.location.origin;

  if (!isSameOrigin && !isAllowedCdn) return;

  event.respondWith(
    caches.match(event.request, { ignoreSearch: true }).then((cachedResponse) => {
      // キャッシュがあれば即座に返す (Cache First)
      if (cachedResponse) return cachedResponse;

      return fetch(event.request).then((networkResponse) => {
        // 正常な応答、または許可されたドメインからの不透明な応答(status 0)をキャッシュする
        const isSafeToCache = networkResponse.status === 200 || (isAllowedCdn && networkResponse.status === 0);
        
        if (!networkResponse || !isSafeToCache) {
          return networkResponse;
        }

        const responseToCache = networkResponse.clone();
        caches.open(CACHE_NAME).then((cache) => {
          cache.put(event.request, responseToCache);
        });

        return networkResponse;
      }).catch(() => {
        if (event.request.mode === 'navigate') {
          return caches.match('index.html');
        }
        return new Response('Offline Content Unavailable', { status: 503 });
      });
    })
  );
});
