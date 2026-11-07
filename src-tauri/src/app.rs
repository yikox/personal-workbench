use crate::{agents, workspace};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, MutexGuard},
};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;
use workspace::{Project, Resource, ResourceKind, Snapshot, TemporaryDirectory};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    terminal_app: String,
    git_app: String,
    editor_app: String,
    agent_paths: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            terminal_app: "Terminal".into(),
            git_app: String::new(),
            editor_app: String::new(),
            agent_paths: ["codex", "claude", "pi"]
                .into_iter()
                .map(|id| (id.into(), String::new()))
                .collect(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Shortcut {
    id: String,
    title: String,
    kind: String,
    target: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AppConfig {
    root: Option<PathBuf>,
    settings: Settings,
    shortcuts: Vec<Shortcut>,
    project_paths: HashMap<String, Vec<PathBuf>>,
    temporary_paths: HashMap<String, Vec<PathBuf>>,
    recent_project_ids: Vec<String>,
}

struct AppState {
    path: PathBuf,
    config: Mutex<AppConfig>,
    mutation: Mutex<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BootState {
    root: Option<PathBuf>,
    snapshot: Option<Snapshot>,
    settings: Settings,
    shortcuts: Vec<Shortcut>,
    recent_project_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    name: String,
    path: String,
    is_dir: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageData {
    mime_type: String,
    data: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextSession {
    #[serde(flatten)]
    session: agents::Session,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temporary_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextSessionList {
    sessions: Vec<ContextSession>,
    errors: Vec<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| "工作台状态不可用，请重启应用".into())
}

fn persist(state: &AppState, config: &AppConfig) -> Result<()> {
    let parent = state.path.parent().ok_or("设置目录无效")?;
    fs::create_dir_all(parent).map_err(|e| format!("创建设置目录失败: {e}"))?;
    let temp = parent.join(format!(".settings-{}.tmp", Uuid::new_v4()));
    let data = serde_json::to_vec_pretty(config).map_err(|e| format!("保存设置失败: {e}"))?;
    fs::write(&temp, data).map_err(|e| format!("写入设置失败: {e}"))?;
    if let Err(e) = fs::rename(&temp, &state.path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("保存设置失败: {e}"));
    }
    Ok(())
}

fn change_config<T>(
    state: &AppState,
    change: impl FnOnce(&mut AppConfig) -> Result<T>,
) -> Result<T> {
    let mut current = lock(&state.config)?;
    let mut next = current.clone();
    let result = change(&mut next)?;
    persist(state, &next)?;
    *current = next;
    Ok(result)
}

fn current_root(state: &AppState) -> Result<PathBuf> {
    lock(&state.config)?
        .root
        .clone()
        .ok_or_else(|| "请先选择工作台目录".into())
}

fn project_at(root: &Path, id: &str) -> Result<Project> {
    workspace::snapshot(root)?
        .projects
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "项目不存在，请刷新后重试".into())
}

fn project_dir(root: &Path, id: &str) -> Result<PathBuf> {
    let project = project_at(root, id)?;
    workspace::resolve_workspace_path(root, &project.path)
}

fn check_project_revision(root: &Path, project_id: &str, expected_revision: &str) -> Result<()> {
    let project = project_at(root, project_id)?;
    if project.revision != expected_revision {
        return Err("项目配置已被外部修改，请刷新后重试".into());
    }
    Ok(())
}

fn validate_resource_name(name: &str) -> Result<()> {
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err("资源名称不能为空，且不能包含路径分隔符".into());
    }
    if matches!(
        name,
        ".git" | ".personal-workbench.json" | ".personal-workbench-temp.json"
    ) {
        return Err("不能使用工作台或 Git 保留名称".into());
    }
    Ok(())
}

fn validate_clone_url(url: &str) -> Result<()> {
    let value = url.trim();
    if value.is_empty() {
        return Err("请输入仓库克隆地址".into());
    }
    if let Some((scheme, rest)) = value.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        let authority = rest.split(&['/', '?', '#'][..]).next().unwrap_or("");
        let http_scheme = matches!(scheme.as_str(), "http" | "https");
        let credentials_present = authority
            .rsplit_once('@')
            .map(|(userinfo, _)| http_scheme || userinfo.contains(':'))
            .unwrap_or(false);
        if credentials_present || (http_scheme && (value.contains('?') || value.contains('#'))) {
            return Err("克隆地址不能内嵌密码或令牌，请使用本机 Git 认证".into());
        }
    }
    Ok(())
}

fn checked_clone_url(url: &str) -> Result<String> {
    validate_clone_url(url)?;
    Ok(url.trim().to_owned())
}

#[derive(Clone, Copy)]
struct StagedIdentity {
    device: u64,
    inode: u64,
}

struct StagedResource {
    path: PathBuf,
    name: String,
    identity: StagedIdentity,
}

fn staged_identity(metadata: &fs::Metadata) -> StagedIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        StagedIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
    #[cfg(not(unix))]
    {
        let created = metadata
            .created()
            .or_else(|_| metadata.modified())
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos() as u64)
            .unwrap_or(0);
        StagedIdentity {
            device: metadata.len(),
            inode: created,
        }
    }
}

fn stage_matches(path: &Path, identity: StagedIdentity, is_dir: bool) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| {
            !metadata.file_type().is_symlink()
                && metadata.is_dir() == is_dir
                && metadata.is_file() != is_dir
                && {
                    let actual = staged_identity(&metadata);
                    actual.device == identity.device && actual.inode == identity.inode
                }
        })
        .unwrap_or(false)
}

