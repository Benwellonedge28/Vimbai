// FinAcc Service Worker - Offline First PWA Support

const CACHE_NAME = 'finacc-v1.0.0';
const STATIC_CACHE = 'finacc-static-v1';
const DYNAMIC_CACHE = 'finacc-dynamic-v1';
const API_CACHE = 'finacc-api-v1';

// Static assets to cache immediately
const STATIC_ASSETS = [
  '/',
  '/index.html',
  '/manifest.json',
  '/css/main.css',
  '/js/app.js',
  '/icons/icon-192x192.png',
  '/icons/icon-512x512.png'
];

// API routes that should be cached
const API_ROUTES = [
  '/accounts',
  '/journal-entries',
  '/financial-statements',
  '/ledgers',
  '/budgets'
];

// Install event - cache static assets
self.addEventListener('install', (event) => {
  console.log('[ServiceWorker] Installing...');

  event.waitUntil(
    caches.open(STATIC_CACHE)
      .then((cache) => {
        console.log('[ServiceWorker] Caching static assets');
        return cache.addAll(STATIC_ASSETS);
      })
      .then(() => self.skipWaiting())
  );
});

// Activate event - clean up old caches
self.addEventListener('activate', (event) => {
  console.log('[ServiceWorker] Activating...');

  event.waitUntil(
    caches.keys()
      .then((keys) => {
        return Promise.all(
          keys
            .filter((key) => key !== STATIC_CACHE && key !== DYNAMIC_CACHE && key !== API_CACHE)
            .map((key) => {
              console.log('[ServiceWorker] Removing old cache:', key);
              return caches.delete(key);
            })
        );
      })
      .then(() => self.clients.claim())
  );
});

// Fetch event - implement caching strategies
self.addEventListener('fetch', (event) => {
  const { request } = event;
  const url = new URL(request.url);

  // Skip non-GET requests
  if (request.method !== 'GET') {
    return;
  }

  // API requests - Network First with Cache Fallback
  if (url.pathname.startsWith('/api/') || API_ROUTES.some(route => url.pathname.startsWith(route))) {
    event.respondWith(networkFirstWithCache(request, API_CACHE));
    return;
  }

  // Static assets - Cache First with Network Fallback
  if (isStaticAsset(url.pathname)) {
    event.respondWith(cacheFirstWithNetwork(request, STATIC_CACHE));
    return;
  }

  // Dynamic content - Stale While Revalidate
  event.respondWith(staleWhileRevalidate(request, DYNAMIC_CACHE));
});

// Network First with Cache Fallback (for API)
async function networkFirstWithCache(request, cacheName) {
  const cache = await caches.open(cacheName);

  try {
    const networkResponse = await fetch(request);

    // Only cache successful responses
    if (networkResponse.ok) {
      cache.put(request, networkResponse.clone());
    }

    return networkResponse;
  } catch (error) {
    console.log('[ServiceWorker] Network failed, trying cache:', error);

    const cachedResponse = await cache.match(request);
    if (cachedResponse) {
      return cachedResponse;
    }

    // Return offline fallback for API requests
    return new Response(
      JSON.stringify({
        error: 'offline',
        message: 'You are currently offline. Please try again when connected.'
      }),
      {
        status: 503,
        headers: { 'Content-Type': 'application/json' }
      }
    );
  }
}

// Cache First with Network Fallback (for static assets)
async function cacheFirstWithNetwork(request, cacheName) {
  const cache = await caches.open(cacheName);

  const cachedResponse = await cache.match(request);
  if (cachedResponse) {
    // Return cached version and update in background
    updateCache(request, cacheName);
    return cachedResponse;
  }

  try {
    const networkResponse = await fetch(request);
    if (networkResponse.ok) {
      cache.put(request, networkResponse.clone());
    }
    return networkResponse;
  } catch (error) {
    // Return offline page for navigation requests
    if (request.mode === 'navigate') {
      return cache.match('/index.html');
    }
    throw error;
  }
}

// Stale While Revalidate (for dynamic content)
async function staleWhileRevalidate(request, cacheName) {
  const cache = await caches.open(cacheName);

  const cachedResponse = await cache.match(request);

  // Fetch and cache in background
  const fetchPromise = fetch(request)
    .then((networkResponse) => {
      if (networkResponse.ok) {
        cache.put(request, networkResponse.clone());
      }
      return networkResponse;
    })
    .catch(() => null);

  // Return cached response immediately, or wait for network
  return cachedResponse || fetchPromise;
}

