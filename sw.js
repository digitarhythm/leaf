// sw.js - Precision Caching & Fallback
const CACHE_NAME = 'leaf-cache-v9';

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

  event.respondWith(
    caches.match(event.request, { ignoreSearch: true }).then((cached) => {
      if (cached) return cached;
      return fetch(event.request).then((network) => {
        if (network && (network.status === 200 || network.status === 0)) {
          const clone = network.clone();
          caches.open(CACHE_NAME).then(c => c.put(event.request, clone));
        }
        return network;
      }).catch(() => {
        // 重要: 画像やCSS/JSに対して index.html を返さない（MIMEタイプエラーの原因）
        if (event.request.mode === 'navigate') return caches.match('index.html');
        return new Response('Offline', { status: 503 });
      });
    })
  );
});