fn new_stage_path(root: &Path) -> Result<PathBuf> {
    for _ in 0..8 {
        let path = root.join(format!(".personal-workbench-stage-{}", Uuid::new_v4()));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Ok(_) => continue,
            Err(error) => return Err(format!("检查暂存路径失败: {error}")),
        }
    }
    Err("无法分配唯一暂存路径".into())
}

fn cleanup_failed_stage(path: &Path, identity: StagedIdentity, is_dir: bool) -> bool {
    if !stage_matches(path, identity, is_dir) {
        return false;
    }
    if is_dir {
        fs::remove_dir(path).is_ok()
    } else {
        fs::remove_file(path).is_ok()
    }
}

fn stage_failure(
    base_error: String,
    path: &Path,
    identity: StagedIdentity,
    is_dir: bool,
) -> String {
    if cleanup_failed_stage(path, identity, is_dir) {
        base_error
    } else {
        format!(
            "{base_error}；暂存内容未能安全清理，请检查 {}",
            path.display()
        )
    }
}

fn clone_repository_to_stage(root: &Path, name: String, url: String) -> Result<StagedResource> {
    let (stage, identity) = loop {
        let path = new_stage_path(root)?;
        match fs::create_dir(&path) {
            Ok(()) => {
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        return Err(format!(
                            "读取克隆暂存目录失败，暂存路径保留在 {}: {error}",
                            path.display()
                        ));
                    }
                };
                break (path, staged_identity(&metadata));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("创建克隆暂存目录失败: {error}")),
        }
    };

    let output = Command::new("/usr/bin/git")
        .arg("clone")
        .arg("--")
        .arg(&url)
        .arg(&stage)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output();

    match output {
        Ok(result) if result.status.success() => {
            if !stage_matches(&stage, identity, true) {
                return Err(format!(
                    "克隆暂存目录在操作期间被外部替换，已保留以供检查: {}",
                    stage.display()
                ));
            }
            Ok(StagedResource {
                path: stage,
                name,
                identity,
            })
        }
        Ok(result) => {
            let code = result
                .status
                .code()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "未知".into());
            Err(stage_failure(
                format!("Git 克隆失败（退出码 {code}），请检查地址和本机 Git 认证"),
                &stage,
                identity,
                true,
            ))
        }
        Err(_) => Err(stage_failure(
            "无法启动 Git 克隆，请检查本机 Git 安装".into(),
            &stage,
            identity,
            true,
        )),
    }
}

fn copy_attachment_to_stage(root: &Path, source: &Path) -> Result<StagedResource> {
    let source_metadata = fs::symlink_metadata(source).map_err(|_| "无法读取所选附件")?;
    if !source_metadata.is_file() || source_metadata.file_type().is_symlink() {
        return Err("所选附件必须是普通文件".into());
    }
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("附件文件名不是有效 UTF-8")?
        .to_owned();
    validate_resource_name(&name)?;
    let mut input = fs::File::open(source).map_err(|_| "无法打开所选附件")?;
    let input_metadata = input.metadata().map_err(|_| "无法检查所选附件")?;
    let selected_identity = staged_identity(&source_metadata);
    let opened_identity = staged_identity(&input_metadata);
    if selected_identity.device != opened_identity.device
        || selected_identity.inode != opened_identity.inode
    {
        return Err("所选附件在打开期间发生变化，请重新选择".into());
    }

    let mut last_collision = None;
    for _ in 0..8 {
        let stage = new_stage_path(root)?;
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last_collision = Some(error);
                continue;
            }
            Err(error) => return Err(format!("创建附件暂存文件失败: {error}")),
        };
        let output_metadata = match output.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                drop(output);
                return Err(format!(
                    "读取附件暂存文件失败，暂存路径保留在 {}: {error}",
                    stage.display()
                ));
            }
        };
        let identity = staged_identity(&output_metadata);
        let copy_result = (|| {
            io::copy(&mut input, &mut output).map_err(|e| format!("复制附件失败: {e}"))?;
            output
                .set_permissions(source_metadata.permissions())
                .map_err(|e| format!("保留附件文件权限失败: {e}"))?;
            output
                .sync_all()
                .map_err(|e| format!("写入附件暂存文件失败: {e}"))
        })();
        drop(output);
        if let Err(error) = copy_result {
            return Err(stage_failure(error, &stage, identity, false));
        }
        if !stage_matches(&stage, identity, false) {
            return Err(format!(
                "附件暂存文件在复制期间被外部替换，已保留以供检查: {}",
                stage.display()
            ));
        }
        return Ok(StagedResource {
            path: stage,
            name,
            identity,
        });
    }
    Err(format!(
        "无法分配唯一附件暂存文件: {}",
        last_collision
            .map(|error| error.to_string())
            .unwrap_or_else(|| "路径持续冲突".into())
    ))
}

fn link_created_resource(
    root: &Path,
    project_id: &str,
    expected_revision: &str,
    resource: &Resource,
) -> Result<()> {
    workspace::link_resource(
        root,
        project_id,
        expected_revision,
        resource.kind,
        &resource.path,
        None,
    )
    .map(|_| ())
    .map_err(|error| format!("资源已创建，但自动关联失败: {}: {error}", resource.path))
}

fn temporary_at(root: &Path, id: &str) -> Result<TemporaryDirectory> {
    workspace::snapshot(root)?
        .temporary_directories
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "临时目录不存在，请刷新后重试".into())
}

fn temporary_dir(root: &Path, id: &str) -> Result<PathBuf> {
    let item = temporary_at(root, id)?;
    workspace::resolve_workspace_path(root, &item.path)
}

