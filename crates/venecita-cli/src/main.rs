//! `venecita`: command line client for Venecita Dock's local API.
//!
//! ```sh
//! venecita status
//! venecita action focus-mode-toggle
//! venecita action move-zone --dir left
//! venecita profile list
//! venecita profile activate Casa
//! venecita token show
//! venecita token rotate
//! ```
//!
//! The URL comes from `api.bind` in the config file (default `http://127.0.0.1:47800`) and
//! the token from `token.secret` next to it; `--url`, `--token` and `VENECITA_TOKEN`
//! override them.

use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use serde_json::{json, Map, Value};
use venecita_api::{paths, token};

/// Control Venecita Dock from the command line.
#[derive(Debug, Parser)]
#[command(name = "venecita", version, about)]
struct Cli {
    /// API address (default: from the config file, else http://127.0.0.1:47800).
    #[arg(long, global = true)]
    url: Option<String>,
    /// Access token (default: the token file next to the config).
    #[arg(long, global = true, env = "VENECITA_TOKEN", hide_env_values = true)]
    token: Option<String>,
    /// Print the raw JSON answer.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Active profile and layout, pause and focus mode.
    Status,
    /// Run an action, e.g. `move-zone --dir left` or `focus-mode-toggle`.
    Action {
        /// Action name (`move-zone`, `relayout-all`, `focus-mode-toggle`…).
        #[arg(value_name = "ACTION")]
        action: String,
        /// Direction for move-zone, grow and shrink.
        #[arg(long, value_parser = ["left", "right", "up", "down"])]
        dir: Option<String>,
        /// Display for move-to-display.
        #[arg(long, value_parser = ["next", "prev"])]
        target: Option<String>,
        /// Profile for activate-profile.
        #[arg(long = "name")]
        profile: Option<String>,
        /// Window for focus-window.
        #[arg(long)]
        id: Option<u64>,
    },
    /// Profiles.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Connected displays and the definitions they resolved to.
    Displays,
    /// The windows Venecita knows about.
    Windows,
    /// The access token.
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    /// List profiles and their layouts.
    List,
    /// Switch to a profile (turns automatic choice off).
    Activate {
        /// Profile name.
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum TokenCommand {
    /// Print the token (e.g. for `curl -H "Authorization: Bearer $(venecita token show)"`).
    Show,
    /// Replace the token with a new one (the running app picks it up by itself).
    Rotate,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("venecita: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<(), String> {
    let token_path = token::path_for(&paths::config_file());
    match &cli.command {
        Command::Token {
            command: TokenCommand::Show,
        } => {
            let token = token::read(&token_path).ok_or_else(|| {
                format!(
                    "no token yet at {} (start Venecita Dock, or run `venecita token rotate`)",
                    token_path.display()
                )
            })?;
            println!("{token}");
            Ok(())
        }
        Command::Token {
            command: TokenCommand::Rotate,
        } => {
            let token = token::rotate(&token_path).map_err(|e| e.to_string())?;
            println!("{token}");
            Ok(())
        }
        Command::Status => {
            let state = client(cli)?.get("/v1/state")?;
            print(cli, &state, |s| {
                let profile = match (s["profile"].as_str(), s["layout"].as_str()) {
                    (Some(p), Some(l)) => {
                        let manual = if s["manual"] == true { " (manual)" } else { "" };
                        format!("{p} – {l}{manual}")
                    }
                    _ => "none fits the connected displays".into(),
                };
                let yes = |v: &Value| if *v == true { "on" } else { "off" };
                format!(
                    "profile:        {profile}\npaused:         {}\nfocus mode:     {}\nauto-arrange:   {}\nwindows:        {}",
                    yes(&s["paused"]),
                    yes(&s["focus_mode"]),
                    yes(&s["auto_arrange"]),
                    s["windows"]
                )
            });
            Ok(())
        }
        Command::Action {
            action,
            dir,
            target,
            profile,
            id,
        } => {
            let body = action_json(
                action,
                dir.as_deref(),
                target.as_deref(),
                profile.as_deref(),
                *id,
            );
            let answer = client(cli)?.post("/v1/actions", &body)?;
            print(cli, &answer, |_| "ok".into());
            Ok(())
        }
        Command::Profile {
            command: ProfileCommand::List,
        } => {
            let profiles = client(cli)?.get("/v1/profiles")?;
            print(cli, &profiles, |list| {
                let mut out = Vec::new();
                for p in list.as_array().into_iter().flatten() {
                    let mark = if p["active"] == true { "*" } else { " " };
                    let fits = if p["fits"] == true {
                        ""
                    } else {
                        "  (doesn't fit the displays)"
                    };
                    out.push(format!(
                        "{mark} {}{fits}",
                        p["name"].as_str().unwrap_or("?")
                    ));
                    for l in p["layouts"].as_array().into_iter().flatten() {
                        let mark = if l["active"] == true { "*" } else { " " };
                        out.push(format!(
                            "    {mark} {}  [{}]  {} rule(s)",
                            l["name"].as_str().unwrap_or("?"),
                            l["displays"]
                                .as_array()
                                .map(|d| d
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(", "))
                                .unwrap_or_default(),
                            l["rules"]
                        ));
                    }
                }
                out.join("\n")
            });
            Ok(())
        }
        Command::Profile {
            command: ProfileCommand::Activate { name },
        } => {
            let path = format!("/v1/profiles/{}/activate", encode_path(name));
            let answer = client(cli)?.post(&path, &json!({}))?;
            print(cli, &answer, |_| format!("profile {name} activated"));
            Ok(())
        }
        Command::Displays => {
            let displays = client(cli)?.get("/v1/displays")?;
            print(cli, &displays, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| {
                        format!(
                            "{:<28} {:>5}×{:<5} {:<9} as {}",
                            d["name"].as_str().unwrap_or("?"),
                            d["frame"]["w"],
                            d["frame"]["h"],
                            if d["builtin"] == true {
                                "built-in"
                            } else {
                                "external"
                            },
                            d["definition"].as_str().unwrap_or("(not defined)")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(())
        }
        Command::Windows => {
            let windows = client(cli)?.get("/v1/windows")?;
            print(cli, &windows, |list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|w| {
                        let focus = if w["focused"] == true { "*" } else { " " };
                        let zone = w["stack"]["zone"].as_str().unwrap_or("-");
                        format!(
                            "{focus} {:<24} {:<20} {}",
                            w["app"]["name"].as_str().unwrap_or("?"),
                            zone,
                            w["title"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(())
        }
    }
}

/// The JSON body of an action: `move-zone --dir left` → `{"action":"move_zone","dir":"left"}`.
fn action_json(
    name: &str,
    dir: Option<&str>,
    target: Option<&str>,
    profile: Option<&str>,
    id: Option<u64>,
) -> Value {
    let mut body = Map::new();
    body.insert("action".into(), json!(name.trim().replace('-', "_")));
    if let Some(dir) = dir {
        body.insert("dir".into(), json!(dir));
    }
    if let Some(target) = target {
        body.insert("target".into(), json!(target));
    }
    if let Some(profile) = profile {
        body.insert("name".into(), json!(profile));
    }
    if let Some(id) = id {
        body.insert("id".into(), json!(id));
    }
    Value::Object(body)
}

/// Percent-encodes a path segment (profile names may contain spaces).
fn encode_path(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn print(cli: &Cli, value: &Value, human: impl FnOnce(&Value) -> String) {
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        );
    } else {
        println!("{}", human(value));
    }
}

struct Client {
    base: String,
    token: String,
    agent: ureq::Agent,
}

fn client(cli: &Cli) -> Result<Client, String> {
    let base = cli.url.clone().unwrap_or_else(default_url);
    let token = match &cli.token {
        Some(t) => t.clone(),
        None => {
            let path = token::path_for(&paths::config_file());
            token::read(&path).ok_or_else(|| {
                format!(
                    "no token found at {}; pass --token or set VENECITA_TOKEN",
                    path.display()
                )
            })?
        }
    };
    Ok(Client {
        base: base.trim_end_matches('/').to_string(),
        token,
        agent: ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build(),
    })
}

/// `http://<api.bind>` from the config file; a wildcard address means "this computer".
fn default_url() -> String {
    let bind = std::fs::read_to_string(paths::config_file())
        .ok()
        .and_then(|text| venecita_core::Config::from_toml_str(&text).ok())
        .map(|c| c.api.bind)
        .unwrap_or_else(|| venecita_core::config::DEFAULT_API_BIND.to_string());
    let bind = bind
        .replace("0.0.0.0:", "127.0.0.1:")
        .replace("[::]:", "[::1]:");
    format!("http://{bind}")
}

impl Client {
    fn get(&self, path: &str) -> Result<Value, String> {
        let request = self.agent.get(&format!("{}{path}", self.base));
        self.send(request, None)
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value, String> {
        let request = self.agent.post(&format!("{}{path}", self.base));
        self.send(request, Some(body))
    }

    fn send(&self, request: ureq::Request, body: Option<&Value>) -> Result<Value, String> {
        let request = request.set("Authorization", &format!("Bearer {}", self.token));
        let result = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        };
        match result {
            Ok(response) => response.into_json().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(code, response)) => {
                let body: Value = response.into_json().unwrap_or_default();
                let message = body["error"].as_str().unwrap_or("request failed");
                Err(format!("{message} (HTTP {code})"))
            }
            Err(ureq::Error::Transport(_)) => Err(format!(
                "cannot reach Venecita Dock at {} (is it running, with [api] enabled?)",
                self.base
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn subcommands_parse() {
        let cli =
            Cli::try_parse_from(["venecita", "action", "move-zone", "--dir", "left"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Action { ref action, dir: Some(ref d), .. } if action == "move-zone" && d == "left"
        ));
        let cli = Cli::try_parse_from(["venecita", "profile", "activate", "Casa"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Profile { command: ProfileCommand::Activate { ref name } } if name == "Casa"
        ));
        assert!(
            Cli::try_parse_from(["venecita", "action", "move-zone", "--dir", "sideways"]).is_err()
        );
        let cli =
            Cli::try_parse_from(["venecita", "--json", "status", "--url", "http://x:1"]).unwrap();
        assert!(cli.json);
        assert_eq!(cli.url.as_deref(), Some("http://x:1"));
    }

    #[test]
    fn actions_become_the_api_json() {
        assert_eq!(
            action_json("move-zone", Some("left"), None, None, None),
            json!({ "action": "move_zone", "dir": "left" })
        );
        assert_eq!(
            action_json("focus-mode-toggle", None, None, None, None),
            json!({ "action": "focus_mode_toggle" })
        );
        assert_eq!(
            action_json("activate_profile", None, None, Some("Casa"), None),
            json!({ "action": "activate_profile", "name": "Casa" })
        );
        // What the CLI sends is exactly what the server parses.
        let body = action_json("move-to-display", None, Some("next"), None, None);
        let action: venecita_core::Action = serde_json::from_value(body).unwrap();
        assert_eq!(
            action,
            venecita_core::Action::MoveToDisplay {
                target: venecita_core::action::DisplayTarget::Next
            }
        );
    }

    #[test]
    fn profile_names_are_encoded_in_paths() {
        assert_eq!(encode_path("Casa"), "Casa");
        assert_eq!(encode_path("Casa grande"), "Casa%20grande");
        assert_eq!(encode_path("Año"), "A%C3%B1o");
    }
}
