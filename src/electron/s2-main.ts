/**
 * System-2 bridge (Electron main process): the renderer never sees API keys. It sends the prompt over IPC,
 * the main process calls the configured provider (Anthropic API or a local OpenAI-compatible server).
 */
import { ipcMain } from 'electron';
import { AnthropicProvider, OpenAICompatProvider, type S2Provider } from '../lang/escalate.js';

export interface S2Settings {
  provider?: 'off' | 'anthropic' | 'openai-compatible';
  anthropicApiKey?: string;
  anthropicModel?: string;
  openaiBaseUrl?: string;
  openaiModel?: string;
  openaiApiKey?: string;
  timeoutMs?: number;
}

export function makeProvider(s: S2Settings | undefined): S2Provider | null {
  const key = s?.anthropicApiKey || process.env.ANTHROPIC_API_KEY || '';
  const kind = s?.provider ?? (key ? 'anthropic' : 'off');
  if (kind === 'anthropic' && key) return new AnthropicProvider(key, s?.anthropicModel || 'claude-haiku-4-5-20251001');
  if (kind === 'openai-compatible') return new OpenAICompatProvider(s?.openaiBaseUrl || 'http://127.0.0.1:11434/v1', s?.openaiModel || 'qwen2.5:7b-instruct', s?.openaiApiKey || '');
  return null;
}

export function installS2Bridge(getSettings: () => S2Settings | undefined) {
  let provider: S2Provider | null = makeProvider(getSettings());
  ipcMain.handle('hl:s2-info', () => ({ enabled: !!provider, name: provider?.name ?? 'off' }));
  ipcMain.handle('hl:s2-complete', async (_e, req: { key: string; system: string; user: string }, timeoutMs: number) => {
    if (!provider) return { ok: false, error: 'System 2 is off' };
    const t0 = Date.now();
    try {
      const text = await provider.complete(req, Math.min(15000, timeoutMs || getSettings()?.timeoutMs || 6000));
      return { ok: true, text, ms: Date.now() - t0 };
    } catch (e) {
      return { ok: false, error: String((e as Error)?.message ?? e).slice(0, 300), ms: Date.now() - t0 };
    }
  });
  return { reload() { provider = makeProvider(getSettings()); } };
}