fn boot(state: &AppState) -> Result<BootState> {
    let config = lock(&state.config)?.clone();
    let snapshot = config
        .root
        .as_deref()
        .map(workspace::snapshot)
        .transpose()?;
    Ok(BootState {
        root: config.root,
        snapshot,
        settings: config.settings,
        shortcuts: config.shortcuts,
        recent_project_ids: config.recent_project_ids,
    })
}

fn snapshot_after_resource_link(root: &Path) -> Result<Snapshot> {
    workspace::snapshot(root).map_err(|error| format!("资源已创建并关联，但刷新失败: {error}"))
}

#[tauri::command]
fn record_project_visit(state: State<'_, AppState>, project_id: String) -> Result<Vec<String>> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    project_at(&root, &project_id)?;
    change_config(&state, |config| {
        config.recent_project_ids.retain(|id| id != &project_id);
        config.recent_project_ids.insert(0, project_id);
        Ok(config.recent_project_ids.clone())
    })
}

#[tauri::command]
fn bootstrap(state: State<'_, AppState>) -> Result<BootState> {
    let _guard = lock(&state.mutation)?;
    boot(&state)
}

#[tauri::command]
async fn choose_workspace(app: tauri::AppHandle) -> Result<Option<BootState>> {
    let Some(selected) = app
        .dialog()
        .file()
        .set_title("选择新的工作台目录")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|e| format!("无法读取所选目录: {e}"))?;
    let path = fs::canonicalize(path).map_err(|e| format!("无法读取所选目录: {e}"))?;
    let state = app.state::<AppState>();
    let _guard = lock(&state.mutation)?;
    workspace::init_workspace(&path)?;
    change_config(&state, |config| {
        config.root = Some(path);
        Ok(())
    })?;
    boot(&state).map(Some)
}

#[tauri::command]
fn create_project(
    state: State<'_, AppState>,
    name: String,
    description: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::create_project(&root, &name, &description)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn create_project_entry(
    state: State<'_, AppState>,
    project_id: String,
    parent_path: String,
    name: String,
    is_dir: bool,
) -> Result<()> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    project_at(&root, &project_id)?;
    workspace::create_project_entry(&root, &project_id, &parent_path, &name, is_dir)
}

fn install_and_link_staged_resource(
    state: &AppState,
    root: &Path,
    project_id: &str,
    expected_revision: &str,
    kind: ResourceKind,
    staged: StagedResource,
) -> Result<Snapshot> {
    let active_root = current_root(state)?;
    if active_root != root {
        return Err(format!(
            "工作台已切换，暂存资源保留在 {}",
            staged.path.display()
        ));
    }
    let is_dir = kind == ResourceKind::Repository;
    if !stage_matches(&staged.path, staged.identity, is_dir) {
        return Err(format!(
            "暂存资源已被外部替换，拒绝安装并保留路径 {}",
            staged.path.display()
        ));
    }
    let resource = workspace::install_staged_resource(root, kind, &staged.name, &staged.path)
        .map_err(|error| {
            format!(
                "资源安装失败，暂存内容保留在 {}: {error}",
                staged.path.display()
            )
        })?;
    link_created_resource(root, project_id, expected_revision, &resource)?;
    snapshot_after_resource_link(root)
}

#[tauri::command]
async fn create_project_resource(
    app: tauri::AppHandle,
    project_id: String,
    expected_revision: String,
    kind: ResourceKind,
    name: String,
    clone_url: Option<String>,
) -> Result<Option<Snapshot>> {
    let root = {
        let state = app.state::<AppState>();
        let _guard = lock(&state.mutation)?;
        let root = current_root(&state)?;
        check_project_revision(&root, &project_id, &expected_revision)?;
        root
    };

    match kind {
        ResourceKind::Document | ResourceKind::Script => {
            if clone_url.is_some() {
                return Err("只有仓库资源可以使用克隆地址".into());
            }
            validate_resource_name(&name)?;
            let state = app.state::<AppState>();
            let _guard = lock(&state.mutation)?;
            if current_root(&state)? != root {
                return Err("工作台已切换，请刷新后重试".into());
            }
            check_project_revision(&root, &project_id, &expected_revision)?;
            let resource = workspace::create_resource(&root, kind, &name)?;
            link_created_resource(&root, &project_id, &expected_revision, &resource)?;
            snapshot_after_resource_link(&root).map(Some)
        }
        ResourceKind::Repository => {
            validate_resource_name(&name)?;
            let clone_url = clone_url.ok_or("请输入仓库克隆地址")?;
            let clone_url = checked_clone_url(&clone_url)?;
            match fs::symlink_metadata(root.join("仓库").join(&name)) {
                Ok(_) => return Err("同名仓库已存在，拒绝覆盖".into()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("检查仓库名称失败: {error}")),
            }
            let clone_root = root.clone();
            let staged = tauri::async_runtime::spawn_blocking(move || {
                clone_repository_to_stage(&clone_root, name, clone_url)
            })
            .await
            .map_err(|_| "Git 克隆后台任务异常结束")??;

            let state = app.state::<AppState>();
            let _guard = lock(&state.mutation)?;
            install_and_link_staged_resource(
                &state,
                &root,
                &project_id,
                &expected_revision,
                kind,
                staged,
            )
            .map(Some)
        }
        ResourceKind::Attachment => {
            if clone_url.is_some() {
                return Err("附件资源不能使用克隆地址".into());
            }
            let Some(selected) = app
                .dialog()
                .file()
                .set_title("选择要关联到项目的附件")
                .set_directory(&root)
                .blocking_pick_file()
            else {
                return Ok(None);
            };
            let source = selected.into_path().map_err(|_| "无法读取所选附件路径")?;
            let copy_root = root.clone();
            let staged = tauri::async_runtime::spawn_blocking(move || {
                copy_attachment_to_stage(&copy_root, &source)
            })
            .await
            .map_err(|_| "附件复制后台任务异常结束")??;

            let state = app.state::<AppState>();
            let _guard = lock(&state.mutation)?;
            install_and_link_staged_resource(
                &state,
                &root,
                &project_id,
                &expected_revision,
                kind,
                staged,
            )
            .map(Some)
        }
    }
}

