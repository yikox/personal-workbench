import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, Bot, ChevronRight, Code2, ExternalLink, File, FileImage, FileText, Folder, GitBranch, Home, Image, Pencil, Plus, RefreshCw, Search, Settings as SettingsIcon, Sparkles, Terminal, Trash2, X } from 'lucide-react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, type AgentInfo, type BootState, type DiscoveredDocument, type Document, type Entry, type ImageData, type Project, type Resource, type ResourceKind, type Session, type Settings, type Shortcut, type Snapshot, type TemporaryDirectory } from './api';

type Page = 'home' | 'project' | 'temporary' | 'resources' | 'shortcuts' | 'settings';
type ModalKind = 'project' | 'temporary' | 'resource' | 'project-resource' | 'project-entry' | 'shortcut' | 'edit-project' | 'promote' | 'rename-project' | 'rename-temporary' | 'delete-temporary' | 'rename-resource' | 'delete-project' | 'delete-resource' | 'link' | 'search' | 'associate-markdown';
const labels: Record<ResourceKind, string> = { repository: 'Git 管理', document: '文档管理', attachment: '附件管理', script: '脚本' };
const projectResourceCreateLabels: Record<ResourceKind, string> = { repository: '克隆仓库', document: '新建文档', attachment: '添加附件', script: '新建脚本' };
const icons: Record<ResourceKind, typeof File> = { repository: Code2, document: FileText, attachment: FileImage, script: Terminal };
const err = (e: unknown) => e instanceof Error ? e.message : String(e);
const isRevisionConflict = (e: unknown) => /revision|conflict|changed|modified|version|并发|版本|已被修改|内容已变/i.test(err(e));
const isProjectResourcePartialSuccess = (message: string) => message.startsWith('资源已创建，但自动关联失败') || message.startsWith('资源已创建并关联，但刷新失败');
type SessionLoadState = { status: 'loading' } | { status: 'loaded'; errors: string[] } | { status: 'error'; message: string };
const mostRecentProject = (projects: Project[], recentProjectIds: string[]) =>
  recentProjectIds.map(id => projects.find(p => p.id === id)).find((p): p is Project => Boolean(p));
const shortcutLabels: Record<Shortcut['kind'], string> = {
  project: '打开项目', application: '打开应用', terminal: '打开终端', script: '运行脚本', command: '运行命令',
};
const MarkdownDocument = React.lazy(() => import('./MarkdownDocument'));

