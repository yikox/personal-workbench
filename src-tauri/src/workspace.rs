//! Local filesystem core for PersonalWorkbench.
//!
//! All paths crossing this API are relative to the explicitly supplied workspace root.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io;
#[cfg(target_os = "macos")]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

pub type WorkspaceResult<T> = Result<T, String>;

const MARKER: &str = ".personal-workbench.json";
const TEMP_MARKER: &str = ".personal-workbench-temp.json";
const CATEGORIES: [&str; 5] = ["项目", "仓库", "文档", "附件", "脚本"];
const TEMP_CATEGORY: &str = "临时";
const CONFIG_VERSION: u32 = 1;

/// Move a file or directory only when the destination does not exist. The macOS
/// implementation uses the kernel's exclusive rename flag to close the check/use race.
fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let from = CString::new(from.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains NUL"))?;
        let to = CString::new(to.as_os_str().as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination path contains NUL")
        })?;
        let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination already exists",
            ));
        }
        fs::rename(from, to)
    }
}

#[cfg(target_os = "macos")]
fn c_path(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "路径包含 NUL 字节"))
}

#[cfg(target_os = "macos")]
fn open_directory(path: &Path) -> io::Result<OwnedFd> {
    let path = c_path(path)?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

#[cfg(target_os = "macos")]
fn open_directory_at(parent: &OwnedFd, name: &std::ffi::OsStr) -> io::Result<OwnedFd> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "目录名包含 NUL 字节"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

#[cfg(target_os = "macos")]
fn open_directory_chain(root: &OwnedFd, path: &Path) -> io::Result<OwnedFd> {
    let duplicate = unsafe { libc::fcntl(root.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut current = unsafe { OwnedFd::from_raw_fd(duplicate) };
    for component in path.components() {
        match component {
            Component::Normal(name) => current = open_directory_at(&current, name)?,
            Component::CurDir => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "目录路径不规范",
                ))
            }
        }
    }
    Ok(current)
}

#[cfg(target_os = "macos")]
fn create_entry_at(parent: &OwnedFd, name: &str, is_dir: bool) -> io::Result<()> {
    let name = std::ffi::CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "名称包含 NUL 字节"))?;
    let result = unsafe {
        if is_dir {
            libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755)
        } else {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o644,
            )
        }
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if !is_dir {
        drop(unsafe { OwnedFd::from_raw_fd(result) });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn rename_at_exclusive(
    source_dir: &OwnedFd,
    source_name: &str,
    destination_dir: &OwnedFd,
    destination_name: &str,
) -> io::Result<()> {
    let source_name = std::ffi::CString::new(source_name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "暂存名称包含 NUL 字节"))?;
    let destination_name = std::ffi::CString::new(destination_name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "资源名称包含 NUL 字节"))?;
    let result = unsafe {
        libc::renameatx_np(
            source_dir.as_raw_fd(),
            source_name.as_ptr(),
            destination_dir.as_raw_fd(),
            destination_name.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Repository,
    Document,
    Attachment,
    Script,
}

impl ResourceKind {
    fn category(self) -> &'static str {
        match self {
            Self::Repository => "仓库",
            Self::Document => "文档",
            Self::Attachment => "附件",
            Self::Script => "脚本",
        }
    }

    fn link_category(self) -> &'static str {
        match self {
            Self::Repository => "关联仓库",
            Self::Document => "关联文档",
            Self::Attachment => "关联附件",
            Self::Script => "关联脚本",
        }
    }

    fn is_file(self) -> bool {
        !matches!(self, Self::Repository)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub kind: ResourceKind,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLink {
    pub kind: ResourceKind,
    pub path: String,
    pub alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDocument {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub resources: Vec<ResourceLink>,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub path: String,
    pub content: String,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub root: String,
    pub projects: Vec<Project>,
    pub resources: Vec<Resource>,
    pub temporary_directories: Vec<TemporaryDirectory>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TemporaryDirectory {
    pub id: String,
    pub name: String,
    pub path: String,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceMarker {
    version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemporaryMarker {
    version: u32,
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectConfig {
    version: u32,
    id: String,
    name: String,
    description: String,
    resources: Vec<ResourceLink>,
}

#[derive(Debug, Clone)]
struct LinkChange {
    path: PathBuf,
    old_target: Option<PathBuf>,
    new_target: Option<PathBuf>,
}

#[derive(Debug)]
struct ProjectLinkPlan {
    project_dir: PathBuf,
    revision: String,
    changes: Vec<LinkChange>,
}

#[derive(Debug, Clone)]
struct RenameLinkChange {
    old_path: PathBuf,
    new_path: PathBuf,
    old_target: PathBuf,
    new_target: PathBuf,
}

#[derive(Debug)]
struct ProjectRenameTransaction {
    dir: PathBuf,
    prior_bytes: Vec<u8>,
    prior_revision: String,
    applied_revision: Option<String>,
    links: Vec<RenameLinkChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResourceIdentity {
    device: u64,
    inode: u64,
}

#[derive(Debug)]
struct CreatedPath {
    path: PathBuf,
    identity: ResourceIdentity,
    bytes: Option<Vec<u8>>,
}

fn err(context: &str, e: impl std::fmt::Display) -> String {
    format!("{context}: {e}")
}

fn validate_name(name: &str) -> WorkspaceResult<()> {
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err("名称不能为空，且不能包含路径分隔符".into());
    }
    Ok(())
}

fn root_path(root: &Path) -> WorkspaceResult<PathBuf> {
    if !root.is_absolute() {
        return Err("工作台根目录必须是绝对路径".into());
    }
    Ok(root.to_path_buf())
}

fn relative_path(path: &Path) -> WorkspaceResult<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "路径不是有效 UTF-8".into())
}

fn resolve(root: &Path, relative: &str) -> WorkspaceResult<PathBuf> {
    let p = Path::new(relative);
    if p.as_os_str().is_empty()
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("路径必须是工作台根目录内的规范相对路径".into());
    }
    let full = root.join(p);
    // Reject symlinks in every existing component; API paths must address managed entities directly.
    let mut cur = root.to_path_buf();
    for component in p.components() {
        if let Component::Normal(part) = component {
            cur.push(part);
            if fs::symlink_metadata(&cur)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(format!("路径经过符号链接，拒绝操作: {}", relative));
            }
        }
    }
    Ok(full)
}

fn resolve_read_target(root: &Path, relative: &str) -> WorkspaceResult<PathBuf> {
    let p = Path::new(relative);
    if p.as_os_str().is_empty()
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("路径必须是工作台根目录内的规范相对路径".into());
    }
    let canonical_root = fs::canonicalize(root).map_err(|e| err("解析工作台根目录失败", e))?;
    let mut current = root.to_path_buf();
    for component in p.components() {
        if let Component::Normal(part) = component {
            current.push(part);
            let meta =
                fs::symlink_metadata(&current).map_err(|e| err("文档或关联文件不存在", e))?;
            if meta.file_type().is_symlink() {
                let canonical =
                    fs::canonicalize(&current).map_err(|e| err("解析关联软链接失败", e))?;
                if !canonical.starts_with(&canonical_root) {
                    return Err("关联文件目标越出工作台根目录，拒绝访问".into());
                }
                let prefix = current
                    .strip_prefix(root)
                    .map_err(|e| err("关联路径越界", e))?;
                if !is_recorded_link(root, prefix, &canonical)? {
                    return Err("只允许通过项目配置中登记的关联软链接访问文件".into());
                }
            }
        }
    }
    let target = fs::canonicalize(root.join(p)).map_err(|e| err("解析工作台路径失败", e))?;
    if !target.starts_with(&canonical_root) {
        return Err("路径目标越出工作台根目录，拒绝访问".into());
    }
    Ok(target)
}

/// Resolve an existing workspace path for read-only directory, image, or external-open operations.
/// Managed project resource links are allowed only when every symlink ancestor matches the project config.
pub fn resolve_workspace_path(root: impl AsRef<Path>, path: &str) -> WorkspaceResult<PathBuf> {
    let root = root_path(root.as_ref())?;
    validate_layout(&root)?;
    if path.is_empty() {
        return fs::canonicalize(&root).map_err(|e| err("解析工作台根目录失败", e));
    }
    resolve_read_target(&root, path)
}

fn is_recorded_link(
    root: &Path,
    relative: &Path,
    canonical_target: &Path,
) -> WorkspaceResult<bool> {
    let parts = relative
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.len() < 3 || parts[0] != "项目" {
        return Ok(false);
    }
    let dir = root.join(parts[0]).join(parts[1]);
    if !fs::symlink_metadata(&dir)
        .map(|m| m.is_dir() && !m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Ok(false);
    }
    let project = match read_project(root, &dir) {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    if project.resources.iter().any(|resource| {
        resource.kind == ResourceKind::Document
            && resource.source_path.as_deref()
                == relative
                    .strip_prefix(Path::new(parts[0]).join(parts[1]))
                    .ok()
                    .and_then(Path::to_str)
            && fs::canonicalize(root.join(&resource.path)).ok().as_deref() == Some(canonical_target)
    }) {
        return Ok(true);
    }
    if parts.len() != 4 {
        return Ok(false);
    }
    let kind = match parts[2] {
        "关联仓库" => ResourceKind::Repository,
        "关联文档" => ResourceKind::Document,
        "关联附件" => ResourceKind::Attachment,
        "关联脚本" => ResourceKind::Script,
        _ => return Ok(false),
    };
    for link in project
        .resources
        .iter()
        .filter(|r| r.kind == kind && r.alias == parts[3])
    {
        let resource = match resolve(root, &link.path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if fs::canonicalize(resource)
            .map(|p| p == canonical_target)
            .unwrap_or(false)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_text_path(path: &Path, writable: bool) -> bool {
    let parts = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if writable && parts.iter().any(|part| *part == ".git") {
        return false;
    }
    if parts.is_empty() || parts.last() == Some(&MARKER) || parts.last() == Some(&TEMP_MARKER) {
        return false;
    }
    if parts.first() == Some(&TEMP_CATEGORY) {
        return parts.len() >= 3;
    }
    match parts[0] {
        "文档" | "脚本" => parts.len() == 2,
        "附件" => !writable && parts.len() == 2,
        "仓库" => parts.len() >= 3,
        "项目" if parts.len() >= 3 => match parts.get(2).copied() {
            Some("关联附件") => !writable,
            _ => true,
        },
        _ => false,
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_new_json(path: &Path, value: &impl Serialize) -> WorkspaceResult<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| err("序列化配置失败", e))?;
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| err("创建配置失败", e))?;
    f.write_all(&bytes).map_err(|e| err("写入配置失败", e))
}

fn write_new_text(path: &Path, content: &str) -> WorkspaceResult<()> {
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| err("创建项目 AGENTS.md 失败", e))?;
    f.write_all(content.as_bytes())
        .map_err(|e| err("写入项目 AGENTS.md 失败", e))
}

fn validate_layout(root: &Path) -> WorkspaceResult<()> {
    let marker =
        fs::symlink_metadata(root.join(MARKER)).map_err(|e| err("读取工作台标识失败", e))?;
    if !marker.is_file() || marker.file_type().is_symlink() {
        return Err("工作台标识必须是普通文件".into());
    }
    for category in CATEGORIES {
        let meta =
            fs::symlink_metadata(root.join(category)).map_err(|e| err("工作台分类目录缺失", e))?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(format!("工作台分类目录无效: {category}"));
        }
    }
    Ok(())
}

fn ensure_temporary_category(root: &Path) -> WorkspaceResult<PathBuf> {
    let path = root.join(TEMP_CATEGORY);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(path),
        Ok(_) => Err("临时分类必须是普通目录".into()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => match fs::create_dir(&path) {
            Ok(()) => Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(&path).map_err(|e| err("检查临时分类失败", e))?;
                if meta.is_dir() && !meta.file_type().is_symlink() {
                    Ok(path)
                } else {
                    Err("临时分类必须是普通目录".into())
                }
            }
            Err(e) => Err(err("创建临时分类失败", e)),
        },
        Err(e) => Err(err("检查临时分类失败", e)),
    }
}

fn read_temporary(path: &Path, root: &Path) -> WorkspaceResult<TemporaryDirectory> {
    let marker_path = path.join(TEMP_MARKER);
    let metadata =
        fs::symlink_metadata(&marker_path).map_err(|e| err("读取临时目录标识失败", e))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("临时目录标识必须是普通文件".into());
    }
    let bytes = fs::read(&marker_path).map_err(|e| err("读取临时目录标识失败", e))?;
    let marker: TemporaryMarker =
        serde_json::from_slice(&bytes).map_err(|e| err("临时目录标识损坏", e))?;
    if marker.version != CONFIG_VERSION {
        return Err("不支持的临时目录版本".into());
    }
    Uuid::parse_str(&marker.id).map_err(|_| "临时目录 UUID 无效")?;
    validate_name(&marker.name)?;
    let disk_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("临时目录名称不是有效 UTF-8")?;
    if disk_name != marker.name {
        return Err("临时目录名称与标识不一致".into());
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|e| err("临时目录路径越界", e))?;
    Ok(TemporaryDirectory {
        id: marker.id,
        name: marker.name,
        path: relative_path(relative)?,
        revision: hash(&bytes),
    })
}

