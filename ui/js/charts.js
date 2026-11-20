// Graphiques sur <canvas>, nets à toutes les résolutions, avec info-bulle au survol.

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
export const fmtRate = (v) => (v == null || isNaN(v) ? '—' : v >= 1000 ? (v / 1000).toLocaleString('fr-FR', { maximumFractionDigits: 2 }) + ' Gb/s' : v >= 100 ? Math.round(v) + ' Mb/s' : v >= 10 ? v.toFixed(1).replace('.', ',') + ' Mb/s' : v >= 0.1 ? v.toFixed(2).replace('.', ',') + ' Mb/s' : v > 0 ? Math.round(v * 1000) + ' kb/s' : '0');
const niceMax = (v) => { if (v <= 0) return 1; const p = 10 ** Math.floor(Math.log10(v)); const m = v / p; return ([1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10].find((x) => m <= x) || 10) * p; };
const fmtAxis = (v) => (v >= 1000 ? (v / 1000).toLocaleString('fr-FR', { maximumFractionDigits: 2 }) + ' G' : v >= 10 || v === 0 ? Math.round(v).toLocaleString('fr-FR') : v.toLocaleString('fr-FR', { maximumFractionDigits: 2 }));
const timeLabel = (t, span) => { const d = new Date(t * 1000); return span > 2 * 86400 ? d.toLocaleDateString('fr-FR', { weekday: 'short', day: 'numeric' }) : d.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: span < 600 ? '2-digit' : undefined }); };

export class LineChart {
  /** series: [{ name, color: '--var', data: [[t, v]], fill: bool, dash: bool, width }] ; opts: { cap, capLabel, unit } */
  constructor(el, opts = {}) {
    this.el = el; this.opts = opts; this.series = []; this.hover = null;
    this.c = document.createElement('canvas'); el.append(this.c);
    this.tip = document.createElement('div'); this.tip.className = 'tip'; this.tip.hidden = true; el.append(this.tip);
    this.ro = new ResizeObserver(() => this.draw()); this.ro.observe(el);
    this.c.addEventListener('pointermove', (e) => { const r = this.c.getBoundingClientRect(); this.hover = e.clientX - r.left; this.draw(); });
    this.c.addEventListener('pointerleave', () => { this.hover = null; this.tip.hidden = true; this.draw(); });
    this.mq = matchMedia('(prefers-color-scheme: dark)'); this.mq.addEventListener?.('change', () => this.draw());
  }
  set(series, opts = {}) { this.series = series; Object.assign(this.opts, opts); this.draw(); }
  draw() {
    const W = this.el.clientWidth, H = this.el.clientHeight; if (!W || !H) return;
    const dpr = devicePixelRatio || 1;
    if (this.c.width !== W * dpr || this.c.height !== H * dpr) { this.c.width = W * dpr; this.c.height = H * dpr; this.c.style.width = W + 'px'; this.c.style.height = H + 'px'; }
    const g = this.c.getContext('2d'); g.setTransform(dpr, 0, 0, dpr, 0, 0); g.clearRect(0, 0, W, H);
    const pad = { l: 48, r: 14, t: 18, b: 26 };
    const all = this.series.flatMap((s) => s.data).filter((p) => p[1] != null);
    const mute = css('--mut'), rule = css('--rule');
    g.font = '12px ' + css('--sans');
    if (all.length < 2) { g.fillStyle = mute; g.textAlign = 'center'; g.fillText(this.opts.empty || 'En attente de données…', W / 2, H / 2); return; }
    const t0 = this.opts.t0 ?? Math.min(...all.map((p) => p[0])), t1 = this.opts.t1 ?? Math.max(...all.map((p) => p[0]));
    let vmax = Math.max(...all.map((p) => p[1] || 0), 0.5);
    if (this.opts.cap && this.opts.capInScale) vmax = Math.max(vmax, this.opts.cap);
    vmax = niceMax(vmax * 1.04);
    const X = (t) => pad.l + ((t - t0) / Math.max(1, t1 - t0)) * (W - pad.l - pad.r);
    const Y = (v) => H - pad.b - (Math.min(v, vmax) / vmax) * (H - pad.t - pad.b);
    g.strokeStyle = rule; g.lineWidth = 1; g.fillStyle = mute; g.textAlign = 'right';
    for (let i = 0; i <= 4; i++) { const v = (vmax * i) / 4, y = Math.round(Y(v)) + 0.5; g.beginPath(); g.moveTo(pad.l, y); g.lineTo(W - pad.r, y); g.stroke(); g.fillText(fmtAxis(v), pad.l - 8, y + 4); }
    g.textAlign = 'left'; g.fillText('Mb/s', 4, 10);
    g.textAlign = 'center';
    const nt = Math.max(2, Math.floor((W - pad.l - pad.r) / 110));
    for (let i = 0; i <= nt; i++) { const t = t0 + ((t1 - t0) * i) / nt; g.fillText(timeLabel(t, t1 - t0), Math.min(W - pad.r - 20, Math.max(pad.l + 20, X(t))), H - 7); }
    if (this.opts.cap && this.opts.cap <= vmax) {
      const y = Y(this.opts.cap); g.setLineDash([5, 5]); g.strokeStyle = css('--crit'); g.globalAlpha = 0.55; g.beginPath(); g.moveTo(pad.l, y); g.lineTo(W - pad.r, y); g.stroke(); g.setLineDash([]); g.globalAlpha = 1;
      g.fillStyle = css('--crit'); g.textAlign = 'left'; g.fillText(this.opts.capLabel || 'capacité', pad.l + 6, y - 5);
    }
    for (const s of this.series) {
      if (s.data.length < 2) continue;
      const col = css(s.color || '--teal');
      // segments séparés par des valeurs nulles (périodes sans relevé)
      const segs = []; let cur = [];
      for (const p of s.data) { if (p[1] == null) { if (cur.length) segs.push(cur); cur = []; } else cur.push(p); }
      if (cur.length) segs.push(cur);
      for (const seg of segs) {
        const path = () => { g.beginPath(); seg.forEach((p, i) => { const x = X(p[0]), y = Y(p[1]); i ? g.lineTo(x, y) : g.moveTo(x, y); }); };
        if (s.fill && seg.length > 1) {
          path(); g.lineTo(X(seg[seg.length - 1][0]), Y(0)); g.lineTo(X(seg[0][0]), Y(0)); g.closePath();
          const gr = g.createLinearGradient(0, pad.t, 0, H - pad.b); gr.addColorStop(0, col + '55'); gr.addColorStop(1, col + '05');
          g.fillStyle = gr; g.fill();
        }
        path();
        g.strokeStyle = col; g.lineWidth = s.width || 2; g.lineJoin = 'round'; if (s.dash) g.setLineDash([3, 3]); g.globalAlpha = s.alpha ?? 1; g.stroke(); g.setLineDash([]); g.globalAlpha = 1;
      }
    }
    if (this.hover != null && this.hover > pad.l) {
      const t = t0 + ((this.hover - pad.l) / (W - pad.l - pad.r)) * (t1 - t0);
      g.strokeStyle = mute; g.globalAlpha = 0.5; g.beginPath(); g.moveTo(this.hover + 0.5, pad.t); g.lineTo(this.hover + 0.5, H - pad.b); g.stroke(); g.globalAlpha = 1;
      const rows = this.series.filter((s) => s.data.length && !s.noTip).map((s) => { let best = null; for (const p of s.data) if (p[1] != null && (!best || Math.abs(p[0] - t) < Math.abs(best[0] - t))) best = p; return { s, p: best }; }).filter((r) => r.p);
      if (rows.length) {
        const d = new Date(rows[0].p[0] * 1000);
        this.tip.innerHTML = `<b>${d.toLocaleString('fr-FR', { weekday: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: t1 - t0 < 3600 ? '2-digit' : undefined })}</b>` + rows.map(({ s, p }) => `<span><i style="background:${css(s.color || '--teal')}"></i>${s.name} <em>${fmtRate(p[1])}</em></span>`).join('');
        this.tip.hidden = false;
        const tw = this.tip.offsetWidth; this.tip.style.left = Math.min(W - tw - 4, Math.max(4, this.hover + 12)) + 'px'; this.tip.style.top = '8px';
        for (const { s, p } of rows) { g.fillStyle = css(s.color || '--teal'); g.beginPath(); g.arc(X(p[0]), Y(p[1] || 0), 3.5, 0, 7); g.fill(); }
      }
    }
  }
}

