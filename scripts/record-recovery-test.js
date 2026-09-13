#!/usr/bin/env node
// Record without QuickTime/Screenshot UI. The supervisor intentionally stays
// alive until the fixed-duration recording finalizes; early SIGINT loses the
// MOV on current macOS. No audio.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, execFileSync } = require('node:child_process');
const { brotliCompressSync, constants } = require('node:zlib');

const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
function isRecorder(meta) {
  try {
    const args = execFileSync('/bin/ps', ['-p', String(meta.pid), '-o', 'args='], {
      encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
    return args.startsWith('/usr/sbin/screencapture ') && args.endsWith(meta.movie);
  } catch { return false; }
}

async function main() {
  const [action, value, display = '1'] = process.argv.slice(2);
  if (action === 'start') {
    const seconds = Number(value ?? '180');
    if (!Number.isInteger(seconds) || seconds < 3 || seconds > 600 || !/^[1-9]$/.test(display)) {
      throw new Error('Usage: start [seconds: 3..600] [display: 1..9]');
    }
    const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'codex-recovery-video-')));
    const movie = path.join(directory, 'recovery.mov');
    const log = path.join(directory, 'recorder.log');
    const fd = fs.openSync(log, 'wx', 0o600);
    // Retain the capture permission of the invoking app. A new LaunchAgent
    // has a different permission context and cannot record on this machine.
    const recorder = spawn('/usr/sbin/screencapture', ['-v', '-V', String(seconds), '-D', display, movie], {
      // A Codex self-restart terminates the invoking UI and its tool session.
      // Put screencapture in its own process group so it can still finalize the
      // fixed-duration MOV after the supervisor disappears.
      detached: true, stdio: ['ignore', fd, fd],
    });
    fs.closeSync(fd);
    await new Promise((resolve, reject) => {
      recorder.once('spawn', resolve);
      recorder.once('error', reject);
    });
    const meta = { pid: recorder.pid, movie, log, seconds, display, startedAt: new Date().toISOString() };
    fs.writeFileSync(path.join(directory, 'recording.json'), JSON.stringify(meta), { mode: 0o600, flag: 'wx' });
    await pause(1000);
    if (!isRecorder(meta)) throw new Error('Recorder did not stay running; inspect ' + log);
    console.log(JSON.stringify({ event: 'started', directory, ...meta }));
    const result = await new Promise((resolve, reject) => {
      recorder.once('error', reject);
      recorder.once('exit', (code, signal) => resolve({ code, signal }));
    });
    if (result.code !== 0) {
      throw new Error(`Recorder exited before finalization (code=${result.code}, signal=${result.signal})`);
    }
    const bytes = fs.existsSync(movie) ? fs.statSync(movie).size : 0;
    if (bytes === 0) throw new Error('Recorder exited without a movie');
    fs.chmodSync(movie, 0o600);
    console.log(JSON.stringify({ event: 'completed', directory, movie, bytes }));
    return;
  }
  if (!['status', 'stop'].includes(action) || !value) {
    throw new Error('Usage: start [seconds] [display] | status <directory> | stop <directory>');
  }
  const directory = fs.realpathSync(value);
  const meta = JSON.parse(fs.readFileSync(path.join(directory, 'recording.json'), 'utf8'));
  if (!Number.isInteger(meta.pid) || meta.pid <= 1
      || meta.movie !== path.join(directory, 'recovery.mov')
      || meta.log !== path.join(directory, 'recorder.log')) {
    throw new Error('Invalid recorder metadata');
  }
  if (action === 'stop') {
    if (isRecorder(meta)) {
      throw new Error('Recorder is still running; wait for its fixed duration so the MOV finalizes safely');
    }
    if (fs.existsSync(meta.movie)) fs.chmodSync(meta.movie, 0o600);
    if (fs.existsSync(meta.log)) {
      fs.writeFileSync(meta.log + '.br', brotliCompressSync(fs.readFileSync(meta.log), {
        params: { [constants.BROTLI_PARAM_QUALITY]: 6 },
      }), { mode: 0o600, flag: 'wx' });
      fs.unlinkSync(meta.log);
    }
  }
  const bytes = fs.existsSync(meta.movie) ? fs.statSync(meta.movie).size : 0;
  console.log(JSON.stringify({ directory, movie: meta.movie, bytes, running: isRecorder(meta) }));
  if (action === 'stop' && bytes === 0) throw new Error('No movie saved; this recording is not evidence');
}

main().catch(error => { console.error(error.message); process.exitCode = 1; });
