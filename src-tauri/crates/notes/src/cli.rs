//! Claude Code and Codex through their own subscription sign-in. OpenDictate never reads or stores
//! their OAuth credentials, and strips inherited API keys so a subscription route can't silently bill an API key.

use crate::{LanguageModel, NotesDraft, FIELDS};
use anyhow::{anyhow, bail, Result};
use opendictate_core::model::NotesCli;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn login_command(cli: NotesCli) -> &'static str {
    match cli {
        NotesCli::ClaudeCode => "claude auth login",
        NotesCli::Codex => "codex login",
    }
}

/// How to launch a CLI: a native executable, or an npm-installed script run by node.
#[derive(Clone, Debug)]
pub struct Launcher {
    pub program: PathBuf,
    pub prefix: Vec<String>,
}

pub fn executable(cli: NotesCli) -> Option<Launcher> {
    let (command, npm_script) = match cli {
        NotesCli::ClaudeCode => ("claude", "@anthropic-ai/claude-code/cli.js"),
        NotesCli::Codex => ("codex", "@openai/codex/bin/codex.js"),
    };
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = vec![PathBuf::from(&home).join(".local").join("bin"), PathBuf::from(&appdata).join("npm")];
    dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    for dir in &dirs {
        let exe = dir.join(format!("{command}.exe"));
        if exe.is_file() {
            return Some(Launcher { program: exe, prefix: vec![] });
        }
    }
    // npm installs a .cmd shim; run its script with node directly so arguments are passed verbatim.
    for dir in &dirs {
        if dir.join(format!("{command}.cmd")).is_file() {
            let script = dir.join("node_modules").join(npm_script);
            let node = dirs.iter().map(|d| d.join("node.exe")).find(|p| p.is_file());
            if let (true, Some(node)) = (script.is_file(), node) {
                return Some(Launcher { program: node, prefix: vec![script.to_string_lossy().into_owned()] });
            }
        }
    }
    None
}

fn failure(cli: NotesCli) -> anyhow::Error {
    anyhow!(
        "{} couldn't write notes. Update the CLI, check your subscription limits and model, and sign in with {} in a terminal. Your meeting is still saved.",
        cli.name(),
        login_command(cli)
    )
}

