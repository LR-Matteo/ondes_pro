# Ondes : diagnostic du réseau domestique Freebox (Windows)

Ondes est une application Windows qui se connecte à ta Freebox par l’accès officiel prévu par Free, relève en continu ce qui se passe sur ton réseau, et te dit en clair ce qui ralentit internet chez toi.

## Ce que fait l’application

| Écran | Contenu |
|---|---|
| **Vue d’ensemble** | Débit de la ligne en direct comparé à sa capacité, score de santé, appareils qui consomment en ce moment et depuis minuit, signal fibre, occupation des canaux Wi-Fi. |
| **Appareils** | Tous les appareils vus par la box : connexion (Wi-Fi 2,4/5/6 GHz ou Ethernet et vitesse du lien), signal en dBm, débit radio, débit en direct, volume du jour, nouveaux appareils. Un clic ouvre la fiche détaillée avec le direct et les dernières 24 h. |
| **Consommation** | Historique de 1 h à 30 jours, classement de qui a consommé, épisodes de saturation avec leurs responsables, carte des heures chargées, événements (nouvel appareil, etc.). |
| **Wi-Fi** | Bornes de la box, taux d’occupation de chaque canal mesuré par la box, spectre 2,4 GHz des réseaux voisins, blocs 5 GHz, canal recommandé, liaison Wi-Fi de ce PC (signal, débit, norme). |
| **Mesure de débit** | Test de débit (latence, gigue, latence en charge, réception, envoi), comparé à ce que la box voit passer au même moment. |
| **Diagnostic** | Constats triés par gravité, chacun avec un niveau de confiance (Certain, Probable, Supposition) et une action concrète. Rapport copiable pour l’assistance Free. |

La collecte continue fenêtre fermée : Ondes reste dans la zone de notification et peut démarrer avec Windows.

## Installer

1. Récupère `Ondes_1.0.0_x64-setup.exe`, en suivant « Obtenir l’installateur » ci-dessous.
2. Lance-le. L’application n’est pas signée : Windows SmartScreen affiche « Windows a protégé votre ordinateur ». Clique sur **Informations complémentaires**, puis **Exécuter quand même**.
3. Au premier lancement, clique sur **Demander l’autorisation**, puis valide avec la flèche **›** sur l’écran de la Freebox Server.

Si la box refuse tout de suite : dans Freebox OS, va dans **Paramètres de la Freebox > Gestion des accès** et active **Permettre les nouvelles demandes d’association**.

Ondes ne modifie aucun réglage de la box. Pour voir l’historique de débit enregistré par la box elle-même (avant l’installation d’Ondes), accorde le droit « Modification des réglages » dans **Gestion des accès > Applications > Ondes**. Ce droit est facultatif.

## Obtenir l’installateur

**Option A : GitHub, sans rien installer.** Crée un dépôt GitHub, envoie-y ce dossier, et la construction démarre toute seule (fichier `.github/workflows/windows.yml`). Environ 10 minutes plus tard, l’installateur est téléchargeable dans l’onglet **Actions**, sous **Ondes-Windows**. Un tag `v1.0.0` publie aussi une version téléchargeable.

**Option B : sur ton PC.**
1. Installe [Rust](https://rustup.rs), [Node.js 22](https://nodejs.org) et les « Outils de génération Visual Studio » avec la charge de travail **Développement Desktop en C++**.
2. Dans ce dossier, lance :
   ```
   npm ci
   npx tauri build
   ```
3. L’installateur est créé dans `src-tauri\target\release\bundle\nsis\`.

Pour développer : `npx tauri dev` ouvre l’application avec rechargement automatique.

## Partager l’application

Envoie le fichier `-setup.exe`. Chaque personne l’installe et associe **sa propre** Freebox. Aucune donnée ne quitte le PC : l’historique est dans `%APPDATA%\fr.ondes.diagnostic`, et la clé d’accès à la box est rangée dans le Gestionnaire d’identifiants de Windows.

## Si quelque chose s’affiche mal avec ta box

La documentation publique de Free s’arrête à la version 4 de l’API, alors que les box actuelles sont en version 14 ou plus. Ondes lit les deux formats, mais certains champs récents peuvent différer. Dans **Réglages > Exporter les données brutes**, Ondes enregistre les réponses de ta box dans un fichier de `Documents`. Ce fichier permet de corriger la lecture précisément.

Point à vérifier en priorité sur une vraie box : le sens des compteurs Wi-Fi. Ondes suppose que `tx_bytes` est envoyé par la box vers l’appareil, ce qui correspond au téléchargement. Si les débits « ↓ » et « ↑ » d’un appareil paraissent inversés, c’est cette hypothèse qu’il faut corriger dans `src-tauri/src/normalize.rs` (fonction `stations`).

## Architecture

```
ui/                     interface (HTML, CSS, JavaScript sans dépendance)
  js/app.js             écrans, navigation, rafraîchissement
  js/diag.js            moteur de diagnostic (règles et niveaux de confiance)
  js/charts.js          graphiques canvas
  js/speedtest.js       test de débit (serveurs Cloudflare)
src-tauri/              moteur natif (Rust, Tauri 2)
  src/freebox.rs        API Freebox : association, HMAC-SHA1, session, renouvellement
  src/engine.rs         collecte continue, calcul des débits par appareil, commandes
  src/normalize.rs      lecture tolérante des réponses de la box
  src/store.rs          historique SQLite (agrégats par minute, saturations, heatmap)
  src/demo.rs           Freebox simulée (mode démo)
  src/winnet.rs         Wi-Fi de ce PC via netsh (français et anglais)
mock/freebox-mock.mjs   fausse Freebox HTTP pour tester l’association sans box
```

**Tester sans box :**
- Le **mode démo** (bouton sur l’écran d’accueil) simule un foyer complet : streaming le soir, téléchargements de console, sauvegarde nocturne, caméra au signal faible, câble Ethernet bridé.
- **Tests du moteur :** `cargo test --manifest-path src-tauri/Cargo.toml --lib`.
- **Association contre la fausse box :** `node mock/freebox-mock.mjs 8090`, puis indique `http://127.0.0.1:8090` comme adresse de la box sur l’écran d’accueil.
- **Interface dans un navigateur :** `npm run bridge`, puis ouvre http://127.0.0.1:1430.

## Limites connues

- Les débits par appareil incluent le trafic local : une copie de fichiers vers un NAS compte, alors qu’elle ne passe pas par internet.
- L’historique par appareil n’existe que pendant qu’Ondes tourne. L’historique global de la ligne peut être complété depuis la box.
- Le « temps d’antenne » est une estimation (débit consommé rapporté au débit radio), marquée « Supposition ».
- Lister les réseaux vus par ce PC exige, sous Windows 11, d’autoriser la localisation. Les réseaux vus par la box restent toujours disponibles.
