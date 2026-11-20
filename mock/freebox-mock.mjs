// Freebox simulée (HTTP) pour tester l'appairage et la collecte sans box réelle.
// Volontairement au format "ancien" de l'API (signal en atténuation positive, l2ident en tableau,
// pas de champ access_point) pour vérifier que l'application tolère les deux formats.
// Usage : node mock/freebox-mock.mjs [port]   puis réglez l'adresse de la box sur http://127.0.0.1:port
import http from 'node:http';
import crypto from 'node:crypto';

const PORT = +process.argv[2] || 8090;
const AUTO_GRANT_AFTER = +(process.env.GRANT_AFTER || 2); // nombre de consultations avant "granted"
const SESSION_CALLS = +(process.env.SESSION_CALLS || 25); // la session expire après N appels → teste le renouvellement
const tokens = new Map(); // app_token -> { track, polls, status }
const sessions = new Map(); // session -> calls
let challenge = crypto.randomBytes(16).toString('hex');
let track = 0;
const stats = { logins: 0, auth_required: 0 };

const devices = [
  { mac: '00:24:d4:aa:00:01', name: 'Portable', type: 'laptop', ap: 1, signal: 54, tx: 0, rx: 0, rate: 30e6 / 8 },
  { mac: '00:24:d4:aa:00:02', name: 'Téléphone', type: 'smartphone', ap: 0, signal: 77, tx: 0, rx: 0, rate: 2e6 / 8 },
  { mac: '00:24:d4:aa:00:03', name: 'Box TV', type: 'freebox_player', ap: null, port: 1, speed: '1000' },
];
let wanBytes = 0;
const reply = (res, code, body) => { res.writeHead(code, { 'Content-Type': 'application/json; charset=utf-8' }); res.end(JSON.stringify(body)); };
const ok = (res, result) => reply(res, 200, { success: true, result });
const err = (res, code, error_code, msg) => reply(res, code, { success: false, error_code, msg });

function authed(req, res) {
  const s = req.headers['x-fbx-app-auth'];
  if (!s || !sessions.has(s)) { stats.auth_required++; err(res, 403, 'auth_required', 'Invalid session token, or not session token sent'); return false; }
  const n = sessions.get(s) + 1;
  if (n > SESSION_CALLS) { sessions.delete(s); stats.auth_required++; err(res, 403, 'auth_required', 'session expired'); return false; }
  sessions.set(s, n); return true;
}

