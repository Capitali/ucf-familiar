//! `ucf-familiar fleet papers`: the human's keys, filed on the captain's record.
//!
//! A captain's own keys belong to the person and follow them from hull to hull
//! (metal#100); a co-pilot key belongs to its hull. The pilots read this record to
//! decide, per fold, whose papers fly each hull (`whisker`'s `select_papers`). This
//! command keeps the record: it files the keys the ship stores already hold, and it
//! signs a hull's co-pilot on the captain's own papers.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{json, Value};
use ucf_pilot::store::{self, CaptainPapers, Paper};
use ucf_wire::{http, Url};

use crate::fleet::{paired_ships, read_env_value, wire_get, Ship};

/// POST on the exchange: the status and whatever JSON came back.
fn wire_post(server: &str, key: &str, path: &str, body: &Value) -> Result<(u16, Value), String> {
    let url = Url::parse(&format!("{}{}", server.trim_end_matches('/'), path))
        .map_err(|e| format!("{e:?}"))?;
    let headers = vec![
        ("Authorization".to_string(), format!("Bearer {key}")),
        ("X-UCF-App".to_string(), "familiar-fleet".to_string()),
    ];
    let bytes = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    let resp = http::post_json(&url, &headers, &bytes).map_err(|e| format!("{e:?}"))?;
    let v = serde_json::from_slice(&resp.body).unwrap_or(Value::Null);
    Ok((resp.status, v))
}

fn key_id(secret: &str) -> String {
    secret.trim_start_matches("ucfk_").chars().take(8).collect()
}

/// What a key is (`captain` / `copilot`) and which hull it answers for right now.
fn read_key(server: &str, secret: &str) -> Option<(String, String, String)> {
    let profile = wire_get(server, secret, "/v1/profile").ok()?;
    let captain = profile
        .get("scopes")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|s| s.as_str() == Some("act")));
    let me = wire_get(server, secret, "/v1/me").ok()?;
    let actor = me.get("actor").and_then(Value::as_str)?.to_string();
    let hull = me
        .get("shipName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some((
        if captain { "captain" } else { "copilot" }.to_string(),
        actor,
        hull,
    ))
}

/// File every key the captain's ship stores hold onto the captain's record.
/// A key already filed keeps its original hull and filing time.
fn gather(ships: &[&Ship], papers: &mut CaptainPapers) -> Vec<String> {
    let mut said = Vec::new();
    for s in ships {
        let env = s.dir.join("ucf.env");
        let server = s.captain.server.clone();
        for var in ["UCF_KEY", store::COPILOT_KEY_VAR] {
            let Some(secret) = read_env_value(&env, var) else {
                continue;
            };
            let id = key_id(&secret);
            if papers.keys.iter().any(|p| p.key_id == id) {
                continue;
            }
            match read_key(&server, &secret) {
                Some((kind, actor, hull)) => {
                    said.push(format!("  filed {id} ({kind}) — answers for {hull}"));
                    store::file_paper(
                        papers,
                        Paper {
                            server: server.clone(),
                            key_id: id,
                            secret,
                            kind,
                            hull_actor: actor,
                            hull,
                            filed_at: super::now_secs(),
                        },
                    );
                }
                None => said.push(format!(
                    "  {id} on {} did not answer — not filed",
                    s.world.label
                )),
            }
        }
        if papers.exchange_captain_id.is_empty() {
            papers.exchange_captain_id = s.captain.exchange_captain_id.clone();
        }
    }
    said
}

/// A captain key on the record that answers for `want` this minute.
fn captain_key_for(papers: &CaptainPapers, server: &str, want: &str) -> Option<String> {
    papers
        .keys
        .iter()
        .filter(|p| p.is_captain() && p.server == server)
        .find(|p| {
            wire_get(server, &p.secret, "/v1/me")
                .ok()
                .and_then(|me| me.get("actor").and_then(Value::as_str).map(String::from))
                .as_deref()
                == Some(want)
        })
        .map(|p| p.secret.clone())
}

