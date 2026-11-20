// Moteur de diagnostic : transforme les données réelles de la box en constats priorisés.
// Chaque constat porte un niveau de confiance : Certain (mesuré), Probable (déduit), Supposition (modèle).
import { fmtRate } from './charts.js';

const pct = (x) => Math.round(x * 100) + ' %';
const list = (a, n = 4) => a.slice(0, n).join(', ') + (a.length > n ? ` et ${a.length - n} autre(s)` : '');
const bandTxt = (b) => (b ? b.replace('.', ',') + ' GHz' : '');
const W24 = [1, 0.9, 0.6, 0.3];
const w = (d) => (d == null ? 0.5 : Math.max(0, Math.min(1.3, (d + 90) / 30)));
export const BLOCKS5 = [
  { id: '36-48', ch: [36, 40, 44, 48], dfs: false }, { id: '52-64', ch: [52, 56, 60, 64], dfs: true },
  { id: '100-112', ch: [100, 104, 108, 112], dfs: true }, { id: '116-128', ch: [116, 120, 124, 128], dfs: true },
  { id: '132-144', ch: [132, 136, 140, 144], dfs: true }, { id: '149-161', ch: [149, 153, 157, 161], dfs: false },
];
export const level = (s) => (s == null ? '—' : s < 0.8 ? 'faible' : s < 2 ? 'modérée' : 'forte');

export function interference(neighbors, aps) {
  const ap24 = aps.find((a) => a.band === '2.4'), ap5 = aps.find((a) => a.band === '5');
  const n24 = neighbors.filter((n) => n.band === '2.4'), n5 = neighbors.filter((n) => n.band === '5');
  const s24 = (c) => n24.reduce((s, n) => { const d = Math.abs(c - n.channel); return s + (d < 4 ? W24[d] * w(n.signal) : 0); }, 0);
  const cand = [1, 6, 11].map((c) => ({ ch: c, score: s24(c) })).sort((a, b) => a.score - b.score);
  const blocks = BLOCKS5.map((b) => ({ ...b, score: n5.filter((n) => b.ch.includes(n.channel)).reduce((s, n) => s + w(n.signal), 0) }));
  const own5 = ap5 && BLOCKS5.find((b) => b.ch.includes(ap5.channel));
  return {
    n24: n24.length, n5: n5.length, strong24: n24.filter((n) => (n.signal ?? -100) > -70).length,
    b24: ap24 ? { own: ap24.channel, width: ap24.width, score: s24(ap24.channel), best: cand[0], offGrid: ![1, 6, 11].includes(ap24.channel), all: Array.from({ length: 13 }, (_, i) => s24(i + 1)) } : null,
    b5: ap5 ? { own: ap5.channel, block: own5?.id, score: own5 ? blocks.find((b) => b.id === own5.id).score : null, blocks, best: [...blocks].sort((a, b) => a.score - b.score || a.dfs - b.dfs)[0] } : null,
  };
}

export function bloat(ping, loaded) {
  if (ping == null || loaded == null) return null;
  const d = Math.max(0, loaded - ping);
  return { delta: Math.round(d), grade: d < 5 ? 'A+' : d < 30 ? 'A' : d < 60 ? 'B' : d < 200 ? 'C' : d < 400 ? 'D' : 'F' };
}

