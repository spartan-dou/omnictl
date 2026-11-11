//! Serveur MCP (Model Context Protocol) sur l'entrée/sortie standard.
//!
//! `omnictl mcp` lit des messages JSON-RPC 2.0, un par ligne, sur stdin et répond sur stdout.
//! Aucun port réseau n'est ouvert : c'est le client MCP (Claude Desktop, Claude Code…) qui
//! lance le programme et lui parle par ses tubes. Rien d'autre que du JSON-RPC ne doit
//! sortir sur stdout ; les diagnostics vont sur stderr.

use crate::json::{self, Value};
use crate::keys;
use crate::link::{Error, Link};
use crate::proto::{self, Direction, Effect, Keyboard, Mode, Rgb, Speed};
use crate::store;
use std::io::{BufRead, Write};

const PROTOCOL_VERSION: &str = "2025-06-18";

/// Description des outils, renvoyée telle quelle par `tools/list`.
const TOOLS: &str = r##"[
{"name":"keyboard_status",
 "description":"État du clavier ROG Strix Scope II 96 Wireless : connexion (USB ou dongle 2,4 GHz), firmware, profil et effet actifs, batterie, réglages d'énergie, molette, éclairage dynamique Windows.",
 "inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
{"name":"set_effect",
 "description":"Applique un effet lumineux géré par le clavier lui-même (il continue sans logiciel). Modes : static (1 couleur), breathing (1-2), cycle (aucune), reactive (1-2), wave (0-7, aucune = arc-en-ciel), ripple (0-7), starry (1-3), quicksand (0 ou 6), current (1-3), raindrop (1-3). Remplace les couleurs touche par touche en cours.",
 "inputSchema":{"type":"object","properties":{
   "mode":{"type":"string","enum":["static","breathing","cycle","reactive","wave","ripple","starry","quicksand","current","raindrop"]},
   "colors":{"type":"array","items":{"type":"string"},"description":"Couleurs : #rrggbb ou nom (red, green, blue, white, yellow, cyan, magenta, orange, purple, pink, off)."},
   "speed":{"type":"string","enum":["slow","medium","fast"]},
   "brightness":{"type":"integer","minimum":0,"maximum":100,"description":"Luminosité en %, 100 par défaut."},
   "direction":{"type":"string","enum":["left","right","up","down","horizontal","vertical"],"description":"Pour wave ; quicksand n'accepte que up ou down."},
   "random":{"type":"boolean","description":"Couleurs aléatoires (breathing, reactive, ripple, starry, current, raindrop)."},
   "save":{"type":"boolean","description":"Enregistre aussi l'effet dans le profil actif du clavier."}},
  "required":["mode"],"additionalProperties":false}},
{"name":"set_key_colors",
 "description":"Colore des touches individuellement (mode direct). Les couleurs sont tenues par l'ordinateur : le clavier les perd à la mise en veille ou à l'extinction, et apply_saved_keys les remet. Noms de touches : voir list_keys ; une lettre seule désigne la touche qui porte cette lettre dans la disposition active (AZERTY, etc.).",
 "inputSchema":{"type":"object","properties":{
   "keys":{"type":"object","additionalProperties":{"type":"string"},"description":"Nom de touche ou de groupe → couleur. Ex. {\"esc\":\"white\",\"arrows\":\"#ff6000\"}."},
   "base":{"type":"string","description":"Couleur de toutes les autres touches. Sans base, elles gardent leur couleur."}},
  "additionalProperties":false}},
{"name":"apply_saved_keys",
 "description":"Renvoie au clavier la dernière carte de couleurs touche par touche mémorisée sur l'ordinateur.",
 "inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
{"name":"list_keys",
 "description":"Liste les noms de touches (positionnels, façon QWERTY US), la lettre gravée selon la disposition active, et les groupes (all, space, wasd, zqsd, arrows, frow, digits, numpad, mods).",
 "inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
{"name":"set_power",
 "description":"Règle la gestion d'énergie du clavier sur batterie. Chaque champ est optionnel ; seuls ceux fournis sont modifiés.",
 "inputSchema":{"type":"object","properties":{
   "sleep":{"type":"string","enum":["1","2","3","5","10","never"],"description":"Délai de mise en veille, en minutes."},
   "saving":{"type":"string","enum":["off","lights-off","dim"],"description":"Économie d'énergie : désactivée, éteindre l'éclairage, ou réduire la luminosité."},
   "low_battery":{"type":"integer","minimum":0,"maximum":100,"description":"Seuil d'alerte batterie faible, en %."}},
  "additionalProperties":false}},
{"name":"set_profile",
 "description":"Bascule sur un profil embarqué du clavier (0 = profil par défaut, 1 à 6 = profils utilisateur).",
 "inputSchema":{"type":"object","properties":{"profile":{"type":"integer","minimum":0,"maximum":6}},"required":["profile"],"additionalProperties":false}},
{"name":"set_dynamic_lighting",
 "description":"Active ou désactive l'éclairage dynamique Windows (LampArray) : activé, Windows 11 peut piloter l'éclairage du clavier lui-même.",
 "inputSchema":{"type":"object","properties":{"enabled":{"type":"boolean"}},"required":["enabled"],"additionalProperties":false}}
]"##;

pub fn run() -> Result<(), Error> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        // Certains lanceurs préfixent le flux d'un BOM UTF-8.
        let line = line.trim_start_matches('\u{FEFF}');
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle_line(line) {
            if writeln!(stdout, "{reply}").and_then(|_| stdout.flush()).is_err() {
                break; // le client a fermé son tube
            }
        }
    }
    Ok(())
}