fn licence(server: &str, key: &str) -> Option<String> {
    let v = wire_get(server, key, "/v1/entitlements").ok()?;
    v.get("entitlements")?
        .as_array()?
        .iter()
        .rev()
        .find(|e| e.get("product").and_then(Value::as_str) == Some("auto:freight"))
        .and_then(|e| e.get("status").and_then(Value::as_str).map(String::from))
}

/// Append (or replace) one `KEY=value` line in a 0600 env file.
fn set_env(path: &Path, var: &str, value: &str) -> std::io::Result<()> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = old
        .lines()
        .filter(|l| l.split_once('=').map(|(k, _)| k.trim()) != Some(var))
        .map(String::from)
        .collect();
    lines.push(format!("{var}={value}"));
    let tmp = path.with_extension(format!("env.{}.tmp", std::process::id()));
    std::fs::write(&tmp, lines.join("\n") + "\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

pub(crate) fn run(
    dir: &Path,
    root: &Path,
    f: &std::collections::HashMap<String, String>,
) -> ExitCode {
    let ships = paired_ships(dir, root);
    let mut captains: Vec<String> = ships
        .iter()
        .map(|s| s.captain.captain_id.clone())
        .filter(|c| !c.is_empty())
        .collect();
    captains.sort();
    captains.dedup();
    if captains.is_empty() {
        println!("fleet papers: no paired ships with a captain id — run `fleet adopt-ids` first");
        return ExitCode::SUCCESS;
    }

    // File what the ship stores already hold, captain by captain.
    for cid in &captains {
        let theirs: Vec<&Ship> = ships
            .iter()
            .filter(|s| &s.captain.captain_id == cid)
            .collect();
        let Some(cdir) = store::captain_dir(&theirs[0].dir, cid) else {
            continue;
        };
        let mut papers = store::load_papers(&cdir);
        let said = gather(&theirs, &mut papers);
        if !said.is_empty() {
            if let Err(e) = store::save_papers(&cdir, &papers) {
                eprintln!("fleet papers: {cid} — {e}");
                return ExitCode::FAILURE;
            }
            for l in said {
                println!("{l}");
            }
        }
    }

    if let Some(target) = f.get("sign-copilot") {
        return sign_copilot(&ships, target, f.contains_key("replace"));
    }

    for cid in &captains {
        let theirs: Vec<&Ship> = ships
            .iter()
            .filter(|s| &s.captain.captain_id == cid)
            .collect();
        let Some(cdir) = store::captain_dir(&theirs[0].dir, cid) else {
            continue;
        };
        let papers = store::load_papers(&cdir);
        println!(
            "{} ({cid}{})",
            theirs[0].captain.captain,
            if papers.exchange_captain_id.is_empty() {
                String::new()
            } else {
                format!(", {}", papers.exchange_captain_id)
            }
        );
        for s in &theirs {
            let want = s.captain.hull_actor.as_str();
            let copilot = papers
                .keys
                .iter()
                .find(|p| !p.is_captain() && p.hull_actor == want)
                .map(|p| p.key_id.clone());
            println!(
                "  {:<24} {:<22} co-pilot: {}",
                s.captain.hull_name,
                want,
                copilot
                    .unwrap_or_else(|| "none — grounded if the captain boards another hull".into())
            );
        }
        for p in &papers.keys {
            println!(
                "    key {} {:<8} issued for {} ({})",
                p.key_id, p.kind, p.hull, p.hull_actor
            );
        }
    }
    ExitCode::SUCCESS
}

fn sign_copilot(ships: &[Ship], target: &str, replace: bool) -> ExitCode {
    let Some(s) = ships
        .iter()
        .find(|s| s.world.id.contains(target) || s.captain.hull_name.contains(target))
    else {
        eprintln!("fleet papers: no paired hull matches `{target}`");
        return ExitCode::FAILURE;
    };
    let Some(want) = Some(s.captain.hull_actor.clone()).filter(|a| !a.is_empty()) else {
        eprintln!(
            "fleet papers: {} has no hull actor on file — re-pair it",
            s.world.label
        );
        return ExitCode::FAILURE;
    };
    let server = s.captain.server.clone();
    let Some(cdir) = store::captain_dir(&s.dir, &s.captain.captain_id) else {
        eprintln!(
            "fleet papers: {} has no captain id — run `fleet adopt-ids`",
            s.world.label
        );
        return ExitCode::FAILURE;
    };
    let mut papers = store::load_papers(&cdir);

    // Never sign over a co-pilot we already fly on: minting retires every live
    // co-pilot of the hull, and this one may be the key the pilot is using.
    if let Some(p) = papers
        .keys
        .iter()
        .find(|p| !p.is_captain() && p.hull_actor == want)
    {
        println!(
            "fleet papers: {} already holds its co-pilot ({}); nothing signed",
            s.captain.hull_name, p.key_id
        );
        return ExitCode::SUCCESS;
    }
    let Some(captain_key) = captain_key_for(&papers, &server, &want) else {
        eprintln!(
            "fleet papers: none of the captain's own keys answers for {} right now — a \
             co-pilot is signed on the papers of the hull it will fly",
            s.captain.hull_name
        );
        return ExitCode::FAILURE;
    };

    // The licence. Already active means someone may already hold a co-pilot for
    // this hull (Haul signs them too), and a new one would retire it.
    match licence(&server, &captain_key).as_deref() {
        Some("active") if !replace => {
            eprintln!(
                "fleet papers: {} already holds an auto:freight licence, so a co-pilot may \
                 already be flying it (Haul signs them). Signing a new one RETIRES it. \
                 Pass --replace to sign anyway.",
                s.captain.hull_name
            );
            return ExitCode::FAILURE;
        }
        Some("active") => {}
        Some("pending") => println!("  licence purchase already on the queue"),
        _ => match wire_post(
            &server,
            &captain_key,
            "/v1/entitlements/purchase",
            &json!({"product": "auto:freight"}),
        ) {
            Ok((c, _)) if (200..300).contains(&c) => {
                println!("  licence bought; the fold decides it at the next tick")
            }
            Ok((c, v)) => {
                eprintln!("fleet papers: licence refused — HTTP {c} {v}");
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("fleet papers: licence purchase did not reach the exchange: {e}");
                return ExitCode::FAILURE;
            }
        },
    }
    // Wait out the fold: two ticks is enough on any world, ten minutes on PROD.
    let tick_secs = wire_get(&server, &captain_key, "/v1/status")
        .ok()
        .and_then(|v| v.get("tickDurationSec").and_then(Value::as_u64))
        .unwrap_or(10);
    let deadline = super::now_secs() + (tick_secs * 3) as i64 + 30;
    loop {
        match licence(&server, &captain_key).as_deref() {
            Some("active") => break,
            Some("refused") => {
                eprintln!("fleet papers: the fold refused the licence — see the hull's receipts");
                return ExitCode::FAILURE;
            }
            _ if super::now_secs() > deadline => {
                eprintln!("fleet papers: the licence is still pending; run this again after the next tick");
                return ExitCode::FAILURE;
            }
            _ => std::thread::sleep(Duration::from_secs(tick_secs.clamp(5, 30))),
        }
    }

    let label = format!("{} co-pilot (familiar)", s.captain.hull_name);
    let (code, v) = match wire_post(
        &server,
        &captain_key,
        "/v1/copilot-keys",
        &json!({"label": label}),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fleet papers: the co-pilot mint did not reach the exchange: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(secret) = v
        .get("key")
        .and_then(Value::as_str)
        .filter(|_| (200..300).contains(&code))
    else {
        eprintln!("fleet papers: co-pilot refused — HTTP {code} {v}");
        return ExitCode::FAILURE;
    };
    let id = key_id(secret);
    // The secret is shown exactly once: store it before anything else can fail.
    if let Err(e) = set_env(&s.dir.join("ucf.env"), store::COPILOT_KEY_VAR, secret) {
        eprintln!(
            "fleet papers: co-pilot {id} signed but NOT stored ({e}). It is live on the \
             exchange; revoke it with DELETE /v1/copilot-keys/{id} and sign again."
        );
        return ExitCode::FAILURE;
    }
    store::file_paper(
        &mut papers,
        Paper {
            server: server.clone(),
            key_id: id.clone(),
            secret: secret.to_string(),
            kind: "copilot".into(),
            hull_actor: want.clone(),
            hull: s.captain.hull_name.clone(),
            filed_at: super::now_secs(),
        },
    );
    if let Err(e) = store::save_papers(&cdir, &papers) {
        eprintln!(
            "fleet papers: co-pilot {id} is in the ship's ucf.env but not on the record: {e}"
        );
        return ExitCode::FAILURE;
    }
    let retired = v.get("retired").and_then(Value::as_i64).unwrap_or(0);
    println!(
        "fleet papers: {} signed co-pilot {id} — freight only{}",
        s.captain.hull_name,
        if retired > 0 {
            format!("; {retired} earlier co-pilot key(s) retired")
        } else {
            String::new()
        }
    );
    ExitCode::SUCCESS
}

/// `fleet fleet-name "<name>" --captain <id|name>`: name the captain's fleet on the
/// exchange (metal#86) and berth every hull the familiar flies for them in it. The
/// exchange keeps one fleet per captain; a name another captain's fleet sails under
/// is refused (409 `fleet-name-held`).
pub(crate) fn name_fleet(dir: &Path, root: &Path, name: &str, who: Option<&String>) -> ExitCode {
    let ships = paired_ships(dir, root);
    let matches = |s: &&Ship| match who {
        Some(w) => s.captain.captain_id == *w || s.captain.captain.contains(w.as_str()),
        None => true,
    };
    let mut ids: Vec<String> = ships
        .iter()
        .filter(matches)
        .map(|s| s.captain.captain_id.clone())
        .filter(|c| !c.is_empty())
        .collect();
    ids.sort();
    ids.dedup();
    let [cid] = ids.as_slice() else {
        eprintln!(
            "fleet fleet-name: {} captains match — pass --captain <id> (see `fleet papers`)",
            ids.len()
        );
        return ExitCode::FAILURE;
    };
    let theirs: Vec<&Ship> = ships
        .iter()
        .filter(|s| &s.captain.captain_id == cid)
        .collect();
    let server = theirs[0].captain.server.clone();
    let Some(cdir) = store::captain_dir(&theirs[0].dir, cid) else {
        return ExitCode::FAILURE;
    };
    let papers = store::load_papers(&cdir);
    let Some(key) = papers
        .keys
        .iter()
        .find(|p| p.is_captain() && p.server == server)
        .map(|p| p.secret.clone())
    else {
        eprintln!("fleet fleet-name: no captain key on {cid}'s record — run `fleet papers` first");
        return ExitCode::FAILURE;
    };
    match wire_post(&server, &key, "/v1/captain", &json!({"fleetName": name})) {
        Ok((c, _)) if (200..300).contains(&c) => println!("fleet: the fleet sails as \"{name}\""),
        Ok((c, v)) => {
            eprintln!("fleet fleet-name: refused — HTTP {c} {v}");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("fleet fleet-name: the exchange did not answer: {e}");
            return ExitCode::FAILURE;
        }
    }
    let mut failed = false;
    for s in &theirs {
        let hull = &s.captain.hull_actor;
        if hull.is_empty() {
            println!(
                "  {} — no hull actor on file, not berthed",
                s.captain.hull_name
            );
            continue;
        }
        match wire_post(&server, &key, "/v1/captain", &json!({"berth": hull})) {
            Ok((c, _)) if (200..300).contains(&c) => {
                println!("  {} berthed", s.captain.hull_name)
            }
            Ok((c, v)) => {
                failed = true;
                println!("  {} not berthed — HTTP {c} {v}", s.captain.hull_name)
            }
            Err(e) => {
                failed = true;
                println!("  {} not berthed — {e}", s.captain.hull_name)
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