// Background cache update
async function updateCache(request, cacheName) {
  try {
    const cache = await caches.open(cacheName);
    const networkResponse = await fetch(request);
    if (networkResponse.ok) {
      cache.put(request, networkResponse);
    }
  } catch (error) {
    console.log('[ServiceWorker] Background cache update failed:', error);
  }
}

// Check if URL is a static asset
function isStaticAsset(pathname) {
  const staticExtensions = ['.html', '.css', '.js', '.json', '.png', '.jpg', '.jpeg', '.gif', '.svg', '.ico', '.woff', '.woff2'];
  return staticExtensions.some(ext => pathname.endsWith(ext));
}

// Handle push notifications
self.addEventListener('push', (event) => {
  if (!event.data) {
    console.log('[ServiceWorker] No push data');
    return;
  }

  const data = event.data.json();
  const options = {
    body: data.body || 'New notification from FinAcc',
    icon: '/icons/icon-192x192.png',
    badge: '/icons/badge-72x72.png',
    vibrate: [100, 50, 100],
    data: {
      url: data.url || '/',
      id: data.id || Date.now()
    },
    actions: [
      { action: 'view', title: 'View' },
      { action: 'dismiss', title: 'Dismiss' }
    ]
  };

  event.waitUntil(
    self.registration.showNotification(data.title || 'FinAcc', options)
  );
});

// Handle notification click
self.addEventListener('notificationclick', (event) => {
  event.notification.close();

  if (event.action === 'dismiss') {
    return;
  }

  const url = event.notification.data?.url || '/';

  event.waitUntil(
    clients.matchAll({ type: 'window', includeUncontrolled: true })
      .then((clientList) => {
        // Focus existing window if available
        for (const client of clientList) {
          if (client.url === url && 'focus' in client) {
            return client.focus();
          }
        }
        // Open new window
        if (clients.openWindow) {
          return clients.openWindow(url);
        }
      })
  );
});

// Background sync for offline actions
self.addEventListener('sync', (event) => {
  if (event.tag === 'sync-transactions') {
    event.waitUntil(syncTransactions());
  } else if (event.tag === 'sync-journal-entries') {
    event.waitUntil(syncJournalEntries());
  }
});

// Sync queued transactions
async function syncTransactions() {
  console.log('[ServiceWorker] Syncing transactions...');

  // Get queued transactions from IndexedDB
  const db = await openDB();
  const txns = await db.getAll('pendingTransactions');

  for (const txn of txns) {
    try {
      const response = await fetch('/api/transactions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(txn)
      });

      if (response.ok) {
        await db.delete('pendingTransactions', txn.id);
        console.log('[ServiceWorker] Synced transaction:', txn.id);
      }
    } catch (error) {
      console.error('[ServiceWorker] Failed to sync transaction:', error);
    }
  }
}

// Sync queued journal entries
async function syncJournalEntries() {
  console.log('[ServiceWorker] Syncing journal entries...');

  const db = await openDB();
  const entries = await db.getAll('pendingJournalEntries');

  for (const entry of entries) {
    try {
      const response = await fetch('/api/journal-entries', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(entry)
      });

      if (response.ok) {
        await db.delete('pendingJournalEntries', entry.id);
        console.log('[ServiceWorker] Synced journal entry:', entry.id);
      }
    } catch (error) {
      console.error('[ServiceWorker] Failed to sync journal entry:', error);
    }
  }
}

// IndexedDB helper
function openDB() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('finacc-offline', 1);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);

    request.onupgradeneeded = (event) => {
      const db = event.target.result;

      // Create stores for offline data
      if (!db.objectStoreNames.contains('pendingTransactions')) {
        db.createObjectStore('pendingTransactions', { keyPath: 'id', autoIncrement: true });
      }

      if (!db.objectStoreNames.contains('pendingJournalEntries')) {
        db.createObjectStore('pendingJournalEntries', { keyPath: 'id', autoIncrement: true });
      }

      if (!db.objectStoreNames.contains('cachedData')) {
        db.createObjectStore('cachedData', { keyPath: 'key' });
      }
    };
  });
}

// Message handling for cache management
self.addEventListener('message', (event) => {
  if (event.data === 'skipWaiting') {
    self.skipWaiting();
  }

  if (event.data === 'clearCache') {
    event.waitUntil(
      caches.keys().then((keys) => {
        return Promise.all(keys.map((key) => caches.delete(key)));
      })
    );
  }
});