export default function App() {
  const [boot, setBoot] = useState<BootState | null>(null);
  const [page, setPage] = useState<Page>('home');
  const [kind, setKind] = useState<ResourceKind>('repository');
  const [projectId, setProjectId] = useState('');
  const [temporaryId, setTemporaryId] = useState('');
  const [selected, setSelected] = useState<Resource | null>(null);
  const [repositoryFile, setRepositoryFile] = useState(false);
  const [document, setDocument] = useState<Document | null>(null);
  const [documentConflict, setDocumentConflict] = useState<{ latest: Document | null; error: string | null } | null>(null);
  const [imageData, setImageData] = useState<ImageData | null>(null);
  const [draft, setDraft] = useState('');
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const [editing, setEditing] = useState(false);
  const [entries, setEntries] = useState<Entry[]>([]);
  const [markdownDocs, setMarkdownDocs] = useState<DiscoveredDocument[]>([]);
  const [markdownScanBusy, setMarkdownScanBusy] = useState(false);
  const markdownScanRequest = useRef(0);
  const [dir, setDir] = useState('');
  const [directoryRefreshIssue, setDirectoryRefreshIssue] = useState<{ path: string; message: string } | null>(null);
  const [sessionsByProject, setSessionsByProject] = useState<Record<string, Session[]>>({});
  const [sessionLoadByProject, setSessionLoadByProject] = useState<Record<string, SessionLoadState>>({});
  const [sessionsByTemporary, setSessionsByTemporary] = useState<Record<string, Session[]>>({});
  const [sessionLoadByTemporary, setSessionLoadByTemporary] = useState<Record<string, SessionLoadState>>({});
  const workspaceEpoch = useRef(0);
  const bootstrapRequest = useRef(0);
  const sessionRequest = useRef<Record<string, number>>({});
  const temporarySessionRequest = useRef<Record<string, number>>({});
  const agentRequest = useRef(0);
  const visitRequest = useRef(0);
  const resourceRequest = useRef(0);
  const directoryRequest = useRef(0);
  const projectCreateOperationBusyRef = useRef(false);
  const conflictRequest = useRef(0);
  const documentWriteCount = useRef(0);
  const documentWriteUiRequest = useRef<number | null>(null);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [filter, setFilter] = useState('');
  const [modal, setModal] = useState<ModalKind | null>(null);
  const [pendingLeave, setPendingLeave] = useState<(() => void) | null>(null);
  const [modalData, setModalData] = useState<Record<string,string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [modalError, setModalError] = useState('');
  const [settingsDraft, setSettingsDraft] = useState<Settings | null>(null);
  const [shortcutTab, setShortcutTab] = useState<'buttons'|'scripts'>('buttons');
  const [projectTab, setProjectTab] = useState<'overview'|'sessions'|'files'>('overview');
  const snapshot = boot?.snapshot;
  const projects = snapshot?.projects ?? [];
  const temporaryDirectories = snapshot?.temporaryDirectories ?? [];
  const resources = snapshot?.resources ?? [];
  const recentProjectIds = boot?.recentProjectIds ?? [];
  const project = projects.find(p => p.id === projectId) ?? mostRecentProject(projects, recentProjectIds) ?? projects[0];
  const refreshProjectMarkdown = async (target = project) => { if (!target) return; const request = ++markdownScanRequest.current; setMarkdownScanBusy(true); try { const docs = await api.discoverProjectMarkdown(target.id); if (request === markdownScanRequest.current) setMarkdownDocs(docs); } catch (e) { if (request === markdownScanRequest.current) setError(err(e)); } finally { if (request === markdownScanRequest.current) setMarkdownScanBusy(false); } };
  useEffect(() => { if (page === 'project' && project) void refreshProjectMarkdown(project); }, [page, project?.id, project?.revision]);
  const projectEntryParentPath = project && (dir === project.path || dir.startsWith(`${project.path}/`)) ? dir : project?.path ?? '';
  const projectEntrySubpath = project && projectEntryParentPath !== project.path ? projectEntryParentPath.slice(project.path.length + 1) : '';
  const projectEntryCategory = projectEntrySubpath.split('/')[0];
  const canCreateProjectEntry = Boolean(project && !['关联仓库', '关联文档', '关联附件', '关联脚本', '.git'].includes(projectEntryCategory));
  const temporary = temporaryDirectories.find(item => item.id === temporaryId);
  const projectNavItems = projects.map((item, index) => ({ item, index, rank: recentProjectIds.indexOf(item.id) })).sort((a, b) => {
    if (a.rank >= 0 && b.rank >= 0) return a.rank - b.rank;
    if (a.rank >= 0) return -1;
    if (b.rank >= 0) return 1;
    return a.index - b.index;
  }).map(row => row.item);
  const shortcuts = boot?.shortcuts ?? [];
  const shortcutTargetLabel = (s: Shortcut) => {
    if (s.kind === 'project' || s.kind === 'terminal') return projects.find(p => p.id === s.target)?.name ?? '关联项目不存在';
    if (s.kind === 'script') return resources.find(r => r.kind === 'script' && r.path === s.target)?.name ?? '脚本资源不存在';
    return s.target || (s.kind === 'application' ? '未配置应用路径' : '未配置命令');
  };
  const linkedProjectsForSelection = selected ? projects.filter(p => p.resources.some(link => link.kind === selected.kind && link.path === selected.path)) : [];

  const refresh = useCallback(async () => { const request = ++bootstrapRequest.current; try { const b = await api.bootstrap(); if (request !== bootstrapRequest.current) return undefined; workspaceEpoch.current += 1; agentRequest.current += 1; sessionRequest.current = {}; setSessionsByProject({}); setSessionLoadByProject({}); setBoot(b); setSettingsDraft(b.settings); setProjectId(current => b.snapshot?.projects.some(p => p.id === current) ? current : mostRecentProject(b.snapshot?.projects ?? [], b.recentProjectIds)?.id || b.snapshot?.projects[0]?.id || ''); setError(''); return b; } catch (e) { if (request === bootstrapRequest.current) throw e; return undefined; } }, []);
  useEffect(() => { void refresh().catch(e => setError(err(e))); }, [refresh]);
  useEffect(() => { const onKeyDown = (e: KeyboardEvent) => { if (projectCreateOperationBusyRef.current) { if (e.key === 'Escape' || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k')) e.preventDefault(); return; } if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); setModalData({ query: '' }); setModal('search'); } if (e.key === 'Escape') { setPendingLeave(null); setModal(null); } }; window.addEventListener('keydown', onKeyDown); return () => window.removeEventListener('keydown', onKeyDown); }, []);
  const run = async (fn: () => Promise<unknown>, after?: (value: never) => void) => { setBusy(true); setError(''); try { const value = await fn(); if (after) after(value as never); } catch(e) { setError(err(e)); } finally { setBusy(false); } };
  const hasUnsavedText = editing && document !== null && draft !== document.content;
  const hasUnsavedTextRef = useRef(false);
  const setPendingLeaveRef = useRef(setPendingLeave);
  const setErrorRef = useRef(setError);
  hasUnsavedTextRef.current = hasUnsavedText;
  setPendingLeaveRef.current = setPendingLeave;
  setErrorRef.current = setError;
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const window = getCurrentWindow();
    void window.onCloseRequested(event => {
      if (projectCreateOperationBusyRef.current) {
        event.preventDefault();
        setModalError('项目操作正在进行，请等待完成后再关闭工作台');
        return;
      }
      if (documentWriteCount.current > 0) {
        event.preventDefault();
        setErrorRef.current('文档正在保存，请等待保存完成后再关闭工作台。');
        return;
      }
      if (!hasUnsavedTextRef.current) return;
      event.preventDefault();
      setPendingLeaveRef.current(() => () => {
        void window.destroy().catch(e => setErrorRef.current(err(e)));
      });
    }).then(stop => { if (disposed) stop(); else unlisten = stop; })
      .catch(e => setErrorRef.current(err(e)));
    return () => { disposed = true; unlisten?.(); };
  }, []);
  const navigateSafely = (action: () => void) => {
    if (documentWriteCount.current > 0) { setError('文档正在保存，请等待保存完成后再切换页面或文件。'); return false; }
    if (hasUnsavedText) { setPendingLeave(() => action); return false; }
    action();
    return true;
  };
  const discardAndLeave = () => { const action = pendingLeave; setPendingLeave(null); action?.(); };
  const invalidateDocumentRequest = () => {
    conflictRequest.current += 1;
    if (documentWriteUiRequest.current !== null) { documentWriteUiRequest.current = null; setBusy(false); }
  };
  const acceptSnapshot = (s: Snapshot) => setBoot(b => b ? { ...b, snapshot: s } : b);
  const loadProjectSessions = useCallback(async (id: string, epoch = workspaceEpoch.current) => {
    const request = (sessionRequest.current[id] ?? 0) + 1;
    sessionRequest.current[id] = request;
    setSessionLoadByProject(current => ({ ...current, [id]: { status: 'loading' } }));
    setSessionsByProject(current => { const next = { ...current }; delete next[id]; return next; });
    try {
      const result = await api.listSessions(id);
      if (epoch !== workspaceEpoch.current || request !== sessionRequest.current[id]) return;
      setSessionsByProject(current => ({ ...current, [id]: result.sessions }));
      setSessionLoadByProject(current => ({ ...current, [id]: { status: 'loaded', errors: result.errors } }));
    } catch (e) {
      if (epoch !== workspaceEpoch.current || request !== sessionRequest.current[id]) return;
      setSessionLoadByProject(current => ({ ...current, [id]: { status: 'error', message: err(e) } }));
    }
  }, []);
  const loadTemporarySessions = useCallback(async (id: string, epoch = workspaceEpoch.current) => {
    const request = (temporarySessionRequest.current[id] ?? 0) + 1;
    temporarySessionRequest.current[id] = request;
    setSessionLoadByTemporary(current => ({ ...current, [id]: { status: 'loading' } }));
    setSessionsByTemporary(current => { const next = { ...current }; delete next[id]; return next; });
    try {
      const result = await api.listTemporarySessions(id);
      if (epoch !== workspaceEpoch.current || request !== temporarySessionRequest.current[id]) return;
      setSessionsByTemporary(current => ({ ...current, [id]: result.sessions }));
      setSessionLoadByTemporary(current => ({ ...current, [id]: { status: 'loaded', errors: result.errors } }));
    } catch (e) {
      if (epoch !== workspaceEpoch.current || request !== temporarySessionRequest.current[id]) return;
      setSessionLoadByTemporary(current => ({ ...current, [id]: { status: 'error', message: err(e) } }));
    }
  }, []);
  useEffect(() => {
    const active = new Set(projects.map(p => p.id));
    for (const id of Object.keys(sessionRequest.current)) if (!active.has(id)) sessionRequest.current[id] += 1;
    setSessionsByProject(current => Object.fromEntries(Object.entries(current).filter(([id]) => active.has(id))));
    setSessionLoadByProject(current => Object.fromEntries(Object.entries(current).filter(([id]) => active.has(id))));
    const epoch = workspaceEpoch.current;
    void Promise.allSettled(projects.map(p => loadProjectSessions(p.id, epoch)));
  }, [snapshot, loadProjectSessions]);
  useEffect(() => {
    const active = new Set(temporaryDirectories.map(item => item.id));
    for (const id of Object.keys(temporarySessionRequest.current)) if (!active.has(id)) temporarySessionRequest.current[id] += 1;
    setSessionsByTemporary(current => Object.fromEntries(Object.entries(current).filter(([id]) => active.has(id))));
    setSessionLoadByTemporary(current => Object.fromEntries(Object.entries(current).filter(([id]) => active.has(id))));
    const epoch = workspaceEpoch.current;
    void Promise.allSettled(temporaryDirectories.map(item => loadTemporarySessions(item.id, epoch)));
  }, [snapshot, loadTemporarySessions]);
  const doChooseWorkspace = async () => { const request = ++bootstrapRequest.current; setBusy(true); try { const next = await api.chooseWorkspace(); if (next && request === bootstrapRequest.current) { workspaceEpoch.current += 1; agentRequest.current += 1; resourceRequest.current += 1; invalidateDocumentRequest(); sessionRequest.current = {}; temporarySessionRequest.current = {}; setSessionsByProject({}); setSessionLoadByProject({}); setSessionsByTemporary({}); setSessionLoadByTemporary({}); setBoot(next); setSettingsDraft(next.settings); setProjectId(mostRecentProject(next.snapshot?.projects ?? [], next.recentProjectIds)?.id ?? next.snapshot?.projects[0]?.id ?? ''); setTemporaryId(''); setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); setError(''); } } catch(e) { if (request === bootstrapRequest.current) setError(err(e)); } finally { setBusy(false); } };
  const chooseWorkspace = () => { navigateSafely(() => { void doChooseWorkspace(); }); };
  const doOpenProject = (p: Project) => { const epoch = workspaceEpoch.current; const request = ++agentRequest.current; const visit = ++visitRequest.current; setProjectId(p.id); setTemporaryId(''); setPage('project'); setProjectTab('overview'); setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); resourceRequest.current += 1; invalidateDocumentRequest(); setError(''); void loadProjectSessions(p.id, epoch); void api.agentStatus().then(result => { if (epoch === workspaceEpoch.current && request === agentRequest.current) setAgents(result); }).catch(e => { if (epoch === workspaceEpoch.current && request === agentRequest.current) setError(err(e)); }); setBoot(current => current ? { ...current, recentProjectIds: [p.id, ...current.recentProjectIds.filter(id => id !== p.id)] } : current); void api.recordProjectVisit(p.id).then(ids => { if (epoch === workspaceEpoch.current && visit === visitRequest.current) setBoot(current => current ? { ...current, recentProjectIds: ids } : current); }).catch(e => { if (epoch === workspaceEpoch.current && visit === visitRequest.current) setError(err(e)); }); };
  const openProject = (p: Project) => { navigateSafely(() => doOpenProject(p)); };
  const doOpenResource = async (r: Resource, isFile = false) => { const request = ++resourceRequest.current; invalidateDocumentRequest(); setSelected(r); setRepositoryFile(r.kind === 'repository' && isFile); if (r.kind === 'script') { setPage('shortcuts'); setShortcutTab('scripts'); } else { setPage('resources'); setKind(r.kind); } setEditing(false); setDocument(null); setDocumentConflict(null); setDraft(''); setImageData(null); setEntries([]); setError(''); if (r.kind === 'repository' && !isFile) { await openDir(r.path); return; } if (r.kind === 'attachment' && /\.(png|jpe?g|webp|gif)$/i.test(r.name)) { await run(() => api.readImage(r.path), v => { if (request === resourceRequest.current) setImageData(v as ImageData); }); return; } if (r.kind === 'document' || r.kind === 'script' || r.kind === 'repository') { await run(() => api.readText(r.path), v => { if (request !== resourceRequest.current) return; const d = v as Document; setDocument(d); setDraft(d.content); }); } };
  const openResource = (r: Resource, isFile = false) => { navigateSafely(() => { void doOpenResource(r, isFile); }); };
  const returnToNotes = () => navigateSafely(() => { setSelected(null); setRepositoryFile(false); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); setImageData(null); resourceRequest.current += 1; invalidateDocumentRequest(); setError(''); });
  const openDir = async (path: string) => { const request = ++directoryRequest.current; const epoch = workspaceEpoch.current; setDir(path); await run(() => api.listDirectory(path), v => { if (epoch === workspaceEpoch.current && request === directoryRequest.current) { setEntries(v as Entry[]); setDirectoryRefreshIssue(current => current?.path === path ? null : current); } }); };
  const doOpenTemporary = (item: TemporaryDirectory) => {
    const epoch = workspaceEpoch.current;
    const agentToken = ++agentRequest.current;
    setTemporaryId(item.id); setPage('temporary'); setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); setEntries([]); setDir(item.path);
    resourceRequest.current += 1; invalidateDocumentRequest(); setError('');
    void openDir(item.path);
    void loadTemporarySessions(item.id, epoch);
    void api.agentStatus().then(result => { if (epoch === workspaceEpoch.current && agentToken === agentRequest.current) setAgents(result); }).catch(e => { if (epoch === workspaceEpoch.current && agentToken === agentRequest.current) setError(err(e)); });
  };
  const openTemporary = (item: TemporaryDirectory) => { if (page === 'temporary' && temporaryId === item.id) return; navigateSafely(() => doOpenTemporary(item)); };
  const openTemporaryFile = async (entry: Entry) => {
    if (entry.isDir) { await openDir(entry.path); return; }
    const ext = entry.name.toLowerCase().split('.').pop() ?? '';
    const isImage = ['png','jpg','jpeg','webp','gif'].includes(ext);
    const isAttachment = isImage || ['pdf','doc','docx'].includes(ext);
    const resource: Resource = { kind: isAttachment ? 'attachment' : 'repository', name: entry.name, path: entry.path };
    navigateSafely(() => {
      const request = ++resourceRequest.current;
      setSelected(resource); setRepositoryFile(true); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); setImageData(null); setError(''); invalidateDocumentRequest();
      if (isImage) void run(() => api.readImage(entry.path), value => { if (request === resourceRequest.current) setImageData(value as ImageData); });
      else if (!isAttachment) void run(() => api.readText(entry.path), value => { if (request === resourceRequest.current) { const next = value as Document; setDocument(next); setDraft(next.content); } });
    });
  };
  const goUp = (base: string) => { if (!dir || dir === base) return; const parent = dir.split('/').slice(0, -1).join('/'); void openDir(parent.length < base.length ? base : parent); };
  const actSnapshot = (fn: () => Promise<Snapshot>) => run(fn, v => { acceptSnapshot(v as Snapshot); setModal(null); });
  const associateMarkdown = (path: string) => { if (!project) return; void run(() => api.associateProjectMarkdown(project.id, path, project.revision), value => { const next = value as Snapshot; acceptSnapshot(next); setMarkdownDocs(current => current.filter(doc => doc.path !== path)); setModal(null); }); };
  const beginMarkdownAssociation = (path: string) => { if (!project) return; if (path.startsWith('关联仓库/')) { setModalData({path, projectId: project.id}); setModal('associate-markdown'); return; } associateMarkdown(path); };
  const filtered = useMemo(() => resources.filter(r => r.kind === kind && `${r.name} ${r.path}`.toLowerCase().includes(filter.toLowerCase())), [resources,kind,filter]);
  const linked = (p: Project) => p.resources.map(link => ({ ...link, resource: resources.find(r => r.path === link.path && r.kind === link.kind) })).filter(x => x.resource);
  const projectSessions = project ? sessionsByProject[project.id] ?? [] : [];
  const temporarySessions = temporary ? sessionsByTemporary[temporary.id] ?? [] : [];
  const sessions = [...projects.flatMap(p => sessionsByProject[p.id] ?? []), ...temporaryDirectories.flatMap(item => sessionsByTemporary[item.id] ?? [])];
  const projectSessionStatus = project ? sessionLoadByProject[project.id] ?? { status: 'loading' as const } : null;
  const temporarySessionStatus = temporary ? sessionLoadByTemporary[temporary.id] ?? { status: 'loading' as const } : null;
  const sessionOwnerLabel = (session: Session) => temporaryDirectories.find(item => item.id === session.temporaryId)?.name ?? projects.find(item => item.id === session.projectId)?.name ?? '未知目录';
  const allSessionsLoadedEmpty = projects.length + temporaryDirectories.length > 0 && sessions.length === 0 && projects.every(p => { const state = sessionLoadByProject[p.id]; return state?.status === 'loaded' && state.errors.length === 0; }) && temporaryDirectories.every(item => { const state = sessionLoadByTemporary[item.id]; return state?.status === 'loaded' && state.errors.length === 0; });
  const setNav = (next: Page, nextKind?: ResourceKind) => { if (page === next && (!nextKind || nextKind === kind)) return; navigateSafely(() => { setPage(next); if(nextKind) setKind(nextKind); setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); resourceRequest.current += 1; invalidateDocumentRequest(); setError(''); }); };
  const switchShortcutTab = (next: 'buttons'|'scripts') => { if (next === shortcutTab) return; navigateSafely(() => { setShortcutTab(next); setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); resourceRequest.current += 1; invalidateDocumentRequest(); }); };

  const createProject = () => { setModalData({name:'',description:''}); setModal('project'); };
  const createTemporary = () => navigateSafely(() => { if (hasUnsavedText) { setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); } setModalData({name:''}); setModal('temporary'); });
  const createResource = (k: ResourceKind) => { setKind(k); setModalData({name:''}); setModal('resource'); };
  const beginProjectResourceCreate = (k: ResourceKind) => { if (!project) return; navigateSafely(() => { setError(''); setModalError(''); setModalData({projectId:project.id,kind:k,name:'',cloneUrl:''}); setModal('project-resource'); }); };
  const beginProjectEntryCreate = (isDir: boolean) => { if (!project) return; navigateSafely(() => { setError(''); setModalError(''); setModalData({projectId:project.id,name:'',entryType:isDir?'directory':'file',parentPath:projectEntryParentPath}); setModal('project-entry'); }); };
  const submitProjectResourceCreate = async (target: Project, d: Record<string,string>) => {
    if (projectCreateOperationBusyRef.current) return;
    projectCreateOperationBusyRef.current = true;
    const epoch = workspaceEpoch.current;
    const resourceKind = d.kind as ResourceKind;
    setBusy(true); setError(''); setModalError('');
    try {
      const result = await api.createProjectResource(target.id, target.revision, resourceKind, resourceKind === 'attachment' ? '' : d.name, d.cloneUrl || undefined);
      if (epoch !== workspaceEpoch.current) return;
      if (result) acceptSnapshot(result);
      setModal(null);
    } catch (e) {
      if (epoch !== workspaceEpoch.current) return;
      const message = err(e);
      if (isProjectResourcePartialSuccess(message)) {
        setModal(null);
        setError(message);
        try { await refresh(); setError(message); }
        catch (refreshError) { setError(`${message}；刷新配置失败：${err(refreshError)}`); }
      } else setModalError(message);
    } finally { projectCreateOperationBusyRef.current = false; setBusy(false); }
  };
  const submitProjectEntryCreate = async (target: Project, d: Record<string,string>) => {
    if (projectCreateOperationBusyRef.current) return;
    projectCreateOperationBusyRef.current = true;
    const parentPath = d.parentPath || target.path;
    const request = ++directoryRequest.current;
    const epoch = workspaceEpoch.current;
    let created = false;
    setBusy(true); setError(''); setModalError(''); setDirectoryRefreshIssue(null);
    try {
      await api.createProjectEntry(target.id, parentPath, d.name, d.entryType === 'directory');
      created = true;
      if (epoch !== workspaceEpoch.current) return;
      setModal(null);
      const nextEntries = await api.listDirectory(parentPath);
      if (epoch === workspaceEpoch.current && request === directoryRequest.current) {
        setDir(parentPath); setEntries(nextEntries); setDirectoryRefreshIssue(null);
      }
    } catch (e) {
      if (epoch !== workspaceEpoch.current || request !== directoryRequest.current) return;
      if (created) setDirectoryRefreshIssue({path:parentPath,message:`项目内容已创建，但目录刷新失败：${err(e)}`});
      else setModalError(err(e));
    } finally { projectCreateOperationBusyRef.current = false; setBusy(false); }
  };
  const submitModal = async () => {
    const d = modalData;
    if (modal === 'associate-markdown') {
      if (!project || project.id !== d.projectId) { setModal(null); setError('项目已切换，请重新选择文档。'); return; }
      return associateMarkdown(d.path);
    }
    if (modal === 'project-resource') {
      if (!project || project.id !== d.projectId || !projects.some(item => item.id === d.projectId)) { setModal(null); setError('项目已切换或不存在，请重新打开目标项目后再创建资源。'); return; }
      return submitProjectResourceCreate(project, d);
    }
    if (modal === 'project-entry') {
      if (!project || project.id !== d.projectId || !projects.some(item => item.id === d.projectId)) { setModal(null); setError('项目已切换或不存在，请重新打开目标项目后再创建文件。'); return; }
      const parentPath = d.parentPath || project.path;
      if (parentPath !== project.path && !parentPath.startsWith(`${project.path}/`)) { setModal(null); setError('项目目录已变化，请重新打开目标项目目录后再创建文件。'); return; }
      return submitProjectEntryCreate(project, d);
    }
    if (modal === 'project') return actSnapshot(() => api.createProject(d.name, d.description));
    if (modal === 'temporary') return run(() => api.createTemporary(d.name), value => { const next = value as Snapshot; const known = new Set(temporaryDirectories.map(item => item.id)); const created = next.temporaryDirectories.find(item => !known.has(item.id)); acceptSnapshot(next); setModal(null); if (created) doOpenTemporary(created); });
    if (modal === 'rename-temporary' && temporary) return run(() => api.renameTemporary(temporary.id, d.name, temporary.revision), value => { const next = value as Snapshot; const renamed = next.temporaryDirectories.find(item => item.id === temporary.id); acceptSnapshot(next); setModal(null); if (renamed) { setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); setDir(renamed.path); setEntries([]); void openDir(renamed.path); } });
    if (modal === 'delete-temporary' && temporary) return run(() => api.deleteTemporary(temporary.id, temporary.revision), value => { acceptSnapshot(value as Snapshot); setTemporaryId(''); setSelected(null); setDocument(null); setDocumentConflict(null); setDraft(''); setEditing(false); setEntries([]); setPage('home'); setModal(null); });
    if (modal === 'resource' && kind !== 'attachment') return actSnapshot(() => api.createResource(kind, d.name));
    if (modal === 'rename-project' && project) return actSnapshot(() => api.renameProject(project.id, d.name, project.revision));
    if (modal === 'edit-project' && project) return actSnapshot(() => api.updateProject(project.id, d.name, d.description, project.revision));
    if (modal === 'rename-resource' && selected) return run(() => api.renameResource(selected.kind, selected.path, d.name), v => { const next = v as Snapshot; const renamed = next.resources.find(r => r.kind === selected.kind && r.name === d.name) ?? null; acceptSnapshot(next); setSelected(renamed); if (document && renamed && document.path === selected.path) setDocument({ ...document, path: renamed.path }); setModal(null); });
    if (modal === 'delete-project' && project) return run(() => api.deleteProject(project.id, project.revision), v => { acceptSnapshot(v as Snapshot); setProjectId(''); setPage('home'); setModal(null); });
    if (modal === 'delete-resource' && selected) return run(() => api.deleteResource(selected.kind, selected.path), v => { acceptSnapshot(v as Snapshot); setSelected(null); setModal(null); });
    if (modal === 'promote' && project && d.path) return run(() => api.promoteFile(project.id, d.path, d.kind as ResourceKind, project.revision), v => { acceptSnapshot(v as Snapshot); setEntries(current => current.filter(entry => entry.path !== d.path)); setModal(null); });
    if (modal === 'link' && project && d.path) return actSnapshot(() => api.linkResource(project.id, d.kind as ResourceKind, d.path, project.revision));
    if (modal === 'shortcut') return run(async () => { const list = await api.saveShortcut({id:d.id || crypto.randomUUID(),title:d.title,kind:d.kind as Shortcut['kind'],target:d.target}); setBoot(b => b ? {...b,shortcuts:list}:b); setModal(null); });
  };
  const loadLatestAfterConflict = async (path: string, resourceToken: number, request: number) => {
    try {
      const latest = await api.readText(path);
      if (resourceToken === resourceRequest.current && request === conflictRequest.current) setDocumentConflict({ latest, error: null });
    } catch (e) {
      if (resourceToken === resourceRequest.current && request === conflictRequest.current) setDocumentConflict({ latest: null, error: `读取外部版本失败：${err(e)}` });
    }
  };
  const saveDoc = async () => {
    if (!document) return;
    const base = document;
    const content = draft;
    const resourceToken = resourceRequest.current;
    const request = ++conflictRequest.current;
    documentWriteCount.current += 1;
    documentWriteUiRequest.current = request;
    setBusy(true); setError('');
    try {
      const saved = await api.writeText(base.path, content, base.revision);
      if (resourceToken !== resourceRequest.current || request !== conflictRequest.current) return;
      setDocument(saved);
      if (draftRef.current === content) { setDraft(saved.content); setEditing(false); }
      else setEditing(true);
      setDocumentConflict(null);
    } catch (e) {
      if (resourceToken !== resourceRequest.current || request !== conflictRequest.current) return;
      if (isRevisionConflict(e)) {
        await loadLatestAfterConflict(base.path, resourceToken, request);
      } else setError(err(e));
    } finally {
      documentWriteCount.current = Math.max(0, documentWriteCount.current - 1);
      if (documentWriteUiRequest.current === request) { documentWriteUiRequest.current = null; setBusy(false); }
    }
  };
  const loadExternalVersion = () => {
    if (!documentConflict?.latest) return;
    conflictRequest.current += 1;
    setDocument(documentConflict.latest); setDraft(documentConflict.latest.content); setEditing(false); setDocumentConflict(null); setError('');
  };
  const overwriteExternalVersion = async () => {
    if (!documentConflict?.latest || !document) return;
    const latest = documentConflict.latest;
    const content = draft;
    const resourceToken = resourceRequest.current;
    const request = ++conflictRequest.current;
    documentWriteCount.current += 1;
    documentWriteUiRequest.current = request;
    setBusy(true); setError('');
    try {
      const saved = await api.writeText(latest.path, content, latest.revision);
      if (resourceToken !== resourceRequest.current || request !== conflictRequest.current) return;
      setDocument(saved);
      if (draftRef.current === content) { setDraft(saved.content); setEditing(false); }
      else setEditing(true);
      setDocumentConflict(null);
    } catch (e) {
      if (resourceToken !== resourceRequest.current || request !== conflictRequest.current) return;
      if (isRevisionConflict(e)) await loadLatestAfterConflict(latest.path, resourceToken, request);
      else { setError(err(e)); setDocumentConflict({ latest, error: `覆盖失败：${err(e)}` }); }
    } finally {
      documentWriteCount.current = Math.max(0, documentWriteCount.current - 1);
      if (documentWriteUiRequest.current === request) { documentWriteUiRequest.current = null; setBusy(false); }
    }
  };
  const saveSettings = () => settingsDraft && run(() => api.saveSettings(settingsDraft), v => setBoot(b => b ? {...b,settings:v as Settings}:b));
  const editShortcut = (s: Shortcut) => { setModalData({ id: s.id, title: s.title, kind: s.kind, target: s.target }); setModal('shortcut'); };
  const executeShortcut = (s: Shortcut) => { if (s.kind === 'project') { const target = projects.find(p => p.id === s.target); if (target) void openProject(target); else setError('快捷按钮关联的项目不存在，请编辑或删除该按钮。'); return; } if (s.kind === 'terminal' && !projects.some(p => p.id === s.target)) { setError('快捷按钮关联的项目不存在，请编辑或删除该按钮。'); return; } if (s.kind === 'script' && !resources.some(r => r.kind === 'script' && r.path === s.target)) { setError('快捷按钮关联的脚本不存在，请编辑或删除该按钮。'); return; } if (!s.target) { setError(`快捷按钮${shortcutLabels[s.kind]}缺少目标，请编辑后重试。`); return; } void run(() => api.runShortcut(s.id, project?.id)); };
  const resumeSession = (session: Session) => {
    const temp = (session.temporaryId && temporaryDirectories.find(item => item.id === session.temporaryId))
      ?? temporaryDirectories.find(item => { const full = `${boot?.root}/${item.path}`.replaceAll('//', '/'); return session.cwd === item.path || session.cwd === full || session.cwd.startsWith(`${full}/`); });
    if (temp) { navigateSafely(() => { doOpenTemporary(temp); void run(() => api.resumeTemporaryAgent(temp.id, session.provider, session.resumeTarget || session.id)); }); return; }
    const owner = (session.projectId && projects.find(p => p.id === session.projectId))
      ?? projects.find(p => { const full = `${boot?.root}/${p.path}`.replaceAll('//', '/'); return session.cwd === p.path || session.cwd === full || session.cwd.startsWith(`${full}/`); });
    if (owner) { navigateSafely(() => { doOpenProject(owner); void run(() => api.resumeAgent(owner.id, session.provider, session.resumeTarget || session.id)); }); return; }
    setError('无法匹配会话所属的项目或临时目录。');
  };
  const refreshWorkspace = () => { setBusy(true); void refresh().then(refreshed => { if (page !== 'project' || !refreshed?.snapshot) return; const target = refreshed.snapshot.projects.find(item => item.id === project?.id) ?? mostRecentProject(refreshed.snapshot.projects, refreshed.recentProjectIds) ?? refreshed.snapshot.projects[0]; if (target) return refreshProjectMarkdown(target); }).catch(e => setError(err(e))).finally(() => setBusy(false)); };

  if (!boot) return <div className="startup"><div className="brand-mark"><i/><i/><i/><i/></div><strong>AI 工作台</strong>{error && <div className="startup-actions"><p className="error-card">{error}</p><button className="button" onClick={() => void refresh()}>重试</button><button className="button primary" onClick={() => void chooseWorkspace()}>重新选择目录</button></div>}</div>;
  if (!boot.root || !snapshot) return <div className="setup-screen"><div className="setup-card"><div className="brand-mark"><i/><i/><i/><i/></div><h1>建立你的本地工作台</h1><p>选择一个新目录，用于存放工作台项目和资源。</p>{error && <div className="error-card">{error}</div>}<button className="button primary" disabled={busy} onClick={() => void chooseWorkspace()}><Folder size={15}/>选择工作台目录</button></div></div>;

  const renderResourceDetail = () => {
    if (!selected) return page==='resources'&&kind==='document' ? <div className="resource-detail empty-state notes-empty">选择一篇笔记查看内容</div> : <div className="resource-detail empty-state">从左侧选择一项资源</div>;
    const isBinary = selected.kind === 'attachment'; const isDirectory = selected.kind === 'repository' && !repositoryFile; const isMarkdown = /\.(md|markdown)$/i.test(selected.name); const suffix = selected.name.toLowerCase().split('.').pop();
    return <section className="resource-detail"><div className="resource-detail-head">{page==='resources'&&kind==='document'&&selected.kind==='document'&&<button className="button notes-back" onClick={returnToNotes}><ArrowLeft size={14}/>返回笔记</button>}<div className="resource-detail-title"><h2>{selected.name}</h2><small>{selected.path}</small></div><div className="head-actions">{editing ? <><button className="button" onClick={()=>setEditing(false)} disabled={busy||documentWriteCount.current>0}>取消</button><button className="button primary" onClick={saveDoc} disabled={busy||documentWriteCount.current>0}>保存</button></> : <>{selected.kind==='repository'&&!repositoryFile&&<button className="icon-button" title="在 Git 应用中打开" onClick={()=>void run(()=>api.openExternal(selected.path,settingsDraft?.gitApp||undefined))}><GitBranch size={15}/></button>}{selected.kind==='repository'&&repositoryFile&&<button className="icon-button" title="在编辑器中打开" onClick={()=>void run(()=>api.openExternal(selected.path,settingsDraft?.editorApp||undefined))}><ExternalLink size={15}/></button>}{!isBinary && !isDirectory && <button className="icon-button" title="编辑" onClick={()=>setEditing(true)}><Pencil size={15}/></button>}{!repositoryFile&&<><button className="icon-button" title="重命名" onClick={()=>{setModalData({name:selected.name});setModal('rename-resource')}}><Pencil size={15}/></button><button className="icon-button" title="删除" onClick={()=>setModal('delete-resource')}><Trash2 size={15}/></button></>}</>}</div></div>
      {isBinary ? <div className="attachment-open"><div className="file-icon attachment"><ExternalLink/></div><strong>{suffix === 'pdf' || suffix === 'docx' || suffix === 'doc' ? '通过外部应用打开' : '附件预览'}</strong>{imageData&&<img className="attachment-image" src={`data:${imageData.mimeType};base64,${imageData.data}`} alt={selected.name}/>}<button className="button" onClick={()=>void run(()=>api.openExternal(selected.path))}><ExternalLink size={14}/>打开附件</button></div> : isDirectory ? null : isMarkdown && !editing ? <React.Suspense fallback={<div className="markdown-loading">正在加载 Markdown 预览…</div>}><MarkdownDocument value={document?.content ?? '正在读取…'}/></React.Suspense> : editing && isMarkdown ? <React.Suspense fallback={<div className="markdown-loading">正在加载 Markdown 编辑器…</div>}><MarkdownDocument value={draft} editing onChange={value=>setDraft(value)}/></React.Suspense> : editing ? <textarea className="markdown-editor" value={draft} onChange={e=>setDraft(e.target.value)} spellCheck={false}/> : selected.kind === 'document' ? <pre className="markdown-preview text-preview">{document?.content ?? '正在读取…'}</pre> : <pre className="markdown-preview code-preview">{document?.content ?? '正在读取…'}</pre>}
      {selected.kind === 'repository' && !repositoryFile && <div className="file-browser"><div className="section-row"><strong>{dir||selected.path}</strong>{dir!==selected.path&&<button className="quiet-link" onClick={()=>goUp(selected.path)}>返回上级</button>}</div><div className="directory-list">{entries.map(e=><button className="directory-entry" key={e.path} onClick={()=>e.isDir?void openDir(e.path):void openResource({kind:'repository',name:e.name,path:e.path},true)}>{e.isDir?<Folder size={15}/>:<File size={15}/>}<span>{e.name}</span><ChevronRight size={14}/></button>)}</div></div>}
    </section>;
  };
  const resourceRows = (items: Resource[], linkedProject?: Project) => items.map(r=>{const Icon=icons[r.kind];return <div className="linked-resource-wrap" key={r.path}><button className={`linked-resource ${selected?.path===r.path?'selected':''}`} onClick={()=>void openResource(r)}><span className={`file-icon ${r.kind}`}><Icon size={15}/></span><span className="linked-copy"><strong>{r.name}</strong><small>{r.path}</small></span><ArrowRight size={14}/></button>{linkedProject&&<button className="unlink-resource" title="解除项目关联" onClick={()=>void actSnapshot(()=>api.unlinkResource(linkedProject.id,r.kind,r.path,linkedProject.revision))}><X size={13}/></button>}</div>});
  const sessionRows = projectSessions.map(s=><button className="session-row" key={s.provider+s.id} onClick={()=>project&&void run(()=>api.resumeAgent(project.id,s.provider,s.resumeTarget||s.id))}><span className="agent-badge">{s.provider.slice(0,2)}</span><span className="session-main"><strong className="session-title">{s.title || s.id}</strong><small className="session-meta">{s.provider} · {s.updatedAt}</small></span><ChevronRight size={15}/></button>);
  const temporarySessionRows = temporarySessions.map(s=><button className="session-row" key={s.provider+s.id} onClick={()=>temporary&&void run(()=>api.resumeTemporaryAgent(temporary.id,s.provider,s.resumeTarget||s.id))}><span className="agent-badge">{s.provider.slice(0,2)}</span><span className="session-main"><strong className="session-title">{s.title || s.id}</strong><small className="session-meta">{s.provider} · {s.updatedAt}</small></span><ChevronRight size={15}/></button>);
  const pageTitle = page==='home'?'首页':page==='project'?(project?.name??'项目目录'):page==='temporary'?(temporary?.name??'临时目录'):page==='resources'?labels[kind]:page==='shortcuts'?'快捷按钮':'设置';

  return <div className="window-frame"><aside className="sidebar"><div className="brand"><div className="brand-mark"><i/><i/><i/><i/></div><div><div className="brand-name">AI 工作台</div><div className="brand-caption">{boot.root}</div></div></div><div className="workspace-label">工作区</div><nav className="primary-nav"><Nav active={page==='home'} onClick={()=>setNav('home')} icon={<Home/>}>首页</Nav>{(['repository','document','attachment'] as ResourceKind[]).map(k=><Nav key={k} active={page==='resources'&&kind===k} onClick={()=>setNav('resources',k)} icon={k==='repository'?<GitBranch/>:k==='document'?<FileText/>:<Image/>}>{labels[k]}</Nav>)}<Nav active={page==='shortcuts'} onClick={()=>setNav('shortcuts')} icon={<Sparkles/>}>快捷按钮</Nav></nav><div className="sidebar-section-heading"><span>项目目录</span><button className="icon-button small" onClick={createProject} title="新建项目"><Plus/></button></div><div className="project-nav-list">{projectNavItems.map(p=><button className={`project-link ${page==='project'&&project?.id===p.id?'selected':''}`} key={p.id} onClick={()=>void openProject(p)}><i className="project-color"/><span>{p.name}</span><small>{p.resources.length}</small></button>)}</div><div className="sidebar-section-heading temporary-nav-heading"><span>临时目录</span><button className="icon-button small" onClick={createTemporary} title="新建临时目录"><Plus/></button></div><div className="project-nav-list">{temporaryDirectories.map(item=><button className={`project-link ${page==='temporary'&&temporary?.id===item.id?'selected':''}`} key={item.id} onClick={()=>openTemporary(item)}><i className="project-color temporary-color"/><span>{item.name}</span></button>)}</div><div className="sidebar-bottom"><button className={`profile-button ${page==='settings'?'active':''}`} onClick={()=>setNav('settings')}><span className="avatar"><SettingsIcon/></span><span><strong>设置</strong><small>工作台偏好</small></span></button></div></aside>
    <main className="main-shell"><header className="topbar"><div className="breadcrumbs"><span>AI 工作台</span><span className="crumb-sep">/</span><strong>{pageTitle}</strong></div><div className="topbar-actions"><button className="button compact" onClick={refreshWorkspace} disabled={busy} title="重新读取工作台配置并扫描当前项目 Markdown"><RefreshCw size={14}/>刷新工作台</button><button className="icon-button search-trigger" onClick={()=>{setModalData({query:''});setModal('search')}}><Search size={15}/><kbd>⌘ K</kbd></button></div></header>
    {error && <div className="inline-error" role="alert"><span>{error}</span>{isProjectResourcePartialSuccess(error)&&<button className="quiet-link" onClick={refreshWorkspace} disabled={busy}>刷新工作台</button>}<button className="icon-button" onClick={()=>setError('')}><X size={15}/></button></div>}
    <section className={`app-view ${page==='resources'&&kind==='document'?'doc-notes-view':''}`}>
      {page==='home'&&<><PageHead eyebrow="本地工作台" title="把注意力留给正在做的事" subtitle="项目、资料和常用入口，都在这里轻轻接上。" actions={<button className="button" onClick={createProject}><Plus/>新建项目</button>}/>{project&&<section className="home-hero"><div className="hero-copy"><div className="hero-kicker"><Sparkles/>最近项目</div><h2 className="hero-title">{project.name}</h2><p className="hero-note">{project.description || '开始整理这个项目。'}</p><div className="hero-meta"><span><Folder/>{project.resources.length} 个关联资源</span></div></div><div className="hero-actions"><button className="button primary" onClick={()=>void openProject(project)}>打开项目 <ArrowRight/></button></div></section>}<section className="panel"><PanelHead title="快捷按钮" subtitle="打开项目、应用，或运行自定义命令"><button className="icon-button small" onClick={()=>{setModalData({title:'',kind:'project',target:''});setModal('shortcut')}}><Plus/></button></PanelHead><div className="panel-body shortcut-strip">{shortcuts.slice(0,5).map(s=><button className="shortcut-button" key={s.id} onClick={()=>executeShortcut(s)}><span className="shortcut-icon"><ShortcutIcon kind={s.kind}/></span><span><strong>{s.title}</strong><small>{shortcutTargetLabel(s)}</small></span></button>)}</div>{!shortcuts.length&&<div className="empty-state">还没有快捷按钮</div>}</section><div className="section-row"><h2 className="section-heading">最近会话</h2></div><section className="panel"><div className="panel-body session-list">{[...sessions].sort((a,b)=>Date.parse(b.updatedAt)-Date.parse(a.updatedAt)).slice(0,5).map(s=><button className="session-row" key={s.provider+s.id+s.cwd} onClick={()=>resumeSession(s)}><span className="agent-badge">{s.provider.slice(0,2)}</span><span className="session-main"><strong className="session-title">{s.title||s.id}</strong><small className="session-meta">{sessionOwnerLabel(s)} · {s.provider} · {s.updatedAt}</small></span><ChevronRight size={15}/></button>)}{projects.length===0&&temporaryDirectories.length===0&&<Empty text="还没有项目或临时目录会话"/>}{allSessionsLoadedEmpty&&<Empty text="还没有工作台启动的会话"/>}{projects.map(p=>{const status=sessionLoadByProject[p.id];if(!status||status.status==='loading')return <div className="session-status-row" key={p.id}><strong>{p.name}</strong><span>正在加载会话…</span></div>;if(status.status==='error')return <div className="session-status-row error-card" key={p.id}><strong>{p.name}</strong><span>会话读取失败：{status.message}</span><button className="quiet-link" onClick={()=>void loadProjectSessions(p.id)}>重试</button></div>;if(status.errors.length)return <div className="session-status-row error-card" key={p.id}><strong>{p.name}</strong><span>部分 Provider 读取失败：{status.errors.join('；')}</span><button className="quiet-link" onClick={()=>void loadProjectSessions(p.id)}>重试</button></div>;return null;})}{temporaryDirectories.map(item=>{const status=sessionLoadByTemporary[item.id];if(!status||status.status==='loading')return <div className="session-status-row" key={item.id}><strong>{item.name} · 临时目录</strong><span>正在加载会话…</span></div>;if(status.status==='error')return <div className="session-status-row error-card" key={item.id}><strong>{item.name} · 临时目录</strong><span>会话读取失败：{status.message}</span><button className="quiet-link" onClick={()=>void loadTemporarySessions(item.id)}>重试</button></div>;if(status.errors.length)return <div className="session-status-row error-card" key={item.id}><strong>{item.name} · 临时目录</strong><span>部分 Provider 读取失败：{status.errors.join('；')}</span><button className="quiet-link" onClick={()=>void loadTemporarySessions(item.id)}>重试</button></div>;return null;})}</div></section></>}
      {page==='project'&&project&&<><PageHead title={project.name} subtitle={project.description} actions={<><button className="button" onClick={()=>{setModalData({name:project.name,description:project.description});setModal('edit-project')}}><Pencil/>编辑项目</button><button className="button" onClick={()=>{setModalData({name:project.name});setModal('rename-project')}}><Pencil/>重命名</button><button className="button" onClick={()=>setModal('delete-project')}><Trash2/>删除项目</button></>}/><div className="workspace-grid"><div className="workspace-main"><div className="workspace-tabs"><Tab active={projectTab==='overview'} onClick={()=>setProjectTab('overview')}>项目概览</Tab><Tab active={projectTab==='files'} onClick={()=>{setProjectTab('files');void openDir(project.path)}}>项目文件</Tab><Tab active={projectTab==='sessions'} onClick={()=>{setProjectTab('sessions');void loadProjectSessions(project.id)}}>历史会话</Tab></div>{projectTab==='overview'?<section className="panel workspace-overview"><div className="workspace-intro"><span className="project-mark">{project.name.slice(0,1)}</span><div><h2>{project.name}</h2><p>{project.path}</p></div></div>{(['repository','document','attachment','script'] as ResourceKind[]).map(k=><React.Fragment key={k}><div className="resource-group-title"><span>关联{labels[k].replace('管理','')}</span><div className="resource-group-actions"><button className="quiet-link" onClick={()=>{setModalData({kind:k,path:''});setModal('link')}}>关联已有</button><button className="quiet-link" onClick={()=>beginProjectResourceCreate(k)}>{projectResourceCreateLabels[k]}</button></div></div>{resourceRows(linked(project).filter(x=>x.kind===k).map(x=>x.resource!),project)}</React.Fragment>)}<div className="resource-group-title"><span>可关联 Markdown 文档</span><button className="quiet-link" disabled={markdownScanBusy} onClick={()=>void refreshProjectMarkdown(project)}>{markdownScanBusy?'扫描中…':'重新扫描'}</button></div>{markdownDocs.length?markdownDocs.map(doc=><div className="directory-entry-wrap" key={doc.path}><div className="directory-entry"><FileText size={15}/><span>{doc.name}</span><small>{doc.path}</small></div><button className="archive-file" disabled={busy} title={doc.path.startsWith('文档/')?'将工作台文档直接关联到项目，不移动文档':'移动到工作台文档并在原位置保留软链接'} onClick={()=>beginMarkdownAssociation(doc.path)}><Plus size={14}/>{doc.path.startsWith('文档/')?'关联已有':'关联并移动'}</button></div>):<Empty text={markdownScanBusy?'正在扫描 Markdown 文档…':'没有未关联的 Markdown 文档'}/>}</section>:projectTab==='sessions'?<section className="panel"><PanelHead title={`${project.name} · 历史会话`} subtitle="会话继续在对应的原生 Agent 中打开"/><div className="panel-body session-list">{sessionRows}{projectSessionStatus?.status==='loading'&&<div className="empty-state">正在加载历史会话…</div>}{projectSessionStatus?.status==='error'&&<div className="error-card">会话读取失败：{projectSessionStatus.message} <button className="quiet-link" onClick={()=>void loadProjectSessions(project.id)}>重试</button></div>}{projectSessionStatus?.status==='loaded'&&projectSessionStatus.errors.length>0&&<div className="error-card">部分 Provider 读取失败：{projectSessionStatus.errors.join('；')} <button className="quiet-link" onClick={()=>void loadProjectSessions(project.id)}>重试</button></div>}{projectSessionStatus?.status==='loaded'&&projectSessionStatus.errors.length===0&&!projectSessions.length&&<Empty text="还没有从工作台启动的历史会话"/>}</div></section>:<section className="panel"><PanelHead title="项目目录" subtitle={dir || project.path}><div className="project-entry-actions">{dir && dir!==project.path&&<button className="quiet-link" onClick={()=>goUp(project.path)}>返回上级</button>}{canCreateProjectEntry&&<><button className="button compact" onClick={()=>beginProjectEntryCreate(false)}>新建文件</button><button className="button compact" onClick={()=>beginProjectEntryCreate(true)}>新建文件夹</button></>}</div></PanelHead>{directoryRefreshIssue?.path===projectEntryParentPath&&<div className="error-card directory-refresh-issue"><span>{directoryRefreshIssue.message}</span><button className="quiet-link" disabled={busy} onClick={()=>void openDir(directoryRefreshIssue.path)}>刷新目录</button></div>}<div className="panel-body directory-list">{entries.map(e=>e.isDir?<button className="directory-entry" key={e.path} onClick={()=>void openDir(e.path)}><Folder size={15}/><span>{e.name}</span><ChevronRight size={14}/></button>:<div className="directory-entry-wrap" key={e.path}><button className="directory-entry" onClick={()=>void openResource({kind:'repository',name:e.name,path:e.path},true)}><File size={15}/><span>{e.name}</span></button>{e.name.toLowerCase()==='agents.md'?<span className="project-rule-label">项目规则</span>:<button className="archive-file" title="归档为资源" onClick={()=>{setModalData({path:e.path,kind:'document'});setModal('promote')}}><Plus size={14}/>归档</button>}</div>)}{!entries.length&&<Empty text="目录为空或尚未读取"/>}</div></section>}</div><aside className="panel detail-panel"><div className="detail-label">启动原生 Agent</div><button className="agent-launch terminal-launch" onClick={()=>void run(()=>api.openTerminal(project.id))}><span><span className="agent-badge terminal-badge"><Terminal size={14}/></span>打开终端</span><ExternalLink size={13}/></button><div className="agent-launch-list">{(agents.length?agents:[{id:'codex',name:'Codex',available:false,path:null},{id:'claude',name:'Claude',available:false,path:null},{id:'pi',name:'Pi',available:false,path:null}]).map(a=><button className="agent-launch" key={a.id} disabled={!a.available} onClick={()=>void run(async()=>{await api.startAgent(project.id,a.id);await loadProjectSessions(project.id)})}><span><span className="agent-badge"><Bot size={14}/></span>{a.name}</span><small>{a.available?'启动': '未检测到'}</small></button>)}</div><div className="detail-line"/><div className="detail-label">资源概况</div>{(['repository','document','attachment','script'] as ResourceKind[]).map(k=><div className="detail-stat-row" key={k}><span>{labels[k]}</span><strong>{project.resources.filter(x=>x.kind===k).length}</strong></div>)}</aside></div></>}
      {page==='temporary'&&temporary&&<>
        <PageHead title={temporary.name} subtitle={`临时目录 · ${temporary.path}`} actions={<><button className="button" onClick={()=>navigateSafely(()=>{setModalData({name:temporary.name});setModal('rename-temporary')})}><Pencil/>重命名</button><button className="button" onClick={()=>navigateSafely(()=>setModal('delete-temporary'))}><Trash2/>删除目录</button></>}/>
        <div className="workspace-grid">
          <div className="workspace-main">
            <section className="panel"><PanelHead title="目录文件" subtitle={dir || temporary.path}>{dir!==temporary.path&&<button className="quiet-link" onClick={()=>goUp(temporary.path)}>返回上级</button>}</PanelHead><div className="panel-body directory-list">{entries.map(entry=><button className="directory-entry" key={entry.path} onClick={()=>void openTemporaryFile(entry)}>{entry.isDir?<Folder size={15}/>:<File size={15}/>}<span>{entry.name}</span><ChevronRight size={14}/></button>)}{!entries.length&&<Empty text="目录为空或尚未读取"/>}</div></section>
            {selected&&renderResourceDetail()}
            <section className="panel"><PanelHead title="历史会话" subtitle="会话继续在对应的原生 Agent 中打开"/><div className="panel-body session-list">{temporarySessionRows}{temporarySessionStatus?.status==='loading'&&<div className="empty-state">正在加载历史会话…</div>}{temporarySessionStatus?.status==='error'&&<div className="error-card">会话读取失败：{temporarySessionStatus.message} <button className="quiet-link" onClick={()=>void loadTemporarySessions(temporary.id)}>重试</button></div>}{temporarySessionStatus?.status==='loaded'&&temporarySessionStatus.errors.length>0&&<div className="error-card">部分 Provider 读取失败：{temporarySessionStatus.errors.join('；')} <button className="quiet-link" onClick={()=>void loadTemporarySessions(temporary.id)}>重试</button></div>}{temporarySessionStatus?.status==='loaded'&&temporarySessionStatus.errors.length===0&&!temporarySessions.length&&<Empty text="还没有此临时目录的历史会话"/>}</div></section>
          </div>
          <aside className="panel detail-panel"><div className="detail-label">临时目录工具</div><button className="agent-launch terminal-launch" onClick={()=>void run(()=>api.openTemporaryTerminal(temporary.id))}><span><span className="agent-badge terminal-badge"><Terminal size={14}/></span>打开终端</span><ExternalLink size={13}/></button><div className="detail-line"/><div className="detail-label">启动原生 Agent</div><div className="agent-launch-list">{(agents.length?agents:[{id:'codex',name:'Codex',available:false,path:null},{id:'claude',name:'Claude',available:false,path:null},{id:'pi',name:'Pi',available:false,path:null}]).map(agent=><button className="agent-launch" key={agent.id} disabled={!agent.available} onClick={()=>void run(async()=>{await api.startTemporaryAgent(temporary.id,agent.id);await loadTemporarySessions(temporary.id)})}><span><span className="agent-badge"><Bot size={14}/></span>{agent.name}</span><small>{agent.available?'启动':'未检测到'}</small></button>)}</div><div className="detail-line"/><div className="detail-label">保留说明</div><div className="detail-value">删除此目录不会删除原生 Agent 的历史会话记录。</div></aside>
        </div>
      </>}
      {page==='resources'&&(kind==='document'?<div className={`notes-workspace ${selected?.kind==='document'?'notes-detail-open':''}`}><aside className="notes-list-pane"><div className="notes-list-head"><h2>笔记</h2><div className="notes-list-actions"><span>{filtered.length}</span><button className="icon-button notes-create" aria-label="新建笔记" title="新建笔记" onClick={()=>createResource('document')}><Plus size={15}/></button></div></div><input className="modal-input notes-search" placeholder="搜索文档" value={filter} onChange={e=>setFilter(e.target.value)}/><div className="notes-list">{filtered.map(r=><button className={`note-row ${selected?.path===r.path?'selected':''}`} key={r.path} onClick={()=>void openResource(r)}><strong>{r.name}</strong><span>{r.path}</span></button>)}{!filtered.length&&<Empty text="没有文档"/>}</div></aside><div className="notes-detail-pane">{renderResourceDetail()}</div></div>:<><PageHead title={labels[kind]} subtitle={kind==='attachment'?"附件可从项目文件归档或在项目中添加本机附件，也可以关联到多个项目。":"独立管理资源，也可以关联到多个项目。"} actions={kind==='attachment'?<button className="button primary" onClick={()=>{if(!project){setError('请先创建或选择项目，再从项目文件归档附件。');return;}setProjectTab('files');setPage('project');void openDir(project.path);}}><Plus/>从项目归档附件</button>:<button className="button primary" onClick={()=>createResource(kind)}><Plus/>新建{kind==='repository'?'仓库':'文档'}</button>}/><div className="resources-layout"><section className="panel resources-list"><div className="panel-head"><input className="modal-input" placeholder="搜索资源" value={filter} onChange={e=>setFilter(e.target.value)}/><button className="icon-button" onClick={()=>setFilter('')}><X size={15}/></button></div><div className="panel-body">{resourceRows(filtered)}{!filtered.length&&<Empty text="没有找到资源"/>}</div></section>{renderResourceDetail()}</div></>)}
      {page==='shortcuts'&&<><PageHead title="快捷按钮" subtitle="运行前会调用对应的本地工作台操作。" actions={shortcutTab==='buttons'?<button className="button primary" onClick={()=>{setModalData({title:'',kind:'project',target:''});setModal('shortcut')}}><Plus/>添加快捷按钮</button>:<button className="button primary" onClick={()=>createResource('script')}><Plus/>新建脚本</button>}/><div className="shortcut-tabs"><Tab active={shortcutTab==='buttons'} onClick={()=>switchShortcutTab('buttons')}>快捷按钮</Tab><Tab active={shortcutTab==='scripts'} onClick={()=>switchShortcutTab('scripts')}>脚本资源</Tab></div>{shortcutTab==='buttons'?<div className="shortcut-grid">{shortcuts.map(s=><div className="shortcut-card" key={s.id}><button className="shortcut-run" onClick={()=>executeShortcut(s)}><span className="shortcut-icon"><ShortcutIcon kind={s.kind}/></span><strong>{s.title}</strong><small>{shortcutLabels[s.kind]} · {shortcutTargetLabel(s)}</small></button><div className="shortcut-card-actions"><button className="quiet-link" onClick={()=>editShortcut(s)}>编辑</button><button className="quiet-link" onClick={()=>void run(async()=>{const list=await api.deleteShortcut(s.id);setBoot(b=>b?{...b,shortcuts:list}:b)})}>删除</button></div></div>)}</div>:<div className="resources-layout"><section className="panel resources-list"><PanelHead title="脚本资源" subtitle="可编辑、归档到项目，也可用于脚本快捷按钮"/><div className="panel-body">{resources.filter(r=>r.kind==='script').map(r=><button className="linked-resource" key={r.path} onClick={()=>void openResource(r)}><Terminal/><span className="linked-copy"><strong>{r.name}</strong><small>{r.path}</small></span><ArrowRight size={14}/></button>)}{!resources.some(r=>r.kind==='script')&&<Empty text="没有脚本资源"/>}</div></section>{renderResourceDetail()}</div>}</>}
      {page==='settings'&&settingsDraft&&<><PageHead title="设置" subtitle="为本地工作台指定常用外部应用。" actions={<><button className="button" onClick={()=>void chooseWorkspace()} disabled={busy}><Folder size={14}/>更换工作台目录</button><button className="button primary" onClick={saveSettings} disabled={busy}>保存设置</button></>}/><div className="settings-layout"><section className="panel settings-panel"><div className="settings-section-head"><Terminal/><div><h2>外部应用</h2><p>应用名称或可执行程序路径</p></div></div>{(['terminalApp','gitApp','editorApp'] as const).map((key,i)=><label className="settings-row" key={key}><span className="settings-copy"><strong>{['终端应用','Git 管理应用','代码编辑器'][i]}</strong><span>{['用于在项目中打开终端','用于打开仓库的 Git 管理界面','用于打开代码编辑器'][i]}</span></span><input className="modal-input" value={settingsDraft[key]} onChange={e=>setSettingsDraft({...settingsDraft,[key]:e.target.value})}/></label>)}</section><section className="panel settings-panel"><div className="settings-section-head"><Bot/><div><h2>Agent 路径</h2><p>留空时使用系统可发现的程序</p></div></div>{Object.entries(settingsDraft.agentPaths).map(([key,value])=><label className="settings-row" key={key}><span className="settings-copy"><strong>{key}</strong></span><input className="modal-input" value={value} onChange={e=>setSettingsDraft({...settingsDraft,agentPaths:{...settingsDraft.agentPaths,[key]:e.target.value}})}/></label>)}</section></div></>}
    </section></main>
    {modal&&<div className="modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget&&!projectCreateOperationBusyRef.current)setModal(null)}}><div className="modal" role="dialog" aria-modal="true"><button className="modal-close icon-button" disabled={projectCreateOperationBusyRef.current} onClick={()=>setModal(null)}><X size={15}/></button><h2>{modal==='shortcut'&&modalData.id?'编辑快捷按钮':modal==='project-resource'?projectResourceCreateLabels[modalData.kind as ResourceKind]:modal==='project-entry'?(modalData.entryType==='directory'?'新建文件夹':'新建文件'):modalTitle(modal)}</h2><p className="modal-copy">{modal==='project-resource'?(modalData.kind==='attachment'?'保存后会打开系统文件选择器，所选附件将添加到当前项目。':modalData.kind==='repository'?'填写仓库名称和 Git 克隆地址，克隆成功后将关联到当前项目。':'填写文件名，新建后将关联到当前项目。'):modal==='project-entry'?'内容会直接创建在当前项目目录中。':modal==='link'?'选择工作台中已有的资源，将其关联到当前项目。':modal==='delete-project'?`删除后，项目目录“${project?.name}”将进入 macOS 废纸篓。`:modal==='delete-temporary'?`删除后，临时目录“${temporary?.name}”将进入 macOS 废纸篓。原生 Agent 的历史会话记录会保留。`:modal==='delete-resource'?`删除后，资源“${selected?.name}”将进入 macOS 废纸篓。仍被项目关联的资源须先解除关联。`:modal==='associate-markdown'?'此文档位于 Git 仓库中。关联会将工作区文件替换为指向工作台文档目录的软链接；Git 会记录软链接变更，其他克隆不会获得文档正文。':'填写信息后保存到本地工作台。'}</p>{modal==='associate-markdown'&&<p className="modal-copy">{modalData.path}</p>}{['project','rename-project','edit-project','temporary','rename-temporary'].includes(modal)&&<><input className="modal-input" placeholder={modal==='temporary'||modal==='rename-temporary'?'临时目录名称':'项目名称'} value={modalData.name??''} onChange={e=>setModalData({...modalData,name:e.target.value})}/>{(modal==='project'||modal==='edit-project')&&<textarea className="modal-input modal-textarea" placeholder="项目说明（可选）" value={modalData.description??''} onChange={e=>setModalData({...modalData,description:e.target.value})}/>}</>}{['resource','rename-resource'].includes(modal)&&<><input className="modal-input" placeholder="名称" value={modalData.name??''} onChange={e=>setModalData({...modalData,name:e.target.value})}/>{modal==='resource'&&<p className="modal-copy">类型：{labels[kind]}。新建内容由本地工作台管理。</p>}</>}{modal==='project-resource'&&<>{modalData.kind!=='attachment'&&<input className="modal-input" autoFocus placeholder={modalData.kind==='repository'?'仓库名称':'文件名'} value={modalData.name??''} onChange={e=>setModalData({...modalData,name:e.target.value})}/> }{modalData.kind==='repository'&&<input className="modal-input" placeholder="Git 克隆地址" value={modalData.cloneUrl??''} onChange={e=>setModalData({...modalData,cloneUrl:e.target.value})}/>}</>}{modal==='project-entry'&&<input className="modal-input" autoFocus placeholder={modalData.entryType==='directory'?'文件夹名称':'文件名'} value={modalData.name??''} onChange={e=>setModalData({...modalData,name:e.target.value})}/>}{modal==='promote'&&<><p className="modal-copy">将项目文件登记为独立资源，并保留项目关联。</p><select className="modal-input" value={modalData.kind} onChange={e=>setModalData({...modalData,kind:e.target.value})}>{(['document','attachment','script'] as ResourceKind[]).map(k=><option key={k} value={k}>{labels[k]}</option>)}</select><div className="modal-copy">{modalData.path}</div></>}{modal==='link'&&<><select className="modal-input" value={modalData.kind} onChange={e=>setModalData({...modalData,kind:e.target.value})}>{(['repository','document','attachment','script'] as ResourceKind[]).map(k=><option key={k} value={k}>{labels[k]}</option>)}</select><select className="modal-input" value={modalData.path} onChange={e=>setModalData({...modalData,path:e.target.value})}><option value="">选择资源</option>{resources.filter(r=>r.kind===modalData.kind).map(r=><option key={r.path} value={r.path}>{r.name} · {r.path}</option>)}</select></>}{modal==='shortcut'&&<><input className="modal-input" placeholder="按钮名称" value={modalData.title??''} onChange={e=>setModalData({...modalData,title:e.target.value})}/><select className="modal-input" value={modalData.kind} onChange={e=>setModalData({...modalData,kind:e.target.value})}>{(['project','application','terminal','script','command'] as const).map(k=><option key={k} value={k}>{shortcutLabels[k]}</option>)}</select>{modalData.kind==='project'||modalData.kind==='terminal'?<select className="modal-input" value={modalData.target??''} onChange={e=>setModalData({...modalData,target:e.target.value})}><option value="">选择项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>:modalData.kind==='script'?<select className="modal-input" value={modalData.target??''} onChange={e=>setModalData({...modalData,target:e.target.value})}><option value="">选择脚本</option>{resources.filter(r=>r.kind==='script').map(r=><option key={r.path} value={r.path}>{r.name}</option>)}</select>:<input className="modal-input" placeholder="应用路径或命令" value={modalData.target??''} onChange={e=>setModalData({...modalData,target:e.target.value})}/> }</>}{modal==='search'&&<><input className="modal-input" autoFocus placeholder="搜索项目和资源" value={modalData.query??''} onChange={e=>setModalData({...modalData,query:e.target.value})}/><div className="modal-results">{[...projects.map(p=>({label:p.name,path:p.path,open:()=>void openProject(p)})),...resources.map(r=>({label:r.name,path:r.path,open:()=>void openResource(r)}))].filter(x=>`${x.label} ${x.path}`.toLowerCase().includes((modalData.query??'').toLowerCase())).map(x=><button className="search-hit" key={x.path} onClick={()=>{setModal(null);x.open()}}>{x.label}<small>{x.path}</small></button>)}</div></>}{modal==='delete-resource'&&linkedProjectsForSelection.length>0&&<div className="error-card">请先在以下项目中解除关联，再删除资源：{linkedProjectsForSelection.map(p=>p.name).join('、')}。</div>}{modalError&&(modal==='project-resource'||modal==='project-entry')&&<div className="error-card modal-operation-error" role="alert">{modalError}</div>}<div className="modal-actions"><button className="button" disabled={projectCreateOperationBusyRef.current} onClick={()=>setModal(null)}>取消</button><button className={`button ${modal.startsWith('delete')?'':'primary'}`} disabled={busy||projectCreateOperationBusyRef.current||(modal==='delete-resource'&&linkedProjectsForSelection.length>0)||(['project','resource','rename-project','edit-project','rename-resource','temporary','rename-temporary'].includes(modal)&&!modalData.name?.trim())||(modal==='project-resource'&&modalData.kind!=='attachment'&&(!modalData.name?.trim()||(modalData.kind==='repository'&&!modalData.cloneUrl?.trim())))||(modal==='project-entry'&&!modalData.name?.trim())||(modal==='shortcut'&&(!modalData.title?.trim()||!modalData.target?.trim()))} onClick={()=>void submitModal()}>{projectCreateOperationBusyRef.current?(modal==='project-resource'&&modalData.kind==='repository'?'克隆中…':'处理中…'):modal.startsWith('delete')?'确认删除':modal==='project-resource'&&modalData.kind==='attachment'?'选择附件':modal==='project-entry'?'创建':modal==='associate-markdown'?'移动并关联':'保存'}</button></div></div></div>}
    {documentConflict&&<div className="modal-backdrop discard-confirm-backdrop"><div className="modal conflict-dialog" role="dialog" aria-modal="true" aria-labelledby="conflict-title"><h2 id="conflict-title">文档已在外部修改</h2><p>保存时发现磁盘版本已变化。你的草稿仍保留；请选择如何处理。</p>{documentConflict.error&&<div className="error-card conflict-error" role="alert">{documentConflict.error}</div>}<div className="conflict-actions"><button className="button" onClick={()=>setDocumentConflict(null)}>继续编辑草稿</button><button className="button" disabled={!documentConflict.latest||busy} onClick={loadExternalVersion}>载入外部版本</button><button className="button primary" disabled={!documentConflict.latest||busy} onClick={()=>void overwriteExternalVersion()}>用草稿覆盖外部版本</button></div>{!documentConflict.latest&&<button className="quiet-link conflict-retry" disabled={busy} onClick={()=>document&&void loadLatestAfterConflict(document.path,resourceRequest.current,conflictRequest.current)}>重新读取外部版本</button>}</div></div>}
    {pendingLeave&&<div className="modal-backdrop discard-confirm-backdrop"><div className="modal discard-confirm" role="alertdialog" aria-modal="true" aria-labelledby="discard-title" aria-describedby="discard-description"><h2 id="discard-title">放弃未保存的修改？</h2><p id="discard-description">“{selected?.name ?? document?.path ?? '当前文件'}”有未保存修改。继续编辑会留在当前页面；放弃修改后才会执行刚才的操作。</p><div className="modal-actions"><button className="button" autoFocus onClick={()=>setPendingLeave(null)}>继续编辑</button><button className="button danger" onClick={discardAndLeave}>放弃修改</button></div></div></div>}
    {busy&&<div className="busy-indicator">处理中…</div>}
  </div>;
}

