use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const PROVIDERS: [(&str, &str); 3] = [("codex", "Codex"), ("claude", "Claude Code"), ("pi", "Pi")];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub provider: String,
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    pub sessions: Vec<Session>,
    pub errors: Vec<String>,
}

pub fn status(agent_paths: &HashMap<String, String>) -> Vec<AgentInfo> {
    PROVIDERS
        .iter()
        .map(|(id, name)| {
            let path = agent_paths
                .get(*id)
                .and_then(|configured| resolve_executable(configured))
                .or_else(|| find_executable(id));
            AgentInfo {
                id: (*id).to_owned(),
                name: (*name).to_owned(),
                available: path.is_some(),
                path: path.map(|p| p.to_string_lossy().into_owned()),
            }
        })
        .collect()
}

pub fn list(
    project_dir: &Path,
    agent_paths: &HashMap<String, String>,
    resource_dir: &Path,
) -> Result<SessionList, String> {
    list_with_resolvers(project_dir, resource_dir, find_node, |provider| {
        agent_executable(provider, agent_paths)
    })
}

fn list_with_resolvers(
    project_dir: &Path,
    resource_dir: &Path,
    node_resolver: impl FnOnce() -> Result<PathBuf, String>,
    agent_resolver: impl Fn(&str) -> Option<PathBuf>,
) -> Result<SessionList, String> {
    let project_dir = historical_project_dir(project_dir)?;
    let providers = PROVIDERS
        .iter()
        .filter_map(|(provider, _)| agent_resolver(provider).map(|path| (*provider, path)))
        .collect::<Vec<_>>();
    if providers.is_empty() {
        return Ok(SessionList {
            sessions: Vec::new(),
            errors: Vec::new(),
        });
    }
    let node = node_resolver()?;
    let reader_dir = reader_resource_dir(resource_dir, cfg!(debug_assertions))?;
    let reader = reader_dir.join("session-reader.mjs");

    let mut sessions = Vec::new();
    let mut failures = Vec::new();
    for (provider, executable) in providers {
        let output = run_reader(
            Command::new(&node)
                .arg(&reader)
                .arg(provider)
                .arg(&project_dir)
                .arg(&reader_dir)
                .arg(&executable)
                .env("PATH", runtime_path(Some(&node))),
            provider,
        );
        collect_provider_output(provider, output, &mut sessions, &mut failures);
    }
    Ok(SessionList {
        sessions,
        errors: failures,
    })
}

fn reader_resource_dir(
    resource_dir: &Path,
    allow_source_fallback: bool,
) -> Result<PathBuf, String> {
    let reader = resource_dir.join("session-reader.mjs");
    if reader.is_file() {
        return Ok(resource_dir.to_path_buf());
    }
    if allow_source_fallback {
        let dev_resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        if dev_resources.join("session-reader.mjs").is_file() {
            return Ok(dev_resources);
        }
    }
    Err(format!("找不到会话读取器: {}", reader.display()))
}

fn collect_provider_output(
    provider: &str,
    output: Result<Output, String>,
    sessions: &mut Vec<Session>,
    errors: &mut Vec<String>,
) {
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            errors.push(format!("{provider}: {error}"));
            return;
        }
    };
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        errors.push(format!(
            "{provider}: {}",
            if detail.is_empty() {
                format!("会话读取器退出，状态：{}", output.status)
            } else {
                detail
            }
        ));
        return;
    }
    match serde_json::from_slice::<Vec<Session>>(&output.stdout) {
        Ok(found) => sessions.extend(found),
        Err(error) => errors.push(format!("{provider}: 会话读取器返回无效结果: {error}")),
    }
}

pub fn launch(
    project_dir: &Path,
    provider: &str,
    session_id: Option<&str>,
    agent_paths: &HashMap<String, String>,
    terminal_app: &str,
    repository_dirs: &[PathBuf],
    project_context: bool,
) -> Result<(), String> {
    if !PROVIDERS.iter().any(|(id, _)| *id == provider) {
        return Err(format!("不支持的 Agent: {provider}"));
    }
    let project_dir = canonical_project_dir(project_dir)?;
    let executable = agent_executable(provider, agent_paths)
        .ok_or_else(|| format!("未找到 {provider} 可执行文件"))?;
    let command = agent_command(
        provider,
        &executable,
        &project_dir,
        session_id,
        repository_dirs,
        project_context,
    )?;
    run_command(&project_dir, terminal_app, &command)
}