/** Spectre 2,4 GHz : une cloche par réseau, la tienne en couleur. */
export function drawSpectrum(el, nets, ownChannel, best) {
  let c = el.querySelector('canvas'); if (!c) { c = document.createElement('canvas'); el.append(c); }
  const W = el.clientWidth, H = el.clientHeight, dpr = devicePixelRatio || 1; if (!W || !H) return;
  c.width = W * dpr; c.height = H * dpr; c.style.width = W + 'px'; c.style.height = H + 'px';
  const g = c.getContext('2d'); g.setTransform(dpr, 0, 0, dpr, 0, 0); g.clearRect(0, 0, W, H);
  const pad = { l: 40, r: 10, t: 12, b: 26 };
  const X = (ch) => pad.l + ((ch + 1) / 16) * (W - pad.l - pad.r);
  const Y = (d) => H - pad.b - ((Math.max(-95, Math.min(-25, d)) + 95) / 70) * (H - pad.t - pad.b);
  g.font = '12px ' + css('--sans'); g.fillStyle = css('--mut'); g.strokeStyle = css('--rule'); g.textAlign = 'right';
  for (const v of [-90, -70, -50, -30]) { const y = Math.round(Y(v)) + 0.5; g.beginPath(); g.moveTo(pad.l, y); g.lineTo(W - pad.r, y); g.stroke(); g.fillText(v, pad.l - 6, y + 4); }
  g.textAlign = 'center';
  for (let ch = 1; ch <= 13; ch++) { g.fillStyle = ch === best ? css('--teal') : css('--mut'); g.font = (ch === best ? '700 ' : '') + '12px ' + css('--sans'); g.fillText(ch, X(ch), H - 8); }
  const curve = (n, mine) => {
    const ch = n.channel, d = n.signal ?? -75, half = (n.width || 20) >= 40 ? 4 : 2;
    g.beginPath(); g.moveTo(X(ch - half), Y(-95));
    g.bezierCurveTo(X(ch - half * 0.6), Y(d), X(ch - half * 0.3), Y(d), X(ch), Y(d));
    g.bezierCurveTo(X(ch + half * 0.3), Y(d), X(ch + half * 0.6), Y(d), X(ch + half), Y(-95));
    const col = mine ? css('--teal') : css('--mut');
    g.fillStyle = col + (mine ? '40' : '14'); g.fill(); g.strokeStyle = col; g.lineWidth = mine ? 2.4 : 1.1; g.stroke();
  };
  for (const n of nets) curve(n, false);
  if (ownChannel) curve({ channel: ownChannel.channel, width: ownChannel.width, signal: -40 }, true);
  if (!nets.length) { g.fillStyle = css('--mut'); g.fillText('Aucun réseau voisin relevé par la box', W / 2, H / 2); }
}
