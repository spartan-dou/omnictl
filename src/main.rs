use scope96::link::{self, Error, Link, Via};
use scope96::proto::{self, Direction, Effect, Keyboard, Mode, Rgb, Speed};
use scope96::{keys, mcp, store};

const USAGE: &str = "\
scope96 — contrôle du clavier ASUS ROG Strix Scope II 96 Wireless

UTILISATION
  scope96 [--usb|--dongle] [-v] <commande> [arguments]

COMMANDES
  info                        connexion, firmware, batterie, réglages
  static <couleur>             couleur fixe sur tout le clavier
  effect <mode> [couleurs...]  effet géré par le clavier (voir MODES)
  off                          éteint le rétroéclairage
  save                         enregistre l'effet courant dans le clavier
  keys <touche>=<couleur>...   couleurs touche par touche (tenues par l'ordinateur)
  apply                        remet en place les dernières couleurs touche par touche
  list                         liste les noms de touches et de groupes
  profile <0-6>                bascule sur un profil embarqué (0 = par défaut)
  sleep <1|2|3|5|10|never>     délai de mise en veille, en minutes
  saving <off|lights-off|dim>  économie d'énergie sur batterie
  low-battery <0-100>          seuil d'alerte batterie faible, en %
  wdl <on|off>                 éclairage dynamique Windows (pilotage par Windows 11)
  raw <octets hexa...>         envoie une commande brute et affiche la réponse
  mcp                          serveur MCP sur stdin/stdout (pour Claude et autres clients)

OPTIONS de static / effect
  --brightness <0-100>         luminosité en % (défaut 100)
  --speed <slow|medium|fast>   vitesse (ou octet brut 0-255, plus petit = plus rapide)
  --direction <left|right|up|down|horizontal|vertical>
  --random                     couleurs aléatoires
  --save                       enregistre aussi dans le clavier

OPTION de keys
  --base <couleur>             couleur de toutes les autres touches

COULEURS  #rrggbb, rrggbb, #rgb, ou un nom : red/rouge, green/vert, blue/bleu, white/blanc,
          yellow/jaune, cyan, magenta, orange, purple/violet, pink/rose, off/noir

EXEMPLES
  scope96 static 00a0ff --brightness 60 --save
  scope96 effect breathing rouge bleu --speed slow
  scope96 effect wave --direction right
  scope96 keys --base 101010 zqsd=rouge esc=blanc space=cyan arrows=orange
";

fn main() {
    if let Err(e) = run() {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

/// Arguments séparés en positionnels et options (`--nom valeur` ou drapeau seul).
struct Args {
    pos: Vec<String>,
    opts: Vec<(String, Option<String>)>,
}

const FLAGS: &[&str] = &[
    "--usb", "--dongle", "-v", "--verbose", "--random", "--save", "--help", "-h",
];

impl Args {
    fn parse() -> Result<Args, Error> {
        let mut pos = Vec::new();
        let mut opts = Vec::new();
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            if FLAGS.contains(&a.as_str()) {
                opts.push((a, None));
            } else if a.starts_with("--") {
                let v = it
                    .next()
                    .ok_or_else(|| Error::Msg(format!("valeur manquante pour {a}")))?;
                opts.push((a, Some(v)));
            } else {
                pos.push(a);
            }
        }
        Ok(Args { pos, opts })
    }

    fn flag(&self, name: &str) -> bool {
        self.opts.iter().any(|(n, _)| n == name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.opts
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.as_deref())
    }

    /// Refuse les options que la commande ne connaît pas, plutôt que de les ignorer.
    fn allow(&self, allowed: &[&str]) -> Result<(), Error> {
        const GLOBAL: &[&str] = &["--usb", "--dongle", "-v", "--verbose"];
        match self
            .opts
            .iter()
            .find(|(n, _)| !GLOBAL.contains(&n.as_str()) && !allowed.contains(&n.as_str()))
        {
            Some((n, _)) => Err(Error::Msg(format!("option {n} inconnue pour cette commande"))),
            None => Ok(()),
        }
    }
}

fn msg<T>(text: impl Into<String>) -> Result<T, Error> {
    Err(Error::Msg(text.into()))
}

fn color(s: &str) -> Result<Rgb, Error> {
    Rgb::parse(s).ok_or_else(|| Error::Msg(format!("couleur invalide : « {s} »")))
}

fn open(args: &Args) -> Result<Keyboard, Error> {
    let prefer = match (args.flag("--usb"), args.flag("--dongle")) {
        (true, true) => return msg("--usb et --dongle sont exclusifs"),
        (true, false) => Some(Via::Usb),
        (false, true) => Some(Via::Dongle),
        _ => None,
    };
    let mut link = Link::open(prefer)?;
    link.verbose = args.flag("-v") || args.flag("--verbose");
    Ok(Keyboard::new(link))
}

/// Construit l'effet à partir des options communes à `static` et `effect`.
fn effect_from(args: &Args, mode: Mode, colors: &[String]) -> Result<Effect, Error> {
    let mut e = Effect::new(mode);
    for c in colors {
        e.colors.push(color(c)?);
    }
    if let Some(b) = args.value("--brightness") {
        e.brightness = match b.parse() {
            Ok(b) if b <= proto::BRIGHTNESS_MAX => b,
            _ => return msg("--brightness : 0 à 100"),
        };
    }
    if let Some(s) = args.value("--speed") {
        e.speed = match Speed::parse(s) {
            Some(s) => s,
            None => return msg("--speed : slow, medium, fast ou 0-255"),
        };
    }
    if let Some(d) = args.value("--direction") {
        e.direction = match Direction::parse(d) {
            Some(d) => d,
            None => return msg(format!("direction invalide : « {d} »")),
        };
    }
    e.random = args.flag("--random");
    Ok(e)
}

/// L'unique argument attendu par une commande de réglage.
fn one<'a>(rest: &'a [String], usage: &str) -> Result<&'a str, Error> {
    match rest {
        [v] => Ok(v.as_str()),
        _ => msg(format!("utilisation : scope96 {usage}")),
    }
}