fn agent_command(
    provider: &str,
    executable: &Path,
    project_dir: &Path,
    session_id: Option<&str>,
    repository_dirs: &[PathBuf],
    project_context: bool,
) -> Result<String, String> {
    let command = match provider {
        "codex" => {
            let mut args = vec![
                shell_quote(executable.to_string_lossy().as_ref()),
                "-C".into(),
                shell_quote(project_dir.to_string_lossy().as_ref()),
            ];
            if project_context && !repository_dirs.is_empty() {
                args.push("--sandbox".into());
                args.push("workspace-write".into());
            }
            if project_context {
                for dir in repository_dirs {
                    args.extend([
                        "--add-dir".into(),
                        shell_quote(dir.to_string_lossy().as_ref()),
                    ]);
                }
            }
            if let Some(id) = session_id {
                args.extend(["resume".into(), shell_quote(id)]);
            }
            args.join(" ")
        }
        "claude" => {
            let mut args = vec![shell_quote(executable.to_string_lossy().as_ref())];
            if project_context {
                args.extend([
                    "--append-system-prompt".into(),
                    shell_quote(&claude_resource_prompt(project_dir)),
                ]);
            }
            if project_context && !repository_dirs.is_empty() {
                args.push("--add-dir".into());
                args.extend(
                    repository_dirs
                        .iter()
                        .map(|dir| shell_quote(dir.to_string_lossy().as_ref())),
                );
            }
            if let Some(id) = session_id {
                args.extend(["--resume".into(), shell_quote(id)]);
            }
            args.join(" ")
        }
        "pi" => {
            let mut args = vec![shell_quote(executable.to_string_lossy().as_ref())];
            if let Some(id) = session_id {
                args.extend(["--session".into(), shell_quote(id)]);
            }
            args.join(" ")
        }
        _ => return Err(format!("不支持的 Agent: {provider}")),
    };
    Ok(command)
}

pub fn open_terminal(project_dir: &Path, terminal_app: &str) -> Result<(), String> {
    let project_dir = canonical_project_dir(project_dir)?;
    run_command(&project_dir, terminal_app, "/bin/zsh -l")
}

pub fn run_command(project_dir: &Path, terminal_app: &str, command: &str) -> Result<(), String> {
    if terminal_app.trim().is_empty() {
        return Err("终端应用名称不能为空".into());
    }
    let project_dir = canonical_project_dir(project_dir)?;
    let file_path =
        std::env::temp_dir().join(format!("personal-workbench-{}.command", Uuid::new_v4()));
    let body = terminal_script(&project_dir, command);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file_path)
        .map_err(|e| format!("创建终端启动脚本失败: {e}"))?;
    file.write_all(body.as_bytes())
        .map_err(|e| format!("写入终端启动脚本失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file_path, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("设置终端启动脚本权限失败: {e}"))?;
    }
    let result = Command::new("/usr/bin/open")
        .arg("-a")
        .arg(terminal_app)
        .arg(&file_path)
        .status()
        .map_err(|e| format!("启动终端应用失败: {e}"))?;
    if result.success() {
        Ok(())
    } else {
        let _ = fs::remove_file(&file_path);
        Err(format!("终端应用启动失败，退出码: {}", result))
    }
}

fn canonical_project_dir(project_dir: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(project_dir).map_err(|e| format!("解析项目目录失败: {e}"))
}

fn claude_resource_prompt(project_dir: &Path) -> String {
    format!(
        "当前工作目录为：{}。本项目的关联信息记录在 `.personal-workbench.json`，请按任务需要读取其中的资源关联。关联资源入口目录为 `关联仓库`、`关联文档`、`关联附件`、`关联脚本`，其中可能包含指向工作台资源的符号链接。沿链接读取关联资源时仅按当前任务需要访问；不要修改或覆盖用户已有的项目规则。",
        project_dir.display()
    )
}

fn terminal_script(project_dir: &Path, command: &str) -> String {
    format!(
        "#!/bin/zsh\nexport PATH={}\ncd -- {} || exit 1\n{}\ncommand_status=$?\nrm -f -- \"$0\"\nexit $command_status\n",
        shell_quote(&runtime_path(None)),
        shell_quote(project_dir.to_string_lossy().as_ref()),
        command
    )
}