/// Le catalogue d'outils sur une seule ligne : le transport stdio de MCP délimite les
/// messages par des fins de ligne, un message ne doit donc en contenir aucune.
fn tools_json() -> String {
    let mut out = String::new();
    json::parse(TOOLS)
        .expect("TOOLS est une constante JSON valide (vérifiée par les tests)")
        .write(&mut out);
    out
}

fn rpc_result(id: &Value, result: &str) -> String {
    let mut out = String::from("{\"jsonrpc\":\"2.0\",\"id\":");
    id.write(&mut out);
    out.push_str(",\"result\":");
    out.push_str(result);
    out.push('}');
    out
}

fn rpc_error(id: &Value, code: i32, message: &str) -> String {
    let mut out = String::from("{\"jsonrpc\":\"2.0\",\"id\":");
    id.write(&mut out);
    out.push_str(&format!(",\"error\":{{\"code\":{code},\"message\":{}}}}}", json::quote(message)));
    out
}

/// Traite un message. `None` pour une notification (aucune réponse attendue).
fn handle_line(line: &str) -> Option<String> {
    let msg = match json::parse(line) {
        Ok(m) => m,
        Err(e) => return Some(rpc_error(&Value::Null, -32700, &format!("JSON invalide : {e}"))),
    };
    let method = msg.get("method").and_then(Value::as_str);
    let Some(id) = msg.get("id") else {
        return None; // notification (notifications/initialized, cancelled…) ou réponse du client
    };
    let Some(method) = method else {
        return Some(rpc_error(id, -32600, "requête sans méthode"));
    };
    let params = msg.get("params");
    Some(match method {
        "initialize" => {
            // On reprend la version demandée par le client si elle est fournie : les
            // échanges utilisés ici (initialize, tools/list, tools/call) sont stables.
            let version = params
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            rpc_result(
                id,
                &format!(
                    "{{\"protocolVersion\":{},\"capabilities\":{{\"tools\":{{}}}},\
                     \"serverInfo\":{{\"name\":\"omnictl\",\"version\":{}}},\
                     \"instructions\":{}}}",
                    json::quote(version),
                    json::quote(env!("CARGO_PKG_VERSION")),
                    json::quote(
                        "Pilote le rétroéclairage et les réglages d'un clavier ASUS ROG Strix Scope II 96 \
                         Wireless branché en USB ou via son dongle 2,4 GHz. Les effets (set_effect) sont \
                         exécutés par le clavier ; les couleurs touche par touche (set_key_colors) sont \
                         volatiles."
                    )
                ),
            )
        }
        "ping" => rpc_result(id, "{}"),
        "tools/list" => rpc_result(id, &format!("{{\"tools\":{}}}", tools_json())),
        "tools/call" => {
            let name = params.and_then(|p| p.get("name")).and_then(Value::as_str);
            let Some(name) = name else {
                return Some(rpc_error(id, -32602, "tools/call : nom d'outil manquant"));
            };
            let empty = Value::Obj(Vec::new());
            let args = params.and_then(|p| p.get("arguments")).unwrap_or(&empty);
            // Un échec d'outil est un résultat (isError), pas une erreur de protocole :
            // le modèle peut ainsi le lire et corriger son appel.
            let (text, is_error) = match call_tool(name, args) {
                Ok(text) => (text, false),
                Err(e) => (e.to_string(), true),
            };
            rpc_result(
                id,
                &format!(
                    "{{\"content\":[{{\"type\":\"text\",\"text\":{}}}],\"isError\":{is_error}}}",
                    json::quote(&text)
                ),
            )
        }
        other => rpc_error(id, -32601, &format!("méthode inconnue : {other}")),
    })
}