const EFFECT_OPTS: &[&str] = &["--brightness", "--speed", "--direction", "--random", "--save"];

fn run() -> Result<(), Error> {
    let args = Args::parse()?;
    if args.flag("--help") || args.flag("-h") || args.pos.first().is_none_or(|c| c == "help") {
        print!("{USAGE}");
        println!("\nMODES");
        for m in proto::MODES {
            let colors = match m.colors {
                (0, 0) => "sans couleur".to_string(),
                (a, b) if a == b => format!("{a} couleur"),
                (0, b) => format!("0 à {b} couleurs (aucune = arc-en-ciel)"),
                (a, b) => format!("{a} à {b} couleurs"),
            };
            println!("  {:<10} {} — {}", m.name, m.label, colors);
        }
        return Ok(());
    }
    let cmd = args.pos.first().map_or("help", String::as_str);
    let rest = args.pos.get(1..).unwrap_or(&[]);

    match cmd {
        "mcp" => {
            args.allow(&[])?;
            mcp::run()?;
        }
        "info" => {
            args.allow(&[])?;
            let mut kb = open(&args)?;
            let st = kb.status()?;
            println!("transport        : {}", kb.link.via.label());
            println!("liaison clavier  : {}", st.connection.label());
            println!("firmware         : {}", st.firmware);
            if let Some(r) = &st.receiver_firmware {
                println!("firmware dongle  : {r}");
            }
            if st.connection == proto::Connection::Unreachable {
                return Ok(()); // le dongle répond seul : rien d'autre à lire
            }
            println!("profil actif     : {}", st.profile);
            let effect = Mode::from_code(st.effect).map_or("inconnu", |m| m.info().label);
            println!("effet actif      : {effect}");
            let pw = kb.power()?;
            let charging = if pw.charging { ", en charge" } else { "" };
            println!("batterie         : {} % ({} mV{charging})", pw.percent, pw.millivolts);
            let pick = |list: &[&'static str], i: u8| list.get(i as usize).copied().unwrap_or("?");
            println!("mise en veille   : {}", pick(&proto::SLEEP_CHOICES, pw.sleep));
            println!("économie énergie : {}", pick(&proto::SAVING_CHOICES, pw.saving));
            println!("alerte batterie  : {} %", pw.low_alert);
            println!("molette          : {}", pick(&proto::LEVER_CHOICES, kb.lever()?));
            let wdl = if kb.wdl()? { "activé" } else { "désactivé" };
            println!("éclairage dynamique Windows : {wdl}");
        }
        "static" => {
            args.allow(EFFECT_OPTS)?;
            one(rest, "static <couleur>")?;
            let e = effect_from(&args, Mode::Static, rest)?;
            let mut kb = open(&args)?;
            kb.set_effect(&e)?;
            if args.flag("--save") {
                kb.save()?;
            }
        }
        "effect" => {
            args.allow(EFFECT_OPTS)?;
            let Some(name) = rest.first() else {
                return msg("utilisation : scope96 effect <mode> [couleurs...]");
            };
            let Some(mode) = Mode::parse(name) else {
                return msg(format!("mode inconnu : « {name} » (voir scope96 help)"));
            };
            let e = effect_from(&args, mode, &rest[1..])?;
            let mut kb = open(&args)?;
            kb.set_effect(&e)?;
            if args.flag("--save") {
                kb.save()?;
            }
        }
        "off" => {
            args.allow(&["--save"])?;
            let mut e = Effect::new(Mode::Static);
            e.colors.push(Rgb::BLACK);
            e.brightness = 0;
            let mut kb = open(&args)?;
            kb.set_effect(&e)?;
            if args.flag("--save") {
                kb.save()?;
            }
        }
        "keys" => {
            args.allow(&["--base"])?;
            if rest.is_empty() && args.value("--base").is_none() {
                return msg("utilisation : scope96 keys [--base <couleur>] <touche>=<couleur>...");
            }
            let legends = keys::layout_legends();
            let mut leds: Vec<(u8, Rgb)> = Vec::new();
            let mut set = |id: u8, c: Rgb| match leds.iter_mut().find(|(i, _)| *i == id) {
                Some(slot) => slot.1 = c,
                None => leds.push((id, c)),
            };
            if let Some(base) = args.value("--base") {
                let c = color(base)?;
                for key in keys::KEYS {
                    set(key.id, c);
                }
            }
            for item in rest {
                let Some((name, col)) = item.split_once('=') else {
                    return msg(format!("« {item} » : attendu <touche>=<couleur>"));
                };
                let c = color(col)?;
                let Some(ids) = keys::resolve(name, &legends) else {
                    return msg(format!("touche inconnue : « {name} » (voir scope96 list)"));
                };
                for id in ids {
                    set(id, c);
                }
            }
            open(&args)?.set_direct(&leds)?;
            if let Err(e) = store::save(&store::merge(&leds)) {
                eprintln!("attention : couleurs appliquées mais non mémorisées ({e})");
            }
        }
        "apply" => {
            args.allow(&[])?;
            let leds = store::load();
            if leds.is_empty() {
                return msg("aucune couleur touche par touche mémorisée (utilisez d'abord keys ou l'interface)");
            }
            open(&args)?.set_direct(&leds)?;
        }
        "save" => {
            args.allow(&[])?;
            open(&args)?.save()?;
        }
        "list" => {
            args.allow(&[])?;
            let legends = keys::layout_legends();
            println!("TOUCHES (nom positionnel, lettre gravée selon votre disposition, id de LED)");
            for key in keys::KEYS {
                let legend = legends
                    .iter()
                    .find(|(s, _)| *s == key.scan && key.scan != 0)
                    .map(|(_, c)| c.to_string())
                    .unwrap_or_default();
                println!("  {:<12} {:<3} 0x{:02x}", key.name, legend, key.id);
            }
            println!("\nGROUPES");
            println!("  {:<12} toutes les touches", "all");
            for (name, members) in keys::GROUPS {
                println!("  {:<12} {}", name, members.join(" "));
            }
        }
        "profile" => {
            args.allow(&[])?;
            let n = match one(rest, "profile <0-6>")?.parse() {
                Ok(n) if n <= 6 => n,
                _ => return msg("profil : 0 à 6"),
            };
            open(&args)?.set_profile(n)?;
        }
        "sleep" => {
            args.allow(&[])?;
            let index = match one(rest, "sleep <1|2|3|5|10|never>")? {
                "1" => 0,
                "2" => 1,
                "3" => 2,
                "5" => 3,
                "10" => 4,
                "never" | "jamais" => 5,
                _ => return msg("veille : 1, 2, 3, 5, 10 (minutes) ou never"),
            };
            open(&args)?.set_sleep(index)?;
        }
        "saving" => {
            args.allow(&[])?;
            let mode = match one(rest, "saving <off|lights-off|dim>")? {
                "off" => 0,
                "lights-off" => 1,
                "dim" => 2,
                _ => return msg("économie d'énergie : off, lights-off ou dim"),
            };
            open(&args)?.set_saving(mode)?;
        }
        "low-battery" => {
            args.allow(&[])?;
            let percent = match one(rest, "low-battery <0-100>")?.parse() {
                Ok(p) if p <= 100 => p,
                _ => return msg("alerte batterie : 0 à 100"),
            };
            open(&args)?.set_low_alert(percent)?;
        }
        "wdl" => {
            args.allow(&[])?;
            let on = match one(rest, "wdl <on|off>")? {
                "on" => true,
                "off" => false,
                _ => return msg("wdl : on ou off"),
            };
            open(&args)?.set_wdl(on)?;
        }
        "raw" => {
            args.allow(&[])?;
            let Ok(bytes) = rest
                .iter()
                .map(|s| u8::from_str_radix(s, 16))
                .collect::<Result<Vec<u8>, _>>()
            else {
                return msg("raw : octets hexadécimaux attendus (ex. 12 00)");
            };
            if bytes.is_empty() {
                return msg("raw : au moins un octet");
            }
            let mut kb = open(&args)?;
            let r = kb.link.request(&bytes)?;
            println!("{}", link::hex(&r));
        }
        other => return msg(format!("commande inconnue : « {other} » (scope96 help pour l'aide)")),
    }
    Ok(())
}