fn historical_project_dir(project_dir: &Path) -> Result<PathBuf, String> {
    if !project_dir.is_absolute() {
        return Err("项目目录必须是绝对路径".into());
    }
    let mut normalized = PathBuf::new();
    for component in project_dir.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err("项目目录路径无效".into());
                }
            }
        }
    }
    Ok(normalized)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn resolve_executable(value: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(value);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        return is_executable(&candidate).then_some(candidate);
    }
    find_executable(value)
}

fn agent_executable(provider: &str, agent_paths: &HashMap<String, String>) -> Option<PathBuf> {
    agent_executable_with(provider, agent_paths, find_executable)
}

fn agent_executable_with(
    provider: &str,
    agent_paths: &HashMap<String, String>,
    discover: impl FnOnce(&str) -> Option<PathBuf>,
) -> Option<PathBuf> {
    agent_paths
        .get(provider)
        .and_then(|configured| resolve_executable(configured))
        .or_else(|| discover(provider))
}

fn find_executable(name: &str) -> Option<PathBuf> {
    executable_search_dirs()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|path| is_executable(path))
}

fn executable_search_dirs() -> Vec<PathBuf> {
    let mut dirs = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.extend(nvm_bin_dirs());
    dirs
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return metadata.permissions().mode() & 0o111 != 0;
    }
    #[cfg(not(unix))]
    true
}

fn find_node() -> Result<PathBuf, String> {
    if let Some(node) = find_executable("node") {
        return Ok(node);
    }
    for dir in [Path::new("/opt/homebrew/bin"), Path::new("/usr/local/bin")] {
        let node = dir.join("node");
        if is_executable(&node) {
            return Ok(node);
        }
    }
    let mut candidates = nvm_bin_dirs()
        .into_iter()
        .map(|dir| dir.join("node"))
        .filter(|path| is_executable(path))
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    if let Some(node) = candidates.into_iter().next() {
        return Ok(node);
    }
    Err(
        "找不到 Node.js；请将 node 放入 PATH、/opt/homebrew/bin 或 ~/.nvm/versions/node/*/bin"
            .into(),
    )
}

fn runtime_path(node: Option<&Path>) -> String {
    let mut dirs = node
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .into_iter()
        .collect::<Vec<_>>();
    dirs.extend(executable_search_dirs());
    std::env::join_paths(dirs)
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
        .to_string_lossy()
        .into_owned()
}

fn nvm_bin_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let root = PathBuf::from(home).join(".nvm/versions/node");
    let mut dirs = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path().join("bin"))
        .collect::<Vec<_>>();
    dirs.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    dirs
}