function Nav({active,onClick,icon,children}:{active:boolean;onClick:()=>void;icon:React.ReactNode;children:React.ReactNode}){return <button className={`nav-item ${active?'active':''}`} onClick={onClick}><span className="nav-icon">{icon}</span><span>{children}</span></button>}
function PageHead({eyebrow,title,subtitle,actions}:{eyebrow?:string;title:string;subtitle?:string;actions?:React.ReactNode}){return <div className="page-head"><div>{eyebrow&&<div className="eyebrow">{eyebrow}</div>}<h1 className="page-title">{title}</h1>{subtitle&&<p className="page-subtitle">{subtitle}</p>}</div>{actions&&<div className="head-actions">{actions}</div>}</div>}
function PanelHead({title,subtitle,children}:{title:string;subtitle?:string;children?:React.ReactNode}){return <div className="panel-head"><div><h2 className="panel-title">{title}</h2>{subtitle&&<div className="panel-subtitle">{subtitle}</div>}</div>{children}</div>}
function Empty({text}:{text:string}){return <div className="empty-state">{text}</div>}
function Tab({active,onClick,children}:{active:boolean;onClick:()=>void;children:React.ReactNode}){return <button className={`workspace-tab ${active?'active':''}`} onClick={onClick}>{children}</button>}
function ShortcutIcon({kind}:{kind:Shortcut['kind']}){return kind==='project'?<Folder/>:kind==='terminal'?<Terminal/>:kind==='script'||kind==='command'?<Code2/>:<ExternalLink/>}
function modalTitle(m: ModalKind) { return ({ project: '新建项目', resource: '新建资源', 'project-resource': '创建项目资源', 'project-entry': '新建项目内容', shortcut: '添加快捷按钮', 'edit-project': '编辑项目', promote: '整理为独立资源', 'rename-project': '重命名项目', temporary: '新建临时目录', 'rename-temporary': '重命名临时目录', 'delete-temporary': '删除临时目录', 'rename-resource': '重命名资源', 'delete-project': '删除项目', 'delete-resource': '删除资源', link: '关联资源', search: '搜索工作台', 'associate-markdown': '关联仓库中的文档' })[m]; }
