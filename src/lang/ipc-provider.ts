/** Renderer-side System-2 provider: forwards to the Electron main process (which holds any API key). */
import type { S2Provider, S2Request } from './escalate.js';

export interface S2Bridge {
  info(): Promise<{ enabled: boolean; name: string }>;
  complete(req: S2Request, timeoutMs: number): Promise<{ ok: boolean; text?: string | null; error?: string; ms?: number }>;
}

export class IpcProvider implements S2Provider {
  name = 'ipc';
  lastMs = 0;
  lastError: string | null = null;
  constructor(private bridge: S2Bridge) {}
  async complete(req: S2Request, timeoutMs: number): Promise<string | null> {
    const r = await this.bridge.complete(req, timeoutMs);
    this.lastMs = r.ms ?? 0;
    if (!r.ok) { this.lastError = r.error ?? 'failed'; throw new Error(this.lastError); }
    this.lastError = null;
    return r.text ?? null;
  }
}
