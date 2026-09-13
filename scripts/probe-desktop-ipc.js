#!/usr/bin/env node
// Read-only Desktop IPC probe. It initializes a temporary client and asks the
// router who owns one thread; it never sends a follower command.
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const { randomUUID } = require('node:crypto');

const threadId = process.argv[2];
if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(threadId ?? '')) {
  throw new Error('Usage: probe-desktop-ipc.js <thread-id>');
}

const socketPath = path.join(os.homedir(), '.codex', 'ipc', 'ipc.sock');
const stat = fs.lstatSync(socketPath);
if (!stat.isSocket() || stat.uid !== process.getuid() || (stat.mode & 0o777) !== 0o600) {
  throw new Error('Unsafe Desktop IPC socket');
}

const socket = net.createConnection(socketPath);
let buffered = Buffer.alloc(0);
const pending = new Map();

function fail(error) {
  for (const { reject, timer } of pending.values()) {
    clearTimeout(timer);
    reject(error);
  }
  pending.clear();
}

socket.on('data', chunk => {
  buffered = Buffer.concat([buffered, chunk]);
  while (buffered.length >= 4) {
    const length = buffered.readUInt32LE(0);
    if (length === 0 || length > 2 * 1024 * 1024) {
      fail(new Error('Invalid Desktop IPC frame length'));
      socket.destroy();
      return;
    }
    if (buffered.length < 4 + length) return;
    const payload = buffered.subarray(4, 4 + length);
    buffered = buffered.subarray(4 + length);
    let message;
    try { message = JSON.parse(payload.toString('utf8')); }
    catch {
      fail(new Error('Invalid Desktop IPC JSON'));
      socket.destroy();
      return;
    }
    const waiter = pending.get(message.requestId);
    if (message.type === 'response' && waiter) {
      pending.delete(message.requestId);
      clearTimeout(waiter.timer);
      waiter.resolve(message);
    }
  }
});
socket.on('error', fail);
socket.on('close', () => fail(new Error('Desktop IPC closed')));

function request(method, version, params, sourceClientId, timeoutMs) {
  const requestId = randomUUID();
  const message = {
    type: 'request', requestId, sourceClientId, version, method, params, timeoutMs,
  };
  const json = Buffer.from(JSON.stringify(message));
  const frame = Buffer.allocUnsafe(4 + json.length);
  frame.writeUInt32LE(json.length, 0);
  json.copy(frame, 4);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(requestId);
      reject(new Error(`${method} timed out`));
    }, timeoutMs + 2000);
    pending.set(requestId, { resolve, reject, timer });
    socket.write(frame);
  });
}

async function main() {
  await new Promise((resolve, reject) => {
    socket.once('connect', resolve);
    socket.once('error', reject);
  });
  const initialized = await request(
    'initialize', 0, { clientType: 'codex-monitor-probe' }, 'initializing-client', 5000,
  );
  if (initialized.resultType !== 'success' || !initialized.result?.clientId) {
    throw new Error('Desktop IPC initialization was not acknowledged');
  }
  const owner = await request(
    'thread-owner-discovery', 1, { hostId: 'local', conversationId: threadId },
    initialized.result.clientId, 12000,
  );
  console.log(JSON.stringify({
    resultType: owner.resultType,
    method: owner.method ?? null,
    error: owner.error ?? null,
    ownerConfirmed: Boolean(owner.handledByClientId),
  }));
  if (!['success', 'error'].includes(owner.resultType)) process.exitCode = 1;
  socket.end();
}

main().catch(error => {
  console.error(error.message);
  socket.destroy();
  process.exitCode = 1;
});