fn bad<T>(text: impl Into<String>) -> Result<T, Error> {
    Err(Error::Msg(text.into()))
}

fn open() -> Result<Keyboard, Error> {
    Ok(Keyboard::new(Link::open(None)?))
}

fn color(v: &Value) -> Result<Rgb, Error> {
    match v.as_str().and_then(Rgb::parse) {
        Some(c) => Ok(c),
        None => bad("couleur invalide : attendu \"#rrggbb\" ou un nom de couleur"),
    }
}

/// Refuse les arguments inconnus : une faute de frappe ne doit pas être ignorée en silence.
fn only(args: &Value, allowed: &[&str]) -> Result<(), Error> {
    let Some(pairs) = args.as_object() else {
        return bad("les arguments doivent être un objet JSON");
    };
    match pairs.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
        Some((k, _)) => bad(format!("argument inconnu : {k}")),
        None => Ok(()),
    }
}

fn call_tool(name: &str, args: &Value) -> Result<String, Error> {
    match name {
        "keyboard_status" => {
            only(args, &[])?;
            let mut kb = open()?;
            let st = kb.status()?;
            if st.connection == proto::Connection::Unreachable {
                return bad(format!("le dongle répond seul : {}", st.connection.label()));
            }
            let pw = kb.power()?;
            let pick = |list: &[&'static str], i: u8| list.get(i as usize).copied().unwrap_or("?");
            let mut out = format!(
                "transport : {}\nliaison clavier : {}\nfirmware : {}\n",
                kb.link.via.label(),
                st.connection.label(),
                st.firmware
            );
            if let Some(r) = &st.receiver_firmware {
                out.push_str(&format!("firmware dongle : {r}\n"));
            }
            out.push_str(&format!(
                "profil actif : {}\neffet actif : {}\nbatterie : {} % ({} mV{})\n\
                 mise en veille : {}\néconomie d'énergie : {}\nalerte batterie faible : {} %\n\
                 molette : {}\néclairage dynamique Windows : {}",
                st.profile,
                Mode::from_code(st.effect).map_or("inconnu", |m| m.info().name),
                pw.percent,
                pw.millivolts,
                if pw.charging { ", en charge" } else { "" },
                pick(&proto::SLEEP_CHOICES, pw.sleep),
                pick(&proto::SAVING_CHOICES, pw.saving),
                pw.low_alert,
                pick(&proto::LEVER_CHOICES, kb.lever()?),
                if kb.wdl()? { "activé" } else { "désactivé" },
            ));
            Ok(out)
        }
        "set_effect" => {
            only(args, &["mode", "colors", "speed", "brightness", "direction", "random", "save"])?;
            let Some(mode) = args.get("mode").and_then(Value::as_str).and_then(Mode::parse) else {
                return bad("mode manquant ou inconnu");
            };
            let mut e = Effect::new(mode);
            if let Some(list) = args.get("colors") {
                let Some(items) = list.as_array() else {
                    return bad("colors doit être une liste");
                };
                for c in items {
                    e.colors.push(color(c)?);
                }
            }
            if let Some(v) = args.get("speed") {
                // Seuls les trois niveaux nommés sont exposés ici, pas l'octet brut.
                e.speed = match v.as_str() {
                    Some("slow") => Speed::Slow,
                    Some("medium") => Speed::Medium,
                    Some("fast") => Speed::Fast,
                    _ => return bad("speed : slow, medium ou fast"),
                };
            }
            if let Some(v) = args.get("brightness") {
                e.brightness = match v.as_u8() {
                    Some(b) if b <= proto::BRIGHTNESS_MAX => b,
                    _ => return bad("brightness : entier de 0 à 100"),
                };
            }
            if let Some(v) = args.get("direction") {
                e.direction = match v.as_str().and_then(Direction::parse) {
                    Some(d) => d,
                    None => return bad("direction invalide"),
                };
            }
            let flag = |name: &str| match args.get(name) {
                None => Ok(false),
                Some(v) => v.as_bool().ok_or_else(|| Error::Msg(format!("{name} doit être true ou false"))),
            };
            e.random = flag("random")?;
            let save = flag("save")?;
            let mut kb = open()?;
            kb.set_effect(&e)?;
            if save {
                kb.save()?;
            }
            Ok(format!(
                "effet {} appliqué{}",
                mode.info().name,
                if save { " et enregistré dans le clavier" } else { "" }
            ))
        }
        "set_key_colors" => {
            only(args, &["keys", "base"])?;
            let legends = keys::layout_legends();
            let mut leds: Vec<(u8, Rgb)> = Vec::new();
            let mut set = |id: u8, c: Rgb| match leds.iter_mut().find(|(i, _)| *i == id) {
                Some(slot) => slot.1 = c,
                None => leds.push((id, c)),
            };
            if let Some(base) = args.get("base") {
                let c = color(base)?;
                for key in keys::KEYS {
                    set(key.id, c);
                }
            }
            if let Some(map) = args.get("keys") {
                let Some(pairs) = map.as_object() else {
                    return bad("keys doit être un objet {touche: couleur}");
                };
                for (name, value) in pairs {
                    let c = color(value)?;
                    let Some(ids) = keys::resolve(name, &legends) else {
                        return bad(format!("touche inconnue : {name} (voir list_keys)"));
                    };
                    for id in ids {
                        set(id, c);
                    }
                }
            }
            if leds.is_empty() {
                return bad("rien à faire : fournir keys et/ou base");
            }
            open()?.set_direct(&leds)?;
            let count = leds.len();
            match store::save(&store::merge(&leds)) {
                Ok(()) => Ok(format!("{count} LED colorées et mémorisées")),
                Err(e) => Ok(format!("{count} LED colorées, mais non mémorisées sur le disque : {e}")),
            }
        }
        "apply_saved_keys" => {
            only(args, &[])?;
            let leds = store::load();
            if leds.is_empty() {
                return bad("aucune couleur touche par touche mémorisée");
            }
            open()?.set_direct(&leds)?;
            Ok(format!("{} LED remises en place", leds.len()))
        }
        "list_keys" => {
            only(args, &[])?;
            let legends = keys::layout_legends();
            let mut out = String::from("touches (nom positionnel, lettre gravée) :\n");
            for key in keys::KEYS {
                out.push_str(key.name);
                if let Some((_, c)) = legends.iter().find(|(s, _)| *s == key.scan && key.scan != 0) {
                    out.push_str(&format!(" ({c})"));
                }
                out.push_str(", ");
            }
            out.truncate(out.len() - 2);
            out.push_str("\ngroupes : all");
            for (name, members) in keys::GROUPS {
                out.push_str(&format!("; {name} = {}", members.join(" ")));
            }
            Ok(out)
        }
        "set_power" => {
            only(args, &["sleep", "saving", "low_battery"])?;
            let sleep = match args.get("sleep").map(Value::as_str) {
                None => None,
                Some(Some("1")) => Some(0),
                Some(Some("2")) => Some(1),
                Some(Some("3")) => Some(2),
                Some(Some("5")) => Some(3),
                Some(Some("10")) => Some(4),
                Some(Some("never")) => Some(5),
                Some(_) => return bad("sleep : \"1\", \"2\", \"3\", \"5\", \"10\" ou \"never\""),
            };
            let saving = match args.get("saving").map(Value::as_str) {
                None => None,
                Some(Some("off")) => Some(0),
                Some(Some("lights-off")) => Some(1),
                Some(Some("dim")) => Some(2),
                Some(_) => return bad("saving : off, lights-off ou dim"),
            };
            let low = match args.get("low_battery").map(Value::as_u8) {
                None => None,
                Some(Some(p)) if p <= 100 => Some(p),
                Some(_) => return bad("low_battery : entier de 0 à 100"),
            };
            if sleep.is_none() && saving.is_none() && low.is_none() {
                return bad("rien à faire : fournir sleep, saving et/ou low_battery");
            }
            // Tout est validé avant le premier envoi : pas de réglage appliqué à moitié
            // pour cause d'argument invalide.
            let mut kb = open()?;
            if let Some(v) = sleep {
                kb.set_sleep(v)?;
            }
            if let Some(v) = saving {
                kb.set_saving(v)?;
            }
            if let Some(v) = low {
                kb.set_low_alert(v)?;
            }
            Ok("réglages d'énergie appliqués".into())
        }
        "set_profile" => {
            only(args, &["profile"])?;
            let n = match args.get("profile").and_then(Value::as_u8) {
                Some(n) if n <= 6 => n,
                _ => return bad("profile : entier de 0 à 6"),
            };
            open()?.set_profile(n)?;
            Ok(format!("profil {n} activé"))
        }
        "set_dynamic_lighting" => {
            only(args, &["enabled"])?;
            let Some(on) = args.get("enabled").and_then(Value::as_bool) else {
                return bad("enabled : true ou false");
            };
            open()?.set_wdl(on)?;
            Ok(format!("éclairage dynamique Windows {}", if on { "activé" } else { "désactivé" }))
        }
        other => bad(format!("outil inconnu : {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(line: &str) -> Value {
        json::parse(&handle_line(line).expect("réponse attendue")).expect("réponse JSON valide")
    }

    #[test]
    fn tools_catalogue_is_valid_json() {
        let tools = json::parse(TOOLS).expect("TOOLS doit être du JSON valide");
        let list = tools.as_array().unwrap();
        assert_eq!(list.len(), 8);
        for t in list {
            assert!(t.get("name").and_then(Value::as_str).is_some());
            assert!(t.get("description").and_then(Value::as_str).is_some());
            assert_eq!(t.get("inputSchema").and_then(|s| s.get("type")).and_then(Value::as_str), Some("object"));
        }
    }

    #[test]
    fn every_listed_tool_is_dispatched() {
        // Un outil listé mais non géré renverrait « outil inconnu ».
        let tools = json::parse(TOOLS).unwrap();
        for t in tools.as_array().unwrap() {
            let name = t.get("name").and_then(Value::as_str).unwrap();
            let bogus = Value::Obj(vec![("__inconnu__".into(), Value::Null)]);
            let err = call_tool(name, &bogus).unwrap_err().to_string();
            assert!(err.contains("argument inconnu"), "{name} : {err}");
        }
    }

    #[test]
    fn initialize_echoes_version_and_id() {
        let r = reply(r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#);
        assert_eq!(r.get("id"), Some(&Value::Num(7.0)));
        let result = r.get("result").unwrap();
        assert_eq!(result.get("protocolVersion").and_then(Value::as_str), Some("2024-11-05"));
        assert!(result.get("capabilities").and_then(|c| c.get("tools")).is_some());
    }

    #[test]
    fn replies_are_single_line() {
        // Régression : le catalogue d'outils est rédigé sur plusieurs lignes dans le source.
        for line in [
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_keys"}}"#,
        ] {
            let reply = handle_line(line).unwrap();
            assert!(!reply.contains('\n') && !reply.contains('\r'), "{line}");
        }
    }

    #[test]
    fn notifications_get_no_reply() {
        assert!(handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }

    #[test]
    fn protocol_errors() {
        let r = reply(r#"{"jsonrpc":"2.0","id":"a","method":"nope"}"#);
        assert_eq!(r.get("error").and_then(|e| e.get("code")), Some(&Value::Num(-32601.0)));
        assert_eq!(r.get("id").and_then(Value::as_str), Some("a"));
        let r = reply("pas du json");
        assert_eq!(r.get("error").and_then(|e| e.get("code")), Some(&Value::Num(-32700.0)));
    }

    #[test]
    fn invalid_tool_arguments_are_tool_errors() {
        let r = reply(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"set_effect","arguments":{"mode":"disco"}}}"#);
        let result = r.get("result").unwrap();
        assert_eq!(result.get("isError").and_then(Value::as_bool), Some(true));
        let r = reply(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"set_power","arguments":{"sleep":"4"}}}"#);
        assert_eq!(r.get("result").and_then(|x| x.get("isError")).and_then(Value::as_bool), Some(true));
    }
}
