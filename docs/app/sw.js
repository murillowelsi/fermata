// Fermata in the browser: keeps the app's own files so it opens offline. Only this folder: the download site
// above it isn't touched, and other sites (GitHub's API, release downloads) always go to the network.
// make_web.sh fills in the version and the file list; a new version gets a new cache and the old one is removed.
const VERSION = "0.3.23-45340db4";
const CACHE = "fermata-app-" + VERSION;
const FILES = ["./","./index.html","./fermata.js?v=0.3.23-45340db4","./fermata_bg.wasm?v=0.3.23-45340db4","./snippets/fermata-7a2449136dc7ca02/inline0.js?v=0.3.23-45340db4","./icons/apple-touch-icon.png","./icons/favicon-64.png","./icons/icon-192.png","./icons/icon-512.png","./icons/icon-maskable-512.png","./manifest.webmanifest"];

self.addEventListener("install", (event) => {
  // straight from the server, not the browser's HTTP cache, so a new version never keeps an old file
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(FILES.map((f) => new Request(f, { cache: "reload" })))));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k.startsWith("fermata-app-") && k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

// the page asks for the new version when the person clicks Reload
self.addEventListener("message", (event) => {
  if (event.data === "skipWaiting") self.skipWaiting();
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin || !url.href.startsWith(self.registration.scope)) return;
  if (req.mode === "navigate") {
    // the app page itself (also when opened with a file from the installed app)
    event.respondWith(caches.match("./index.html", { cacheName: CACHE }).then((r) => r || fetch(req)));
    return;
  }
  event.respondWith(caches.match(req, { cacheName: CACHE }).then((r) => r || fetch(req)));
});
