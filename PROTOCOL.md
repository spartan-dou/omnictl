# Protocole HID du ROG Strix Scope II 96 Wireless

Notes d'interopérabilité. Sources, recoupées entre elles :

- **[M]** mesuré sur le matériel (firmware clavier 8.00.04, récepteur 7.00.08) ;
- **[O]** OpenRGB, `AsusAuraKeyboardController` ;
- **[P]** lecture du pilote ASUS du clavier (nom de code interne « X901 ») et des modules
  d'Armoury Crate. Non confirmé par une mesure sauf mention [M].

## Transport

| Mode | VID:PID | Collection | Rapport de sortie |
|---|---|---|---|
| Câble USB | `0B05:1AAE` | usage page `FF00` | report ID `00` + 64 octets |
| Dongle ROG Omni | `0B05:1ACE` | usage page `FF00` | report ID `02` + 63 octets |

Les rapports d'entrée ont la même forme. Dans la suite, l'octet 0 est le premier octet après
le report ID. Pas de somme de contrôle ; les octets inutilisés sont à zéro. [M]

- Le clavier répond en reprenant les deux premiers octets de la commande, sauf `C0 81` qui est
  acquitté par `C0 82 n`. [M]
- `FF AA` signale un refus. Via le dongle, c'est aussi la réponse quand le clavier est en
  veille ou éteint. [M]
- Autres collections du dongle : `FF01` (report ID 3) est le canal d'un second périphérique
  appairé (souris), `FF02` (report ID 1) répond pour le dongle seul. [M]
- En mode 2,4 GHz, le clavier ne s'énumère plus en USB même si le câble reste branché. [M]

## Lecture d'état

| Commande | Réponse |
|---|---|
| `12 03` | octet 4 : liaison — `00` filaire, `01` 2,4 GHz, `02` Bluetooth, `FF` clavier injoignable [P] (`00` et `FF` vus [M]) |
| `12 00` | octets 4-6 : version (affichée octet 6.5.4) ; octet 9 : effet actif [M] ; octet 10 : profil actif [P]. Via le dongle, cette version est celle du récepteur et celle du clavier est aux octets 11-13 [M] |
| `12 01` | octet 5 : batterie en % [M] ; octet 6 : index du délai de veille ; octet 7 : économie d'énergie ; octet 8 : `01` = en charge ; octet 9 : seuil d'alerte batterie en % [P] ; octets 11-12 : tension en mV, petit-boutiste [M] |
| `12 12` | octets 4-5 : disposition (`02 05` = ISO, français) [M] |
| `12 07` | octet 4 : éclairage dynamique Windows activé [P] |
| `22 01` | octet 4 : fonction de la molette — 0 volume, 1 piste multimédia, 2 luminosité, 3 personnalisé [P] |

## Éclairage

### Effets intégrés — `51 2C`

```
[0]=51 [1]=2C [2]=mode [3]=paquets restants [4]=vitesse [5]=luminosité (0-100)
[6]=options [7]=direction ou FF [8]=FF [9..]=couleurs RVB
```

| Mode | Effet | Couleurs | Vitesses rapide / moyenne / lente [P] |
|---|---|---|---|
| 0 | statique | 1 | `FF` |
| 1 | respiration | 1-2 | `0F` `1E` `2A` |
| 2 | cycle | — | `12` `1F` `3E` |
| 3 | réactif | 1-2 | `08` `0F` `16` |
| 4 | vague | dégradé | `23` `3E` `55` |
| 5 | ondulation | dégradé | `50` `64` `78` |
| 6 | nuit étoilée | 1-3 | `0A` `0F` `19` |
| 7 | sables mouvants | 6 | `0C` `0F` `14` |
| 8 | courant | 1-3 | `07` `0A` `0C` |
| 9 | gouttes | 1-3 | `0E` `14` `1B` |

- Octet 6 : `01` = couleurs aléatoires, `10` = une seconde couleur accompagne la première. [O][P]
- Direction (vague, sables mouvants) : `04` gauche, `00` droite, `06` haut, `02` bas,
  `08` horizontale, `01` verticale. [O]
- Vague et ondulation, trois paquets (octet 3 = 2, 1, 0) : le premier porte `[8]=02`,
  `[9]=nombre de couleurs` puis deux quadruplets (position %, R, V, B) ; le deuxième quatre
  quadruplets à partir de l'octet 4 ; le troisième un quadruplet. [O][P]
- Sables mouvants, deux paquets (octet 3 = 1, 0) : trois couleurs à partir de l'octet 9, puis
  trois à partir de l'octet 4. [O][P]
- Il n'existe pas de commande « luminosité seule » : il faut réémettre l'effet. [P]

Statique et respiration ont été vérifiés visuellement [M] ; les autres effets sont construits
d'après [O] et [P].

### Mode direct — `C0 81`

```
C0 81 n 00  puis n × (id_led, R, V, B)
```

Jusqu'à 15 LED par paquet en USB, 14 via le dongle. Aucune commande d'entrée dans ce mode ;
on en sort en envoyant un `51 2C`. Les couleurs sont volatiles. [M]

`id_led = colonne × 8 + ligne` dans la matrice du clavier (19 colonnes × 6 lignes) ; la table
complète est dans `src/keys.rs`. [O][M]

Le pilote ASUS ne contient **aucune commande pour enregistrer une couleur par touche** dans le
clavier. [P]

### Enregistrement — `50 55`

Enregistre l'effet courant dans le profil actif. [O][P]

## Réglages

Tous de la forme `51 xx 00 00 valeur`, acquittés par `51 xx`. [P]

| Commande | Réglage | Valeurs |
|---|---|---|
| `51 00` | profil actif | 0 = profil par défaut, 1 à 6 |
| `51 38` | délai de veille | index 0-5 : 1, 2, 3, 5, 10 min, jamais. Vérifié [M] ; « jamais » se relit `FF` dans `12 01` |
| `51 39` | économie d'énergie | 0 non, 1 éteindre l'éclairage, 2 réduire la luminosité |
| `51 3A` | alerte batterie faible | pourcentage |
| `51 31` | fréquence de scrutation | codage non établi |
| `73 00 00 00 v` | éclairage dynamique Windows | 0 / 1 |

## Identifié mais non implémenté

- Remappage : `51 20 src_lo src_hi dst_lo dst_hi`. [P]
- Molette : `71 00 00 00 a b` (activer une fonction), `71 01 00 00 m k1 k2` (fonction
  personnalisée). [P]
- Macros : `51 18 …` puis paquets `53 …`. [P]
- Réinitialisation d'usine : `B0 00 00 00 "reset"`. Volontairement non exposée. [P]
- Bluetooth (PID `1AB2`) : mêmes octets de commande sur 20 octets, par GATT ; effets intégrés
  seulement, pas de mode direct. [P]
