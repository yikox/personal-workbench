import { invoke } from '@tauri-apps/api/core';

export type ResourceKind = 'repository' | 'document' | 'attachment' | 'script';
export interface ResourceLink { kind: ResourceKind; path: string; alias: string }
export interface Project { id: string; name: string; description: string; path: string; resources: ResourceLink[]; revision: string }
export interface TemporaryDirectory { id: string; name: string; path: string; revision: string }
export interface Resource { kind: ResourceKind; name: string; path: string }
export interface Snapshot { root: string; projects: Project[]; resources: Resource[]; temporaryDirectories: TemporaryDirectory[] }
export interface Settings { terminalApp: string; gitApp: string; editorApp: string; agentPaths: Record<string, string> }
export interface Shortcut { id: string; title: string; kind: 'project' | 'application' | 'terminal' | 'script' | 'command'; target: string }
export interface BootState { root: string | null; snapshot: Snapshot | null; settings: Settings; shortcuts: Shortcut[]; recentProjectIds: string[] }
export interface Document { path: string; content: string; revision: string }
export interface ImageData { mimeType: string; data: string }
export interface Entry { name: string; path: string; isDir: boolean }
export interface DiscoveredDocument { name: string; path: string }
export interface AgentInfo { id: string; name: string; available: boolean; path: string | null }
export interface Session { provider: string; id: string; title: string; updatedAt: string; cwd: string; projectId?: string; temporaryId?: string; resumeTarget?: string }
export interface SessionListResult { sessions: Session[]; errors: string[] }

const call = <T>(command: string, args?: Record<string, unknown>) => invoke<T>(command, args);
export const api = {
  bootstrap: () => call<BootState>('bootstrap'), chooseWorkspace: () => call<BootState | null>('choose_workspace'),
  createProject: (name: string, description: string) => call<Snapshot>('create_project', { name, description }),
  updateProject: (projectId: string, name: string, description: string, expectedRevision: string) => call<Snapshot>('update_project', { projectId, name, description, expectedRevision }),
  renameProject: (projectId: string, newName: string, expectedRevision: string) => call<Snapshot>('rename_project', { projectId, newName, expectedRevision }),
  deleteProject: (projectId: string, expectedRevision: string) => call<Snapshot>('delete_project', { projectId, expectedRevision }),
  createTemporary: (name: string) => call<Snapshot>('create_temporary', { name }),
  renameTemporary: (temporaryId: string, newName: string, expectedRevision: string) => call<Snapshot>('rename_temporary', { temporaryId, newName, expectedRevision }),
  deleteTemporary: (temporaryId: string, expectedRevision: string) => call<Snapshot>('delete_temporary', { temporaryId, expectedRevision }),
  createResource: (kind: ResourceKind, name: string) => call<Snapshot>('create_resource', { kind, name }),
  createProjectEntry: (projectId: string, parentPath: string, name: string, isDir: boolean) => call<void>('create_project_entry', { projectId, parentPath, name, isDir }),
  createProjectResource: (projectId: string, expectedRevision: string, kind: ResourceKind, name: string, cloneUrl?: string) => call<Snapshot | null>('create_project_resource', { projectId, expectedRevision, kind, name, ...(cloneUrl ? { cloneUrl } : {}) }),
  renameResource: (kind: ResourceKind, path: string, newName: string) => call<Snapshot>('rename_resource', { kind, path, newName }),
  deleteResource: (kind: ResourceKind, path: string) => call<Snapshot>('delete_resource', { kind, path }),
  linkResource: (projectId: string, kind: ResourceKind, path: string, expectedRevision: string) => call<Snapshot>('link_resource', { projectId, kind, path, expectedRevision }),
  unlinkResource: (projectId: string, kind: ResourceKind, path: string, expectedRevision: string) => call<Snapshot>('unlink_resource', { projectId, kind, path, expectedRevision }),
  listDirectory: (path: string) => call<Entry[]>('list_directory', { path }), readText: (path: string) => call<Document>('read_text', { path }),
  readImage: (path: string) => call<ImageData>('read_image', { path }),
  writeText: (path: string, content: string, expectedRevision: string) => call<Document>('write_text', { path, content, expectedRevision }),
  openExternal: (path: string, application?: string) => call<void>('open_external', { path, ...(application ? { application } : {}) }),
  agentStatus: () => call<AgentInfo[]>('agent_status'), startAgent: (projectId: string, provider: string) => call<void>('start_agent', { projectId, provider }),
  listSessions: (projectId: string) => call<SessionListResult>('list_sessions', { projectId }), resumeAgent: (projectId: string, provider: string, sessionId: string) => call<void>('resume_agent', { projectId, provider, sessionId }),
  startTemporaryAgent: (temporaryId: string, provider: string) => call<void>('start_temporary_agent', { temporaryId, provider }),
  listTemporarySessions: (temporaryId: string) => call<SessionListResult>('list_temporary_sessions', { temporaryId }),
  resumeTemporaryAgent: (temporaryId: string, provider: string, sessionId: string) => call<void>('resume_temporary_agent', { temporaryId, provider, sessionId }),
  openTemporaryTerminal: (temporaryId: string) => call<void>('open_temporary_terminal', { temporaryId }),
  openTerminal: (projectId: string) => call<void>('open_terminal', { projectId }),
  saveShortcut: (shortcut: Shortcut) => call<Shortcut[]>('save_shortcut', { shortcut }), deleteShortcut: (id: string) => call<Shortcut[]>('delete_shortcut', { id }),
  runShortcut: (id: string, projectId?: string) => call<void>('run_shortcut', { id, ...(projectId ? { projectId } : {}) }),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', { settings }), promoteFile: (projectId: string, path: string, kind: ResourceKind, expectedRevision: string) => call<Snapshot>('promote_file', { projectId, path, kind, expectedRevision }),
  recordProjectVisit: (projectId: string) => call<string[]>('record_project_visit', { projectId }),
  discoverProjectMarkdown: (projectId: string) => call<DiscoveredDocument[]>('discover_project_markdown', { projectId }),
  associateProjectMarkdown: (projectId: string, path: string, expectedRevision: string) => call<Snapshot>('associate_project_markdown', { projectId, path, expectedRevision }),
};