fn temporary_dir(root: &Path, id: &str) -> WorkspaceResult<PathBuf> {
    validate_layout(root)?;
    Uuid::parse_str(id).map_err(|_| "临时目录 UUID 无效")?;
    let category = ensure_temporary_category(root)?;
    let mut found = None;
    for entry in fs::read_dir(&category).map_err(|e| err("读取临时目录分类失败", e))? {
        let path = entry.map_err(|e| err("读取临时目录分类失败", e))?.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => m,
            _ => continue,
        };
        let _ = meta;
        let marker = match path.join(TEMP_MARKER).symlink_metadata() {
            Ok(m) if m.is_file() && !m.file_type().is_symlink() => m,
            _ => continue,
        };
        let _ = marker;
        if read_temporary(&path, root)?.id == id {
            if found.is_some() {
                return Err("临时目录 UUID 重复，标识异常".into());
            }
            found = Some(path);
        }
    }
    found.ok_or_else(|| format!("未找到临时目录: {id}"))
}

fn temporary_marker_bytes(id: &str, name: &str) -> WorkspaceResult<Vec<u8>> {
    serde_json::to_vec_pretty(&TemporaryMarker {
        version: CONFIG_VERSION,
        id: id.to_owned(),
        name: name.to_owned(),
    })
    .map_err(|e| err("序列化临时目录标识失败", e))
}

fn save_temporary(
    dir: &Path,
    current: &TemporaryDirectory,
    new_name: &str,
) -> WorkspaceResult<TemporaryDirectory> {
    let marker_path = dir.join(TEMP_MARKER);
    let current_bytes = fs::read(&marker_path).map_err(|e| err("读取临时目录标识失败", e))?;
    if hash(&current_bytes) != current.revision {
        return Err("临时目录标识已被外部修改，请刷新后重试".into());
    }
    let bytes = temporary_marker_bytes(&current.id, new_name)?;
    let temp_path = dir.join(format!(".pw-temp-{}.tmp", Uuid::new_v4()));
    fs::write(&temp_path, &bytes).map_err(|e| err("写入临时目录标识失败", e))?;
    if let Err(e) = fs::rename(&temp_path, &marker_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err("更新临时目录标识失败", e));
    }
    Ok(TemporaryDirectory {
        id: current.id.clone(),
        name: new_name.to_owned(),
        path: current.path.clone(),
        revision: hash(&bytes),
    })
}

fn read_project(root: &Path, project_path: &Path) -> WorkspaceResult<Project> {
    let cfg_path = project_path.join(MARKER);
    let bytes =
        fs::read(&cfg_path).map_err(|e| format!("读取项目配置失败 {}: {e}", cfg_path.display()))?;
    let cfg: ProjectConfig = serde_json::from_slice(&bytes)
        .map_err(|e| format!("项目配置损坏 {}: {e}", cfg_path.display()))?;
    if cfg.version != CONFIG_VERSION {
        return Err(format!("不支持的项目配置版本: {}", cfg_path.display()));
    }
    Uuid::parse_str(&cfg.id).map_err(|_| format!("项目 UUID 无效: {}", cfg_path.display()))?;
    let rel = project_path
        .strip_prefix(root)
        .map_err(|e| err("项目路径越界", e))?;
    Ok(Project {
        id: cfg.id,
        name: cfg.name,
        description: cfg.description,
        path: relative_path(rel)?,
        resources: cfg.resources,
        revision: hash(&bytes),
    })
}

fn config_from(project: &Project) -> ProjectConfig {
    ProjectConfig {
        version: CONFIG_VERSION,
        id: project.id.clone(),
        name: project.name.clone(),
        description: project.description.clone(),
        resources: project.resources.clone(),
    }
}

