//! Answers to the local API's questions (see `venecita_api`), built from the engine. Kept
//! apart from the tray app so they can be tested with the fake platform.

use std::collections::HashMap;

use serde_json::{json, Value};
use venecita_api::ActionError;
use venecita_core::display::{resolve_displays, DisplayInfo};
use venecita_core::engine::Engine;
use venecita_core::window::WindowId;
use venecita_core::Action;

/// `GET /v1/state`: active profile and layout, and the flags.
pub fn state(engine: &Engine) -> Value {
    let s = engine.status();
    json!({
        "profile": s.profile,
        "layout": s.layout,
        "manual": s.manual,
        "paused": s.paused,
        "focus_mode": engine.focus_mode(),
        "auto_arrange": engine.config().general.auto_arrange,
        "windows": s.windows,
    })
}

/// `GET /v1/displays`: connected displays and the definition each one resolved to.
pub fn displays(engine: &Engine, connected: &[DisplayInfo]) -> Value {
    let resolved = resolve_displays(&engine.config().displays, connected);
    let list: Vec<Value> = connected
        .iter()
        .map(|d| {
            let definition = resolved
                .iter()
                .find(|r| r.handle == d.handle)
                .map(|r| r.def_id.clone());
            json!({
                "handle": d.handle,
                "name": d.name,
                "builtin": d.builtin,
                "frame": d.frame,
                "work_area": d.work_area,
                "scale": d.scale,
                "definition": definition,
            })
        })
        .collect();
    json!(list)
}

/// `GET /v1/windows`: the windows Venecita knows, with their zone and stack.
pub fn windows(engine: &Engine) -> Value {
    let mut stacks: HashMap<WindowId, Value> = HashMap::new();
    for (key, stack) in engine.stacks() {
        for (index, id) in stack.windows().iter().enumerate() {
            stacks.insert(
                *id,
                json!({
                    "zone": key.to_string(),
                    "index": index,
                    "size": stack.len(),
                    "active": stack.active() == Some(*id),
                }),
            );
        }
    }
    let list: Vec<Value> = engine
        .windows()
        .map(|w| {
            json!({
                "id": w.id.0,
                "app": { "id": w.app.id, "name": w.app.name },
                "title": w.title,
                "kind": w.kind,
                "frame": w.frame,
                "focused": engine.focused() == Some(w.id),
                "stack": stacks.get(&w.id),
            })
        })
        .collect();
    json!(list)
}

/// `GET /v1/profiles`: profiles and their layouts, marking the active ones.
pub fn profiles(engine: &Engine) -> Value {
    let status = engine.status();
    let list: Vec<Value> = engine
        .config()
        .profiles
        .iter()
        .map(|p| {
            let s = status.profiles.iter().find(|x| x.name == p.name);
            let active = s.is_some_and(|x| x.active);
            let layouts: Vec<Value> = p
                .layouts
                .iter()
                .map(|l| {
                    json!({
                        "name": l.name,
                        "displays": l.displays,
                        "rules": l.rules.len(),
                        "active": active && status.layout.as_deref() == Some(l.name.as_str()),
                    })
                })
                .collect();
            json!({
                "name": p.name,
                "active": active,
                "fits": s.is_some_and(|x| x.fits),
                "layouts": layouts,
            })
        })
        .collect();
    json!(list)
}

/// Checks an action from the API before it runs: profiles must exist.
pub fn check_action(engine: &Engine, action: &Action) -> Result<(), ActionError> {
    match action {
        Action::ActivateProfile { name } if engine.config().profile(name).is_none() => {
            Err(ActionError::UnknownProfile(name.clone()))
        }
        _ => Ok(()),
    }
}

/// The event sent to WebSocket clients when a window gets the focus.
pub fn focus_event(engine: &Engine, id: WindowId) -> Value {
    let app = engine
        .windows()
        .find(|w| w.id == id)
        .map(|w| json!({ "id": w.app.id, "name": w.app.name }));
    json!({ "type": "focus", "window": id.0, "app": app })
}

#[cfg(test)]
mod tests {
    use super::*;
    use venecita_core::Config;

    const EXAMPLE: &str = include_str!("../../../examples/config.toml");

    #[test]
    fn unknown_profiles_are_refused() {
        let engine = Engine::new(Config::from_toml_str(EXAMPLE).unwrap());
        let ok = Action::ActivateProfile {
            name: "casa".into(), // case-insensitive, like the tray
        };
        assert_eq!(check_action(&engine, &ok), Ok(()));
        let missing = Action::ActivateProfile {
            name: "Luna".into(),
        };
        assert_eq!(
            check_action(&engine, &missing),
            Err(ActionError::UnknownProfile("Luna".into()))
        );
        assert_eq!(check_action(&engine, &Action::RelayoutAll), Ok(()));
    }

    #[test]
    fn profiles_list_their_layouts() {
        let engine = Engine::new(Config::from_toml_str(EXAMPLE).unwrap());
        let value = profiles(&engine);
        let casa = &value[0];
        assert_eq!(casa["name"], "Casa");
        assert_eq!(casa["layouts"][0]["name"], "Notebook abierta");
        assert_eq!(casa["layouts"][0]["rules"], 2);
    }

    #[test]
    fn state_has_the_flags() {
        let engine = Engine::new(Config::from_toml_str(EXAMPLE).unwrap());
        let value = state(&engine);
        for key in [
            "profile",
            "layout",
            "manual",
            "paused",
            "focus_mode",
            "auto_arrange",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
    }
}
