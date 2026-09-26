/**
 * Cloudflare quick tunnel (Node only, no Electron import): locate the cloudflared binary, run
 * `cloudflared tunnel --no-autoupdate --url http://127.0.0.1:<port>`, parse the public
 * https://<words>.trycloudflare.com URL from its log output and report readiness / failure.
 */
import { spawn, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import { delimiter, join } from 'node:path';
import { findQuickTunnelUrl, roomCodeFromUrl } from '../net/protocol.js';

const exeName = process.platform === 'win32' ? 'cloudflared.exe' : 'cloudflared';

/**
 * Search order: $HARDLIGHT_CLOUDFLARED, <resources>/cloudflared/cloudflared(.exe) (packaged build),
 * PATH, then `extra` (dev paths such as release/cache/…).
 */
export function findCloudflared(opts: { resourcesPath?: string; extra?: string[]; env?: NodeJS.ProcessEnv } = {}): string | null {
  const env = opts.env ?? process.env;
  const candidates: string[] = [];
  if (env.HARDLIGHT_CLOUDFLARED) candidates.push(env.HARDLIGHT_CLOUDFLARED);
  if (opts.resourcesPath) candidates.push(join(opts.resourcesPath, 'cloudflared', exeName));
  for (const dir of (env.PATH ?? env.Path ?? '').split(delimiter)) if (dir) candidates.push(join(dir, exeName));
  candidates.push(...(opts.extra ?? []));
  return candidates.find((p) => { try { return existsSync(p); } catch { return false; } }) ?? null;
}

export interface TunnelInfo { url: string; code: string }

export interface QuickTunnel {
  /** Resolves once the URL is known AND cloudflared registered an edge connection (or a grace period passed). */
  readonly ready: Promise<TunnelInfo>;
  /** Last ~60 log lines (for error reports). */
  readonly log: string[];
  readonly url: string | null;
  /** Called if the process exits after being ready (tunnel lost). */
  onExit(cb: (reason: string) => void): void;
  stop(): void;
}

export interface QuickTunnelOptions {
  timeoutMs?: number;
  /** After the URL appears, wait at most this long for "Registered tunnel connection". */
  registerGraceMs?: number;
  /** Progress callback: 'url' when the address is known (edge not registered yet). */
  onProgress?: (stage: 'spawned' | 'url', info?: TunnelInfo) => void;
  args?: string[];
}

/** Pull a useful one-line failure reason out of cloudflared's log. */
export function tunnelFailureReason(log: string[], fallback: string): string {
  const interesting = log.filter((l) => /\b(ERR|error|failed|refused|denied|timeout|status \d{3})\b/i.test(l));
  const line = (interesting[interesting.length - 1] ?? '').replace(/^\S+Z\s+(ERR|INF|WRN)\s+/, '').trim();
  return line ? line.slice(0, 240) : fallback;
}

export function startQuickTunnel(bin: string, port: number, opts: QuickTunnelOptions = {}): QuickTunnel {
  const timeoutMs = opts.timeoutMs ?? 45000;
  const graceMs = opts.registerGraceMs ?? 8000;
  const log: string[] = [];
  let url: string | null = null;
  let settled = false;
  let stopped = false;
  const exitCbs: ((reason: string) => void)[] = [];
  let child: ChildProcess;
  let resolveReady!: (i: TunnelInfo) => void, rejectReady!: (e: Error) => void;
  const ready = new Promise<TunnelInfo>((res, rej) => { resolveReady = res; rejectReady = rej; });
  ready.catch(() => {}); // callers may attach later
  let graceTimer: NodeJS.Timeout | undefined;
  const timeout = setTimeout(() => fail(`cloudflared did not come up within ${Math.round(timeoutMs / 1000)} s: ${tunnelFailureReason(log, 'no tunnel URL')}`), timeoutMs);

  const succeed = () => {
    if (settled || !url) return;
    settled = true;
    clearTimeout(timeout); clearTimeout(graceTimer);
    resolveReady({ url, code: roomCodeFromUrl(url) ?? url });
  };
  function fail(reason: string) {
    if (settled) return;
    settled = true;
    clearTimeout(timeout); clearTimeout(graceTimer);
    try { child?.kill(); } catch { /* gone */ }
    rejectReady(new Error(reason));
  }

  try {
    child = spawn(bin, opts.args ?? ['tunnel', '--no-autoupdate', '--url', `http://127.0.0.1:${port}`], { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  } catch (e) {
    child = undefined as any;
    fail(`could not start cloudflared: ${(e as Error).message}`);
  }
  if (child) {
    opts.onProgress?.('spawned');
    let buf = '';
    const onData = (d: Buffer) => {
      buf += d.toString('utf8');
      let i: number;
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i).replace(/\r$/, '');
        buf = buf.slice(i + 1);
        if (!line.trim()) continue;
        log.push(line);
        if (log.length > 60) log.shift();
        if (!url) {
          const u = findQuickTunnelUrl(line);
          if (u) {
            url = u;
            opts.onProgress?.('url', { url, code: roomCodeFromUrl(url) ?? url });
            graceTimer = setTimeout(succeed, graceMs);
          }
        } else if (/Registered tunnel connection/i.test(line)) succeed();
      }
      if (buf.length > 65536) buf = buf.slice(-4096);
    };
    child.stdout?.on('data', onData);
    child.stderr?.on('data', onData);
    child.on('error', (e) => fail(`could not start cloudflared: ${e.message}`));
    child.on('exit', (code, sig) => {
      const reason = tunnelFailureReason(log, `cloudflared exited (${code ?? sig})`);
      if (!settled) fail(reason);
      else if (!stopped) exitCbs.forEach((cb) => cb(reason));
    });
  }

  return {
    ready,
    log,
    get url() { return url; },
    onExit(cb) { exitCbs.push(cb); },
    stop() {
      stopped = true;
      fail('stopped');
      try { if (child && child.exitCode === null) child.kill(); } catch { /* gone */ }
    },
  };
}