http.createServer((req, res) => {
  let body = '';
  req.on('data', (c) => (body += c));
  req.on('end', () => {
    const url = new URL(req.url, 'http://x');
    const p = url.pathname;
    if (p === '/__stats') return ok(res, stats);
    if (p === '/api_version') return reply(res, 200, { uid: 'mock0001', device_name: 'Freebox Server', api_version: '15.0', api_base_url: '/api/', device_type: 'FreeboxServer7,1', box_model_name: 'Freebox Delta (simulée)' });
    const m = p.match(/^\/api\/v(\d+)\/(.*)$/);
    if (!m) return err(res, 404, 'invalid_request', 'not found');
    const path = m[2].replace(/\/$/, '');
    const json = body ? JSON.parse(body) : {};
    // --- authentification ---
    if (path === 'login/authorize' && req.method === 'POST') {
      if (!json.app_id) return err(res, 400, 'invalid_request', 'app_id manquant');
      const t = crypto.randomBytes(24).toString('base64');
      tokens.set(t, { track: ++track, polls: 0, status: 'pending' });
      return ok(res, { app_token: t, track_id: track });
    }
    const tm = path.match(/^login\/authorize\/(\d+)$/);
    if (tm) {
      const e = [...tokens.values()].find((x) => x.track === +tm[1]);
      if (!e) return ok(res, { status: 'unknown', challenge });
      e.polls++; if (e.polls > AUTO_GRANT_AFTER) e.status = 'granted';
      return ok(res, { status: e.status, challenge });
    }
    if (path === 'login') { challenge = crypto.randomBytes(16).toString('hex'); return ok(res, { logged_in: false, challenge }); }
    if (path === 'login/session' && req.method === 'POST') {
      for (const [t, e] of tokens) {
        if (e.status !== 'granted') continue;
        const pw = crypto.createHmac('sha1', t).update(challenge).digest('hex');
        if (pw === json.password) {
          const s = crypto.randomBytes(24).toString('base64'); sessions.set(s, 0); stats.logins++;
          return ok(res, { session_token: s, challenge, permissions: { settings: false, contacts: false, calls: false, explorer: false, downloader: true } });
        }
      }
      return err(res, 403, 'invalid_token', 'mauvais mot de passe');
    }
    if (path === 'login/logout') return ok(res, null);
    if (!authed(req, res)) return;
    // --- données ---
    const t = Date.now() / 1000;
    for (const d of devices) if (d.rate) { d.tx += d.rate * 5 * (0.8 + Math.random() * 0.4); d.rx += d.rate * 0.3; }
    const down = devices.reduce((s, d) => s + (d.rate || 1.5e6), 0);
    wanBytes += down * 5;
    switch (path) {
      case 'connection': return ok(res, { type: 'ethernet', media: 'ftth', state: 'up', rate_down: Math.round(down), rate_up: 120000, bandwidth_down: 8000000000, bandwidth_up: 700000000, bytes_down: Math.round(wanBytes), bytes_up: 1e9, ipv4: '88.1.2.3' });
      case 'connection/ftth': return ok(res, { sfp_present: true, link: true, sfp_has_signal: true, sfp_pwr_rx: -2810, sfp_pwr_tx: 180 });
      case 'system': return ok(res, { firmware_version: '4.8.1', uptime_val: 86400 * 3, temp_cpub: 71, temp_sw: 55, model_info: { pretty_name: 'Freebox Delta (simulée)' } });
      case 'lan/browser/pub': return ok(res, devices.map((d, i) => ({ id: 'ether-' + d.mac, primary_name: d.name, host_type: d.type, vendor_name: 'Freebox SAS', active: true, reachable: true, last_activity: Math.round(t), l2ident: [{ id: d.mac.toUpperCase(), type: 'mac_address' }], l3connectivities: [{ addr: `192.168.0.${10 + i}`, af: 'ipv4', active: true }] })));
      case 'wifi/ap': return ok(res, [
        { id: 0, name: '2.4G', config: { band: '2d4g', channel_width: '20', primary_channel: 11 }, status: { state: 'active', channel_width: '20', primary_channel: 11 } },
        { id: 1, name: '5G', config: { band: '5g', channel_width: '80', primary_channel: 0 }, status: { state: 'active', channel_width: '80', primary_channel: 52 } },
      ]);
      case 'rrd': return err(res, 403, 'insufficient_rights', 'Cette application n’est pas autorisée à accéder à cette fonction');
      case 'switch/status': return ok(res, [{ id: 1, link: 'up', speed: '1000', duplex: 'full', mac_list: [{ mac: devices[2].mac.toUpperCase(), hostname: 'Box TV' }] }, { id: 2, link: 'down', mac_list: [] }]);
      case 'switch/port/1/stats': return ok(res, { tx_bytes_rate: 1.8e6, rx_bytes_rate: 4e4 });
    }
    let w;
    if ((w = path.match(/^wifi\/ap\/(\d)\/stations$/))) return ok(res, devices.filter((d) => d.ap === +w[1]).map((d) => ({ mac: d.mac.toUpperCase(), hostname: d.name, state: 'associated', signal: d.signal, tx_bytes: Math.round(d.tx), rx_bytes: Math.round(d.rx), last_tx: { bitrate: 4330 }, last_rx: { bitrate: 3900 }, flags: { ht: true, vht: true } })));
    if ((w = path.match(/^wifi\/ap\/(\d)\/neighbors$/))) return ok(res, +w[1] === 0 ? [{ ssid: 'Voisin', bssid: 'aa:aa:aa:aa:aa:01', band: '2d4g', channel: 11, channel_width: 20, signal: 60 }] : []);
    if ((w = path.match(/^wifi\/ap\/(\d)\/channel_usage$/))) return ok(res, [{ channel: +w[1] === 0 ? 11 : 52, band: +w[1] === 0 ? '2d4g' : '5g', rx_busy_percent: 33, noise_level: -91 }]);
    if ((w = path.match(/^wifi\/ap\/(\d)\/neighbors\/scan$/))) return ok(res, null);
    err(res, 404, 'invalid_request', 'chemin inconnu ' + path);
  });
}).listen(PORT, '127.0.0.1', () => console.log(`Freebox simulée sur http://127.0.0.1:${PORT}`));