fn project_dir(root: &Path, id: &str) -> WorkspaceResult<PathBuf> {
    let mut found = None;
    for e in fs::read_dir(root.join("项目")).map_err(|e| err("读取项目目录失败", e))? {
        let p = e.map_err(|e| err("读取项目目录失败", e))?.path();
        if !fs::symlink_metadata(&p)
            .map(|m| m.is_dir() && !m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        if !fs::symlink_metadata(p.join(MARKER))
            .map(|m| m.is_file() && !m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let project = read_project(root, &p)?;
        if project.id == id {
            if found.is_some() {
                return Err("项目 UUID 重复，配置异常".into());
            }
            found = Some(p);
        }
    }
    found.ok_or_else(|| format!("未找到项目: {id}"))
}

fn load_project(root: &Path, id: &str, expected: &str) -> WorkspaceResult<(PathBuf, Project)> {
    let dir = project_dir(root, id)?;
    let project = read_project(root, &dir)?;
    if project.revision != expected {
        return Err("项目配置已被外部修改，请刷新后重试".into());
    }
    Ok((dir, project))
}

fn save_project(dir: &Path, project: &Project) -> WorkspaceResult<Project> {
    let target = dir.join(MARKER);
    let current =
        fs::read(&target).map_err(|e| format!("读取项目配置失败 {}: {e}", target.display()))?;
    if hash(&current) != project.revision {
        return Err("项目配置已被外部修改，请刷新后重试".into());
    }
    let bytes =
        serde_json::to_vec_pretty(&config_from(project)).map_err(|e| err("序列化配置失败", e))?;
    let temp = dir.join(format!(".pw-config-{}.tmp", Uuid::new_v4()));
    fs::write(&temp, &bytes).map_err(|e| err("写入临时配置失败", e))?;
    if let Err(e) = fs::rename(&temp, &target) {
        let _ = fs::remove_file(&temp);
        return Err(err("更新项目配置失败", e));
    }
    let mut updated = project.clone();
    updated.revision = hash(&bytes);
    Ok(updated)
}

/// Initialize only an empty directory, or return an already initialized workspace.
pub fn init_workspace(root: impl AsRef<Path>) -> WorkspaceResult<Snapshot> {
    let root = root_path(root.as_ref())?;
    fs::create_dir_all(&root).map_err(|e| err("创建工作台根目录失败", e))?;
    let marker = root.join(MARKER);
    if fs::symlink_metadata(&marker).is_ok() {
        validate_layout(&root)?;
        let m: WorkspaceMarker =
            serde_json::from_slice(&fs::read(&marker).map_err(|e| err("读取工作台标识失败", e))?)
                .map_err(|e| err("工作台标识损坏", e))?;
        if m.version != CONFIG_VERSION {
            return Err("不支持的工作台版本".into());
        }
        return snapshot(&root);
    }
    if fs::read_dir(&root)
        .map_err(|e| err("检查工作台目录失败", e))?
        .next()
        .is_some()
    {
        return Err("目录非空且没有 PersonalWorkbench 标识，拒绝初始化".into());
    }
    for name in CATEGORIES {
        fs::create_dir(root.join(name)).map_err(|e| err("创建工作台分类目录失败", e))?;
    }
    fs::create_dir(root.join(TEMP_CATEGORY)).map_err(|e| err("创建临时分类失败", e))?;
    write_new_json(
        &marker,
        &WorkspaceMarker {
            version: CONFIG_VERSION,
        },
    )?;
    snapshot(&root)
}

pub fn snapshot(root: impl AsRef<Path>) -> WorkspaceResult<Snapshot> {
    let root = root_path(root.as_ref())?;
    validate_layout(&root)?;
    let marker: WorkspaceMarker = serde_json::from_slice(
        &fs::read(root.join(MARKER)).map_err(|e| err("不是已初始化的工作台", e))?,
    )
    .map_err(|e| err("工作台标识损坏", e))?;
    if marker.version != CONFIG_VERSION {
        return Err("不支持的工作台版本".into());
    }
    let temporary_category = ensure_temporary_category(&root)?;
    let mut projects = Vec::new();
    for e in fs::read_dir(root.join("项目")).map_err(|e| err("读取项目目录失败", e))? {
        let p = e.map_err(|e| err("读取项目目录失败", e))?.path();
        if !fs::symlink_metadata(&p)
            .map(|m| m.is_dir() && !m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        if fs::symlink_metadata(p.join(MARKER))
            .map(|m| m.is_file() && !m.file_type().is_symlink())
            .unwrap_or(false)
        {
            let project = read_project(&root, &p)?;
            projects.push(project);
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    let mut temporary_directories = Vec::new();
    for entry in fs::read_dir(&temporary_category).map_err(|e| err("读取临时目录分类失败", e))?
    {
        let path = entry.map_err(|e| err("读取临时目录分类失败", e))?.path();
        if !fs::symlink_metadata(&path)
            .map(|m| m.is_dir() && !m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let marker_path = path.join(TEMP_MARKER);
        if fs::symlink_metadata(&marker_path)
            .map(|m| m.is_file() && !m.file_type().is_symlink())
            .unwrap_or(false)
        {
            temporary_directories.push(read_temporary(&path, &root)?);
        }
    }
    temporary_directories.sort_by(|a, b| a.name.cmp(&b.name));

    let mut project_ids = HashMap::new();
    for project in &projects {
        let config_path = root.join(&project.path).join(MARKER);
        if let Some(previous) = project_ids.insert(project.id.as_str(), config_path.clone()) {
            return Err(format!(
                "项目 UUID 重复，配置文件冲突: {} 和 {}",
                previous.display(),
                config_path.display()
            ));
        }
    }
    let mut temporary_ids = HashMap::new();
    for temporary in &temporary_directories {
        let marker_path = root.join(&temporary.path).join(TEMP_MARKER);
        if let Some(previous) = temporary_ids.insert(temporary.id.as_str(), marker_path.clone()) {
            return Err(format!(
                "临时目录 UUID 重复，标识文件冲突: {} 和 {}",
                previous.display(),
                marker_path.display()
            ));
        }
    }

    // Complete project and temporary configuration preflight before reconciliation mutates links.
    reconcile_project_links(&root, &projects)?;
    let mut resources = Vec::new();
    for kind in [
        ResourceKind::Repository,
        ResourceKind::Document,
        ResourceKind::Attachment,
        ResourceKind::Script,
    ] {
        for e in fs::read_dir(root.join(kind.category())).map_err(|e| err("读取资源目录失败", e))?
        {
            let p = e.map_err(|e| err("读取资源目录失败", e))?.path();
            let meta = match fs::symlink_metadata(&p) {
                Ok(m) if !m.file_type().is_symlink() => m,
                _ => continue,
            };
            if kind.is_file() != meta.is_file() {
                continue;
            }
            resources.push(Resource {
                kind,
                name: p
                    .file_name()
                    .and_then(|x| x.to_str())
                    .ok_or("资源名称不是有效 UTF-8")?
                    .to_owned(),
                path: relative_path(p.strip_prefix(&root).map_err(|e| err("资源路径异常", e))?)?,
            });
        }
    }
    resources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Snapshot {
        root: relative_path(&root)?,
        projects,
        resources,
        temporary_directories,
    })
}

fn walk_markdown(
    dir: &Path,
    project_root: &Path,
    relative_prefix: &str,
    found: &mut Vec<DiscoveredDocument>,
) -> WorkspaceResult<()> {
    for entry in fs::read_dir(dir).map_err(|e| err("扫描项目文档失败", e))? {
        let path = entry.map_err(|e| err("扫描项目文档失败", e))?.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("项目文件名不是有效 UTF-8")?;
        let meta = fs::symlink_metadata(&path).map_err(|e| err("读取项目文档信息失败", e))?;
        if meta.file_type().is_symlink() || name.starts_with('.') {
            continue;
        }
        if meta.is_dir() {
            if matches!(
                name,
                ".git"
                    | "node_modules"
                    | "dist"
                    | "build"
                    | "target"
                    | "vendor"
                    | "关联仓库"
                    | "关联文档"
                    | "关联附件"
                    | "关联脚本"
            ) {
                continue;
            }
            walk_markdown(&path, project_root, relative_prefix, found)?;
        } else if meta.is_file()
            && !name.eq_ignore_ascii_case("AGENTS.md")
            && (name.to_ascii_lowercase().ends_with(".md")
                || name.to_ascii_lowercase().ends_with(".markdown"))
        {
            let suffix = relative_path(
                path.strip_prefix(project_root)
                    .map_err(|e| err("项目文档路径异常", e))?,
            )?;
            let relative = if relative_prefix.is_empty() {
                suffix
            } else if suffix.is_empty() {
                relative_prefix.to_owned()
            } else {
                format!("{relative_prefix}/{suffix}")
            };
            found.push(DiscoveredDocument {
                name: name.into(),
                path: relative,
            });
        }
    }
    Ok(())
}

pub fn discover_markdown(
    root: impl AsRef<Path>,
    project_id: &str,
) -> WorkspaceResult<Vec<DiscoveredDocument>> {
    let root = root_path(root.as_ref())?;
    validate_layout(&root)?;
    let dir = project_dir(&root, project_id)?;
    let project = read_project(&root, &dir)?;
    let mut docs = Vec::new();
    walk_markdown(&dir, &dir, "", &mut docs)?;
    for link in project
        .resources
        .iter()
        .filter(|link| link.kind == ResourceKind::Repository)
    {
        let target = resolve(&root, &link.path)?;
        let link_path = link_path(&dir, ResourceKind::Repository, &link.alias);
        if !fs::symlink_metadata(&link_path)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false)
            || fs::read_link(&link_path).ok().as_deref() != Some(target.as_path())
        {
            continue;
        }
        let meta = fs::symlink_metadata(&target).map_err(|e| err("读取关联仓库失败", e))?;
        if meta.is_dir() && !meta.file_type().is_symlink() {
            let prefix = format!(
                "{}/{}",
                ResourceKind::Repository.link_category(),
                link.alias
            );
            walk_markdown(&target, &target, &prefix, &mut docs)?;
        }
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

pub fn create_temporary(root: impl AsRef<Path>, name: &str) -> WorkspaceResult<TemporaryDirectory> {
    validate_name(name)?;
    let root = root_path(root.as_ref())?;
    validate_layout(&root)?;
    let category = ensure_temporary_category(&root)?;
    let dir = category.join(name);
    fs::create_dir(&dir).map_err(|e| err("创建临时目录失败（可能重名）", e))?;
    let id = Uuid::new_v4().to_string();
    let marker = temporary_marker_bytes(&id, name)?;
    let marker_path = dir.join(TEMP_MARKER);
    let write_result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker_path)?;
        file.write_all(&marker)
    })();
    if let Err(e) = write_result {
        // Only remove the new empty directory; never recursively clean up unknown content.
        let _ = fs::remove_dir(&dir);
        return Err(err("创建临时目录标识失败", e));
    }
    read_temporary(&dir, &root)
}

pub fn rename_temporary(
    root: impl AsRef<Path>,
    id: &str,
    expected_revision: &str,
    new_name: &str,
) -> WorkspaceResult<TemporaryDirectory> {
    validate_name(new_name)?;
    let root = root_path(root.as_ref())?;
    let dir = temporary_dir(&root, id)?;
    let current = read_temporary(&dir, &root)?;
    if current.revision != expected_revision {
        return Err("临时目录标识已被外部修改，请刷新后重试".into());
    }
    let category = dir.parent().ok_or("临时目录路径异常")?;
    let new_dir = category.join(new_name);
    let identity = resource_identity(&dir)?;
    if new_dir != dir && path_exists_with_other_identity(&new_dir, identity)? {
        return Err("同名临时目录已存在".into());
    }
    if new_dir != dir {
        rename_exclusive(&dir, &new_dir).map_err(|e| err("重命名临时目录失败", e))?;
    }
    let renamed = TemporaryDirectory {
        name: new_name.to_owned(),
        path: relative_path(
            new_dir
                .strip_prefix(&root)
                .map_err(|e| err("临时目录路径异常", e))?,
        )?,
        ..current.clone()
    };
    match save_temporary(&new_dir, &renamed, new_name) {
        Ok(mut updated) => {
            updated.path = renamed.path;
            Ok(updated)
        }
        Err(e) if new_dir != dir => {
            match restore_project_directory_if_safe(&new_dir, &dir, identity) {
                Ok(()) => Err(e),
                Err(rollback) => Err(format!(
                    "临时目录更新失败: {e}；目录未回滚，需人工处理: {rollback}"
                )),
            }
        }
        Err(e) => Err(e),
    }
}

pub fn delete_temporary<F>(
    root: impl AsRef<Path>,
    id: &str,
    expected_revision: &str,
    trash: F,
) -> WorkspaceResult<()>
where
    F: FnOnce(&Path) -> Result<(), String>,
{
    let root = root_path(root.as_ref())?;
    let dir = temporary_dir(&root, id)?;
    let current = read_temporary(&dir, &root)?;
    if current.revision != expected_revision {
        return Err("临时目录标识已被外部修改，请刷新后重试".into());
    }
    trash(&dir).map_err(|e| format!("移入废纸篓失败，临时目录未变更: {e}"))
}

fn normalize_absolute(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            Component::Normal(part) => result.push(part),
        }
    }
    Some(result)
}

fn confirmed_link_target(root: &Path, kind: ResourceKind, link: &Path) -> WorkspaceResult<PathBuf> {
    let raw = fs::read_link(link).map_err(|e| err("读取关联软链接失败", e))?;
    let raw = if raw.is_absolute() {
        raw
    } else {
        link.parent().ok_or("关联软链接路径无效")?.join(raw)
    };
    let target = normalize_absolute(&raw).ok_or("关联软链接目标无效")?;
    let workspace = normalize_absolute(root).ok_or("工作台根路径无效")?;
    if target.parent() != Some(workspace.join(kind.category()).as_path()) {
        return Err("关联软链接不指向对应的工作台资源分类".into());
    }
    validate_name(
        target
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("关联软链接目标名称无效")?,
    )?;
    match fs::symlink_metadata(&target) {
        Ok(meta)
            if meta.file_type().is_symlink()
                || (kind.is_file() != meta.is_file())
                || (!kind.is_file() && !meta.is_dir()) =>
        {
            return Err("关联软链接目标类型不属于对应资源分类".into());
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(err("检查关联软链接目标失败", e)),
    }
    Ok(target)
}

fn plan_project_links(root: &Path, project: &Project) -> WorkspaceResult<ProjectLinkPlan> {
    validate_name(&project.name)?;
    let dir = resolve(root, &project.path)?;
    if !fs::symlink_metadata(&dir)
        .map(|m| m.is_dir() && !m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(format!("项目目录无效: {}", project.path));
    }
    let mut desired: HashMap<(ResourceKind, String), PathBuf> = HashMap::new();
    for resource in &project.resources {
        validate_name(&resource.alias)
            .map_err(|e| format!("项目 {} 的关联别名无效: {e}", project.name))?;
        let target = resolve(root, &resource.path).map_err(|e| {
            format!(
                "项目 {} 的关联资源路径无效 {}: {e}",
                project.name, resource.path
            )
        })?;
        let target = normalize_absolute(&target).ok_or_else(|| {
            format!(
                "项目 {} 的关联资源目标路径无效: {}",
                project.name, resource.path
            )
        })?;
        let meta = fs::symlink_metadata(&target).map_err(|e| {
            format!(
                "项目 {} 的关联资源不存在 {}: {e}",
                project.name, resource.path
            )
        })?;
        if meta.file_type().is_symlink()
            || (resource.kind.is_file() != meta.is_file())
            || (!resource.kind.is_file() && !meta.is_dir())
            || target.parent()
                != Some(
                    normalize_absolute(root)
                        .ok_or("工作台根路径无效")?
                        .join(resource.kind.category())
                        .as_path(),
                )
        {
            return Err(format!(
                "项目 {} 的关联资源类型或路径无效: {}",
                project.name, resource.path
            ));
        }
        let key = (resource.kind, resource.alias.clone());
        if desired.insert(key, target).is_some() {
            return Err(format!(
                "项目 {} 存在重复关联别名: {}",
                project.name, resource.alias
            ));
        }
    }

    let mut source_links = Vec::new();
    for resource in &project.resources {
        let Some(source_path) = resource.source_path.as_deref() else {
            continue;
        };
        if resource.kind != ResourceKind::Document {
            return Err("只有文档资源可以记录项目原路径".into());
        }
        let relative = Path::new(source_path);
        if source_path.is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(format!("项目原文档路径无效: {source_path}"));
        }
        let source = dir.join(relative);
        if !source.starts_with(&dir) {
            return Err("项目原文档路径越界".into());
        }
        let path_parts = relative
            .iter()
            .filter_map(|part| part.to_str())
            .collect::<Vec<_>>();
        let repository_alias =
            if path_parts.len() >= 3 && path_parts[0] == ResourceKind::Repository.link_category() {
                Some(path_parts[1].to_owned())
            } else {
                None
            };
        let target = resolve(root, &resource.path)?;
        source_links.push((source, target, repository_alias));
    }

    let mut changes = Vec::new();
    for kind in [
        ResourceKind::Repository,
        ResourceKind::Document,
        ResourceKind::Attachment,
        ResourceKind::Script,
    ] {
        let category = dir.join(kind.link_category());
        let category_meta =
            fs::symlink_metadata(&category).map_err(|e| err("项目关联目录缺失", e))?;
        if !category_meta.is_dir() || category_meta.file_type().is_symlink() {
            return Err(format!("项目关联目录无效: {}", kind.link_category()));
        }
        let mut present: HashMap<String, (PathBuf, PathBuf)> = HashMap::new();
        for entry in fs::read_dir(&category).map_err(|e| err("读取项目关联目录失败", e))?
        {
            let link = entry.map_err(|e| err("读取项目关联目录失败", e))?.path();
            let alias = link
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("关联别名不是有效 UTF-8")?
                .to_string();
            let meta = fs::symlink_metadata(&link).map_err(|e| err("读取项目关联项失败", e))?;
            if !meta.file_type().is_symlink() {
                if desired.contains_key(&(kind, alias.clone())) {
                    return Err(format!(
                        "项目关联别名被普通文件或目录占用: {}",
                        link.display()
                    ));
                }
                continue;
            }
            let target = confirmed_link_target(root, kind, &link)
                .map_err(|e| format!("无法确认关联软链接归属，拒绝修改 {}: {e}", link.display()))?;
            if present.insert(alias, (link.clone(), target)).is_some() {
                return Err("关联目录中存在重复名称".into());
            }
        }
        for ((desired_kind, alias), target) in desired.iter().filter(|((k, _), _)| *k == kind) {
            let _ = desired_kind;
            if let Some((path, old_target)) = present.remove(alias) {
                if old_target != *target {
                    changes.push(LinkChange {
                        path,
                        old_target: Some(old_target),
                        new_target: Some(target.clone()),
                    });
                }
            } else {
                let path = category.join(alias);
                if fs::symlink_metadata(&path).is_ok() {
                    return Err(format!("项目关联别名已被占用: {}", path.display()));
                }
                changes.push(LinkChange {
                    path,
                    old_target: None,
                    new_target: Some(target.clone()),
                });
            }
        }
        for (_, (path, old_target)) in present {
            changes.push(LinkChange {
                path,
                old_target: Some(old_target),
                new_target: None,
            });
        }
    }
    for (path, target, repository_alias) in source_links {
        let mut parent = path.parent().ok_or("项目原文档路径无效")?.to_path_buf();
        while parent != dir {
            let meta = fs::symlink_metadata(&parent).map_err(|e| err("项目原文档父目录缺失", e))?;
            if meta.file_type().is_symlink() {
                let alias = repository_alias.as_deref().ok_or_else(|| {
                    format!("项目原文档父目录是未登记的软链接: {}", parent.display())
                })?;
                if parent != link_path(&dir, ResourceKind::Repository, alias) {
                    return Err(format!(
                        "项目原文档父目录是未登记的软链接: {}",
                        parent.display()
                    ));
                }
                let repository = project
                    .resources
                    .iter()
                    .find(|resource| {
                        resource.kind == ResourceKind::Repository && resource.alias == alias
                    })
                    .ok_or("原文档所在仓库未登记")?;
                if fs::read_link(&parent).map_err(|e| err("读取关联仓库链接失败", e))?
                    != resolve(root, &repository.path)?
                {
                    return Err("原文档所在仓库链接已被修改".into());
                }
                break;
            }
            if !meta.is_dir() {
                return Err(format!("项目原文档父目录无效: {}", parent.display()));
            }
            parent = parent.parent().ok_or("项目原文档路径越界")?.to_path_buf();
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let old = fs::read_link(&path).map_err(|e| err("读取项目原文档链接失败", e))?;
                let old = if old.is_absolute() {
                    old
                } else {
                    path.parent().unwrap().join(old)
                };
                if old != target {
                    return Err(format!(
                        "项目原文档链接目标已变化，拒绝覆盖: {}",
                        path.display()
                    ));
                }
            }
            Ok(_) => {
                return Err(format!(
                    "项目原文档路径已被占用，拒绝覆盖: {}",
                    path.display()
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => changes.push(LinkChange {
                path,
                old_target: None,
                new_target: Some(target),
            }),
            Err(e) => return Err(err("检查项目原文档路径失败", e)),
        }
    }
    Ok(ProjectLinkPlan {
        project_dir: dir,
        revision: project.revision.clone(),
        changes,
    })
}

fn check_plan_revisions(plans: &[ProjectLinkPlan]) -> WorkspaceResult<()> {
    for plan in plans {
        let config_path = plan.project_dir.join(MARKER);
        let meta = fs::symlink_metadata(&config_path)
            .map_err(|e| format!("读取项目配置失败 {}: {e}", config_path.display()))?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("项目配置在关联协调期间被替换，拒绝继续".into());
        }
        let bytes = fs::read(&config_path)
            .map_err(|e| format!("读取项目配置失败 {}: {e}", config_path.display()))?;
        if hash(&bytes) != plan.revision {
            return Err("项目配置在关联协调期间发生变化，拒绝覆盖".into());
        }
        let config: ProjectConfig = serde_json::from_slice(&bytes)
            .map_err(|e| format!("项目配置损坏 {}: {e}", config_path.display()))?;
        if config.version != CONFIG_VERSION {
            return Err(format!(
                "项目配置版本在关联协调期间发生变化: {}",
                config_path.display()
            ));
        }
    }
    Ok(())
}

fn rollback_link_changes(changes: &[LinkChange]) -> Vec<String> {
    let mut failures = Vec::new();
    for change in changes.iter().rev() {
        if let Some(new_target) = &change.new_target {
            match fs::symlink_metadata(&change.path) {
                Ok(meta)
                    if meta.file_type().is_symlink()
                        && fs::read_link(&change.path).ok().as_ref() == Some(new_target) =>
                {
                    if let Err(e) = fs::remove_file(&change.path) {
                        failures.push(err("回滚时移除新链接失败", e));
                        continue;
                    }
                }
                Ok(_) => {
                    failures.push(format!(
                        "回滚时拒绝移除已被替换的路径: {}",
                        change.path.display()
                    ));
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    failures.push(err("回滚时检查链接失败", e));
                    continue;
                }
            }
        }
        if let Some(old_target) = &change.old_target {
            if fs::symlink_metadata(&change.path).is_ok() {
                failures.push(format!("回滚时拒绝覆盖已有路径: {}", change.path.display()));
            } else if let Err(e) = std::os::unix::fs::symlink(old_target, &change.path) {
                failures.push(err("恢复旧链接失败", e));
            }
        }
    }
    failures
}

fn reconcile_project_links(root: &Path, projects: &[Project]) -> WorkspaceResult<()> {
    let plans = projects
        .iter()
        .map(|p| plan_project_links(root, p))
        .collect::<WorkspaceResult<Vec<_>>>()?;
    check_plan_revisions(&plans)?;
    let mut applied = Vec::new();
    for plan in &plans {
        for change in &plan.changes {
            let current = fs::symlink_metadata(&change.path);
            match (&change.old_target, current) {
                (None, Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
                (Some(old), Ok(meta))
                    if meta.file_type().is_symlink()
                        && fs::read_link(&change.path).ok().as_ref() == Some(old) => {}
                _ => {
                    let rollback = rollback_link_changes(&applied);
                    let detail = if rollback.is_empty() {
                        String::new()
                    } else {
                        format!("；回滚问题: {}", rollback.join("; "))
                    };
                    return Err(format!("关联目录在协调期间发生变化，拒绝覆盖{detail}"));
                }
            }
            if change.old_target.is_some() {
                if let Err(e) = fs::remove_file(&change.path) {
                    let rollback = rollback_link_changes(&applied);
                    let detail = if rollback.is_empty() {
                        String::new()
                    } else {
                        format!("；回滚问题: {}", rollback.join("; "))
                    };
                    return Err(format!("移除旧关联失败: {e}{detail}"));
                }
            }
            if let Some(target) = &change.new_target {
                if let Err(e) = std::os::unix::fs::symlink(target, &change.path) {
                    if change.old_target.is_some() {
                        applied.push(change.clone());
                    }
                    let rollback = rollback_link_changes(&applied);
                    let detail = if rollback.is_empty() {
                        String::new()
                    } else {
                        format!("；回滚问题: {}", rollback.join("; "))
                    };
                    return Err(format!("创建关联软链接失败: {e}{detail}"));
                }
            }
            applied.push(change.clone());
        }
    }
    if let Err(e) = check_plan_revisions(&plans) {
        let rollback = rollback_link_changes(&applied);
        let detail = if rollback.is_empty() {
            String::new()
        } else {
            format!("；回滚问题: {}", rollback.join("; "))
        };
        return Err(format!("{e}{detail}"));
    }
    Ok(())
}

pub fn create_project(
    root: impl AsRef<Path>,
    name: &str,
    description: &str,
) -> WorkspaceResult<Project> {
    create_project_with_hook(root.as_ref(), name, description, |_, _| Ok(()))
}

/// Create a plain file or directory inside a project's real working tree.
/// Resource association directories are managed by the workspace and cannot be
/// used as destinations for project-owned files.
pub fn create_project_entry(
    root: impl AsRef<Path>,
    project_id: &str,
    parent_path: &str,
    name: &str,
    is_dir: bool,
) -> WorkspaceResult<()> {
    let root = root_path(root.as_ref())?;
    validate_layout(&root)?;
    validate_name(name)?;
    if name == ".git" || name == MARKER || name == TEMP_MARKER {
        return Err("不能创建工作台或 Git 保留名称".into());
    }

    let project = snapshot(&root)?
        .projects
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| "项目不存在".to_string())?;
    let project_dir = resolve(&root, &project.path)?;
    let parent = resolve(&root, parent_path)?;
    if parent != project_dir && !parent.starts_with(&project_dir) {
        return Err("只能在当前项目目录内创建内容".into());
    }
    let relative_parent = parent
        .strip_prefix(&project_dir)
        .map_err(|e| err("项目目录越界", e))?;
    let parent_components = relative_parent
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    let is_managed_root = matches!(
        parent_components.as_slice(),
        ["关联仓库" | "关联文档" | "关联附件" | "关联脚本", ..]
    );
    let contains_git_directory = parent_components.contains(&".git");
    if is_managed_root || contains_git_directory {
        return Err("不能在关联资源或 Git 托管目录中直接创建项目内容".into());
    }
    #[cfg(target_os = "macos")]
    {
        use std::io::Read;
        use std::os::fd::FromRawFd;
        let root_fd = open_directory(&root).map_err(|e| err("固定工作台目录失败", e))?;
        let project_relative = Path::new(&project.path);
        let project_fd = open_directory_chain(&root_fd, project_relative)
            .map_err(|e| err("固定项目目录失败", e))?;
        let marker_name = std::ffi::CString::new(MARKER).expect("static marker name");
        let marker_fd = unsafe {
            libc::openat(
                project_fd.as_raw_fd(),
                marker_name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if marker_fd < 0 {
            return Err(err("读取项目配置失败", io::Error::last_os_error()));
        }
        let mut bytes = Vec::new();
        unsafe { fs::File::from_raw_fd(marker_fd) }
            .read_to_end(&mut bytes)
            .map_err(|e| err("读取项目配置失败", e))?;
        let config: ProjectConfig =
            serde_json::from_slice(&bytes).map_err(|e| err("项目配置损坏", e))?;
        if config.version != CONFIG_VERSION || config.id != project_id {
            return Err("项目目录已变化，请刷新后重试".into());
        }
        let tail = Path::new(parent_path)
            .strip_prefix(project_relative)
            .map_err(|e| err("项目父目录越界", e))?;
        let parent_fd =
            open_directory_chain(&project_fd, tail).map_err(|e| err("固定项目父目录失败", e))?;
        create_entry_at(&parent_fd, name, is_dir)
            .map_err(|e| err("创建项目内容失败（可能重名或父目录已变化）", e))?;
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let parent_meta = fs::symlink_metadata(&parent).map_err(|e| err("项目父目录不存在", e))?;
        if !parent_meta.is_dir() || parent_meta.file_type().is_symlink() {
            return Err("项目父路径必须是实际目录，不能经过符号链接".into());
        }
        let destination = parent.join(name);
        if is_dir {
            fs::create_dir(&destination).map_err(|e| err("创建项目目录失败（可能重名）", e))
        } else {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map(|_| ())
                .map_err(|e| err("创建项目文件失败（可能重名）", e))
        }
    }
}

/// Install a staged repository or attachment into its managed resource category.
/// The staged entry must be a direct child of the workspace root and carry a UUID name.
pub fn install_staged_resource(
    root: impl AsRef<Path>,
    kind: ResourceKind,
    name: &str,
    staged: &Path,
) -> WorkspaceResult<Resource> {
    let root = root_path(root.as_ref())?;
    validate_layout(&root)?;
    validate_name(name)?;
    if !matches!(kind, ResourceKind::Repository | ResourceKind::Attachment) {
        return Err("只允许安装仓库或附件".into());
    }
    if staged.parent() != Some(root.as_path()) {
        return Err("暂存项必须直接位于工作台根目录".into());
    }
    let file_name = staged
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("暂存项名称无效")?;
    let uuid = file_name
        .strip_prefix(".personal-workbench-stage-")
        .ok_or("暂存项名称无效")?;
    Uuid::parse_str(uuid).map_err(|_| "暂存项 UUID 无效")?;

    let metadata = fs::symlink_metadata(staged).map_err(|e| err("读取暂存项失败", e))?;
    if metadata.file_type().is_symlink() {
        return Err("暂存项不能是符号链接".into());
    }
    let expected_dir = kind == ResourceKind::Repository;
    if metadata.is_dir() != expected_dir || (!metadata.is_dir() && !metadata.is_file()) {
        return Err("暂存项类型与资源类型不匹配".into());
    }

    let relative = Path::new(kind.category()).join(name);
    #[cfg(target_os = "macos")]
    {
        let root_fd = open_directory(&root).map_err(|e| err("固定工作台目录失败", e))?;
        let category_fd = open_directory_chain(&root_fd, Path::new(kind.category()))
            .map_err(|e| err("固定资源分类目录失败", e))?;
        let staged_name = staged
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("暂存项名称无效")?;
        rename_at_exclusive(&root_fd, staged_name, &category_fd, name)
            .map_err(|e| err("安装暂存资源失败（可能重名或分类目录已变化）", e))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let destination = root.join(&relative);
        rename_exclusive(staged, &destination)
            .map_err(|e| err("安装暂存资源失败（可能重名）", e))?;
    }
    Ok(Resource {
        kind,
        name: name.to_owned(),
        path: relative_path(&relative)?,
    })
}

fn create_project_with_hook<F>(
    root: &Path,
    name: &str,
    description: &str,
    mut after_step: F,
) -> WorkspaceResult<Project>
where
    F: FnMut(usize, &Path) -> Result<(), String>,
{
    validate_name(name)?;
    let root = root_path(root)?;
    let dir = root.join("项目").join(name);
    fs::create_dir(&dir).map_err(|e| err("创建项目目录失败（可能重名）", e))?;
    let dir_identity = resource_identity(&dir)?;
    let mut created_dirs = Vec::new();
    let mut created_files = Vec::new();
    let mut step = 0;
    for k in [
        ResourceKind::Repository,
        ResourceKind::Document,
        ResourceKind::Attachment,
        ResourceKind::Script,
    ] {
        let path = dir.join(k.link_category());
        if let Err(e) = fs::create_dir(&path) {
            let cleanup =
                cleanup_created_project(&dir, dir_identity, &created_dirs, &created_files);
            return Err(format!(
                "创建项目关联目录失败: {e}{}",
                cleanup_suffix(&cleanup)
            ));
        }
        created_dirs.push(CreatedPath {
            identity: resource_identity(&path)?,
            path: path.clone(),
            bytes: None,
        });
        if let Err(e) = after_step(step, &path) {
            let cleanup =
                cleanup_created_project(&dir, dir_identity, &created_dirs, &created_files);
            return Err(format!("项目创建中断: {e}{}", cleanup_suffix(&cleanup)));
        }
        step += 1;
    }
    const AGENT_TEXT: &str = "# 项目工作目录\n\n此目录是本工作台中的项目工作目录。项目关联信息记录在 `.personal-workbench.json`。\n\n关联资源入口：`关联仓库`、`关联文档`、`关联附件`、`关联脚本`。\n";
    let agent_path = dir.join("AGENTS.md");
    if let Err(e) = write_new_text(&agent_path, AGENT_TEXT) {
        let cleanup = cleanup_created_project(&dir, dir_identity, &created_dirs, &created_files);
        return Err(format!("{e}{}", cleanup_suffix(&cleanup)));
    }
    created_files.push(CreatedPath {
        identity: resource_identity(&agent_path)?,
        path: agent_path,
        bytes: Some(AGENT_TEXT.as_bytes().to_vec()),
    });
    if let Err(e) = after_step(step, &dir.join("AGENTS.md")) {
        let cleanup = cleanup_created_project(&dir, dir_identity, &created_dirs, &created_files);
        return Err(format!("项目创建中断: {e}{}", cleanup_suffix(&cleanup)));
    }
    let p = Project {
        id: Uuid::new_v4().to_string(),
        name: name.into(),
        description: description.into(),
        path: relative_path(
            dir.strip_prefix(&root)
                .map_err(|e| err("项目路径异常", e))?,
        )?,
        resources: vec![],
        revision: String::new(),
    };
    let marker_path = dir.join(MARKER);
    let marker_bytes =
        serde_json::to_vec_pretty(&config_from(&p)).map_err(|e| err("序列化配置失败", e))?;
    if let Err(e) = write_new_json(&marker_path, &config_from(&p)) {
        let cleanup = cleanup_created_project(&dir, dir_identity, &created_dirs, &created_files);
        return Err(format!("{e}{}", cleanup_suffix(&cleanup)));
    }
    created_files.push(CreatedPath {
        identity: resource_identity(&marker_path)?,
        path: marker_path.clone(),
        bytes: Some(marker_bytes),
    });
    if let Err(e) = after_step(step + 1, &marker_path) {
        let cleanup = cleanup_created_project(&dir, dir_identity, &created_dirs, &created_files);
        return Err(format!("项目创建中断: {e}{}", cleanup_suffix(&cleanup)));
    }
    read_project(&root, &dir)
}

fn cleanup_suffix(issues: &[String]) -> String {
    if issues.is_empty() {
        String::new()
    } else {
        format!("；部分清理已停止: {}", issues.join("；"))
    }
}

fn cleanup_created_project(
    dir: &Path,
    identity: ResourceIdentity,
    dirs: &[CreatedPath],
    files: &[CreatedPath],
) -> Vec<String> {
    let mut issues = Vec::new();
    match resource_identity(dir) {
        Ok(current) if current == identity => {}
        _ => return vec![format!("项目目录实体已变化，保留 {}", dir.display())],
    }
    for file in files.iter().rev() {
        let current = match resource_identity(&file.path) {
            Ok(id) if id == file.identity => id,
            _ => continue,
        };
        let _ = current;
        let Ok(meta) = fs::symlink_metadata(&file.path) else {
            continue;
        };
        if !meta.is_file() || meta.file_type().is_symlink() {
            continue;
        }
        if file
            .bytes
            .as_ref()
            .is_some_and(|expected| fs::read(&file.path).ok().as_ref() == Some(expected))
        {
            if let Err(e) = fs::remove_file(&file.path) {
                issues.push(err("移除本次创建的文件失败", e));
            }
        }
    }
    for created in dirs.iter().rev() {
        match resource_identity(&created.path) {
            Ok(id) if id == created.identity => {}
            Ok(_) => {
                issues.push(format!(
                    "关联目录实体已变化，保留 {}",
                    created.path.display()
                ));
                continue;
            }
            Err(_) if fs::symlink_metadata(&created.path).is_err() => continue,
            Err(e) => {
                issues.push(e);
                continue;
            }
        }
        match fs::symlink_metadata(&created.path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
                if let Err(e) = fs::remove_dir(&created.path) {
                    issues.push(format!(
                        "关联目录非空或已被修改，保留 {}: {e}",
                        created.path.display()
                    ));
                }
            }
            Ok(_) => issues.push(format!("关联目录已被替换，保留 {}", created.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => issues.push(err("检查关联目录失败", e)),
        }
    }
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            if let Err(e) = fs::remove_dir(dir) {
                if e.kind() != std::io::ErrorKind::DirectoryNotEmpty {
                    issues.push(format!("项目目录保留以保护其他内容 {}: {e}", dir.display()));
                }
            }
        }
        Ok(_) => issues.push(format!("项目目录已被替换，保留 {}", dir.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => issues.push(err("检查项目目录失败", e)),
    }
    issues
}

pub fn update_project(
    root: impl AsRef<Path>,
    id: &str,
    expected_revision: &str,
    name: &str,
    description: &str,
) -> WorkspaceResult<Project> {
    update_project_with_hook(
        root.as_ref(),
        id,
        expected_revision,
        name,
        description,
        |_, _| Ok(()),
    )
}

fn update_project_with_hook<F>(
    root: &Path,
    id: &str,
    expected_revision: &str,
    name: &str,
    description: &str,
    mut after_rename: F,
) -> WorkspaceResult<Project>
where
    F: FnMut(&Path, &Path) -> Result<(), String>,
{
    validate_name(name)?;
    let root = root_path(root)?;
    let (dir, mut p) = load_project(&root, id, expected_revision)?;
    let new_dir = root.join("项目").join(name);
    let identity = resource_identity(&dir)?;
    if new_dir != dir && path_exists_with_other_identity(&new_dir, identity)? {
        return Err("同名项目已存在".into());
    }
    p.name = name.into();
    p.description = description.into();
    p.path = relative_path(
        new_dir
            .strip_prefix(&root)
            .map_err(|e| err("项目路径异常", e))?,
    )?;
    if new_dir != dir {
        rename_exclusive(&dir, &new_dir).map_err(|e| err("重命名项目目录失败", e))?;
    }
    let operation = if new_dir != dir {
        after_rename(&dir, &new_dir)
    } else {
        Ok(())
    };
    let result = operation.and_then(|_| save_project(&new_dir, &p));
    match result {
        Ok(updated) => Ok(updated),
        Err(e) => {
            if new_dir != dir {
                return match restore_project_directory_if_safe(&new_dir, &dir, identity) {
                    Ok(()) => Err(e),
                    Err(rollback) => Err(format!(
                        "项目更新失败: {e}；项目目录未回滚，需人工处理: {rollback}"
                    )),
                };
            }
            Err(e)
        }
    }
}

fn restore_project_directory_if_safe(
    current: &Path,
    original: &Path,
    identity: ResourceIdentity,
) -> Result<(), String> {
    match fs::symlink_metadata(original) {
        Ok(_) if entry_identity(original).ok() == Some(identity) => {}
        Ok(_) => {
            return Err(format!(
                "原项目路径已被外部占用，保留 {}",
                original.display()
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(err("检查原项目路径失败", e)),
    }
    if resource_identity(current)? != identity {
        return Err(format!(
            "当前项目目录实体已变化，保留 {}",
            current.display()
        ));
    }
    if path_exists_with_other_identity(original, identity)? {
        return Err(format!(
            "原项目路径在回滚期间被占用，保留 {}",
            original.display()
        ));
    }
    rename_exclusive(current, original).map_err(|e| err("恢复原项目目录失败", e))
}

pub fn rename_project(
    root: impl AsRef<Path>,
    id: &str,
    expected_revision: &str,
    new_name: &str,
) -> WorkspaceResult<Project> {
    let root = root_path(root.as_ref())?;
    let (_, p) = load_project(&root, id, expected_revision)?;
    update_project(root, id, expected_revision, new_name, &p.description)
}

pub fn delete_project<F>(
    root: impl AsRef<Path>,
    id: &str,
    expected_revision: &str,
    trash: F,
) -> WorkspaceResult<()>
where
    F: FnOnce(&Path) -> Result<(), String>,
{
    let root = root_path(root.as_ref())?;
    let (dir, _) = load_project(&root, id, expected_revision)?;
    trash(&dir).map_err(|e| format!("移入废纸篓失败，项目配置未变更: {e}"))
}

pub fn create_resource(
    root: impl AsRef<Path>,
    kind: ResourceKind,
    name: &str,
) -> WorkspaceResult<Resource> {
    validate_name(name)?;
    let root = root_path(root.as_ref())?;
    let path = root.join(kind.category()).join(name);
    if kind.is_file() {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| err("创建资源失败（可能重名）", e))?;
    } else {
        fs::create_dir(&path).map_err(|e| err("创建资源失败（可能重名）", e))?;
    }
    Ok(Resource {
        kind,
        name: name.into(),
        path: relative_path(
            path.strip_prefix(&root)
                .map_err(|e| err("资源路径异常", e))?,
        )?,
    })
}

fn project_resources(root: &Path, id: &str, expected: &str) -> WorkspaceResult<(PathBuf, Project)> {
    let (dir, project) = load_project(root, id, expected)?;
    reconcile_project_links(root, std::slice::from_ref(&project))?;
    Ok((dir, project))
}

fn link_path(project_dir: &Path, kind: ResourceKind, alias: &str) -> PathBuf {
    project_dir.join(kind.link_category()).join(alias)
}

pub fn link_resource(
    root: impl AsRef<Path>,
    project_id: &str,
    expected_revision: &str,
    kind: ResourceKind,
    resource_path: &str,
    alias: Option<&str>,
) -> WorkspaceResult<Project> {
    link_resource_with_hook(
        root.as_ref(),
        project_id,
        expected_revision,
        kind,
        resource_path,
        alias,
        |_, _| Ok(()),
    )
}

fn link_resource_with_hook<F>(
    root: &Path,
    project_id: &str,
    expected_revision: &str,
    kind: ResourceKind,
    resource_path: &str,
    alias: Option<&str>,
    after_link: F,
) -> WorkspaceResult<Project>
where
    F: FnOnce(&Path, &Path) -> Result<(), String>,
{
    let root = root_path(root)?;
    let target = resolve(&root, resource_path)?;
    let meta = fs::symlink_metadata(&target).map_err(|e| err("资源不存在", e))?;
    if meta.file_type().is_symlink()
        || (kind.is_file() != meta.is_file())
        || (!kind.is_file() && !meta.is_dir())
    {
        return Err("资源类型或路径无效".into());
    }
    if target.parent() != Some(root.join(kind.category()).as_path()) {
        return Err("资源路径与类型不匹配".into());
    }
    let (dir, mut p) = project_resources(&root, project_id, expected_revision)?;
    let alias = alias.unwrap_or_else(|| target.file_name().and_then(|x| x.to_str()).unwrap_or(""));
    validate_name(alias)?;
    if p.resources
        .iter()
        .any(|r| r.kind == kind && r.path == resource_path)
    {
        return Err("资源已关联到该项目".into());
    }
    let dst = link_path(&dir, kind, alias);
    if fs::symlink_metadata(&dst).is_ok() {
        return Err("关联目标已存在，拒绝覆盖".into());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &dst).map_err(|e| err("创建资源链接失败", e))?;
    #[cfg(not(unix))]
    return Err("当前平台不支持符号链接".into());
    p.resources.push(ResourceLink {
        kind,
        path: resource_path.into(),
        alias: alias.into(),
        source_path: None,
    });
    match after_link(&dir, &dst).and_then(|_| save_project(&dir, &p)) {
        Ok(p) => Ok(p),
        Err(e) => match remove_link_if_safe(&dst, &target) {
            Ok(()) => Err(e),
            Err(cleanup) => Err(format!(
                "保存项目关联失败: {e}；保留被外部替换的路径并需人工处理: {cleanup}"
            )),
        },
    }
}

pub fn unlink_resource(
    root: impl AsRef<Path>,
    project_id: &str,
    expected_revision: &str,
    kind: ResourceKind,
    resource_path: &str,
) -> WorkspaceResult<Project> {
    let root = root_path(root.as_ref())?;
    let (dir, mut p) = project_resources(&root, project_id, expected_revision)?;
    let i = p
        .resources
        .iter()
        .position(|r| r.kind == kind && r.path == resource_path)
        .ok_or("项目未关联该资源")?;
    let link = p.resources[i].clone();
    let link_file = link_path(&dir, kind, &link.alias);
    let meta =
        fs::symlink_metadata(&link_file).map_err(|e| err("关联链接缺失或已被外部修改", e))?;
    if !meta.file_type().is_symlink() {
        return Err("关联项已被替换为普通文件，拒绝删除".into());
    }
    let actual = fs::read_link(&link_file).map_err(|e| err("读取关联链接失败", e))?;
    let target = resolve(&root, resource_path)?;
    if actual != target {
        return Err("关联链接目标已被外部修改，拒绝删除".into());
    }
    let source_file = if let Some(source_path) = link.source_path.as_deref() {
        let relative = Path::new(source_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("项目原文档路径无效".into());
        }
        let source = dir.join(relative);
        let meta = fs::symlink_metadata(&source).map_err(|e| err("项目原文档软链接缺失", e))?;
        if !meta.file_type().is_symlink()
            || fs::read_link(&source).ok().as_deref() != Some(&resolve(&root, resource_path)?)
        {
            return Err("项目原文档链接已被修改，拒绝删除".into());
        }
        Some(source)
    } else {
        None
    };
    fs::remove_file(&link_file).map_err(|e| err("移除关联链接失败", e))?;
    if let Some(source) = &source_file {
        if let Err(e) = fs::remove_file(source) {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&actual, &link_file);
            return Err(err("移除项目原文档软链接失败", e));
        }
    }
    p.resources.remove(i);
    match save_project(&dir, &p) {
        Ok(p) => Ok(p),
        Err(e) => {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&actual, &link_file);
            #[cfg(unix)]
            if let Some(source) = source_file {
                let _ = std::os::unix::fs::symlink(&target, source);
            }
            Err(e)
        }
    }
}

pub fn set_document_source_path(
    root: impl AsRef<Path>,
    project_id: &str,
    resource_path: &str,
    source_path: &str,
) -> WorkspaceResult<Project> {
    let root = root_path(root.as_ref())?;
    let dir = project_dir(&root, project_id)?;
    let mut project = read_project(&root, &dir)?;
    let relative = Path::new(source_path);
    if source_path.is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("项目原文档路径无效".into());
    }
    let link = project
        .resources
        .iter_mut()
        .find(|link| link.kind == ResourceKind::Document && link.path == resource_path)
        .ok_or("项目文档关联不存在")?;
    link.source_path = Some(source_path.to_owned());
    save_project(&dir, &project)
}

pub fn rename_resource(
    root: impl AsRef<Path>,
    kind: ResourceKind,
    resource_path: &str,
    new_name: &str,
) -> WorkspaceResult<Resource> {
    rename_resource_with_hook(root.as_ref(), kind, resource_path, new_name, |_, _, _| {
        Ok(())
    })
}

fn rename_resource_with_hook<F>(
    root: &Path,
    kind: ResourceKind,
    resource_path: &str,
    new_name: &str,
    mut after_project_save: F,
) -> WorkspaceResult<Resource>
where
    F: FnMut(usize, &Path, &Project) -> Result<(), String>,
{
    validate_name(new_name)?;
    let root = root_path(root)?;
    let src = resolve(&root, resource_path)?;
    if src.parent() != Some(root.join(kind.category()).as_path()) {
        return Err("资源路径与类型不匹配".into());
    }
    let dst = src.parent().ok_or("资源路径无效")?.join(new_name);
    let identity_before = resource_identity(&src)?;
    if src.file_name().and_then(|s| s.to_str()) == Some(new_name) {
        return Ok(Resource {
            kind,
            name: new_name.into(),
            path: resource_path.into(),
        });
    }
    if path_exists_with_other_identity(&dst, identity_before)? {
        return Err("同名资源已存在".into());
    }
    let mut projects = snapshot(&root)?
        .projects
        .into_iter()
        .filter(|p| {
            p.resources
                .iter()
                .any(|r| r.kind == kind && r.path == resource_path)
        })
        .collect::<Vec<_>>();
    // Preflight all project configs and links before moving the resource.
    let mut work = Vec::new();
    for old in projects.drain(..) {
        let dir = resolve(&root, &old.path)?;
        let p = read_project(&root, &dir)?;
        if p.revision != old.revision {
            return Err("项目配置已被外部修改，请刷新后重试".into());
        }
        let config_bytes = fs::read(dir.join(MARKER)).map_err(|e| err("读取项目配置失败", e))?;
        if hash(&config_bytes) != p.revision {
            return Err("项目配置已被外部修改，请刷新后重试".into());
        }
        let mut links = Vec::new();
        for r in p
            .resources
            .iter()
            .filter(|r| r.kind == kind && r.path == resource_path)
        {
            let lp = link_path(&dir, kind, &r.alias);
            let meta =
                fs::symlink_metadata(&lp).map_err(|e| err("项目关联链接缺失，资源未重命名", e))?;
            if !meta.file_type().is_symlink()
                || fs::read_link(&lp).map_err(|e| err("读取项目链接失败", e))? != src
            {
                return Err("项目关联链接已被外部修改，资源未重命名".into());
            }
            let old_name = src.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let alias = if r.alias == old_name {
                new_name
            } else {
                &r.alias
            };
            let new_link = link_path(&dir, kind, alias);
            if new_link != lp
                && fs::symlink_metadata(&new_link).is_ok()
                && entry_identity(&new_link).ok() != entry_identity(&lp).ok()
            {
                return Err("重命名后的项目关联名称已存在".into());
            }
            links.push((lp, new_link, r.alias.clone(), alias.to_string()));
            if let Some(source_path) = r.source_path.as_deref() {
                let source_link = dir.join(source_path);
                let source_meta = fs::symlink_metadata(&source_link)
                    .map_err(|e| err("项目原文档链接缺失，资源未重命名", e))?;
                if !source_meta.file_type().is_symlink()
                    || fs::read_link(&source_link).map_err(|e| err("读取项目原文档链接失败", e))?
                        != src
                {
                    return Err("项目原文档链接已被外部修改，资源未重命名".into());
                }
                links.push((
                    source_link.clone(),
                    source_link,
                    String::new(),
                    String::new(),
                ));
            }
        }
        work.push((dir, p, config_bytes, links));
    }
    rename_exclusive(&src, &dst).map_err(|e| err("重命名资源失败", e))?;
    let identity_after = match resource_identity(&dst) {
        Ok(id) if id == identity_before => id,
        _ => {
            let mut no_transactions = Vec::new();
            let details =
                rollback_resource_rename(&src, &dst, identity_before, &mut no_transactions);
            return Err(if details.is_empty() {
                "重命名资源后身份校验失败，资源已安全回滚".into()
            } else {
                format!(
                    "重命名资源后身份校验失败，需人工处理: {}",
                    details.join("；")
                )
            });
        }
    };
    let new_path = relative_path(
        dst.strip_prefix(&root)
            .map_err(|e| err("资源路径异常", e))?,
    )?;
    let mut transactions = Vec::new();
    let mut failure = None;
    for (project_index, (dir, prior, prior_bytes, links)) in work.into_iter().enumerate() {
        let mut p = prior.clone();
        for r in &mut p.resources {
            if r.kind == kind && r.path == resource_path {
                r.path = new_path.clone();
                if let Some((_, _, _, alias)) = links
                    .iter()
                    .find(|(_, _, old_alias, _)| *old_alias == r.alias)
                {
                    r.alias = alias.clone();
                }
            }
        }
        let mut transaction = ProjectRenameTransaction {
            dir: dir.clone(),
            prior_revision: prior.revision.clone(),
            prior_bytes,
            applied_revision: None,
            links: Vec::new(),
        };
        for (lp, new_link, _, _) in &links {
            let old_meta = fs::symlink_metadata(lp).map_err(|e| err("检查旧项目链接失败", e));
            match old_meta {
                Ok(meta)
                    if meta.file_type().is_symlink()
                        && fs::read_link(lp).ok().as_ref() == Some(&src) => {}
                _ => {
                    failure = Some("项目关联链接在重命名期间被外部修改".to_string());
                    break;
                }
            }
            if new_link != lp
                && fs::symlink_metadata(new_link).is_ok()
                && entry_identity(new_link).ok() != entry_identity(lp).ok()
            {
                failure = Some(format!(
                    "重命名后的项目关联名称被外部占用: {}",
                    new_link.display()
                ));
                break;
            }
            if let Err(e) = fs::remove_file(lp) {
                failure = Some(err("移除旧项目链接失败", e));
                break;
            }
            let link_change = RenameLinkChange {
                old_path: lp.clone(),
                new_path: new_link.clone(),
                old_target: src.clone(),
                new_target: dst.clone(),
            };
            transaction.links.push(link_change);
            #[cfg(unix)]
            if let Err(e) = std::os::unix::fs::symlink(&dst, new_link) {
                failure = Some(err("更新项目链接失败", e));
                break;
            }
        }
        if failure.is_some() {
            transactions.push(transaction);
            break;
        }
        match save_project(&dir, &p) {
            Ok(saved) => {
                transaction.applied_revision = Some(saved.revision.clone());
                transactions.push(transaction);
                if let Err(e) = after_project_save(project_index, &dir, &saved) {
                    failure = Some(e);
                    break;
                }
            }
            Err(e) => {
                transactions.push(transaction);
                failure = Some(e);
                break;
            }
        }
    }
    if let Some(e) = failure {
        let rollback = rollback_resource_rename(&src, &dst, identity_after, &mut transactions);
        return Err(if rollback.is_empty() {
            format!("资源重命名失败，已安全回滚: {e}")
        } else {
            format!(
                "资源重命名失败: {e}；部分回滚已停止，需人工处理: {}",
                rollback.join("；")
            )
        });
    }
    Ok(Resource {
        kind,
        name: new_name.into(),
        path: new_path,
    })
}

fn resource_identity(path: &Path) -> WorkspaceResult<ResourceIdentity> {
    let meta = fs::symlink_metadata(path).map_err(|e| err("读取资源身份失败", e))?;
    if meta.file_type().is_symlink() {
        return Err(format!("拒绝用符号链接确认资源实体: {}", path.display()));
    }
    entry_identity(path)
}

fn entry_identity(path: &Path) -> WorkspaceResult<ResourceIdentity> {
    let meta = fs::symlink_metadata(path).map_err(|e| err("读取路径身份失败", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(ResourceIdentity {
            device: meta.dev(),
            inode: meta.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        Ok(ResourceIdentity {
            device: meta.len(),
            inode: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
        })
    }
}

/// `true` means a destination exists but is not the same filesystem object as the source.
fn path_exists_with_other_identity(path: &Path, source: ResourceIdentity) -> WorkspaceResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(entry_identity(path)? != source),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(err("检查改名目标失败", e)),
    }
}

fn restore_link_if_safe(path: &Path, target: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta)
            if meta.file_type().is_symlink()
                && fs::read_link(path).ok().as_deref() == Some(target) =>
        {
            Ok(())
        }
        Ok(_) => Err(format!("拒绝覆盖外部占用的链接路径 {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::os::unix::fs::symlink(target, path).map_err(|e| err("恢复旧关联失败", e))
        }
        Err(e) => Err(err("检查旧关联失败", e)),
    }
}

fn remove_link_if_safe(path: &Path, target: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta)
            if meta.file_type().is_symlink()
                && fs::read_link(path).ok().as_deref() == Some(target) =>
        {
            fs::remove_file(path).map_err(|e| err("移除本次创建的关联失败", e))
        }
        Ok(_) => Err(format!("关联路径已被外部修改，保留 {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(err("检查本次关联失败", e)),
    }
}

fn restore_config_if_safe(tx: &ProjectRenameTransaction) -> Result<(), String> {
    let Some(applied_revision) = &tx.applied_revision else {
        return Ok(());
    };
    let path = tx.dir.join(MARKER);
    let meta = fs::symlink_metadata(&path).map_err(|e| err("读取待回滚项目配置失败", e))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!("项目配置已被替换，保留现状: {}", path.display()));
    }
    let bytes = fs::read(&path).map_err(|e| err("读取待回滚项目配置失败", e))?;
    if hash(&bytes) == tx.prior_revision {
        return Ok(());
    }
    if hash(&bytes) != *applied_revision {
        return Err(format!(
            "项目配置已被外部修改，保留现状: {}",
            path.display()
        ));
    }
    let temp = tx.dir.join(format!(".pw-rollback-{}.tmp", Uuid::new_v4()));
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| err("创建配置回滚文件失败", e))?;
    if let Err(e) = f.write_all(&tx.prior_bytes) {
        let _ = fs::remove_file(&temp);
        return Err(err("写入配置回滚文件失败", e));
    }
    let current = fs::read(&path).map_err(|e| err("复核项目配置失败", e))?;
    let meta = fs::symlink_metadata(&path).map_err(|e| err("复核项目配置失败", e))?;
    if !meta.is_file() || meta.file_type().is_symlink() || hash(&current) != *applied_revision {
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "项目配置回滚前发生外部修改，保留现状: {}",
            path.display()
        ));
    }
    fs::rename(&temp, &path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        err("恢复项目配置失败", e)
    })
}