fn run_reader(command: &mut Command, provider: &str) -> Result<Output, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 {provider} 会话读取器失败: {e}"))?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let out_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::BufReader::new(stdout).read_to_end(&mut bytes);
        bytes
    });
    let err_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::BufReader::new(stderr).read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(35);
    let status = loop {
        let result = match child.try_wait() {
            Ok(result) => result,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_reader.join();
                let _ = err_reader.join();
                return Err(format!("等待 {provider} 会话读取器失败: {error}"));
            }
        };
        match result {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_reader.join();
                let _ = err_reader.join();
                return Err(format!("{provider} 会话列表读取超时（35 秒）"));
            }
            None => thread::sleep(Duration::from_millis(40)),
        }
    };
    Ok(Output {
        status,
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

fn version_key(path: &Path) -> Vec<u64> {
    let version = path
        .ancestors()
        .find_map(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| name.starts_with('v'))
        })
        .unwrap_or_default();
    version
        .trim_start_matches('v')
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn shell_quote_handles_quotes_and_shell_metacharacters() {
        let value = "/tmp/a project's; $(touch nope)";
        assert_eq!(
            shell_quote(value),
            r#"'/tmp/a project'"'"'s; $(touch nope)'"#
        );
    }

    #[test]
    fn launch_builds_provider_specific_resume_commands() {
        let dir = tempdir().unwrap();
        let bin = dir.path().join("fake agent");
        fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
        }
        for provider in ["codex", "claude", "pi"] {
            let command = agent_command(
                provider,
                &bin,
                dir.path(),
                Some("id with ' quote"),
                &[],
                true,
            )
            .unwrap();
            assert!(command.contains("'\"'\"'"));
            match provider {
                "codex" => {
                    assert!(command.contains(" -C "));
                    assert!(!command.contains("--sandbox"));
                    assert!(command.contains(" resume "));
                }
                "claude" => {
                    assert!(command.contains(" --resume "));
                    assert!(command.contains("--append-system-prompt"));
                    assert!(command.contains(".personal-workbench.json"));
                    for directory in ["关联仓库", "关联文档", "关联附件", "关联脚本"]
                    {
                        assert!(command.contains(directory));
                    }
                }
                "pi" => assert!(command.contains(" --session ")),
                _ => unreachable!(),
            }
        }
        assert!(agent_command("other", &bin, dir.path(), None, &[], true).is_err());
    }

    #[test]
    fn repository_dirs_are_added_only_to_codex_and_claude_with_shell_quoting() {
        let project = Path::new("/tmp/project");
        let executable = Path::new("/tmp/fake agent");
        let repositories = [
            PathBuf::from("/tmp/repo with spaces"),
            PathBuf::from("/tmp/owner's repository"),
        ];
        for session_id in [None, Some("resume id")] {
            let codex = agent_command(
                "codex",
                executable,
                project,
                session_id,
                &repositories,
                true,
            )
            .unwrap();
            let claude = agent_command(
                "claude",
                executable,
                project,
                session_id,
                &repositories,
                true,
            )
            .unwrap();
            assert!(codex.contains("--add-dir '/tmp/repo with spaces'"));
            assert!(codex.contains("--add-dir '/tmp/owner'\"'\"'s repository'"));
            assert!(codex.contains("-C '/tmp/project' --sandbox workspace-write --add-dir"));
            assert!(claude
                .contains("--add-dir '/tmp/repo with spaces' '/tmp/owner'\"'\"'s repository'"));
            assert_eq!(codex.matches("--add-dir").count(), 2);
            assert_eq!(claude.matches("--add-dir").count(), 1);
            assert_eq!(codex.contains(" resume 'resume id'"), session_id.is_some());
            assert_eq!(
                claude.contains(" --resume 'resume id'"),
                session_id.is_some()
            );
            if session_id.is_some() {
                assert!(claude.contains("'s repository' --resume 'resume id'"));
            }
        }
        let pi = agent_command(
            "pi",
            executable,
            project,
            Some("resume id"),
            &repositories,
            true,
        )
        .unwrap();
        assert!(!pi.contains("--add-dir"));
        assert!(!pi.contains("--sandbox"));
        assert!(pi.contains("--session 'resume id'"));

        let codex_without_repositories =
            agent_command("codex", executable, project, Some("resume id"), &[], true).unwrap();
        assert!(!codex_without_repositories.contains("--sandbox"));
        assert!(codex_without_repositories.contains("-C '/tmp/project' resume 'resume id'"));
    }

    #[test]
    fn temporary_context_does_not_add_repository_access_or_project_prompt() {
        let project = Path::new("/tmp/isolated project");
        let executable = Path::new("/tmp/fake agent");
        let repositories = [PathBuf::from("/tmp/other repository")];

        let codex =
            agent_command("codex", executable, project, None, &repositories, false).unwrap();
        assert!(codex.contains("-C '/tmp/isolated project'"));
        assert!(!codex.contains("--add-dir"));
        assert!(!codex.contains("--sandbox"));
        assert!(!codex.contains("other repository"));

        let claude =
            agent_command("claude", executable, project, None, &repositories, false).unwrap();
        assert!(!claude.contains("--append-system-prompt"));
        assert!(!claude.contains(".personal-workbench.json"));
        assert!(!claude.contains("--add-dir"));
        assert!(!claude.contains("--sandbox"));
        assert!(!claude.contains("other repository"));

        let claude_resume = agent_command(
            "claude",
            executable,
            project,
            Some("session id"),
            &repositories,
            false,
        )
        .unwrap();
        assert!(claude_resume.contains(" --resume 'session id'"));
        assert!(!claude_resume.contains("--append-system-prompt"));

        let project_claude = agent_command("claude", executable, project, None, &[], true).unwrap();
        assert!(project_claude.contains("--append-system-prompt"));
        assert!(project_claude.contains(".personal-workbench.json"));
        for directory in ["关联仓库", "关联文档", "关联附件", "关联脚本"] {
            assert!(project_claude.contains(directory));
        }
    }

    #[test]
    fn agent_path_lookup_falls_back_when_provider_key_is_missing() {
        let paths = HashMap::new();
        assert_eq!(
            agent_executable_with("codex", &paths, |_| Some(PathBuf::from("/tmp/codex"))),
            Some(PathBuf::from("/tmp/codex"))
        );
    }

    #[test]
    fn missing_agents_returns_empty_without_resolving_node_or_reader() {
        let result = list_with_resolvers(
            Path::new("/tmp/nonexistent-project-history"),
            Path::new("/tmp/nonexistent-resources"),
            || Err("Node resolver must not run".into()),
            |_| None,
        )
        .unwrap();
        assert!(result.sessions.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn reader_source_fallback_is_explicitly_gated() {
        let missing_resources = Path::new("/tmp/nonexistent-agent-resources");
        assert!(reader_resource_dir(missing_resources, false).is_err());
        if cfg!(debug_assertions) {
            let fallback = reader_resource_dir(missing_resources, true).unwrap();
            assert_eq!(
                fallback,
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
            );
        }
    }

    #[test]
    fn session_list_keeps_successful_providers_and_reports_provider_failures() {
        use std::os::unix::process::ExitStatusExt;

        let mut sessions = Vec::new();
        let mut errors = Vec::new();
        let success_status = Command::new("/usr/bin/true").status().unwrap();
        collect_provider_output(
            "codex",
            Ok(Output {
                status: success_status,
                stdout: br#"[{"provider":"codex","id":"session-1","title":"isolated","updatedAt":"2026-11-06T00:00:00Z","cwd":"/tmp/project"}]"#.to_vec(),
                stderr: Vec::new(),
            }),
            &mut sessions,
            &mut errors,
        );
        collect_provider_output(
            "claude",
            Err("会话列表读取超时".into()),
            &mut sessions,
            &mut errors,
        );
        collect_provider_output(
            "pi",
            Ok(Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: b"mock provider failure".to_vec(),
            }),
            &mut sessions,
            &mut errors,
        );
        collect_provider_output(
            "codex-json",
            Ok(Output {
                status: success_status,
                stdout: b"not json".to_vec(),
                stderr: Vec::new(),
            }),
            &mut sessions,
            &mut errors,
        );

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "session-1");
        assert_eq!(errors.len(), 3);
        assert!(errors[0].contains("claude: 会话列表读取超时"));
        assert!(errors[1].contains("pi: mock provider failure"));
        assert!(errors[2].contains("codex-json: 会话读取器返回无效结果"));

        let value = serde_json::to_value(SessionList { sessions, errors }).unwrap();
        assert!(value.get("sessions").is_some());
        assert!(value.get("errors").is_some());
        assert!(value["sessions"][0].get("updatedAt").is_some());
    }

    #[test]
    fn status_returns_all_supported_providers_and_resolves_paths() {
        let dir = tempdir().unwrap();
        let bin = dir.path().join("tool");
        fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut paths = HashMap::new();
        paths.insert("pi".into(), bin.to_string_lossy().into_owned());
        let result = status(&paths);
        assert_eq!(result.len(), 3);
        assert!(result.iter().any(|item| item.id == "pi" && item.available));
    }

    #[test]
    fn historical_cwd_allows_missing_absolute_path_only() {
        let missing = Path::new("/tmp/a project that does not exist");
        assert_eq!(historical_project_dir(missing).unwrap(), missing);
        assert!(historical_project_dir(Path::new("relative/project")).is_err());
    }

    #[test]
    fn generated_terminal_script_quotes_paths_and_adds_runtime_path() {
        let cwd = Path::new("/tmp/a project's; $(touch nope)");
        let script = terminal_script(cwd, "exec '/tmp/fake cli'");
        assert!(script.contains(&format!("cd -- {}", shell_quote(cwd.to_str().unwrap()))));
        assert!(script.contains(&format!("export PATH={}", shell_quote(&runtime_path(None)))));
        assert!(script.contains("exec '/tmp/fake cli'"));
        assert!(script.contains("rm -f -- \"$0\""));
        assert!(script.contains("command_status=$?"));
        assert!(script.contains("exit $command_status"));
        assert!(!script.contains("\nstatus=$?"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn generated_terminal_script_preserves_exit_status_and_removes_itself() {
        use std::os::unix::fs::PermissionsExt;

        for expected_status in [0, 23] {
            let dir = tempdir().unwrap();
            let script_path = dir.path().join("terminal.command");
            let command = if expected_status == 0 {
                "true"
            } else {
                "(exit 23)"
            };
            fs::write(&script_path, terminal_script(dir.path(), command)).unwrap();
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700)).unwrap();

            let status = Command::new("/bin/zsh").arg(&script_path).status().unwrap();
            assert_eq!(status.code(), Some(expected_status));
            assert!(!script_path.exists());
        }
    }
}
