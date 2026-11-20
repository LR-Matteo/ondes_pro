// Moteur de mesure : latence au repos, débit descendant, latence en charge, débit montant.
// Serveur par défaut : endpoints publics de Cloudflare (speed.cloudflare.com), CORS ouverts.
// Toutes les requêtes sont des "requêtes simples" (pas d'en-tête custom) → aucun preflight CORS.

const now = () => performance.now();
const rid = () => Math.random().toString(36).slice(2);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const median = (a) => {
  if (!a.length) return null;
  const s = [...a].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const jitterOf = (a) => {
  if (a.length < 2) return 0;
  let t = 0;
  for (let i = 1; i < a.length; i++) t += Math.abs(a[i] - a[i - 1]);
  return t / (a.length - 1);
};
const mbps = (bytes, ms) => (ms > 0 ? (bytes * 8) / (ms * 1000) : 0);

export class SpeedTest {
  constructor({ base = 'https://speed.cloudflare.com', duration = 8, streams = 4, onUpdate = () => {} } = {}) {
    this.base = base.replace(/\/+$/, '');
    this.duration = duration;
    this.streams = streams;
    this.onUpdate = onUpdate;
    this.ctrl = new AbortController();
  }

  abort() { this.ctrl.abort(); }

  async onePing() {
    const url = `${this.base}/__down?bytes=0&measId=${rid()}`;
    const t0 = now();
    const r = await fetch(url, { cache: 'no-store', signal: this.ctrl.signal });
    await r.arrayBuffer();
    const wall = now() - t0;
    // Mesure plus fine si le serveur expose Timing-Allow-Origin
    const e = performance.getEntriesByName(url).pop();
    if (e && e.requestStart > 0 && e.responseStart > e.requestStart) return e.responseStart - e.requestStart;
    return wall;
  }

  async latency(n = 20) {
    try { performance.clearResourceTimings(); } catch {}
    const samples = [];
    for (let i = 0; i < n; i++) {
      const ms = await this.onePing();
      if (i >= 2) samples.push(ms); // les 2 premières incluent l'ouverture TLS
      this.onUpdate({ phase: 'latency', progress: (i + 1) / n, value: median(samples) ?? ms });
    }
    return { ping: median(samples), jitter: jitterOf(samples), samples };
  }

  async download() {
    try { performance.clearResourceTimings(); } catch {}
    const t0 = now();
    const end = t0 + this.duration * 1000;
    let bytes = 0;
    const series = [];
    const loaded = [];
    let stop = false;
    let lastB = 0, lastT = t0;

    const worker = async (k) => {
      let size = 1e6;
      while (!stop && now() < end) {
        const r = await fetch(`${this.base}/__down?bytes=${size}&measId=${rid()}`, { cache: 'no-store', signal: this.ctrl.signal });
        if (!r.ok) throw new Error('HTTP ' + r.status);
        const reader = r.body.getReader();
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          bytes += value.length;
          if (now() >= end) { stop = true; reader.cancel().catch(() => {}); break; }
        }
        size = Math.min(size * 4, 100e6);
      }
    };
    // Sonde de latence pendant le chargement (bufferbloat)
    const prober = async () => {
      await sleep(1200);
      while (!stop && now() < end - 300) {
        try { loaded.push(await this.onePing()); } catch { break; }
        await sleep(250);
      }
    };
    const timer = setInterval(() => {
      const t = now();
      const inst = mbps(bytes - lastB, t - lastT);
      lastB = bytes; lastT = t;
      series.push({ t: (t - t0) / 1000, v: inst });
      this.onUpdate({ phase: 'download', progress: Math.min(1, (t - t0) / (end - t0)), value: inst, series });
    }, 250);

    try {
      await Promise.all([...Array.from({ length: this.streams }, (_, k) => worker(k)), prober()]);
    } finally {
      clearInterval(timer);
      stop = true;
    }
    return { down: steady(series), series, loadedPing: median(loaded), loadedSamples: loaded };
  }

  async upload() {
    const t0 = now();
    const end = t0 + this.duration * 1000;
    let bytes = 0;
    const series = [];
    let lastB = 0, lastT = t0;
    const pool = makeRandom(4 << 20);

    const worker = async () => {
      let size = 256 << 10;
      while (now() < end) {
        const body = size <= pool.length
          ? new Blob([pool.subarray(0, size)])
          : new Blob(Array(Math.ceil(size / pool.length)).fill(pool)).slice(0, size);
        const s = now();
        const r = await fetch(`${this.base}/__up?measId=${rid()}`, {
          method: 'POST', body, cache: 'no-store', signal: this.ctrl.signal,
          headers: { 'Content-Type': 'text/plain' },
        });
        await r.arrayBuffer().catch(() => {});
        if (!r.ok) throw new Error('HTTP ' + r.status);
        const dt = now() - s;
        bytes += size;
        // On vise ~700 ms par requête pour une mesure stable
        size = Math.max(64 << 10, Math.min(64 << 20, Math.round(size * Math.min(4, Math.max(0.5, 700 / Math.max(dt, 1))))));
      }
    };
    const timer = setInterval(() => {
      const t = now();
      const inst = mbps(bytes - lastB, t - lastT);
      lastB = bytes; lastT = t;
      series.push({ t: (t - t0) / 1000, v: inst });
      this.onUpdate({ phase: 'upload', progress: Math.min(1, (t - t0) / (end - t0)), value: avgRate(series), series });
    }, 500);
    try {
      await Promise.all(Array.from({ length: this.streams }, worker));
    } finally { clearInterval(timer); }
    const total = now() - t0;
    return { up: mbps(bytes, total), series };
  }

  async run() {
    const lat = await this.latency();
    const dl = await this.download();
    const ul = await this.upload();
    this.onUpdate({ phase: 'done', progress: 1 });
    return {
      down: round(dl.down), up: round(ul.up),
      ping: round(lat.ping), jitter: round(lat.jitter),
      loadedPing: dl.loadedPing != null ? round(dl.loadedPing) : null,
      downSeries: dl.series.map((p) => [round(p.t), round(p.v)]),
    };
  }
}

function makeRandom(n) {
  const a = new Uint8Array(n);
  for (let i = 0; i < n; i += 65536) crypto.getRandomValues(a.subarray(i, Math.min(n, i + 65536)));
  return a;
}
// Débit stable : on écarte la montée en charge (premier quart) et les 10 % de pics
function steady(series) {
  const v = series.slice(Math.floor(series.length / 4)).map((p) => p.v).filter((x) => x > 0).sort((a, b) => a - b);
  if (!v.length) return 0;
  const top = v.slice(0, Math.max(1, Math.ceil(v.length * 0.9)));
  return top.reduce((a, b) => a + b, 0) / top.length;
}
function avgRate(series) {
  const v = series.map((p) => p.v).filter((x) => x > 0);
  return v.length ? v.reduce((a, b) => a + b, 0) / v.length : 0;
}
const round = (x) => (x == null ? null : Math.round(x * 10) / 10);

export { median, jitterOf, steady };