/// Runs a CLI with file-backed stdio in a private temporary directory, honoring cancellation and a timeout.
pub fn run(
    launcher: &Launcher,
    arguments: &[String],
    input: &str,
    cli: NotesCli,
    final_message: bool,
    timeout: Duration,
    cancel: &AtomicBool,
    until: Option<&dyn Fn(&str) -> bool>,
) -> Result<String> {
    let directory = std::env::temp_dir().join(format!("opendictate-notes-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory)?;
    let result = run_in(&directory, launcher, arguments, input, cli, final_message, timeout, cancel, until);
    let _ = fs::remove_dir_all(&directory);
    result
}

#[allow(clippy::too_many_arguments)]
fn run_in(
    directory: &Path,
    launcher: &Launcher,
    arguments: &[String],
    input: &str,
    cli: NotesCli,
    final_message: bool,
    timeout: Duration,
    cancel: &AtomicBool,
    until: Option<&dyn Fn(&str) -> bool>,
) -> Result<String> {
    let input_path = directory.join("input");
    let output_path = directory.join("output");
    let final_path = directory.join("reply");
    fs::write(&input_path, input)?;
    let mut args: Vec<String> = launcher.prefix.clone();
    args.extend(arguments.iter().cloned());
    if final_message {
        args.extend(["--output-last-message".into(), final_path.to_string_lossy().into_owned(), "-".into()]);
    }
    // Keep only what the CLIs need to find their own sign-in; drop API keys and provider URLs.
    const ALLOWED: &[&str] = &[
        "USERPROFILE", "HOMEDRIVE", "HOMEPATH", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP", "SYSTEMROOT", "SYSTEMDRIVE",
        "WINDIR", "COMSPEC", "PATHEXT", "USERNAME", "USERDOMAIN", "PROGRAMDATA", "PROGRAMFILES", "PROGRAMFILES(X86)",
        "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE", "OS", "LANG", "CODEX_HOME", "CLAUDE_CONFIG_DIR",
    ];
    let environment: HashMap<String, String> =
        std::env::vars().filter(|(k, _)| ALLOWED.contains(&k.to_uppercase().as_str())).collect();
    let program_dir = launcher.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let system = std::env::var("SYSTEMROOT").unwrap_or_else(|_| "C:\\Windows".into());
    let path = std::env::join_paths([program_dir, PathBuf::from(&system).join("System32"), PathBuf::from(&system)])?;
    let mut command = Command::new(&launcher.program);
    command
        .args(&args)
        .current_dir(directory)
        .env_clear()
        .envs(&environment)
        .env("PATH", path)
        .stdin(if until.is_some() { Stdio::piped() } else { Stdio::from(fs::File::open(&input_path)?) })
        .stdout(Stdio::from(fs::File::create(&output_path)?))
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|e| anyhow!("Couldn't start {}: {e}", cli.name()))?;
    let mut control = child.stdin.take();
    if let Some(stdin) = control.as_mut() {
        use std::io::Write;
        stdin.write_all(input.as_bytes())?;
        stdin.flush()?;
    }
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            if until.is_some() {
                let output = fs::read_to_string(&output_path).unwrap_or_default();
                if until.is_some_and(|f| f(&output)) {
                    return Ok(output);
                }
            }
            if !status.success() {
                return Err(failure(cli));
            }
            break;
        }
        let stop = |child: &mut std::process::Child| {
            let _ = child.kill();
            let _ = child.wait();
        };
        if cancel.load(Ordering::Relaxed) {
            stop(&mut child);
            bail!("Cancelled.");
        }
        if Instant::now() > deadline {
            stop(&mut child);
            bail!("{} took too long to respond. Try again; your meeting is still saved.", cli.name());
        }
        if fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0) > 2_000_000 {
            stop(&mut child);
            return Err(failure(cli));
        }
        if let Some(until) = until {
            let output = fs::read_to_string(&output_path).unwrap_or_default();
            if until(&output) {
                drop(control.take());
                stop(&mut child);
                return Ok(output);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let text = fs::read_to_string(if final_message { &final_path } else { &output_path })?.trim().to_string();
    if text.is_empty() {
        return Err(failure(cli));
    }
    Ok(text)
}

pub struct CliModel {
    pub cli: NotesCli,
    pub model: String,
    pub instructions: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeAuth {
    logged_in: bool,
    auth_method: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeResult {
    is_error: bool,
    result: Option<String>,
    #[serde(rename = "modelUsage")]
    model_usage: Option<HashMap<String, Value>>,
}

impl CliModel {
    fn response(&self, prompt: &str, cancel: &AtomicBool) -> Result<(String, String)> {
        let cli = self.cli;
        let launcher = executable(cli)
            .ok_or_else(|| anyhow!("Install {}, then sign in with {} in a terminal.", cli.name(), login_command(cli)))?;
        if cli == NotesCli::ClaudeCode {
            let status = run(&launcher, &["auth".into(), "status".into()], "", cli, false, Duration::from_secs(30), cancel, None)?;
            let ok = serde_json::from_str::<ClaudeAuth>(&status)
                .is_ok_and(|a| a.logged_in && a.auth_method.as_deref() == Some("claude.ai"));
            if !ok {
                bail!("Sign in to your Claude subscription with claude auth login in a terminal.");
            }
        }
        let chosen = self.model.trim();
        if chosen.chars().count() > 200 || chosen.chars().any(char::is_control) {
            bail!("Enter a model ID of at most 200 characters without control characters.");
        }
        let mut arguments: Vec<String> = match cli {
            NotesCli::ClaudeCode => [
                "--print", "--output-format", "json", "--no-session-persistence", "--safe-mode", "--tools", "",
                "--strict-mcp-config", "--disable-slash-commands", "--system-prompt", &self.instructions,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            NotesCli::Codex => [
                "--no-daemon", "exec", "--ignore-user-config", "--skip-git-repo-check", "--ephemeral", "--sandbox", "read-only",
                "--color", "never", "-c", "approval_policy=\"never\"", "-c", "forced_login_method=\"chatgpt\"",
                "-c", "project_doc_max_bytes=0", "-c", "web_search=\"disabled\"", "-c", "history.persistence=\"none\"",
                "--disable", "shell_tool", "--disable", "apps", "--disable", "plugins", "--disable", "multi_agent",
                "--disable", "hooks", "--disable", "skill_search",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        };
        if !chosen.is_empty() {
            arguments.extend(["--model".into(), chosen.into()]);
        }
        let input = format!("{}\n\n{prompt}", self.instructions);
        let output = run(&launcher, &arguments, &input, cli, cli == NotesCli::Codex, Duration::from_secs(600), cancel, None)?;
        if cli == NotesCli::ClaudeCode {
            let result: ClaudeResult = serde_json::from_str(&output).map_err(|_| failure(cli))?;
            let text = result.result.filter(|t| !result.is_error && !t.trim().is_empty()).ok_or_else(|| failure(cli))?;
            let mut used: Vec<String> = result.model_usage.map(|m| m.into_keys().collect()).unwrap_or_default();
            used.sort();
            let actual = used.join(", ");
            return Ok((text, format!("{} · {}", cli.name(), if actual.is_empty() { chosen } else { &actual })));
        }
        Ok((output, format!("{} · {chosen}", cli.name())))
    }
}

impl LanguageModel for CliModel {
    fn respond(&self, prompt: &str, cancel: &AtomicBool) -> Result<String> {
        Ok(self.response(prompt, cancel)?.0)
    }

    fn draft(&self, prompt: &str, cancel: &AtomicBool) -> Result<NotesDraft> {
        let (text, name) = self.response(&format!("{prompt}\n\n{FIELDS}"), cancel)?;
        let mut draft = NotesDraft::decode(&text)?;
        draft.generated_with = Some(name);
        Ok(draft)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliModelEntry {
    pub id: String,
    pub name: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliCatalog {
    pub models: Vec<CliModelEntry>,
    pub default_id: Option<String>,
}

fn unavailable(cli: NotesCli) -> anyhow::Error {
    anyhow!(
        "Couldn't list {} models. Update the CLI and sign in with {}, then refresh. You can also enter an exact model ID.",
        cli.name(),
        login_command(cli)
    )
}

/// Asks the signed-in CLI for its picker catalog. No inference or meeting text is sent.
pub fn catalog(cli: NotesCli) -> Result<CliCatalog> {
    let launcher = executable(cli).ok_or_else(|| unavailable(cli))?;
    let cancel = AtomicBool::new(false);
    match cli {
        NotesCli::ClaudeCode => {
            let args: Vec<String> = [
                "--print", "--safe-mode", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
                "--tools", "", "--strict-mcp-config", "--disable-slash-commands", "--no-session-persistence",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            let input = "{\"type\":\"control_request\",\"request_id\":\"models\",\"request\":{\"subtype\":\"initialize\"}}\n";
            let until = |output: &str| claude_catalog(output).is_ok();
            let output = run(&launcher, &args, input, cli, false, Duration::from_secs(30), &cancel, Some(&until))?;
            claude_catalog(&output)
        }
        NotesCli::Codex => {
            let mut models = Vec::new();
            let mut default_id = None;
            let mut cursor: Option<String> = None;
            let mut seen = std::collections::HashSet::new();
            loop {
                let mut params = json!({"limit": 100, "includeHidden": false});
                if let Some(cursor) = &cursor {
                    params["cursor"] = json!(cursor);
                }
                let messages = [
                    json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "opendictate", "version": "0.1.0"}}}),
                    json!({"method": "initialized"}),
                    json!({"id": 2, "method": "model/list", "params": params}),
                ];
                let input = messages.iter().map(Value::to_string).collect::<Vec<_>>().join("\n") + "\n";
                let args: Vec<String> = ["--no-daemon", "-c", "forced_login_method=\"chatgpt\"", "-c", "model_provider=\"openai\"", "app-server", "--stdio"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                let until = |output: &str| codex_reply(output).is_some();
                let output = run(&launcher, &args, &input, cli, false, Duration::from_secs(30), &cancel, Some(&until))?;
                let page = codex_reply(&output).and_then(|r| r.get("result").cloned()).ok_or_else(|| unavailable(cli))?;
                for model in page.get("data").and_then(Value::as_array).cloned().unwrap_or_default() {
                    if model.get("hidden").and_then(Value::as_bool) == Some(true) {
                        continue;
                    }
                    let id = model.get("model").and_then(Value::as_str).unwrap_or_default().to_string();
                    if default_id.is_none() && model.get("isDefault").and_then(Value::as_bool) == Some(true) {
                        default_id = Some(id.clone());
                    }
                    models.push(CliModelEntry {
                        name: model.get("displayName").and_then(Value::as_str).unwrap_or(&id).to_string(),
                        detail: model.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
                        id,
                    });
                }
                cursor = page.get("nextCursor").and_then(Value::as_str).map(String::from);
                match &cursor {
                    Some(c) if !seen.insert(c.clone()) => return Err(unavailable(cli)),
                    Some(_) => continue,
                    None => break,
                }
            }
            if models.is_empty() {
                return Err(unavailable(cli));
            }
            Ok(CliCatalog { models, default_id })
        }
    }
}

fn claude_catalog(output: &str) -> Result<CliCatalog> {
    for line in output.lines() {
        let Ok(envelope) = serde_json::from_str::<Value>(line) else { continue };
        if envelope.pointer("/response/request_id").and_then(Value::as_str) != Some("models") {
            continue;
        }
        let Some(entries) = envelope.pointer("/response/response/models").and_then(Value::as_array) else { continue };
        let mut seen = std::collections::HashSet::new();
        let mut models = Vec::new();
        let mut default_id = None;
        for entry in entries {
            let value = entry.get("value").and_then(Value::as_str).unwrap_or_default();
            let resolved = entry.get("resolvedModel").and_then(Value::as_str);
            if value == "default" {
                default_id = resolved.map(String::from);
                continue;
            }
            let id = resolved.unwrap_or(value).to_string();
            if seen.insert(id.clone()) {
                models.push(CliModelEntry {
                    name: entry.get("displayName").and_then(Value::as_str).unwrap_or(&id).to_string(),
                    detail: entry.get("description").and_then(Value::as_str).unwrap_or_default().to_string(),
                    id,
                });
            }
        }
        if models.is_empty() {
            return Err(unavailable(NotesCli::ClaudeCode));
        }
        return Ok(CliCatalog { models, default_id });
    }
    Err(unavailable(NotesCli::ClaudeCode))
}

fn codex_reply(output: &str) -> Option<Value> {
    output
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v.get("id").and_then(Value::as_i64) == Some(2))
}
