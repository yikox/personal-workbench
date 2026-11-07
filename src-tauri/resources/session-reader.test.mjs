import assert from 'node:assert/strict';
import { chmod, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

test('Codex thread/list includes exec sources, avoids rollout repair, and filters by cwd', async () => {
  const root = await mkdtemp(join(tmpdir(), 'pw-session-reader-contract-'));
  try {
    const cwd = join(root, 'project');
    const otherCwd = join(root, 'other-project');
    const paramsPath = join(root, 'thread-list-params.json');
    const mockCodex = join(root, 'mock-codex');
    await writeFile(mockCodex, `#!${process.execPath}
import readline from 'node:readline';
import { writeFileSync } from 'node:fs';
const lines = readline.createInterface({ input: process.stdin });
lines.on('line', (line) => {
  const request = JSON.parse(line);
  if (request.method === 'initialize') {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: {} }) + '\\n');
  } else if (request.method === 'thread/list') {
    writeFileSync(process.env.MOCK_PARAMS_PATH, JSON.stringify(request.params));
    process.stdout.write(JSON.stringify({
      jsonrpc: '2.0', id: request.id,
      result: { data: [
        { id: 'in-cwd', name: 'matching session', updatedAt: 1, cwd: request.params.cwd },
        { id: 'outside-cwd', name: 'unrelated session', updatedAt: 1, cwd: ${JSON.stringify(otherCwd)} },
      ] },
    }) + '\\n');
  }
});
`);
    await chmod(mockCodex, 0o700);

    const reader = join(import.meta.dirname, 'session-reader.mjs');
    const result = spawnSync(process.execPath, [reader, 'codex', cwd, import.meta.dirname, mockCodex], {
      encoding: 'utf8',
      env: { ...process.env, MOCK_PARAMS_PATH: paramsPath },
      timeout: 5_000,
    });
    assert.equal(result.status, 0, result.stderr);

    const params = JSON.parse(await readFile(paramsPath, 'utf8'));
    assert.equal(params.cwd, cwd);
    assert.deepEqual(params.sourceKinds, ['cli', 'vscode', 'exec', 'appServer']);
    assert.equal(params.useStateDbOnly, true);

    const sessions = JSON.parse(result.stdout);
    assert.equal(sessions.length, 1);
    assert.equal(sessions[0].id, 'in-cwd');
    assert.equal(sessions[0].cwd, cwd);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