fn remember_project_path(state: &AppState, project: &Project, root: &Path) -> Result<()> {
    let path = root.join(&project.path);
    change_config(state, |config| {
        let paths = config.project_paths.entry(project.id.clone()).or_default();
        if !paths.contains(&path) {
            paths.push(path);
        }
        Ok(())
    })
}

#[tauri::command]
fn update_project(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
    description: String,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    let prior = project_at(&root, &project_id)?;
    if prior.name != name {
        remember_project_path(&state, &prior, &root)?;
    }
    workspace::update_project(&root, &project_id, &expected_revision, &name, &description)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn rename_project(
    state: State<'_, AppState>,
    project_id: String,
    new_name: String,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    let prior = project_at(&root, &project_id)?;
    if prior.name != new_name {
        remember_project_path(&state, &prior, &root)?;
    }
    workspace::rename_project(&root, &project_id, &expected_revision, &new_name)?;
    workspace::snapshot(root)
}

fn move_to_trash(path: &Path) -> Result<()> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let value = path.to_str().ok_or("路径不是有效 UTF-8")?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(value));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn delete_project(
    state: State<'_, AppState>,
    project_id: String,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::delete_project(&root, &project_id, &expected_revision, move_to_trash)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn create_temporary(state: State<'_, AppState>, name: String) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::create_temporary(&root, &name)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn rename_temporary(
    state: State<'_, AppState>,
    temporary_id: String,
    new_name: String,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    let prior = temporary_at(&root, &temporary_id)?;
    if prior.name != new_name {
        let old_path = root.join(&prior.path);
        change_config(&state, |config| {
            let paths = config
                .temporary_paths
                .entry(temporary_id.clone())
                .or_default();
            if !paths.contains(&old_path) {
                paths.push(old_path);
            }
            Ok(())
        })?;
    }
    workspace::rename_temporary(&root, &temporary_id, &expected_revision, &new_name)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn delete_temporary(
    state: State<'_, AppState>,
    temporary_id: String,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::delete_temporary(&root, &temporary_id, &expected_revision, move_to_trash)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn create_resource(
    state: State<'_, AppState>,
    kind: ResourceKind,
    name: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    if kind == ResourceKind::Attachment {
        return Err("请在项目文件中将文件整理为附件".into());
    }
    let resource = workspace::create_resource(&root, kind, &name)?;
    if kind == ResourceKind::Repository {
        let output = Command::new("/usr/bin/git")
            .arg("init")
            .arg(root.join(&resource.path))
            .output()
            .map_err(|e| format!("仓库目录已创建，但无法启动 Git: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "仓库目录已创建，但 Git 初始化失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    workspace::snapshot(root)
}

#[tauri::command]
fn rename_resource(
    state: State<'_, AppState>,
    kind: ResourceKind,
    path: String,
    new_name: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::rename_resource(&root, kind, &path, &new_name)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn delete_resource(
    state: State<'_, AppState>,
    kind: ResourceKind,
    path: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::delete_resource(&root, kind, &path, move_to_trash)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn link_resource(
    state: State<'_, AppState>,
    project_id: String,
    kind: ResourceKind,
    path: String,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::link_resource(&root, &project_id, &expected_revision, kind, &path, None)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn unlink_resource(
    state: State<'_, AppState>,
    project_id: String,
    kind: ResourceKind,
    path: String,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    workspace::unlink_resource(&root, &project_id, &expected_revision, kind, &path)?;
    workspace::snapshot(root)
}

#[tauri::command]
fn list_directory(state: State<'_, AppState>, path: String) -> Result<Vec<Entry>> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    let dir = workspace::resolve_workspace_path(&root, &path)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("读取目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取目录失败: {e}"))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "文件名不是有效 UTF-8")?;
        if name == ".git"
            || name == ".personal-workbench.json"
            || name == ".personal-workbench-temp.json"
            || name.starts_with(".personal-workbench-stage-")
        {
            continue;
        }
        let relative = Path::new(&path).join(&name).to_string_lossy().into_owned();
        let is_dir = entry.path().is_dir();
        entries.push(Entry {
            name,
            path: relative,
            is_dir,
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

#[tauri::command]
fn read_text(state: State<'_, AppState>, path: String) -> Result<workspace::Document> {
    let _guard = lock(&state.mutation)?;
    workspace::read_text(current_root(&state)?, &path)
}

#[tauri::command]
fn write_text(
    state: State<'_, AppState>,
    path: String,
    content: String,
    expected_revision: String,
) -> Result<workspace::Document> {
    let _guard = lock(&state.mutation)?;
    workspace::write_text(
        current_root(&state)?,
        &path,
        &content,
        Some(&expected_revision),
    )
}

#[tauri::command]
fn read_image(state: State<'_, AppState>, path: String) -> Result<ImageData> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    let file = workspace::resolve_workspace_path(&root, &path)?;
    let mime = match file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err("不支持预览此图片格式".into()),
    };
    let meta = fs::metadata(&file).map_err(|e| format!("读取图片失败: {e}"))?;
    if !meta.is_file() || meta.len() > 20 * 1024 * 1024 {
        return Err("图片不是普通文件或超过 20 MB，请使用外部应用打开".into());
    }
    let bytes = fs::read(file).map_err(|e| format!("读取图片失败: {e}"))?;
    Ok(ImageData {
        mime_type: mime.into(),
        data: STANDARD.encode(bytes),
    })
}

fn open_path(path: &Path, application: Option<&str>) -> Result<()> {
    let mut command = Command::new("/usr/bin/open");
    if let Some(app) = application.filter(|s| !s.trim().is_empty()) {
        command.arg("-a").arg(app);
    }
    let result = command
        .arg(path)
        .status()
        .map_err(|e| format!("打开失败: {e}"))?;
    if result.success() {
        Ok(())
    } else {
        Err("系统无法打开指定项目或应用".into())
    }
}

#[tauri::command]
fn open_external(
    state: State<'_, AppState>,
    path: String,
    application: Option<String>,
) -> Result<()> {
    let target = {
        let _guard = lock(&state.mutation)?;
        workspace::resolve_workspace_path(current_root(&state)?, &path)?
    };
    open_path(&target, application.as_deref())
}

fn remove_staged_copy(source: &Path, target: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let same = fs::symlink_metadata(source)
            .ok()
            .zip(fs::symlink_metadata(target).ok())
            .map(|(a, b)| a.is_file() && b.is_file() && a.dev() == b.dev() && a.ino() == b.ino())
            .unwrap_or(false);
        same && fs::remove_file(target).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = (source, target);
        false
    }
}

#[tauri::command]
fn promote_file(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
    kind: ResourceKind,
    expected_revision: String,
) -> Result<Snapshot> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    let project = project_at(&root, &project_id)?;
    if project.revision != expected_revision {
        return Err("项目配置已被外部修改，请刷新后重试".into());
    }
    let folder = match kind {
        ResourceKind::Document => "文档",
        ResourceKind::Attachment => "附件",
        ResourceKind::Script => "脚本",
        ResourceKind::Repository => return Err("单个项目文件不能整理成仓库".into()),
    };
    let source = workspace::resolve_workspace_path(&root, &path)?;
    let project_dir = workspace::resolve_workspace_path(&root, &project.path)?;
    if !source.starts_with(&project_dir)
        || !source.is_file()
        || source.file_name().and_then(|s| s.to_str()) == Some(".personal-workbench.json")
    {
        return Err("只能整理当前项目内的普通文件".into());
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("文件名无效")?;
    if name.eq_ignore_ascii_case("AGENTS.md") {
        return Err("项目规则文件不能整理为独立资源".into());
    }
    if kind != ResourceKind::Attachment {
        let bytes = fs::read(&source).map_err(|e| format!("读取项目文件失败: {e}"))?;
        std::str::from_utf8(&bytes).map_err(|_| "文档和脚本必须是 UTF-8 文本")?;
    }
    let relative = format!("{folder}/{name}");
    let target = root.join(&relative);
    fs::hard_link(&source, &target)
        .map_err(|e| format!("创建资源失败（可能重名或不在同一磁盘）: {e}"))?;
    let updated = match workspace::link_resource(
        &root,
        &project_id,
        &expected_revision,
        kind,
        &relative,
        None,
    ) {
        Ok(p) => p,
        Err(e) => {
            return if remove_staged_copy(&source, &target) {
                Err(e)
            } else {
                Err(format!(
                    "{e}；为避免覆盖外部修改，请检查 {}",
                    target.display()
                ))
            };
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let unchanged = fs::symlink_metadata(&source)
            .ok()
            .zip(fs::metadata(&target).ok())
            .map(|(a, b)| a.is_file() && a.dev() == b.dev() && a.ino() == b.ino())
            .unwrap_or(false);
        if !unchanged {
            if workspace::unlink_resource(&root, &project_id, &updated.revision, kind, &relative)
                .is_ok()
            {
                let _ = remove_staged_copy(&source, &target);
            }
            return Err(format!(
                "项目文件在整理期间发生变化，请检查原文件和 {}",
                target.display()
            ));
        }
    }
    if let Err(e) = fs::remove_file(&source) {
        if workspace::unlink_resource(&root, &project_id, &updated.revision, kind, &relative)
            .is_ok()
        {
            let _ = remove_staged_copy(&source, &target);
        }
        return Err(format!(
            "无法从项目原位置移走文件，请检查原文件和 {}: {e}",
            target.display()
        ));
    }
    workspace::snapshot(root)
}

#[tauri::command]
fn agent_status(state: State<'_, AppState>) -> Result<Vec<agents::AgentInfo>> {
    Ok(agents::status(&lock(&state.config)?.settings.agent_paths))
}

fn agent_project(root: &Path, id: &str) -> Result<(PathBuf, Vec<PathBuf>)> {
    let project = project_at(root, id)?;
    let dir = workspace::resolve_workspace_path(root, &project.path)?;
    let repositories = project
        .resources
        .iter()
        .filter(|resource| resource.kind == ResourceKind::Repository)
        .map(|resource| workspace::resolve_workspace_path(root, &resource.path))
        .collect::<Result<Vec<_>>>()?;
    Ok((dir, repositories))
}

#[tauri::command]
fn start_agent(state: State<'_, AppState>, project_id: String, provider: String) -> Result<()> {
    let (dir, repositories, settings) = {
        let _guard = lock(&state.mutation)?;
        let root = current_root(&state)?;
        let (dir, repositories) = agent_project(&root, &project_id)?;
        (dir, repositories, lock(&state.config)?.settings.clone())
    };
    agents::launch(
        &dir,
        &provider,
        None,
        &settings.agent_paths,
        &settings.terminal_app,
        &repositories,
        true,
    )
}

#[tauri::command]
fn resume_agent(
    state: State<'_, AppState>,
    project_id: String,
    provider: String,
    session_id: String,
) -> Result<()> {
    let (dir, repositories, settings) = {
        let _guard = lock(&state.mutation)?;
        let root = current_root(&state)?;
        let (dir, repositories) = agent_project(&root, &project_id)?;
        (dir, repositories, lock(&state.config)?.settings.clone())
    };
    agents::launch(
        &dir,
        &provider,
        Some(&session_id),
        &settings.agent_paths,
        &settings.terminal_app,
        &repositories,
        true,
    )
}

fn launch_temporary_agent(
    state: &AppState,
    temporary_id: &str,
    provider: &str,
    session_id: Option<&str>,
) -> Result<()> {
    let (dir, settings) = {
        let _guard = lock(&state.mutation)?;
        let root = current_root(state)?;
        (
            temporary_dir(&root, temporary_id)?,
            lock(&state.config)?.settings.clone(),
        )
    };
    agents::launch(
        &dir,
        provider,
        session_id,
        &settings.agent_paths,
        &settings.terminal_app,
        &[],
        false,
    )
}

#[tauri::command]
fn start_temporary_agent(
    state: State<'_, AppState>,
    temporary_id: String,
    provider: String,
) -> Result<()> {
    launch_temporary_agent(&state, &temporary_id, &provider, None)
}

#[tauri::command]
fn resume_temporary_agent(
    state: State<'_, AppState>,
    temporary_id: String,
    provider: String,
    session_id: String,
) -> Result<()> {
    launch_temporary_agent(&state, &temporary_id, &provider, Some(&session_id))
}

#[derive(Clone, Copy)]
enum SessionContext {
    Project,
    Temporary,
}

fn list_context_sessions(
    app: &tauri::AppHandle,
    context_id: &str,
    kind: SessionContext,
) -> Result<ContextSessionList> {
    let state = app.state::<AppState>();
    let (dir, config, occupied) = {
        let _guard = lock(&state.mutation)?;
        let root = current_root(&state)?;
        let snapshot = workspace::snapshot(&root)?;
        let relative = match kind {
            SessionContext::Project => snapshot
                .projects
                .iter()
                .find(|item| item.id == context_id)
                .map(|item| &item.path),
            SessionContext::Temporary => snapshot
                .temporary_directories
                .iter()
                .find(|item| item.id == context_id)
                .map(|item| &item.path),
        }
        .ok_or("工作目录不存在，请刷新后重试")?;
        let dir = workspace::resolve_workspace_path(&root, relative)?;
        let config = lock(&state.config)?.clone();
        let occupied: HashSet<_> = snapshot
            .projects
            .iter()
            .map(|item| root.join(&item.path))
            .chain(
                snapshot
                    .temporary_directories
                    .iter()
                    .map(|item| root.join(&item.path)),
            )
            .filter(|path| path != &dir)
            .collect();
        (dir, config, occupied)
    };
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("读取应用资源失败: {e}"))?;
    let mut paths = match kind {
        SessionContext::Project => &config.project_paths,
        SessionContext::Temporary => &config.temporary_paths,
    }
    .get(context_id)
    .cloned()
    .unwrap_or_default();
    paths.retain(|p| !occupied.contains(p));
    if !paths.contains(&dir) {
        paths.push(dir);
    }
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        let listed = agents::list(&path, &config.settings.agent_paths, &resource_dir)?;
        errors.extend(
            listed
                .errors
                .into_iter()
                .map(|error| format!("{}: {error}", path.display())),
        );
        for session in listed.sessions {
            if seen.insert((session.provider.clone(), session.id.clone())) {
                result.push(ContextSession {
                    session,
                    project_id: matches!(kind, SessionContext::Project)
                        .then(|| context_id.to_owned()),
                    temporary_id: matches!(kind, SessionContext::Temporary)
                        .then(|| context_id.to_owned()),
                });
            }
        }
    }
    result.sort_by(|a, b| b.session.updated_at.cmp(&a.session.updated_at));
    Ok(ContextSessionList {
        sessions: result,
        errors,
    })
}

#[tauri::command]
async fn list_sessions(app: tauri::AppHandle, project_id: String) -> Result<ContextSessionList> {
    list_context_sessions(&app, &project_id, SessionContext::Project)
}

#[tauri::command]
async fn list_temporary_sessions(
    app: tauri::AppHandle,
    temporary_id: String,
) -> Result<ContextSessionList> {
    list_context_sessions(&app, &temporary_id, SessionContext::Temporary)
}

#[tauri::command]
fn open_terminal(state: State<'_, AppState>, project_id: String) -> Result<()> {
    let (dir, settings) = {
        let _guard = lock(&state.mutation)?;
        let root = current_root(&state)?;
        (
            project_dir(&root, &project_id)?,
            lock(&state.config)?.settings.clone(),
        )
    };
    agents::open_terminal(&dir, &settings.terminal_app)
}

#[tauri::command]
fn open_temporary_terminal(state: State<'_, AppState>, temporary_id: String) -> Result<()> {
    let (dir, settings) = {
        let _guard = lock(&state.mutation)?;
        let root = current_root(&state)?;
        (
            temporary_dir(&root, &temporary_id)?,
            lock(&state.config)?.settings.clone(),
        )
    };
    agents::open_terminal(&dir, &settings.terminal_app)
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    if settings.terminal_app.trim().is_empty() {
        return Err("请输入终端应用名称".into());
    }
    change_config(&state, |config| {
        config.settings = settings.clone();
        Ok(settings)
    })
}

#[tauri::command]
fn save_shortcut(state: State<'_, AppState>, shortcut: Shortcut) -> Result<Vec<Shortcut>> {
    if shortcut.id.trim().is_empty()
        || shortcut.title.trim().is_empty()
        || shortcut.target.trim().is_empty()
    {
        return Err("请填写快捷按钮名称和目标".into());
    }
    if !["project", "application", "terminal", "script", "command"]
        .contains(&shortcut.kind.as_str())
    {
        return Err("不支持的快捷按钮类型".into());
    }
    change_config(&state, |config| {
        if let Some(old) = config.shortcuts.iter_mut().find(|s| s.id == shortcut.id) {
            *old = shortcut;
        } else {
            config.shortcuts.push(shortcut);
        }
        Ok(config.shortcuts.clone())
    })
}

#[tauri::command]
fn delete_shortcut(state: State<'_, AppState>, id: String) -> Result<Vec<Shortcut>> {
    change_config(&state, |config| {
        config.shortcuts.retain(|s| s.id != id);
        Ok(config.shortcuts.clone())
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[tauri::command]
fn run_shortcut(state: State<'_, AppState>, id: String, project_id: Option<String>) -> Result<()> {
    let _guard = lock(&state.mutation)?;
    let root = current_root(&state)?;
    let config = lock(&state.config)?.clone();
    let shortcut = config
        .shortcuts
        .iter()
        .find(|s| s.id == id)
        .ok_or("快捷按钮不存在")?;
    let cwd = match project_id {
        Some(id) => project_dir(&root, &id)?,
        None => root.clone(),
    };
    match shortcut.kind.as_str() {
        "project" => open_path(&project_dir(&root, &shortcut.target)?, None),
        "application" => {
            let status = Command::new("/usr/bin/open")
                .arg("-a")
                .arg(&shortcut.target)
                .status()
                .map_err(|e| format!("打开应用失败: {e}"))?;
            if status.success() {
                Ok(())
            } else {
                Err("系统无法打开指定应用".into())
            }
        }
        "terminal" => agents::open_terminal(
            &project_dir(&root, &shortcut.target)?,
            &config.settings.terminal_app,
        ),
        "command" => agents::run_command(&cwd, &config.settings.terminal_app, &shortcut.target),
        "script" => {
            let resource = workspace::snapshot(&root)?
                .resources
                .into_iter()
                .find(|r| r.kind == ResourceKind::Script && r.path == shortcut.target)
                .ok_or("脚本资源不存在")?;
            let path = workspace::resolve_workspace_path(&root, &resource.path)?;
            agents::run_command(
                &cwd,
                &config.settings.terminal_app,
                &format!("/bin/zsh {}", shell_quote(&path.to_string_lossy())),
            )
        }
        _ => Err("不支持的快捷按钮类型".into()),
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{Menu, MenuItem, MenuItemKind};
                let menu = Menu::default(app.handle())?;
                if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
                    let items = app_menu.items()?;
                    if !items.is_empty() {
                        app_menu.remove_at(items.len() - 1)?;
                    }
                    let quit = MenuItem::with_id(
                        app,
                        "workbench-quit",
                        "退出 AI 工作台",
                        true,
                        Some("CmdOrCtrl+Q"),
                    )?;
                    app_menu.append(&quit)?;
                    app.set_menu(menu)?;
                }
            }
            let directory = match std::env::var_os("PERSONAL_WORKBENCH_CONFIG_DIR") {
                Some(path) => PathBuf::from(path),
                None => app.path().app_config_dir()?,
            };
            let path = directory.join("settings.json");
            let config = match fs::read(&path) {
                Ok(data) => {
                    serde_json::from_slice(&data).map_err(|e| format!("设置文件损坏: {e}"))?
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
                Err(e) => return Err(format!("读取设置失败: {e}").into()),
            };
            app.manage(AppState {
                path,
                config: Mutex::new(config),
                mutation: Mutex::new(()),
            });
            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "workbench-quit" {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            record_project_visit,
            choose_workspace,
            create_project,
            create_project_entry,
            create_project_resource,
            update_project,
            rename_project,
            delete_project,
            create_temporary,
            rename_temporary,
            delete_temporary,
            create_resource,
            rename_resource,
            delete_resource,
            link_resource,
            unlink_resource,
            list_directory,
            read_text,
            write_text,
            read_image,
            open_external,
            promote_file,
            agent_status,
            start_agent,
            list_sessions,
            resume_agent,
            start_temporary_agent,
            list_temporary_sessions,
            resume_temporary_agent,
            open_terminal,
            open_temporary_terminal,
            save_settings,
            save_shortcut,
            delete_shortcut,
            run_shortcut
        ])
        .build(tauri::generate_context!())
        .expect("启动 AI 工作台失败");
    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if let Some(window) = handle.get_webview_window("main") {
                api.prevent_exit();
                if let Err(error) = window.close() {
                    eprintln!("请求关闭工作台窗口失败: {error}");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_url_credentials_are_rejected_without_echoing_secrets() {
        for url in [
            "https://user:secret-token@example.invalid/org/repo.git",
            "https://example.invalid/org/repo.git?access_token=secret-token",
            "http://user:secret-token@example.invalid/org/repo.git",
            "ssh://user:secret-token@example.invalid/org/repo.git",
        ] {
            let error = validate_clone_url(url).unwrap_err();
            assert!(error.contains("本机 Git 认证"));
            assert!(!error.contains("secret-token"));
        }
        assert!(validate_clone_url("ssh://git@example.invalid/org/repo.git").is_ok());
        assert!(validate_clone_url("git@example.invalid:org/repo.git").is_ok());
        assert_eq!(
            checked_clone_url("  https://example.invalid/org/repo.git  ").unwrap(),
            "https://example.invalid/org/repo.git"
        );
    }

    #[test]
    fn copied_attachment_is_staged_installed_linked_and_keeps_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        workspace::init_workspace(&root).unwrap();
        let project = workspace::create_project(&root, "Project", "").unwrap();
        let source = dir.path().join("source.pdf");
        fs::write(&source, b"synthetic attachment").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        }

        let staged = copy_attachment_to_stage(&root, &source).unwrap();
        assert!(staged.path.starts_with(&root));
        assert!(staged
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".personal-workbench-stage-"));
        let resource = workspace::install_staged_resource(
            &root,
            ResourceKind::Attachment,
            &staged.name,
            &staged.path,
        )
        .unwrap();
        link_created_resource(&root, &project.id, &project.revision, &resource).unwrap();

        assert_eq!(fs::read(&source).unwrap(), b"synthetic attachment");
        assert_eq!(
            fs::read(root.join(&resource.path)).unwrap(),
            b"synthetic attachment"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(root.join(&resource.path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o640
            );
        }
        assert_eq!(
            workspace::snapshot(&root).unwrap().projects[0].resources[0].path,
            resource.path
        );
    }

    #[test]
    fn auto_link_failure_reports_partial_success_path_and_keeps_created_resource() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        workspace::init_workspace(&root).unwrap();
        let project = workspace::create_project(&root, "Project", "").unwrap();
        let resource =
            workspace::create_resource(&root, ResourceKind::Document, "note.md").unwrap();

        let error =
            link_created_resource(&root, &project.id, "stale-revision", &resource).unwrap_err();
        assert!(error.starts_with("资源已创建，但自动关联失败"));
        assert!(error.contains(&resource.path));
        assert!(error.contains("外部修改"));
        assert!(root.join(&resource.path).is_file());
        assert!(workspace::snapshot(&root).unwrap().projects[0]
            .resources
            .is_empty());
    }

    #[test]
    fn linked_resource_refresh_failure_has_a_distinct_partial_success_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        workspace::init_workspace(&root).unwrap();
        let project = workspace::create_project(&root, "Project", "").unwrap();
        let resource =
            workspace::create_resource(&root, ResourceKind::Document, "note.md").unwrap();
        link_created_resource(&root, &project.id, &project.revision, &resource).unwrap();
        fs::write(
            root.join(&project.path).join(".personal-workbench.json"),
            "invalid json",
        )
        .unwrap();

        let error = snapshot_after_resource_link(&root).unwrap_err();
        assert!(error.starts_with("资源已创建并关联，但刷新失败"));
        assert!(root.join(&resource.path).is_file());
        assert!(root.join(&project.path).join("关联文档/note.md").exists());
    }

    #[test]
    fn repository_clone_stages_installs_links_and_never_overwrites_existing_targets() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        workspace::init_workspace(&root).unwrap();
        let project = workspace::create_project(&root, "Project", "").unwrap();
        let source = dir.path().join("synthetic-repository");
        let templates = dir.path().join("empty-git-templates");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&templates).unwrap();
        fs::write(source.join("sample.txt"), "synthetic repository data").unwrap();

        let run_git = |arguments: &[&str]| {
            let output = Command::new("/usr/bin/git")
                .current_dir(&source)
                .args(arguments)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_TEMPLATE_DIR", &templates)
                .env("GIT_AUTHOR_NAME", "PersonalWorkbench Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "PersonalWorkbench Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .env("GIT_TERMINAL_PROMPT", "0")
                .stdin(Stdio::null())
                .output()
                .expect("run synthetic Git command");
            assert!(
                output.status.success(),
                "synthetic Git command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run_git(&["init", "--quiet"]);
        run_git(&["add", "--", "sample.txt"]);
        run_git(&["commit", "--quiet", "-m", "synthetic fixture"]);

        let source_content = fs::read(source.join("sample.txt")).unwrap();
        let staged = clone_repository_to_stage(
            &root,
            "linked-repository".into(),
            source.to_string_lossy().into_owned(),
        )
        .unwrap();
        let resource = workspace::install_staged_resource(
            &root,
            ResourceKind::Repository,
            &staged.name,
            &staged.path,
        )
        .unwrap();
        link_created_resource(&root, &project.id, &project.revision, &resource).unwrap();

        let installed = root.join(&resource.path);
        assert_eq!(
            fs::read(installed.join("sample.txt")).unwrap(),
            source_content
        );
        assert_eq!(fs::read(source.join("sample.txt")).unwrap(), source_content);
        assert!(installed.join(".git").is_dir());
        assert!(
            fs::symlink_metadata(root.join(&project.path).join("关联仓库/linked-repository"))
                .unwrap()
                .file_type()
                .is_symlink()
        );

        let occupied = root.join("仓库/existing-repository");
        fs::create_dir(&occupied).unwrap();
        fs::write(occupied.join("keep.txt"), "do not overwrite").unwrap();
        let second_stage = clone_repository_to_stage(
            &root,
            "existing-repository".into(),
            source.to_string_lossy().into_owned(),
        )
        .unwrap();
        assert!(workspace::install_staged_resource(
            &root,
            ResourceKind::Repository,
            "existing-repository",
            &second_stage.path
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(occupied.join("keep.txt")).unwrap(),
            "do not overwrite"
        );
        assert!(second_stage.path.join("sample.txt").is_file());
        assert_eq!(fs::read(source.join("sample.txt")).unwrap(), source_content);
    }

    #[test]
    fn staged_copy_cleanup_preserves_a_replaced_target() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, "original").unwrap();
        fs::hard_link(&source, &target).unwrap();
        fs::remove_file(&target).unwrap();
        fs::write(&target, "external change").unwrap();

        assert!(!remove_staged_copy(&source, &target));
        assert_eq!(fs::read_to_string(&source).unwrap(), "original");
        assert_eq!(fs::read_to_string(&target).unwrap(), "external change");
    }
}
