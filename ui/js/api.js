// Appel au moteur : via Tauri dans l'application, via le pont HTTP en développement.
export async function call(cmd, args = null) {
  const T = window.__TAURI__;
  if (T?.core?.invoke) return T.core.invoke('api', { cmd, args });
  const r = await fetch('/invoke', { method: 'POST', body: JSON.stringify({ cmd, args }) });
  const j = await r.json();
  if (j.error) throw new Error(j.error);
  return j.result;
}
export const inApp = () => !!window.__TAURI__?.core;
// Traduit les codes d'erreur du moteur et de la Freebox en phrases.
export function explain(e) {
  const s = String(e?.message || e || '');
  const code = s.split(':')[0];
  const M = {
    unreachable: 'La Freebox ne répond pas. Ce PC est-il bien connecté au réseau de la Freebox (Wi-Fi ou câble) ?',
    not_freebox: 'Cette adresse ne répond pas comme une Freebox.',
    new_apps_denied: 'La box refuse les nouvelles applications. Dans Freebox OS > Paramètres > Gestion des accès, active « Permettre les nouvelles demandes d’association ».',
    apps_denied: 'L’accès des applications est désactivé dans Freebox OS (Gestion des accès).',
    denied_from_external_ip: 'L’association doit se faire depuis le réseau local de la Freebox.',
    invalid_token: 'L’autorisation a été révoquée dans Freebox OS. Il faut associer Ondes à nouveau.',
    insufficient_rights: 'Ondes n’a pas le droit d’accéder à cette fonction. Accorde le droit dans Freebox OS > Gestion des accès > Applications.',
    ratelimited: 'Trop de tentatives : la box bloque temporairement. Réessaie dans quelques minutes.',
    no_token: 'Aucune autorisation enregistrée : associe la Freebox.',
    windows_only: 'Disponible uniquement sous Windows.',
    no_store: 'Historique indisponible pour le moment.',
  };
  return M[code] || s.replace(/^[a-z_]+: /, '');
}
