import { call, explain, inApp } from './api.js';
import { LineChart, drawSpectrum, fmtRate } from './charts.js';
import { diagnose, report, interference, level, bloat } from './diag.js';
import { SpeedTest } from './speedtest.js';

{
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const fmtBytes = (b) => (b == null ? '—' : b >= 1e12 ? (b / 1e12).toFixed(2).replace('.', ',') + ' To' : b >= 1e9 ? (b / 1e9).toFixed(b >= 1e10 ? 0 : 1).replace('.', ',') + ' Go' : b >= 1e6 ? Math.round(b / 1e6) + ' Mo' : b >= 1e3 ? Math.round(b / 1e3) + ' ko' : Math.round(b) + ' o');
const ago = (ts) => { const d = Date.now() / 1000 - ts; return d < 90 ? 'à l’instant' : d < 3600 ? `il y a ${Math.round(d / 60)} min` : d < 86400 ? `il y a ${Math.round(d / 3600)} h` : `il y a ${Math.round(d / 86400)} j`; };
const dur = (s) => (s == null ? '—' : s < 3600 ? Math.round(s / 60) + ' min' : s < 86400 ? Math.round(s / 3600) + ' h' : Math.round(s / 86400) + ' j');
const band = (b) => (b ? b.replace('.', ',') + ' GHz' : '');
const neg = (x) => String(x).replace('-', '−');
const dbm = (x) => (x == null ? '—' : neg(Math.round(x)) + ' dBm');

const S = { status: null, live: null, wifi: null, sat: null, top7: null, events: [], tests: [], D: null, view: 'overview', sort: { k: 'down_mbps', asc: false }, filter: 'all', q: '', range: 86400, drawer: null, hl: {}, local: null };

// ---------- icônes par type d'appareil ----------
const IC = {
  smartphone: 'M8 2h8a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1zM11 18h2',
  tablet: 'M5 3h14a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM11 17h2',
  laptop: 'M5 5h14v10H5zM2 19h20',
  workstation: 'M3 4h18v12H3zM8 20h8M12 16v4',
  television: 'M3 5h18v12H3zM8 21h8',
  vg_console: 'M6 9h12a4 4 0 0 1 0 8l-2-2H8l-2 2a4 4 0 0 1 0-8zM8 12v2M7 13h2M15.5 12.5h.01M17 14h.01',
  nas: 'M5 3h14v6H5zM5 9h14v6H5zM5 15h14v6H5zM8 6h.01M8 12h.01M8 18h.01',
  ip_camera: 'M3 7h12v10H3zM15 10l6-3v10l-6-3',
  printer: 'M6 9V3h12v6M6 17H4v-6h16v6h-2M6 14h12v7H6z',
  freebox_player: 'M3 8h18v8H3zM7 12h.01M11 12h6',
  multimedia_device: 'M7 3h10v18H7zM12 14a2.5 2.5 0 1 0 0 .01M12 7h.01',
  networking_device: 'M4 14h16v5H4zM8 14V9M16 14V9M7.5 16.5h.01M10.5 16.5h.01',
  ip_phone: 'M5 4h4l2 5-3 2a11 11 0 0 0 5 5l2-3 5 2v4a2 2 0 0 1-2 2A17 17 0 0 1 3 6a2 2 0 0 1 2-2',
  other: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 8v5M12 16h.01',
};
const icon = (k) => `<svg viewBox="0 0 24 24"><path d="${IC[k] || IC[{ freebox_hd: 'freebox_player', freebox_mini: 'freebox_player', car: 'other' }[k]] || IC.other}"/></svg>`;
const KIND = { smartphone: 'Smartphone', tablet: 'Tablette', laptop: 'Portable', workstation: 'Ordinateur', television: 'Téléviseur', vg_console: 'Console', nas: 'NAS', ip_camera: 'Caméra', printer: 'Imprimante', freebox_player: 'Freebox Player', freebox_hd: 'Freebox HD', multimedia_device: 'Multimédia', networking_device: 'Équipement réseau', ip_phone: 'Téléphone IP', car: 'Voiture', other: 'Autre' };
const sigLevel = (s) => (s == null ? 0 : s >= -60 ? 4 : s >= -68 ? 3 : s >= -76 ? 2 : 1);
const sigHtml = (s) => (s == null ? '<span class="muted">—</span>' : `<span class="sig l${sigLevel(s)}"><i></i><i></i><i></i><i></i></span>${neg(Math.round(s))}`);
const connBadge = (h) => {
  if (h.conn === 'ethernet') return `<span class="badge eth${h.eth_speed && h.eth_speed <= 100 ? ' bad' : ''}">Ethernet${h.eth_speed ? ' ' + (h.eth_speed >= 1000 ? h.eth_speed / 1000 + ' Gb/s' : h.eth_speed + ' Mb/s') : ''}</span>`;
  if (h.conn === 'wifi') return `<span class="badge${h.band === '2.4' ? ' b24' : ''}">Wi-Fi ${band(h.band)}</span>${h.standard ? ` <small class="muted">${esc(h.standard)}</small>` : ''}${h.via_repeater ? ' <small class="muted">via répéteur</small>' : ''}`;
  return '<span class="muted">—</span>';
};
// Insère des points là où il manque des relevés : zéro (appareil inactif) ou coupure (application fermée).
function gaps(rows, bucket, fill) {
  const out = [];
  for (let i = 0; i < rows.length; i++) {
    const r = rows[i];
    if (i && r[0] - rows[i - 1][0] > bucket * 1.5) {
      const z = rows[i - 1].map((_, k) => (k === 0 ? null : fill));
      out.push([rows[i - 1][0] + bucket, ...z.slice(1)], [r[0] - bucket, ...z.slice(1)]);
    }
    out.push(r);
  }
  return out;
}
const isNew = (h) => h.first_seen && Date.now() / 1000 - h.first_seen < 7 * 86400;

// ---------- utilitaires d'interface ----------
let toastT;
function toast(msg) { const t = $('#toast'); t.textContent = msg; t.hidden = false; clearTimeout(toastT); toastT = setTimeout(() => (t.hidden = true), 3200); }
const tweens = new WeakMap();
function tween(el, v, f) {
  if (!el) return;
  if (v == null || isNaN(v)) { el.textContent = '—'; el._v = null; return; }
  const from = el._v ?? v, t0 = performance.now();
  cancelAnimationFrame(tweens.get(el));
  const step = (t) => { const k = Math.min(1, (t - t0) / 450), e = 1 - (1 - k) ** 3; el.textContent = f(from + (v - from) * e); if (k < 1) tweens.set(el, requestAnimationFrame(step)); };
  el._v = v; tweens.set(el, requestAnimationFrame(step));
}
function ring(el, score, size = 112) {
  const C = 2 * Math.PI * 44;
  const col = score == null ? 'var(--rule)' : score >= 75 ? 'var(--teal)' : score >= 45 ? 'var(--warn)' : 'var(--crit)';
  if (!el.querySelector('svg')) el.innerHTML = `<svg viewBox="0 0 100 100"><circle cx="50" cy="50" r="44" fill="none" stroke="var(--rule)" stroke-width="8"/><circle class="v" cx="50" cy="50" r="44" fill="none" stroke-width="8" stroke-linecap="round" stroke-dasharray="${C}" stroke-dashoffset="${C}" transform="rotate(-90 50 50)"/></svg><div class="n"><span>—</span><small>sur 100</small></div>`;
  const v = el.querySelector('.v'); v.style.stroke = col; requestAnimationFrame(() => (v.style.strokeDashoffset = C * (1 - (score ?? 0) / 100)));
  el.querySelector('.n span').textContent = score ?? '—';
}
function armButton(btn, fn) {
  const label = btn.textContent; let t;
  btn.addEventListener('click', () => {
    if (!btn.classList.contains('armed')) { btn.classList.add('armed'); btn.textContent = 'Confirmer ?'; t = setTimeout(() => { btn.classList.remove('armed'); btn.textContent = label; }, 3500); return; }
    clearTimeout(t); btn.classList.remove('armed'); btn.textContent = label; fn();
  });
}
async function copyText(txt) { try { await navigator.clipboard.writeText(txt); return true; } catch { return false; } }

// ---------- navigation ----------
const VIEWS = ['overview', 'appareils', 'conso', 'wifi', 'mesure', 'diagnostic', 'reglages'];
function route() {
  const v = VIEWS.includes(location.hash.slice(1)) ? location.hash.slice(1) : 'overview';
  S.view = v;
  for (const s of $$('.view')) s.hidden = s.dataset.view !== v;
  for (const a of $$('[data-v]')) a.toggleAttribute('aria-current', a.dataset.v === v);
  $('#main').scrollTop = 0;
  render(true);
}
addEventListener('hashchange', route);

// ---------- données ----------
const ctx = () => ({ live: S.live, wifi: S.wifi, saturation: S.sat, top: S.top7, events: S.events, speedtests: S.tests, status: S.status });
function recompute() {
  S.D = diagnose(ctx());
  const n = $('#nav-diag'); const k = (S.D.crit || 0) + (S.D.warn || 0);
  n.textContent = k || ''; n.classList.toggle('w', !S.D.crit);
}
async function pollLive() {
  try {
    S.live = await call('live');
    const t = S.live.ts;
    for (const h of S.live.hosts || []) {
      if (h.down_mbps == null) continue;
      const a = (S.hl[h.mac] ||= []); a.push([t, h.down_mbps, h.up_mbps]); while (a.length && a[0][0] < t - 900) a.shift();
    }
  } catch (e) { S.live = S.live || null; }
  renderConn(); recompute(); render(false);
}
async function pollSlow() {
  const safe = (p) => p.catch(() => null);
  const [wifi, sat, top, ev, tests] = await Promise.all([safe(call('wifi')), safe(call('saturation', { range: 7 * 86400 })), safe(call('top', { range: 7 * 86400, limit: 20 })), safe(call('events', { range: 7 * 86400 })), safe(call('speedtests'))]);
  S.wifi = wifi; S.sat = sat; S.top7 = top; S.events = ev || []; S.tests = tests || [];
  recompute(); render(false);
}

function renderConn() {
  const c = $('#conn'), L = S.live, st = S.status;
  c.className = 'conn';
  if (st?.demo) { c.classList.add('demo'); c.querySelector('span').textContent = 'Mode démo · box simulée'; }
  else if (L?.connected) { c.classList.add('ok'); c.querySelector('span').textContent = `Connecté · relevé toutes les ${L.interval} s`; }
  else if (st?.configured) { c.classList.add('bad'); c.querySelector('span').textContent = L?.last_error ? explain(L.last_error).slice(0, 60) : 'Connexion à la box…'; }
  else c.querySelector('span').textContent = 'Non associé';
  for (const p of $$('.demo-pill')) p.hidden = !st?.demo;
  $('#box-name').textContent = L?.box?.model || st?.box_name || 'Diagnostic réseau';
  const n = (L?.hosts || []).filter((h) => h.active).length; $('#nav-dev').textContent = n || '';
}

function render(entering) {
  if (!S.live && S.view !== 'reglages') return;
  ({ overview: renderOverview, appareils: renderDevices, conso: entering ? renderConso : null, wifi: renderWifi, mesure: entering ? renderMesure : null, diagnostic: renderDiag, reglages: entering ? renderSettings : null }[S.view])?.(entering);
  if (S.drawer && !entering) renderDrawer(false);
}

// ---------- VUE D'ENSEMBLE ----------
const ovChart = new LineChart($('#ov-chart'), { empty: 'Les premières mesures arrivent…' });
function renderOverview() {
  const L = S.live, w = L.wan;
  $('#ov-sub').textContent = `${L.box?.model || 'Freebox'} · ${w.media ? w.media.toUpperCase() : 'ligne'} · ${L.connected ? 'mis à jour ' + new Date(L.ts * 1000).toLocaleTimeString('fr-FR') : 'hors ligne'}`;
  tween($('#ov-down'), w.down_mbps, fmtRate); tween($('#ov-up'), w.up_mbps, fmtRate);
  $('#ov-bw').innerHTML = `${fmtRate(w.bw_down_mbps)} <span class="muted" style="font-size:.6em">↓ · ${fmtRate(w.bw_up_mbps)} ↑</span>`;
  $('#ov-down-pct').textContent = w.bw_down_mbps ? `${Math.round((w.down_mbps / w.bw_down_mbps) * 100)} % de la capacité` : '';
  $('#ov-up-pct').textContent = w.bw_up_mbps ? `${Math.round((w.up_mbps / w.bw_up_mbps) * 100)} % de la capacité` : '';
  $('#ov-media').textContent = w.state === 'up' ? `${w.media === 'ftth' ? 'Fibre' : w.media || 'Ligne'} active${w.ipv4 ? ' · ' + w.ipv4 : ''}` : `État : ${w.state}`;
  const ser = L.wan_series || [];
  ovChart.set([{ name: 'Réception', color: '--teal', data: ser.map((p) => [p[0], p[1]]), fill: true }, { name: 'Envoi', color: '--coral', data: ser.map((p) => [p[0], p[2]]) }], { cap: w.bw_down_mbps, capLabel: 'capacité' });
  // santé
  const D = S.D; ring($('#ov-ring'), D.score);
  const bad = D.findings.filter((f) => f.sev === 'crit' || f.sev === 'warn');
  $('#ov-verdict').innerHTML = `<b>${bad.length ? esc(bad[0].title) : 'Ton réseau est en bonne santé'}</b><span>${D.crit} critique(s) · ${D.warn} à corriger</span>`;
  $('#ov-findings').innerHTML = D.findings.filter((f) => f.sev !== 'ok').slice(bad.length ? 1 : 0, 5).map((f) => `<li><a href="#diagnostic"><i class="sev ${f.sev}"></i><span>${esc(f.title)}</span></a></li>`).join('');
  // indicateurs
  const hosts = L.hosts || [], act = hosts.filter((h) => h.active);
  const ap24 = (L.aps || []).find((a) => a.band === '2.4'), ap5 = (L.aps || []).find((a) => a.band === '5'), ap6 = (L.aps || []).find((a) => a.band === '6');
  const todayD = hosts.reduce((s, h) => s + (h.today_down || 0), 0);
  const busy = (a) => (a?.busy != null ? `<small class="${a.busy > 70 ? 'c' : a.busy > 50 ? 'w' : ''}">occupé à ${Math.round(a.busy)} %</small>` : '<small>occupation inconnue</small>');
  const k = [
    `<div class="kpi"><span>Appareils actifs</span><b>${act.length} <span class="muted" style="font-size:15px">/ ${hosts.length}</span></b><small>${hosts.filter((h) => h.conn === 'wifi').length} en Wi-Fi · ${hosts.filter((h) => h.conn === 'ethernet').length} en Ethernet</small></div>`,
    ap24 ? `<div class="kpi"><span>Wi-Fi 2,4 GHz</span><b>Canal ${ap24.channel}${ap24.auto_channel ? ' <span class="muted" style="font-size:13px">auto</span>' : ''}</b>${busy(ap24)}</div>` : '',
    ap5 ? `<div class="kpi"><span>Wi-Fi 5 GHz</span><b>Canal ${ap5.channel}</b>${busy(ap5)}</div>` : '',
    ap6 ? `<div class="kpi"><span>Wi-Fi 6 GHz</span><b>Canal ${ap6.channel}</b>${busy(ap6)}</div>` : '',
    L.ftth?.rx_dbm != null ? `<div class="kpi"><span>Signal fibre reçu</span><b>${neg(L.ftth.rx_dbm.toFixed(1).replace('.', ','))} dBm</b><small class="${L.ftth.rx_dbm < -27 ? 'w' : ''}">${L.ftth.rx_dbm < -27 ? 'faible' : 'normal'}</small></div>` : '',
    `<div class="kpi"><span>Reçu depuis minuit</span><b>${fmtBytes(todayD)}</b><small>tous appareils confondus</small></div>`,
  ];
  $('#ov-kpis').innerHTML = k.join('');
  // classements
  const now = [...hosts].filter((h) => (h.down_mbps || 0) + (h.up_mbps || 0) > 0.01).sort((a, b) => b.down_mbps + b.up_mbps - (a.down_mbps + a.up_mbps)).slice(0, 6);
  const mx = Math.max(1, ...now.map((h) => h.down_mbps + h.up_mbps));
  $('#ov-now').innerHTML = now.length ? now.map((h) => rankRow(h, h.down_mbps, h.up_mbps, mx, `${fmtRate(h.down_mbps)}<small>↑ ${fmtRate(h.up_mbps)}</small>`)).join('') : '<p class="empty">Aucun trafic notable en ce moment.</p>';
  const td = [...hosts].filter((h) => h.today_down + h.today_up > 0).sort((a, b) => b.today_down + b.today_up - (a.today_down + a.today_up)).slice(0, 6);
  const mt = Math.max(1, ...td.map((h) => h.today_down + h.today_up));
  $('#ov-today').innerHTML = td.length ? td.map((h) => rankRow(h, h.today_down, h.today_up, mt, `${fmtBytes(h.today_down)}<small>↑ ${fmtBytes(h.today_up)}</small>`)).join('') : '<p class="empty">L’historique se construit au fil des minutes.</p>';
}
const rankRow = (h, d, u, max, val) => `<div class="rk" data-mac="${esc(h.mac)}"><span class="ic">${icon(h.kind)}</span><span class="nm"><b>${esc(h.name)}</b><span class="bar"><i style="width:${(d / max) * 100}%"></i><i class="u" style="width:${(u / max) * 100}%"></i></span></span><span class="val">${val}</span></div>`;
document.addEventListener('click', (e) => { const r = e.target.closest('[data-mac]'); if (r) openDrawer(r.dataset.mac); });

// ---------- APPAREILS ----------
function renderDevices() {
  const hosts = S.live.hosts || [];
  const q = S.q.toLowerCase();
  let rows = hosts.filter((h) => {
    if (q && ![h.name, h.vendor, h.ip, h.mac, KIND[h.kind]].some((x) => (x || '').toLowerCase().includes(q))) return false;
    return { all: true, active: h.active, wifi: h.conn === 'wifi', ethernet: h.conn === 'ethernet', weak: h.signal != null && h.signal < -72, new: isNew(h) }[S.filter];
  });
  const { k, asc } = S.sort;
  rows.sort((a, b) => { const x = a[k] ?? -1e9, y = b[k] ?? -1e9; const r = typeof x === 'string' ? x.localeCompare(y, 'fr') : x - y; return asc ? r : -r; });
  $('#dev-sub').textContent = `${hosts.length} appareils connus · ${hosts.filter((h) => h.active).length} actifs · débits mesurés par la box`;
  $('#dev-empty').hidden = rows.length > 0;
  for (const th of $$('#dev-table th')) { th.classList.toggle('sorted', th.dataset.s === k); th.classList.toggle('asc', th.dataset.s === k && asc); }
  $('#dev-table tbody').innerHTML = rows.map((h) => `<tr data-mac="${esc(h.mac)}">
    <td><div class="dev${h.active ? '' : ' off'}"><span class="ic">${icon(h.kind)}</span><div><b>${esc(h.name)}${isNew(h) ? ' <span class="badge newb">nouveau</span>' : ''}</b><small>${esc([KIND[h.kind], h.vendor, h.ip].filter(Boolean).join(' · '))}</small></div></div></td>
    <td>${connBadge(h)}</td><td class="num">${sigHtml(h.signal)}</td>
    <td class="num">${h.phy_down ? fmtRate(h.phy_down) : h.conn === 'ethernet' && h.eth_speed ? fmtRate(h.eth_speed) : '—'}</td>
    <td class="num">${h.down_mbps != null ? fmtRate(h.down_mbps) : '—'}</td><td class="num">${h.up_mbps != null ? fmtRate(h.up_mbps) : '—'}</td>
    <td class="num">${fmtBytes(h.today_down + h.today_up)}</td>
    <td>${h.active ? '<span class="badge eth">actif</span>' : `<span class="muted">${h.last_activity ? ago(h.last_activity) : '—'}</span>`}</td></tr>`).join('');
}
$('#dev-q').addEventListener('input', (e) => { S.q = e.target.value; renderDevices(); });
$('#dev-filter').addEventListener('click', (e) => { const b = e.target.closest('button'); if (!b) return; S.filter = b.dataset.f; for (const x of $$('#dev-filter button')) x.classList.toggle('on', x === b); renderDevices(); });
$('#dev-table thead').addEventListener('click', (e) => { const th = e.target.closest('th'); if (!th) return; const k = th.dataset.s; S.sort = { k, asc: S.sort.k === k ? !S.sort.asc : ['name', 'conn'].includes(k) }; renderDevices(); });

// ---------- VOLET APPAREIL ----------
let drLive, drDay;
function openDrawer(mac) { S.drawer = mac; $('#drawer').hidden = false; renderDrawer(true); }
$('#dr-close').addEventListener('click', () => { S.drawer = null; $('#drawer').hidden = true; });
addEventListener('keydown', (e) => { if (e.key === 'Escape' && S.drawer) $('#dr-close').click(); });
async function renderDrawer(full) {
  const h = (S.live?.hosts || []).find((x) => x.mac === S.drawer); if (!h) return;
  if (full) {
    const adv = (S.D?.findings || []).filter((f) => f.hosts?.includes(h.mac));
    if (h.conn === 'wifi' && h.signal != null && h.signal >= -65 && !adv.length) adv.push({ sev: 'ok', title: 'Bonne réception Wi-Fi', detail: `Signal de ${Math.round(h.signal)} dBm.` });
    if (h.conn === 'ethernet' && h.eth_speed >= 1000) adv.push({ sev: 'ok', title: 'Lien Ethernet à pleine vitesse', detail: `${h.eth_speed / 1000} Gb/s négociés avec la box.` });
    $('#dr-body').innerHTML = `<div class="dev"><span class="ic">${icon(h.kind)}</span><div><small>${esc(KIND[h.kind] || 'Appareil')}${h.vendor ? ' · ' + esc(h.vendor) : ''}</small></div></div>
      <h2>${esc(h.name)}</h2><p class="sub">${h.active ? 'Actif maintenant' : 'Inactif · ' + (h.last_activity ? ago(h.last_activity) : '')}${isNew(h) ? ' · apparu ' + ago(h.first_seen) : ''}</p>
      <h3>En direct</h3><div class="chart" id="dr-live"></div>
      <h3>Dernières 24 heures</h3><div class="chart" id="dr-day"></div>
      <h3>Connexion</h3><dl class="kv">
        <dt>Liaison</dt><dd>${h.conn === 'wifi' ? `Wi-Fi ${band(h.band)}${h.ssid ? ' · ' + esc(h.ssid) : ''}` : h.conn === 'ethernet' ? `Ethernet${h.port ? ' · port ' + h.port : ''}` : 'Inconnue'}</dd>
        ${h.conn === 'wifi' ? `<dt>Signal</dt><dd>${dbm(h.signal)}</dd><dt>Débit radio</dt><dd>${fmtRate(h.phy_down)} vers l’appareil · ${fmtRate(h.phy_up)} depuis</dd><dt>Norme</dt><dd>${esc(h.standard || '—')}</dd><dt>Connecté depuis</dt><dd>${dur(h.conn_duration)}</dd>` : ''}
        ${h.conn === 'ethernet' ? `<dt>Vitesse du lien</dt><dd>${h.eth_speed ? fmtRate(h.eth_speed) : '—'}</dd>` : ''}
        <dt>Adresse IP</dt><dd class="selectable">${esc(h.ip || '—')}</dd><dt>Adresse MAC</dt><dd class="selectable">${esc(h.mac)}</dd>
        <dt>Aujourd’hui</dt><dd>${fmtBytes(h.today_down)} reçus · ${fmtBytes(h.today_up)} envoyés</dd>
        ${h.first_seen ? `<dt>Vu la première fois</dt><dd>${new Date(h.first_seen * 1000).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' })}</dd>` : ''}</dl>
      <h3>Analyse</h3><ul class="advice">${adv.length ? adv.map((f) => `<li class="${f.sev}"><b>${esc(f.title)}</b><br>${esc(f.action || f.detail)}</li>`).join('') : '<li>Rien à signaler pour cet appareil.</li>'}</ul>`;
    drLive = new LineChart($('#dr-live'), { empty: 'Le direct démarre…' });
    drDay = new LineChart($('#dr-day'), { empty: 'Pas encore d’historique pour cet appareil' });
    call('host_history', { mac: h.mac, range: 86400, points: 288 }).then((r) => (r.rows = gaps(r.rows, r.bucket, 0), r)).then((r) => drDay.set([{ name: 'Réception', color: '--teal', data: r.rows.map((x) => [x[0], x[1]]), fill: true }, { name: 'Envoi', color: '--coral', data: r.rows.map((x) => [x[0], x[2]]) }])).catch(() => {});
  }
  const s = S.hl[h.mac] || [];
  drLive?.set([{ name: 'Réception', color: '--teal', data: s.map((p) => [p[0], p[1]]), fill: true }, { name: 'Envoi', color: '--coral', data: s.map((p) => [p[0], p[2]]) }]);
}

// ---------- CONSOMMATION ----------
const csChart = new LineChart($('#cs-chart'), { empty: 'L’historique se construit au fil des minutes.' });
$('#cs-range').addEventListener('click', (e) => { const b = e.target.closest('button'); if (!b) return; S.range = +b.dataset.r; for (const x of $$('#cs-range button')) x.classList.toggle('on', x === b); renderConso(); });
async function renderConso() {
  const r = S.range;
  const [hist, top, sat, heat, ev] = await Promise.all([call('wan_history', { range: r, points: 480 }), call('top', { range: r, limit: 15 }), call('saturation', { range: r }), call('heatmap', { days: 7 }), call('events', { range: Math.max(r, 7 * 86400) })].map((p) => p.catch(() => null)));
  drawHist(hist);
  const since = S.status?.data_since;
  const note = $('#cs-note');
  if (since && Date.now() / 1000 - since < r * 0.9 && !S.status?.demo) {
    note.innerHTML = `Historique local depuis le ${new Date(since * 1000).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' })}. <button class="btn ghost" id="cs-rrd">Compléter avec l’historique de la box</button>`;
    $('#cs-rrd').onclick = async () => { try { drawHist(await call('rrd', { range: r }), true); note.textContent = 'Courbe issue de l’historique de la box (débit total de la ligne uniquement).'; } catch (e) { note.textContent = explain(e); } };
  } else note.textContent = hist?.bucket ? `Une valeur toutes les ${hist.bucket >= 3600 ? hist.bucket / 3600 + ' h' : hist.bucket / 60 + ' min'} · pics = maximum relevé sur l’intervalle.` : '';
  // qui a consommé
  const T = top || [], tot = T.reduce((s, t) => s + t.down + t.up, 0), mx = Math.max(1, ...T.map((t) => t.down + t.up));
  $('#cs-total').textContent = tot ? `${fmtBytes(tot)} échangés` : '';
  $('#cs-top').innerHTML = T.length ? T.map((t) => rankRow({ mac: t.mac, name: t.name, kind: t.kind }, t.down, t.up, mx, `${fmtBytes(t.down + t.up)}<small>${Math.round(((t.down + t.up) / tot) * 100)} %</small>`)).join('') : '<p class="empty">Pas encore de données sur cette période.</p>';
  // saturations
  const eps = sat?.episodes || [];
  $('#cs-sat').innerHTML = !sat ? '' : !eps.length ? `<div class="big-ok">✓ Aucune saturation sur ${Math.round((sat.observed_minutes || 0) / 60)} h observées.</div>`
    : `<p class="muted" style="margin:0 0 10px">${sat.saturated_minutes} min au-dessus de 90 % de la capacité, en ${eps.length} épisode(s).</p><div class="sat">${[...eps].reverse().slice(0, 40).map((e) => `<div class="ep"><b>${new Date(e.start * 1000).toLocaleString('fr-FR', { weekday: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}</b><span>${Math.round((e.end - e.start) / 60)} min · ${e.down ? 'réception' : ''}${e.down && e.up ? ' + ' : ''}${e.up ? 'envoi' : ''}</span><span></span><span>${e.top.length ? 'Surtout : ' + e.top.filter((t) => t.down + t.up >= (e.top[0].down + e.top[0].up) * 0.15).map((t) => esc(t.name)).join(', ') : 'Responsable non identifié'}</span></div>`).join('')}</div>`;
  // carte de chaleur
  if (heat) {
    const max = Math.max(1, ...heat.flat().filter((x) => x != null));
    const days = ['Lun', 'Mar', 'Mer', 'Jeu', 'Ven', 'Sam', 'Dim'];
    $('#cs-heat').innerHTML = '<span></span>' + Array.from({ length: 24 }, (_, h) => `<span class="hl">${h % 3 === 0 ? h : ''}</span>`).join('') + heat.map((row, d) => `<span>${days[d]}</span>` + row.map((v, h) => (v == null ? '<i class="c na"></i>' : `<i class="c" style="opacity:${(0.08 + 0.92 * Math.sqrt(v / max)).toFixed(2)}" title="${days[d]} ${h} h : ${fmtRate(v)} en moyenne"></i>`)).join('')).join('');
  }
  $('#cs-events').innerHTML = (ev || []).length ? ev.map((e) => `<li><time>${new Date(e.ts * 1000).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' })}</time><span>${esc(e.detail)}</span></li>`).join('') : '<li class="muted">Aucun événement.</li>';
}
function drawHist(hist, rrd) {
  const rows = hist?.bucket ? gaps(hist.rows || [], hist.bucket, null) : hist?.rows || [];
  const cap = Math.max(0, ...rows.map((x) => x[5] || 0));
  csChart.set([
    { name: 'Pics', color: '--teal', data: rows.map((x) => [x[0], x[2]]), alpha: 0.3, width: 1 },
    { name: 'Réception', color: '--teal', data: rows.map((x) => [x[0], x[1]]), fill: true },
    { name: 'Envoi', color: '--coral', data: rows.map((x) => [x[0], x[3]]) },
  ], { cap, capLabel: 'capacité', capInScale: rrd ? false : rows.some((x) => x[2] > cap * 0.5) });
}

// ---------- WI-FI ----------
$('#wf-scan').addEventListener('click', async () => {
  try { await call('scan_neighbors'); toast('Scan lancé : les résultats arrivent dans une quinzaine de secondes.'); setTimeout(pollSlow, 15000); }
  catch (e) { toast(explain(e)); }
});
function renderWifi(entering) {
  const L = S.live, aps = L.aps || [], nb = S.wifi?.neighbors || [];
  const C = 2 * Math.PI * 30;
  $('#wf-aps').innerHTML = aps.length ? aps.map((a) => {
    const b = a.busy, col = b == null ? 'var(--rule)' : b > 70 ? 'var(--crit)' : b > 50 ? 'var(--warn)' : 'var(--teal)';
    return `<div class="ap"><h3>${band(a.band)}</h3><div class="busy"><svg viewBox="0 0 70 70"><circle cx="35" cy="35" r="30" fill="none" stroke="var(--rule)" stroke-width="7"/><circle cx="35" cy="35" r="30" fill="none" stroke="${col}" stroke-width="7" stroke-linecap="round" stroke-dasharray="${C}" stroke-dashoffset="${C * (1 - (b || 0) / 100)}" transform="rotate(-90 35 35)"/></svg><b>${b == null ? '—' : Math.round(b) + '%'}</b></div>
      <span class="meta">Canal ${a.channel}${a.auto_channel ? ' (automatique)' : ''} · ${a.width} MHz</span><span class="meta">${a.stations} appareil(s) · ${a.state === 'active' ? 'active' : esc(a.state)}</span></div>`;
  }).join('') : '<div class="card">La box ne renvoie aucune borne Wi-Fi.</div>';
  const I = interference(nb, aps);
  const n24 = nb.filter((n) => n.band === '2.4'), n5 = nb.filter((n) => n.band === '5');
  $('#wf-n24').textContent = `${n24.length} réseau(x)`; $('#wf-n5').textContent = `${n5.length} réseau(x)`;
  const ap24 = aps.find((a) => a.band === '2.4');
  drawSpectrum($('#wf-spectrum'), n24, ap24, I.b24?.best?.ch);
  $('#wf-reco24').innerHTML = !I.b24 ? '' : I.b24.offGrid || (I.b24.score > 0.8 && I.b24.best.ch !== I.b24.own && I.b24.best.score < I.b24.score * 0.6)
    ? `Ta box est sur le canal <b>${I.b24.own}</b> (interférence ${level(I.b24.score)}). Le canal <b>${I.b24.best.ch}</b> est plus calme (${level(I.b24.best.score)}).${I.b24.width >= 40 ? ' Passe aussi la largeur à 20 MHz.' : ''}`
    : `Canal <b>${I.b24.own}</b> : interférence ${level(I.b24.score)}, c’est un bon choix.${I.b24.width >= 40 && n24.length >= 5 ? ' Une largeur de 20 MHz serait plus adaptée ici.' : ''}`;
  if (I.b5) {
    const mx = Math.max(1, ...I.b5.blocks.map((b) => b.score));
    $('#wf-blocks').innerHTML = I.b5.blocks.map((b) => `<div class="brow${b.id === I.b5.block ? ' me' : ''}"><b>${b.id}${b.id === I.b5.block ? ' · ta box' : ''}</b><span class="bar"><i class="${b.score >= 2 ? 'c' : b.score >= 0.8 ? 'w' : ''}" style="width:${Math.max(2, (b.score / mx) * 100)}%"></i></span><span class="lv">${level(b.score)}${b.dfs ? ' · DFS' : ''}</span></div>`).join('');
    $('#wf-reco5').innerHTML = I.b5.block && I.b5.best.id !== I.b5.block && I.b5.score > 1 && I.b5.best.score < I.b5.score * 0.5 ? `Ton bloc ${I.b5.block} est partagé. Le bloc <b>${I.b5.best.id}</b> est plus libre.` : I.b5.block ? `Bloc ${I.b5.block} : interférence ${level(I.b5.score)}.` : '';
  }
  $('#wf-usage').innerHTML = aps.filter((a) => a.usage?.length).map((a) => `<div class="ug"><b>${band(a.band)}</b><div class="cols">${a.usage.map((u) => `<div class="col${u.channel === a.channel ? ' me' : ''}" title="Canal ${u.channel} : ${u.busy == null ? '—' : Math.round(u.busy) + ' %'}"><i class="${u.busy > 70 ? 'c' : u.busy > 50 ? 'w' : ''}" style="height:${Math.max(2, u.busy || 0)}%"></i>${u.channel}</div>`).join('')}</div></div>`).join('') || '<p class="muted">La box ne communique pas l’occupation des canaux.</p>';
  $('#wf-table tbody').innerHTML = [...nb].sort((a, b) => (b.signal ?? -200) - (a.signal ?? -200)).map((n) => `<tr><td>${esc(n.ssid)}</td><td>${band(n.band)}</td><td class="num">${n.channel}</td><td class="num">${n.width} MHz</td><td class="num">${sigHtml(n.signal)}</td></tr>`).join('') || '<tr><td colspan="5" class="muted">Aucun voisin relevé.</td></tr>';
  if (entering) loadLocal();
}
async function loadLocal() {
  const el = $('#wf-local');
  try {
    const r = await call('local_wifi'); const i = (r.interfaces || []).find((x) => x.ssid) || (r.interfaces || [])[0];
    if (!i) { el.innerHTML = '<p class="muted">Aucune carte Wi-Fi active : ce PC est sans doute relié par câble.</p>'; return; }
    el.innerHTML = `<dl class="kv"><dt>Réseau</dt><dd>${esc(i.ssid || '—')}</dd><dt>Signal</dt><dd>${sigHtml(i.signal_dbm)} dBm (${i.signal_pct ?? '—'} %)</dd><dt>Bande / canal</dt><dd>${esc(i.band || '')} · canal ${i.channel ?? '—'}</dd><dt>Norme</dt><dd>${esc(i.radio || '—')}</dd><dt>Débit du lien</dt><dd>${fmtRate(i.rx_mbps)} en réception · ${fmtRate(i.tx_mbps)} en envoi</dd></dl>${r.location_blocked ? '<p class="hint">Pour lister les réseaux vus par ce PC, Windows exige d’autoriser la localisation (Paramètres > Confidentialité > Localisation).</p>' : `<p class="hint">${(r.networks || []).length} réseau(x) visibles depuis ce PC.</p>`}`;
  } catch (e) { el.innerHTML = `<p class="muted">${esc(explain(e))}</p>`; }
}

// ---------- MESURE ----------
const G = { cx: 150, cy: 132, r: 106, a0: 135, sweep: 270, max: 2000 };
const pol = (a, r) => [G.cx + r * Math.cos((a * Math.PI) / 180), G.cy + r * Math.sin((a * Math.PI) / 180)];
const gfrac = (v) => Math.max(0, Math.min(1, Math.log10(Math.max(1, v)) / Math.log10(G.max)));
const ARC = (2 * Math.PI * G.r * G.sweep) / 360;
{
  const [x0, y0] = pol(G.a0, G.r), [x1, y1] = pol(G.a0 + G.sweep, G.r), d = `M${x0} ${y0} A${G.r} ${G.r} 0 1 1 ${x1} ${y1}`;
  $('#gauge').innerHTML = `<path class="trk" d="${d}" fill="none" stroke-width="14" stroke-linecap="round"/><path class="val" id="g-val" d="${d}" fill="none" stroke-width="14" stroke-linecap="round" stroke-dasharray="${ARC}" stroke-dashoffset="${ARC}"/>` +
    [1, 5, 10, 50, 100, 500, 1000, 2000].map((t) => { const a = G.a0 + gfrac(t) * G.sweep, [tx, ty] = pol(a, G.r + 20); return `<text class="tk" x="${tx}" y="${ty + 4}" text-anchor="middle">${t >= 1000 ? t / 1000 + 'G' : t}</text>`; }).join('') +
    `<text class="big" id="g-num" x="150" y="142" text-anchor="middle">0</text><text class="unit" id="g-unit" x="150" y="166" text-anchor="middle">Mb/s</text>`;
}
const setGauge = (v, unit = 'Mb/s', lat) => { $('#g-val').style.strokeDashoffset = ARC * (1 - (lat ? Math.min(1, v / 200) : gfrac(v))); $('#g-num').textContent = v >= 100 ? Math.round(v) : (Math.round(v * 10) / 10).toLocaleString('fr-FR'); $('#g-unit').textContent = unit; };
let msConn = 'wifi5';
$('#ms-conn').addEventListener('click', (e) => { const b = e.target.closest('button'); if (!b) return; msConn = b.dataset.c; for (const x of $$('#ms-conn button')) x.classList.toggle('on', x === b); });
function renderMesure() {
  const T = S.tests || [];
  $('#ms-empty').hidden = T.length > 0;
  $('#ms-rooms').innerHTML = [...new Set(T.map((t) => t.room).filter(Boolean))].map((r) => `<option value="${esc(r)}">`).join('');
  const C = { eth: 'Ethernet', wifi5: 'Wi-Fi 5 GHz', wifi24: 'Wi-Fi 2,4 GHz', wifi6: 'Wi-Fi 6 GHz' };
  $('#ms-table tbody').innerHTML = T.map((t) => `<tr><td>${new Date(t.ts * 1000).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' })}</td><td>${esc(t.room || '—')}</td><td>${C[t.conn] || '—'}</td><td class="num">${fmtRate(t.down)}</td><td class="num">${fmtRate(t.up)}</td><td class="num">${t.ping != null ? Math.round(t.ping) + ' ms' : '—'}</td><td class="num">${t.loadedPing != null ? Math.round(t.loadedPing) + ' ms' : '—'}</td><td class="num">${fmtRate(t.boxDown)}</td><td><button class="btn icon ghost" data-del="${t.ts}" aria-label="Supprimer">×</button></td></tr>`).join('');
}
$('#ms-table').addEventListener('click', async (e) => { const b = e.target.closest('[data-del]'); if (!b) return; await call('speedtest_delete', { ts: +b.dataset.del }); S.tests = await call('speedtests'); renderMesure(); });
let running = null;
$('#ms-go').addEventListener('click', async () => {
  if (running) { running.abort(); return; }
  const go = $('#ms-go'), msg = $('#ms-msg'); msg.hidden = true;
  for (const id of ['#ms-down', '#ms-up', '#ms-ping', '#ms-jit', '#ms-load', '#ms-box']) $(id).textContent = '—';
  let boxMax = 0; const boxT = setInterval(() => { if (S.live?.wan) boxMax = Math.max(boxMax, S.live.wan.down_mbps); }, 1000);
  const labels = { latency: 'Latence…', download: 'Réception…', upload: 'Envoi…', done: 'Terminé' };
  running = new SpeedTest({ duration: 8, streams: 4, onUpdate: (u) => {
    $('#ms-phase').textContent = labels[u.phase] || '';
    $('#ms-prog').style.width = Math.min(100, (({ latency: 0, download: 1, upload: 2, done: 3 }[u.phase] + (u.progress || 0)) / 3) * 100) + '%';
    if (u.phase === 'latency' && u.value != null) { setGauge(u.value, 'ms', true); $('#ms-ping').textContent = Math.round(u.value) + ' ms'; }
    if (u.phase === 'download' || u.phase === 'upload') setGauge(u.value);
  } });
  go.textContent = 'Arrêter'; go.classList.remove('primary');
  try {
    const r = await running.run();
    clearInterval(boxT);
    setGauge(r.down);
    $('#ms-down').textContent = fmtRate(r.down); $('#ms-up').textContent = fmtRate(r.up); $('#ms-ping').textContent = Math.round(r.ping) + ' ms'; $('#ms-jit').textContent = Math.round(r.jitter) + ' ms';
    const b = bloat(r.ping, r.loadedPing); $('#ms-load').textContent = r.loadedPing != null ? `${Math.round(r.loadedPing)} ms · ${b.grade}` : '—';
    $('#ms-box').textContent = fmtRate(boxMax);
    await call('speedtest_add', { ...r, conn: msConn, room: $('#ms-room').value.trim(), boxDown: boxMax });
    S.tests = await call('speedtests'); renderMesure(); recompute();
    msg.className = 'note ok'; msg.hidden = false;
    msg.textContent = boxMax > r.down * 1.5 + 20 ? `La box a vu passer ${fmtRate(boxMax)} pendant ton test : d’autres appareils consommaient. Refais la mesure à un moment calme.` : msConn === 'eth' ? 'Mesure enregistrée. En Ethernet, elle reflète la ligne elle-même.' : 'Mesure enregistrée. Compare avec une mesure en Ethernet pour isoler le Wi-Fi.';
  } catch (e) {
    clearInterval(boxT);
    $('#ms-phase').textContent = e.name === 'AbortError' ? 'Mesure arrêtée' : 'Échec de la mesure';
    if (e.name !== 'AbortError') { msg.className = 'note'; msg.hidden = false; msg.textContent = `Le serveur de mesure (Cloudflare) est injoignable : ${e.message}. Vérifie la connexion Internet ou un pare-feu.`; }
    setGauge(0);
  } finally { running = null; go.textContent = 'Relancer la mesure'; go.classList.add('primary'); $('#ms-prog').style.width = '0'; }
});

// ---------- DIAGNOSTIC ----------
let dgArea = 'all';
$('#dg-filter').addEventListener('click', (e) => { const b = e.target.closest('button'); if (!b) return; dgArea = b.dataset.a; for (const x of $$('#dg-filter button')) x.classList.toggle('on', x === b); renderDiag(true); });
const SEV = { crit: 'Critique', warn: 'À corriger', info: 'Info', ok: 'OK' };
let lastDiagKey = '';
function renderDiag(force) {
  const D = S.D; const F = D.findings.filter((f) => dgArea === 'all' || f.area === dgArea);
  $('#dg-title').textContent = D.crit ? D.findings[0].title : D.warn ? 'Quelques points à corriger' : 'Ton réseau est en bonne santé';
  $('#dg-sub').textContent = `Score ${D.score}/100 · ${D.crit} critique(s), ${D.warn} à corriger · analyse des 7 derniers jours et de l’état actuel`;
  const key = dgArea + F.map((f) => f.title).join('|');
  if (!force && key === lastDiagKey) return; lastDiagKey = key;
  $('#dg-list').innerHTML = F.map((f, i) => `<article class="f ${f.sev}" style="animation-delay:${Math.min(i, 10) * 25}ms"><header><h3>${esc(f.title)}</h3><span class="pill ${f.sev}">${SEV[f.sev]}</span><span class="pill conf">${esc(f.conf)}</span></header><p>${esc(f.detail)}</p>${f.action ? `<p class="act">${esc(f.action)}</p>` : ''}${f.link ? `<p><a href="${f.link}">Voir le détail</a></p>` : ''}</article>`).join('') || '<p class="muted">Rien dans cette catégorie.</p>';
}
$('#dg-copy').addEventListener('click', async () => { toast((await copyText(report(S.D, ctx()))) ? 'Rapport copié : colle-le dans un message à l’assistance.' : 'Copie refusée par le système.'); });

// ---------- RÉGLAGES ----------
async function renderSettings() {
  try { S.status = await call('status'); } catch {}
  const st = S.status || {}, L = S.live || {};
  $('#rg-sub').textContent = `Ondes ${st.version || ''} · données dans ${st.data_dir || '—'}`;
  const P = L.permissions || st.permissions || {};
  const perms = Object.entries(P).filter(([, v]) => v).map(([k]) => k).join(', ') || 'aucun droit particulier (suffisant pour la lecture)';
  $('#rg-box').innerHTML = `<dt>Box</dt><dd>${esc(L.box?.model || st.box_name || '—')}</dd><dt>État</dt><dd>${st.demo ? 'Mode démo' : L.connected ? 'Connectée' : st.paired ? 'Associée, connexion en cours' : 'Non associée'}</dd><dt>API</dt><dd>${esc(st.api || L.box?.api || '—')}</dd><dt>Micrologiciel</dt><dd>${esc(L.box?.firmware || '—')}</dd><dt>Allumée depuis</dt><dd>${dur(L.box?.uptime)}</dd><dt>Droits accordés</dt><dd>${esc(perms)}</dd>`;
  $('#rg-base').value = st.base_url || '';
  $('#rg-token').textContent = st.token_in_file ? 'Attention : le coffre d’identifiants de Windows était indisponible, la clé d’accès est stockée dans le fichier de configuration.' : st.paired ? 'La clé d’accès à la box est rangée dans le Gestionnaire d’identifiants de Windows.' : '';
  $('#rg-interval').value = String(st.interval || 5);
  $('#rg-tray').checked = !!st.close_to_tray;
  $('#rg-ret').value = String(st.retention_days || 90);
  $('#rg-sd').value = st.subscribed_down ?? ''; $('#rg-su').value = st.subscribed_up ?? '';
  $('#rg-demo').textContent = st.demo ? 'Quitter le mode démo' : 'Mode démo';
  $('#rg-data').innerHTML = `<dt>Historique depuis</dt><dd>${st.data_since ? new Date(st.data_since * 1000).toLocaleString('fr-FR', { dateStyle: 'medium', timeStyle: 'short' }) : '—'}</dd><dt>Dossier</dt><dd class="selectable">${esc(st.data_dir || '—')}</dd>`;
  if (inApp()) call('autostart_get').then((v) => ($('#rg-auto').checked = !!v)).catch(() => ($('#rg-auto').disabled = true));
  else $('#rg-auto').disabled = true;
}
const setting = async (obj) => { try { S.status = await call('settings', obj); toast('Réglage enregistré'); recompute(); } catch (e) { toast(explain(e)); } };
$('#rg-interval').addEventListener('change', (e) => setting({ interval: +e.target.value }));
$('#rg-tray').addEventListener('change', (e) => setting({ close_to_tray: e.target.checked }));
$('#rg-ret').addEventListener('change', (e) => setting({ retention_days: +e.target.value }));
$('#rg-sd').addEventListener('change', (e) => setting({ subscribed_down: e.target.value ? +e.target.value : null }));
$('#rg-su').addEventListener('change', (e) => setting({ subscribed_up: e.target.value ? +e.target.value : null }));
$('#rg-base').addEventListener('change', (e) => setting({ base_url: e.target.value }));
$('#rg-auto').addEventListener('change', async (e) => { try { await call('autostart_set', { on: e.target.checked }); toast(e.target.checked ? 'Ondes démarrera avec Windows' : 'Démarrage automatique désactivé'); } catch (err) { toast(explain(err)); e.target.checked = !e.target.checked; } });
$('#rg-export').addEventListener('click', async () => { try { const r = await call('export'); $('#rg-exp-msg').textContent = `Export enregistré : ${r.path}`; } catch (e) { $('#rg-exp-msg').textContent = explain(e); } });
$('#rg-demo').addEventListener('click', async () => { S.status = await call('demo', { on: !S.status?.demo }); S.live = null; S.hl = {}; await boot(); });
$('#rg-repair').addEventListener('click', () => showOnboard(true));
armButton($('#rg-unpair'), async () => { S.status = await call('unpair'); S.live = null; await boot(); });

// ---------- ASSOCIATION ----------
let obTimer = null;
function step(id, cls, text, spanId) { const li = $(id); li.className = cls; if (text != null) $(spanId).textContent = text; }
async function showOnboard(force) {
  $('#onboard').hidden = false;
  $('#ob-base').value = S.status?.base_url || 'http://mafreebox.freebox.fr';
  $('#ob-lcd').hidden = true; $('#ob-msg').hidden = true;
  step('#ob-s2', '', 'Une demande va s’afficher sur l’écran de ta Freebox.', '#ob-ask'); step('#ob-s3', '', 'Avec les flèches de la box, réponds « Oui ».', '#ob-wait');
  await findBox();
  if (force) $('#ob-demo').textContent = S.status?.configured ? 'Annuler' : 'Essayer avec une box de démonstration';
}
async function findBox() {
  step('#ob-s1', 'act', 'Recherche en cours…', '#ob-find');
  try {
    const r = await call('discover', { base: $('#ob-base').value.trim() });
    step('#ob-s1', 'done', `${r.model} trouvée (API ${r.api}) à ${r.base}`, '#ob-find'); $('#ob-go').disabled = false; step('#ob-s2', 'act', null, '#ob-ask');
  } catch (e) { step('#ob-s1', 'err', explain(e), '#ob-find'); $('#ob-go').disabled = true; document.querySelector('.ob-adv').open = true; }
}
$('#ob-base').addEventListener('change', findBox);
$('#ob-go').addEventListener('click', async () => {
  const msg = $('#ob-msg'); msg.hidden = true; $('#ob-go').disabled = true;
  try {
    await call('pair_start', { base: $('#ob-base').value.trim() });
    step('#ob-s2', 'done', 'Demande envoyée.', '#ob-ask'); $('#ob-go').textContent = 'En attente de validation…'; step('#ob-s3', 'act', 'En attente de ta réponse sur la box…', '#ob-wait'); $('#ob-lcd').hidden = false;
    const t0 = Date.now(); clearInterval(obTimer);
    obTimer = setInterval(async () => {
      try {
        const { status } = await call('pair_poll');
        if (status === 'pending' && Date.now() - t0 < 180000) return;
        clearInterval(obTimer); $('#ob-lcd').hidden = true;
        if (status === 'granted') { step('#ob-s3', 'done', 'Autorisation accordée. Première collecte en cours…', '#ob-wait'); setTimeout(async () => { $('#onboard').hidden = true; await boot(); toast('Ondes est connecté à ta Freebox'); }, 900); return; }
        step('#ob-s3', 'err', { denied: 'Demande refusée sur la box.', timeout: 'La demande a expiré.', unknown: 'La box a perdu la demande.' }[status] || 'Délai dépassé.', '#ob-wait');
        $('#ob-go').disabled = false; $('#ob-go').textContent = 'Recommencer';
      } catch (e) { clearInterval(obTimer); msg.hidden = false; msg.textContent = explain(e); $('#ob-go').disabled = false; }
    }, 1500);
  } catch (e) { step('#ob-s2', 'err', explain(e), '#ob-ask'); $('#ob-go').disabled = false; }
});
$('#ob-demo').addEventListener('click', async () => {
  clearInterval(obTimer);
  if (S.status?.configured && $('#ob-demo').textContent === 'Annuler') { $('#onboard').hidden = true; return; }
  S.status = await call('demo', { on: true }); $('#onboard').hidden = true; await boot();
});

// ---------- démarrage ----------
let liveT, slowT;
async function boot() {
  try { S.status = await call('status'); } catch (e) { $('#conn span').textContent = explain(e); return; }
  clearInterval(liveT); clearInterval(slowT);
  if (!S.status.configured) { showOnboard(false); renderConn(); return; }
  $('#onboard').hidden = true;
  for (let i = 0; i < 20 && !(S.live?.connected); i++) { await pollLive(); if (!S.live?.connected) await new Promise((r) => setTimeout(r, 700)); }
  await pollSlow(); route();
  liveT = setInterval(() => { if (!document.hidden) pollLive(); }, 2000);
  slowT = setInterval(() => { if (!document.hidden) pollSlow(); }, 60000);
}
document.addEventListener('visibilitychange', () => { if (!document.hidden && S.status?.configured) { pollLive(); pollSlow(); } });
route(); boot();
}