/** ctx: { live, wifi, saturation, top, events, speedtests, status } */
export function diagnose(ctx) {
  const F = [];
  const add = (sev, conf, area, title, detail, action = '', extra = {}) => F.push({ sev, conf, area, title, detail, action, ...extra });
  const L = ctx.live; if (!L || !L.wan) return { score: null, findings: [] };
  const wan = L.wan, hosts = L.hosts || [], aps = L.aps || [];
  const sub = ctx.status?.subscribed_down;

  // ---------- Ligne ----------
  if (wan.state && wan.state !== 'up') add('crit', 'Certain', 'ligne', 'La connexion Internet est coupée', `La box indique l’état « ${wan.state} ».`, 'Vérifie le câble fibre et le voyant de la box, puis consulte l’état du réseau sur l’espace abonné Free.');
  if (sub && wan.bw_down_mbps && wan.bw_down_mbps < sub * 0.9) add('warn', 'Probable', 'ligne', 'La box annonce moins que ton offre', `Capacité annoncée par la box : ${fmtRate(wan.bw_down_mbps)}, pour ${fmtRate(sub)} souscrits.`, 'Redémarre la box ; si l’écart persiste, contacte l’assistance Free.');
  if (L.ftth?.rx_dbm != null) {
    const rx = L.ftth.rx_dbm;
    if (rx < -27) add('warn', 'Probable', 'ligne', `Signal optique faible (${rx.toFixed(1).replace('.', ',')} dBm)`, 'La puissance reçue sur la fibre est proche de la limite de fonctionnement (environ −28 dBm) : risque de coupures.', 'Vérifie que la jarretière n’est pas pliée et que les connecteurs sont propres et bien enfoncés ; sinon signale-le à Free.');
    else if (rx <= -8) add('ok', 'Probable', 'ligne', `Signal optique correct (${rx.toFixed(1).replace('.', ',')} dBm)`, 'La fibre reçoit une puissance dans la plage normale.');
  }
  const S = ctx.saturation;
  if (S && S.observed_minutes > 60) {
    const eps = S.episodes || [];
    const blame = {};
    for (const e of eps) { const lead = (e.top?.[0]?.down || 0) + (e.top?.[0]?.up || 0); for (const t of e.top || []) if (t.down + t.up >= lead * 0.15) blame[t.name] = (blame[t.name] || 0) + t.down + t.up; }
    const tot = Object.values(blame).reduce((a, b) => a + b, 0);
    const culprits = Object.entries(blame).sort((a, b) => b[1] - a[1]).slice(0, 3).map(([n, v]) => `${n} (${pct(v / (tot || 1))})`);
    const downEps = eps.filter((e) => e.down).length, upEps = eps.filter((e) => e.up).length;
    if (S.saturated_minutes >= 10) add(S.saturated_minutes > 120 ? 'crit' : 'warn', 'Certain', 'ligne', `La ligne a saturé ${S.saturated_minutes} min sur la période`,
      `${eps.length} épisode(s) au-dessus de 90 % de la capacité (${downEps} en réception, ${upEps} en envoi). Principaux responsables : ${culprits.join(', ') || 'non identifiés'}.`,
      'Pendant une saturation, tout le foyer ralentit et la latence explose. Programme les mises à jour de console et les sauvegardes la nuit, ou limite leur débit dans leurs réglages.', { link: '#conso' });
    else add('ok', 'Certain', 'ligne', 'La ligne n’a pas saturé', `Sur ${Math.round(S.observed_minutes / 60)} h observées, le débit n’a jamais dépassé 90 % de la capacité plus de ${S.saturated_minutes} min.`);
  }

  // ---------- Appareils ----------
  const wifi = hosts.filter((h) => h.conn === 'wifi' && h.signal != null);
  const veryWeak = wifi.filter((h) => h.signal < -80), weak = wifi.filter((h) => h.signal >= -80 && h.signal < -72);
  if (veryWeak.length) add('crit', 'Certain', 'appareils', `Signal très faible : ${list(veryWeak.map((h) => h.name))}`, `En dessous de −80 dBm, la connexion devient instable et occupe longtemps l’antenne (${veryWeak.map((h) => `${h.name} ${h.signal} dBm`).join(', ')}).`, 'Rapproche l’appareil ou la box, ajoute un répéteur Freebox à mi-chemin, ou relie l’appareil par câble/CPL.', { hosts: veryWeak.map((h) => h.mac) });
  if (weak.length) add('warn', 'Certain', 'appareils', `Signal faible : ${list(weak.map((h) => h.name))}`, `Entre −72 et −80 dBm, le débit chute fortement (${weak.map((h) => `${h.name} ${h.signal} dBm`).join(', ')}).`, 'Dégage la box (hors meuble, en hauteur) ou rapproche ces appareils.', { hosts: weak.map((h) => h.mac) });
  const slowEth = hosts.filter((h) => h.conn === 'ethernet' && h.eth_speed && h.eth_speed <= 100);
  if (slowEth.length) add('warn', 'Certain', 'appareils', `Lien Ethernet bridé à 100 Mb/s : ${list(slowEth.map((h) => h.name))}`, 'La box négocie seulement 100 Mb/s avec cet appareil : câble abîmé, câble à 4 fils, ou carte réseau ancienne.', 'Change le câble pour un Cat 5e ou Cat 6 (8 fils) et vérifie le port. Gain attendu : ×10.', { hosts: slowEth.map((h) => h.mac) });
  const close24 = hosts.filter((h) => h.conn === 'wifi' && h.band === '2.4' && h.signal > -60 && !['multimedia_device', 'ip_camera', 'networking_device', 'other', 'printer'].includes(h.kind) && h.standard !== 'Wi-Fi ancien' && h.standard !== 'Wi-Fi 4');
  if (close24.length) add('info', 'Supposition', 'appareils', `Pourrait passer en 5 GHz : ${list(close24.map((h) => h.name))}`, 'Ces appareils captent bien la box mais restent en 2,4 GHz, la bande la plus lente et la plus encombrée.', 'Sur l’appareil, oublie le réseau puis reconnecte-toi près de la box ; ou sépare les noms de réseau 2,4/5 GHz dans Freebox OS.', { hosts: close24.map((h) => h.mac) });
  const legacy = hosts.filter((h) => h.conn === 'wifi' && ['Wi-Fi 4', 'Wi-Fi ancien'].includes(h.standard) && h.band === '5');
  if (legacy.length) add('info', 'Probable', 'appareils', `Appareils Wi-Fi anciens en 5 GHz : ${list(legacy.map((h) => h.name))}`, 'Ils ralentissent l’efficacité de la bande pour les appareils récents.');
  // temps d'antenne estimé
  for (const ap of aps) {
    const st = hosts.filter((h) => h.ap_id === ap.id && h.phy_down && h.down_mbps != null);
    const air = st.map((h) => ({ h, a: ((h.down_mbps || 0) + (h.up_mbps || 0)) / (h.phy_down * 0.6) })).filter((x) => x.a > 0.02).sort((a, b) => b.a - a.a);
    const sum = air.reduce((s, x) => s + x.a, 0);
    if (sum > 0.5) add(sum > 0.8 ? 'crit' : 'warn', 'Supposition', 'wifi', `Borne ${bandTxt(ap.band)} : antenne occupée à ${pct(Math.min(1, sum))} par tes appareils`, `Le Wi-Fi ne parle qu’à un appareil à la fois. Plus gros consommateurs de temps d’antenne : ${air.slice(0, 3).map((x) => `${x.h.name} (${pct(Math.min(1, x.a))})`).join(', ')}.`, 'Câble les appareils fixes gourmands et rapproche ceux qui ont un débit radio faible.');
  }
  const ev = (ctx.events || []).filter((e) => e.kind === 'new_device');
  if (ev.length) add('info', 'Certain', 'appareils', `${ev.length} nouvel(s) appareil(s) ces 7 derniers jours`, ev.slice(0, 5).map((e) => e.detail).join(' · '), 'Vérifie que tu les reconnais. Un appareil inconnu ? Change la clé Wi-Fi dans Freebox OS.', { link: '#appareils' });
  const top = ctx.top || [];
  const totTop = top.reduce((s, t) => s + t.down + t.up, 0);
  if (top.length && totTop > 1e9) add('info', 'Certain', 'appareils', `Plus gros consommateur : ${top[0].name}`, `${pct((top[0].down + top[0].up) / totTop)} du volume échangé sur la période (${(top[0].down / 1e9).toFixed(1).replace('.', ',')} Go reçus).`, '', { link: '#conso' });

  // ---------- Wi-Fi ----------
  for (const ap of aps) {
    if (ap.state && ap.state !== 'active') add('warn', 'Certain', 'wifi', `Borne ${bandTxt(ap.band)} inactive (${ap.state})`, 'Cette bande n’émet pas actuellement.', 'Vérifie la planification Wi-Fi et l’état de la borne dans Freebox OS.');
    if (ap.busy != null) {
      if (ap.busy > 70) add('crit', 'Certain', 'wifi', `Canal ${ap.channel} (${bandTxt(ap.band)}) saturé à ${Math.round(ap.busy)} %`, 'La box mesure que le canal est occupé presque en permanence, par tes appareils et ceux des voisins.', ap.band === '2.4' ? 'Change de canal (voir ci-dessous) et réserve le 2,4 GHz aux objets connectés.' : 'Choisis un bloc de canaux moins occupé.');
      else if (ap.busy > 50) add('warn', 'Certain', 'wifi', `Canal ${ap.channel} (${bandTxt(ap.band)}) chargé à ${Math.round(ap.busy)} %`, 'Au-delà de 50 % d’occupation, les débits et la latence se dégradent.', 'Envisage un canal plus calme.');
    }
  }
  if (ctx.wifi?.neighbors) {
    const I = interference(ctx.wifi.neighbors, aps);
    if (I.b24) {
      if (I.b24.width >= 40 && I.n24 >= 5) add('warn', 'Probable', 'wifi', 'Largeur de canal 40 MHz en 2,4 GHz', `Avec ${I.n24} réseaux voisins, 40 MHz chevauche presque toute la bande et multiplie les interférences.`, 'Dans Freebox OS > Wi-Fi > Configuration, règle la largeur 2,4 GHz sur 20 MHz.');
      if (I.b24.offGrid) add('warn', 'Probable', 'wifi', `Canal 2,4 GHz ${I.b24.own} : à cheval sur deux canaux`, 'Seuls les canaux 1, 6 et 11 ne se chevauchent pas.', `Fixe le canal 2,4 GHz sur ${I.b24.best.ch}.`);
      else if (I.b24.score > 0.8 && I.b24.best.ch !== I.b24.own && I.b24.best.score < I.b24.score * 0.6) add('warn', 'Probable', 'wifi', `Canal 2,4 GHz ${I.b24.own} encombré par les voisins`, `Interférence ${level(I.b24.score)} ; le canal ${I.b24.best.ch} est nettement plus calme (${level(I.b24.best.score)}).`, `Fixe le canal 2,4 GHz sur ${I.b24.best.ch} dans Freebox OS.`, { link: '#wifi' });
      else if (I.n24) add('ok', 'Probable', 'wifi', `Canal 2,4 GHz ${I.b24.own} : bon choix`, `Interférence ${level(I.b24.score)} face aux ${I.n24} réseaux voisins.`);
    }
    if (I.b5?.block && I.b5.score > 1 && I.b5.best.id !== I.b5.block && I.b5.best.score < I.b5.score * 0.5) add('warn', 'Probable', 'wifi', `5 GHz : le bloc ${I.b5.block} est partagé avec les voisins`, `Le bloc ${I.b5.best.id} est plus libre${I.b5.best.dfs ? ' (canaux DFS : la box peut en changer si un radar est détecté)' : ''}.`, `Choisis un canal du bloc ${I.b5.best.id}.`, { link: '#wifi' });
    if (I.b5?.own >= 52 && I.b5?.own <= 144) add('info', 'Certain', 'wifi', `Canal 5 GHz ${I.b5.own} : canal DFS`, 'Si un radar météo est détecté, la box doit changer de canal et le Wi-Fi 5 GHz coupe une minute.', 'Si tu constates des coupures brèves et inexpliquées, fixe un canal entre 36 et 48.');
    if (I.strong24 >= 8) add('info', 'Probable', 'wifi', `${I.strong24} réseaux voisins forts en 2,4 GHz`, 'Environnement dense : le 2,4 GHz y restera médiocre quel que soit le canal.', 'Privilégie le 5 GHz pour tout ce qui le permet.');
  }

  // ---------- Box ----------
  const hot = (L.box?.temps || []).filter((t) => t.value > 80);
  if (hot.length) add('warn', 'Certain', 'box', 'La box chauffe', hot.map((t) => `${t.name} : ${Math.round(t.value)} °C`).join(', '), 'Dégage les aérations, ne la pose pas dans un meuble fermé.');

  // ---------- Mesures ----------
  const lastTest = (ctx.speedtests || [])[0];
  if (lastTest) {
    const b = bloat(lastTest.ping, lastTest.loadedPing);
    if (b && ['C', 'D', 'F'].includes(b.grade)) add(b.grade === 'C' ? 'warn' : 'crit', 'Certain', 'ligne', `Latence en charge : note ${b.grade}`, `+${b.delta} ms quand la ligne est chargée : la visio et les jeux décrochent dès qu’un téléchargement démarre.`, 'Limite les téléchargements lourds pendant les appels ; un routeur avec SQM derrière la box règle ce problème.');
    if (lastTest.boxDown && lastTest.down && lastTest.boxDown > lastTest.down * 1.5 + 20) add('info', 'Certain', 'ligne', 'D’autres appareils consommaient pendant ta dernière mesure', `La box a vu ${fmtRate(lastTest.boxDown)} passer, ton test n’en a obtenu que ${fmtRate(lastTest.down)}.`, 'Refais la mesure quand le réseau est calme pour connaître le débit réel.');
  }

  // ---------- Accès ----------
  const errs = Object.entries(L.errors || {});
  const rights = errs.filter(([, e]) => e.startsWith('insufficient_rights'));
  if (rights.length) add('info', 'Certain', 'box', 'Certaines données ne sont pas accessibles', `La box refuse : ${rights.map(([p]) => p).join(', ')}.`, 'Dans Freebox OS > Paramètres de la Freebox > Gestion des accès > Applications, ouvre Ondes et coche les droits manquants.');
  if (!sub) add('info', 'Certain', 'ligne', 'Débit souscrit non renseigné', 'Le diagnostic ne peut pas comparer la ligne à ce que tu paies.', 'Renseigne-le dans Réglages.', { link: '#reglages' });

  const order = { crit: 0, warn: 1, info: 2, ok: 3 };
  F.sort((a, b) => order[a.sev] - order[b.sev]);
  const c = F.filter((f) => f.sev === 'crit').length, wn = F.filter((f) => f.sev === 'warn').length;
  return { score: Math.round(100 * 0.8 ** c * 0.91 ** wn), findings: F, crit: c, warn: wn };
}

export function report(D, ctx) {
  const L = ctx.live; const tag = { crit: 'CRITIQUE', warn: 'À CORRIGER', info: 'INFO', ok: 'OK' };
  const out = [`DIAGNOSTIC ONDES — ${new Date().toLocaleString('fr-FR')}`, `${L.box?.model || 'Freebox'} · API ${L.box?.api || '?'} · ligne ${L.wan?.media || '?'} ${fmtRate(L.wan?.bw_down_mbps)} / ${fmtRate(L.wan?.bw_up_mbps)}`, `Score : ${D.score}/100`, ''];
  for (const f of D.findings) { out.push(`[${tag[f.sev]}] [${f.conf}] ${f.title}`, '   ' + f.detail); if (f.action) out.push('   → ' + f.action); }
  return out.join('\n');
}
