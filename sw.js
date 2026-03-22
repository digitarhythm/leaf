// sw.js - Precision Caching & Fallback
const CACHE_NAME = 'leaf-cache-v10';

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
  // --- External Libraries ---
  'https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/tokyo-night-dark.min.css',
  'https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ace.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-gruvbox.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-monokai.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-dracula.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-nord_dark.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-solarized_dark.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-one_dark.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-twilight.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-tomorrow_night.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-chrome.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-clouds.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-crimson_editor.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-dawn.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-dreamweaver.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-eclipse.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-github.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/theme-solarized_light.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/mode-text.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/mode-javascript.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/mode-markdown.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ext-language_tools.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ext-modelist.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/ext-searchbox.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/keybinding-vim.js',
  'https://cdnjs.cloudflare.com/ajax/libs/ace/1.32.3/worker-javascript.js',
  'https://cdn.jsdelivr.net/npm/marked/marked.min.js',
  'https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js'
];

const ALLOWED_DOMAINS = ['cdnjs.cloudflare.com', 'cdn.jsdelivr.net', 'fonts.googleapis.com', 'fonts.gstatic.com'];

self.addEventListener('install', (event) => {
  self.skipWaiting();
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      return Promise.all(
        PRECACHE_ASSETS.map(url => {
          return fetch(url, { mode: url.startsWith('http') ? 'cors' : 'same-origin' })
            .then(res => cache.put(url, res))
            .catch(e => console.warn("Precache failed:", url, e));
        })
      );
    })
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(caches.keys().then(keys => Promise.all(keys.filter(k => k !== CACHE_NAME).map(k => caches.delete(k)))));
  return self.clients.claim();
});

self.addEventListener('fetch', (event) => {
  if (event.request.method !== 'GET') return;
  const url = new URL(event.request.url);
  if (!ALLOWED_DOMAINS.some(d => url.hostname === d) && url.origin !== self.location.origin) return;

  // ナビゲーションリクエスト: ネットワーク優先（常に最新HTMLを取得）
  if (event.request.mode === 'navigate') {
    event.respondWith(
      fetch(event.request).then((response) => {
        if (response && response.status === 200) {
          const clone = response.clone();
          caches.open(CACHE_NAME).then(c => c.put(event.request, clone));
        }
        return response;
      }).catch(() => caches.match('index.html'))
    );
    return;
  }

  // その他のリクエスト: キャッシュ優先 + MIMEタイプ検証
  event.respondWith(
    caches.match(event.request, { ignoreSearch: true }).then((cached) => {
      if (cached) return cached;
      return fetch(event.request).then((network) => {
        if (network && (network.status === 200 || network.status === 0)) {
          // JS/WASMリクエストにHTMLが返された場合は拒否（デプロイ直後のSPAフォールバック防止）
          const dest = event.request.destination;
          const ct = network.headers.get('content-type') || '';
          if ((dest === 'script' || dest === 'worker') && ct.includes('text/html')) {
            return new Response('Asset not found', { status: 404 });
          }
          const clone = network.clone();
          caches.open(CACHE_NAME).then(c => c.put(event.request, clone));
        }
        return network;
      }).catch(() => {
        return new Response('Offline', { status: 503 });
      });
    })
  );
});
