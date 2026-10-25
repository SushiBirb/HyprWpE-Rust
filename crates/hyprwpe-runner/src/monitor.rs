use std::process::Command;
use serde_json::Value;

pub fn detect_monitors() -> Vec<String> {
    let output = Command::new("hyprctl")
        .arg("monitors")
        .arg("-j")
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            if let Ok(monitors_json) = serde_json::from_slice::<Value>(&out.stdout) {
                if let Some(arr) = monitors_json.as_array() {
                    let mut names = Vec::new();
                    for m in arr {
                        if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                            names.push(name.to_string());
                        }
                    }
                    if !names.is_empty() {
                        return names;
                    }
                }
            }
        }
    }

    vec!["DP-1".to_string()]
}
