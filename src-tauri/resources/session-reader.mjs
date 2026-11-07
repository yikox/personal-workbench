import { spawn } from 'node:child_process';
import { existsSync, realpathSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import readline from 'node:readline';

const [provider, requestedCwd, resourceDir, agentExecutable] = process.argv.slice(2);

try {
  if (!provider || !requestedCwd || !resourceDir || !agentExecutable) {
    throw new Error('用法: session-reader.mjs <provider> <project-dir> <resource-dir> <agent-executable>');
  }
  const cwd = resolve(requestedCwd);
  let sessions;
  if (provider === 'codex') sessions = await listCodex(agentExecutable, cwd);
  else if (provider === 'claude') sessions = await listClaude(resourceDir, cwd);
  else if (provider === 'pi') sessions = await listPi(agentExecutable, cwd);
  else throw new Error(`不支持的 Agent: ${provider}`);
  process.stdout.write(JSON.stringify(sessions));
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}

function toSession(provider, id, title, timestamp, cwd, resumeTarget) {
  if (typeof id !== 'string' || id.length === 0) throw new Error(`${provider} 返回的会话缺少 ID`);
  const date = timestamp instanceof Date ? timestamp : new Date(timestamp);
  if (Number.isNaN(date.getTime())) throw new Error(`${provider} 返回了无效的更新时间: ${timestamp}`);
  return {
    provider,
    id,
    title: String(title || id),
    updatedAt: date.toISOString(),
    cwd: String(cwd || ''),
    ...(resumeTarget ? { resumeTarget } : {}),
  };
}

async function listClaude(resources, cwd) {
  const bundled = join(resources, 'claude-sdk.mjs');
  let sdk;
  if (existsSync(bundled)) {
    sdk = await import(pathToFileURL(bundled).href);
  } else {
    const localPackage = resolve(dirname(fileURLToPath(import.meta.url)), '../../node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs');
    const cwdPackage = resolve(process.cwd(), 'node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs');
    const sdkPath = [localPackage, cwdPackage].find(existsSync);
    if (!sdkPath) throw new Error(`Claude Agent SDK 不可用: ${bundled}、${localPackage} 与 ${cwdPackage} 均不存在`);
    sdk = await import(pathToFileURL(sdkPath).href);
  }
  if (typeof sdk.listSessions !== 'function') throw new Error('Claude Agent SDK 未导出 listSessions');
  const found = await withProviderTimeout('Claude', sdk.listSessions({ dir: cwd, includeWorktrees: false }));
  if (!Array.isArray(found)) throw new Error('Claude Agent SDK listSessions 返回值不是数组');
  return found
    .filter((session) => !session.cwd || resolve(session.cwd) === cwd)
    .map((session) => toSession('claude', session.sessionId, session.customTitle || session.summary || session.firstPrompt, session.lastModified, session.cwd || cwd));
}

async function listPi(executable, cwd) {
  const realExecutable = realpathSync(executable);
  const packageRoot = resolve(dirname(realExecutable), '../..');
  const installedEntry = join(packageRoot, 'dist/index.js');
  const localEntry = resolve(dirname(fileURLToPath(import.meta.url)), '../../node_modules/@earendil-works/pi-coding-agent/dist/index.js');
  const cwdEntry = resolve(process.cwd(), 'node_modules/@earendil-works/pi-coding-agent/dist/index.js');
  const entry = [installedEntry, localEntry, cwdEntry].find(existsSync);
  if (!entry) throw new Error(`Pi SessionManager 不可用: ${installedEntry}、${localEntry} 与 ${cwdEntry} 均不存在`);
  const pi = await import(pathToFileURL(entry).href);
  if (!pi.SessionManager || typeof pi.SessionManager.list !== 'function') throw new Error('Pi 包未导出 SessionManager.list');
  const found = await withProviderTimeout('Pi', pi.SessionManager.list(cwd, process.env.PI_CODING_AGENT_SESSION_DIR));
  if (!Array.isArray(found)) throw new Error('Pi SessionManager.list 返回值不是数组');
  return found
    .filter((session) => typeof session.cwd === 'string' && resolve(session.cwd) === cwd)
    .map((session) => toSession('pi', session.id, session.name || session.firstMessage, session.modified, session.cwd, session.path));
}

function withProviderTimeout(provider, operation) {
  return new Promise((resolvePromise, rejectPromise) => {
    const timer = setTimeout(() => {
      process.stderr.write(`${provider} 会话列表读取超时（30 秒）\n`);
      process.exit(1);
    }, 30_000);
    Promise.resolve(operation).then(
      (value) => { clearTimeout(timer); resolvePromise(value); },
      (error) => { clearTimeout(timer); rejectPromise(error); },
    );
  });
}

async function listCodex(executable, cwd) {
  const child = spawn(executable, ['app-server', '--listen', 'stdio://'], {
    env: process.env,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  const lines = readline.createInterface({ input: child.stdout });
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', (chunk) => { stderr += chunk; });
  const pending = new Map();
  let nextId = 1;
  let ended = false;
  let exitError;

  lines.on('line', (line) => {
    let message;
    try { message = JSON.parse(line); } catch { return; }
    if (!Object.hasOwn(message, 'id')) return;
    const resolvePending = pending.get(message.id);
    if (!resolvePending) return;
    pending.delete(message.id);
    resolvePending.resolve(message);
  });
  child.on('error', (error) => { exitError = error; ended = true; rejectPending(error); });
  child.on('close', (code) => {
    if (code !== 0) exitError ||= new Error(`Codex app-server 退出码 ${code}${stderr.trim() ? `: ${stderr.trim()}` : ''}`);
    ended = true;
    rejectPending(exitError || new Error('Codex app-server 已关闭'));
  });

  function rejectPending(error) {
    for (const [id, handlers] of pending) {
      pending.delete(id);
      handlers.reject(error);
    }
  }

  function sendNotification(method, params = {}) {
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  function request(method, params) {
    const id = nextId++;
    const promise = new Promise((resolvePromise, rejectPromise) => pending.set(id, { resolve: resolvePromise, reject: rejectPromise }));
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    return promise;
  }

  const timeout = setTimeout(() => {
    exitError = new Error('等待 Codex app-server 响应超时');
    child.kill('SIGTERM');
    ended = true;
    rejectPending(exitError);
  }, 30_000);

  try {
    const initialized = await request('initialize', {
      clientInfo: { name: 'PersonalWorkbench', version: '0.1.0' },
    });
    if (initialized.error) throw new Error(initialized.error.message || JSON.stringify(initialized.error));
    sendNotification('initialized');

    const all = [];
    let cursor;
    do {
      const response = await request('thread/list', {
        cwd,
        sourceKinds: ['cli', 'vscode', 'exec', 'appServer'],
        useStateDbOnly: true,
        limit: 100,
        sortKey: 'updated_at',
        sortDirection: 'desc',
        ...(cursor ? { cursor } : {}),
      });
      if (response.error) throw new Error(response.error.message || JSON.stringify(response.error));
      const data = response.result?.data;
      const items = Array.isArray(data) ? data : data?.items;
      if (!Array.isArray(items)) throw new Error('Codex thread/list 返回值缺少 data.items');
      all.push(...items);
      cursor = response.result?.nextCursor || null;
    } while (cursor);

    return all
      .filter((thread) => typeof thread.cwd === 'string' && resolve(thread.cwd) === cwd)
      .map((thread) => toSession('codex', thread.id, thread.name || thread.preview, thread.updatedAt * 1000, thread.cwd));
  } finally {
    clearTimeout(timeout);
    child.stdin.end();
    if (!ended) child.kill('SIGTERM');
    lines.close();
  }
}