fn normal_config_revision(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return None;
    }
    fs::read(path).ok().map(|bytes| hash(&bytes))
}

fn rollback_resource_rename(
    src: &Path,
    dst: &Path,
    identity: ResourceIdentity,
    transactions: &mut [ProjectRenameTransaction],
) -> Vec<String> {
    let mut issues = Vec::new();
    for tx in transactions.iter_mut().rev() {
        if let Err(e) = restore_config_if_safe(tx) {
            issues.push(e);
        }
    }
    for tx in transactions.iter().rev() {
        let config_path = tx.dir.join(MARKER);
        let current_revision = normal_config_revision(&config_path);
        let config_is_prior = current_revision.as_deref() == Some(tx.prior_revision.as_str());
        if !config_is_prior {
            if !tx.links.is_empty() {
                issues.push(format!(
                    "项目配置未恢复，保留其关联状态: {}",
                    config_path.display()
                ));
            }
            continue;
        }
        for link in tx.links.iter().rev() {
            if link.old_path == link.new_path {
                match fs::symlink_metadata(&link.old_path) {
                    Ok(meta)
                        if meta.file_type().is_symlink()
                            && fs::read_link(&link.old_path).ok().as_deref()
                                == Some(link.new_target.as_path()) =>
                    {
                        if let Err(e) = fs::remove_file(&link.old_path) {
                            issues.push(err("移除重命名后的关联失败", e));
                            continue;
                        }
                        if let Err(e) = restore_link_if_safe(&link.old_path, &link.old_target) {
                            issues.push(e);
                        }
                    }
                    Ok(meta)
                        if meta.file_type().is_symlink()
                            && fs::read_link(&link.old_path).ok().as_deref()
                                == Some(link.old_target.as_path()) => {}
                    Ok(_) => issues.push(format!(
                        "关联链接已被外部修改，保留 {}",
                        link.old_path.display()
                    )),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        if let Err(e) = restore_link_if_safe(&link.old_path, &link.old_target) {
                            issues.push(e);
                        }
                    }
                    Err(e) => issues.push(err("检查关联链接失败", e)),
                }
            } else {
                if let Err(e) = remove_link_if_safe(&link.new_path, &link.new_target) {
                    issues.push(e);
                }
                if let Err(e) = restore_link_if_safe(&link.old_path, &link.old_target) {
                    issues.push(e);
                }
            }
        }
    }
    if transactions.iter().any(|tx| {
        tx.applied_revision.is_some()
            && normal_config_revision(&tx.dir.join(MARKER)).as_deref()
                != Some(tx.prior_revision.as_str())
    }) {
        issues.push("至少一个项目配置未恢复，资源保留在新路径以避免破坏配置指向".into());
        return issues;
    }
    match fs::symlink_metadata(src) {
        Ok(_) if resource_identity(src).ok() == Some(identity) => match resource_identity(dst) {
            Ok(current) if current == identity => {
                if let Err(e) = rename_exclusive(dst, src) {
                    issues.push(err("恢复旧资源路径失败", e));
                }
            }
            Ok(_) => issues.push(format!("新资源路径的实体已被替换，保留 {}", dst.display())),
            Err(e) => issues.push(format!("无法确认新资源实体，保留 {}: {e}", dst.display())),
        },
        Ok(_) => issues.push(format!(
            "旧资源路径已被外部占用，保留新资源 {}",
            dst.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match resource_identity(dst) {
            Ok(current) if current == identity => {
                if let Err(e) = rename_exclusive(dst, src) {
                    issues.push(err("恢复旧资源路径失败", e));
                }
            }
            Ok(_) => issues.push(format!("新资源路径的实体已被替换，保留 {}", dst.display())),
            Err(e) => issues.push(format!("无法确认新资源实体，保留 {}: {e}", dst.display())),
        },
        Err(e) => issues.push(err("检查旧资源路径失败", e)),
    }
    issues
}

pub fn delete_resource<F>(
    root: impl AsRef<Path>,
    kind: ResourceKind,
    resource_path: &str,
    trash: F,
) -> WorkspaceResult<()>
where
    F: FnOnce(&Path) -> Result<(), String>,
{
    let root = root_path(root.as_ref())?;
    let path = resolve(&root, resource_path)?;
    if path.parent() != Some(root.join(kind.category()).as_path()) {
        return Err("资源路径与类型不匹配".into());
    }
    // Refuse deletion while associations exist. The UI can unlink projects first, which keeps
    // this operation atomic with respect to the resource and all project configs.
    let snap = snapshot(&root)?;
    if snap.projects.iter().any(|p| {
        p.resources
            .iter()
            .any(|r| r.kind == kind && r.path == resource_path)
    }) {
        return Err("资源仍关联到项目，请先解除所有项目关联后再删除".into());
    }
    trash(&path).map_err(|e| format!("移入废纸篓失败，关联配置未变更: {e}"))?;
    Ok(())
}

pub fn read_text(root: impl AsRef<Path>, path: &str) -> WorkspaceResult<Document> {
    let root = root_path(root.as_ref())?;
    let file = resolve_read_target(&root, path)?;
    if !is_text_path(Path::new(path), false) {
        return Err("只允许读取文档或附件分类中的文件".into());
    }
    let bytes = fs::read(&file).map_err(|e| err("读取文档失败", e))?;
    let content = String::from_utf8(bytes.clone()).map_err(|e| err("文档不是 UTF-8 文本", e))?;
    Ok(Document {
        path: path.into(),
        content,
        revision: hash(&bytes),
    })
}

pub fn write_text(
    root: impl AsRef<Path>,
    path: &str,
    content: &str,
    expected_revision: Option<&str>,
) -> WorkspaceResult<Document> {
    let root = root_path(root.as_ref())?;
    let file = resolve_read_target(&root, path)?;
    if !is_text_path(Path::new(path), true) {
        return Err("只允许保存文档分类中的文件".into());
    }
    let meta = fs::symlink_metadata(&file).map_err(|e| err("文档不存在", e))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("目标不是普通文件".into());
    }
    let current = fs::read(&file).map_err(|e| err("读取文档失败", e))?;
    if expected_revision
        .map(|r| r != hash(&current))
        .unwrap_or(true)
    {
        return Err("文档已被外部修改或未提供 revision，请重新读取后保存".into());
    }
    let temp = file.with_file_name(format!(".pw-save-{}.tmp", Uuid::new_v4()));
    fs::write(&temp, content.as_bytes()).map_err(|e| err("写入临时文档失败", e))?;
    let permissions = fs::metadata(&file)
        .map_err(|e| err("读取原文档权限失败", e))?
        .permissions();
    if let Err(e) = fs::set_permissions(&temp, permissions) {
        let _ = fs::remove_file(&temp);
        return Err(err("保留原文件权限失败", e));
    }
    if let Err(e) = fs::rename(&temp, &file) {
        let _ = fs::remove_file(&temp);
        return Err(err("保存文档失败", e));
    }
    read_text(&root, path)
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
