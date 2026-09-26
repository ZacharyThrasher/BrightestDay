/**
 * "Play online" lobby panel inside the main menu card. Host (Cloudflare quick tunnel or LAN), Join by
 * room code / address, Cancel. For now a connection only exchanges greetings and shows the ping;
 * the game sync hooks into `net` (src/net/client.ts) later.
 * Uses the existing menu classes (.tag, .logo, .scenes, .settings, .foot, .menu button) — no new styles.
 */
import { net, type PeerEvent, type NetStatus } from '../net/client.js';

interface Toaster { toast(kind: 'good' | 'warn' | 'bad' | 'info' | 'note', text: string, ms?: number): void }

const h = <K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, parent: HTMLElement, text?: string) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  parent.appendChild(e);
  return e;
};

export function installLobby(card: HTMLElement, hud: Toaster) {
  if (!net.available) return; // plain web build: no main process, no online play
  const playerName = (() => {
    const s = (window as any).hardlight?.settings?.playerName;
    return typeof s === 'string' && s.trim() ? s.trim().slice(0, 24) : `Lantern-${Math.floor(1000 + Math.random() * 9000)}`;
  })();

  const openBtn = document.createElement('button');
  openBtn.className = 'online';
  openBtn.textContent = 'Play online';
  const play = card.querySelector('button.play');
  if (play) play.after(openBtn); else card.appendChild(openBtn);

  const root = h('div', 'lobby hidden', card);
  root.style.cssText = 'display:flex;flex-direction:column;align-items:center;gap:12px;min-width:420px';
  h('div', 'logo', root).innerHTML = 'ONLINE <span>DUEL</span>';
  h('div', 'tag', root, 'play online · 1v1');
  const actions = h('div', 'scenes', root);
  const hostBtn = h('button', 'host', actions, 'Host match');
  const lanBtn = h('button', 'host-lan', actions, 'Host on LAN');
  const joinToggle = h('button', 'join-open', actions, 'Join match');

  // host view: big room code + copy
  const hostView = h('div', 'lobby-host hidden', root);
  hostView.style.cssText = 'display:flex;flex-direction:column;align-items:center;gap:8px';
  const codeTag = h('div', 'tag', hostView, 'room code');
  const codeEl = h('div', 'logo code', hostView, '…');
  codeEl.style.cssText = 'font-size:26px;letter-spacing:0.06em;font-style:normal;user-select:text;text-align:center;word-break:break-all';
  const addrEl = h('div', 'foot addr', hostView, '');
  const copyBtn = h('button', 'copy', hostView, 'Copy');

  // join view: code field + Join
  const joinView = h('div', 'settings lobby-join hidden', root);
  joinView.style.gridTemplateColumns = '1fr auto';
  const lbl = h('label', '', joinView, 'Room code ');
  const codeIn = h('input', '', lbl);
  codeIn.name = 'code';
  codeIn.placeholder = 'apple-banana-cherry-delta';
  codeIn.spellcheck = false;
  codeIn.autocomplete = 'off';
  codeIn.style.cssText = 'width:240px;background:rgba(255,255,255,0.08);color:#fff;border:1px solid rgba(255,255,255,0.2);border-radius:6px;padding:5px 8px;font-family:var(--mono);user-select:text';
  const joinBtn = h('button', 'join', joinView, 'Join');

  const statusEl = h('div', 'foot net-status', root, 'Host a match and send your friend the room code, or join theirs.');
  statusEl.style.cssText = 'max-width:460px;text-align:center;line-height:1.5';
  const cancelBtn = h('button', 'cancel', root, 'Cancel');

  let hiddenBefore: Element[] = [];
  let opponent: string | null = null;
  let state: 'idle' | 'busy' | 'waiting' | 'connected' | 'failed' = 'idle';

  const setStatus = (text: string, kind: 'dim' | 'good' | 'bad' = 'dim') => {
    statusEl.textContent = text;
    statusEl.style.color = kind === 'good' ? 'var(--good)' : kind === 'bad' ? 'var(--bad)' : '';
  };
  const setState = (s: typeof state) => { state = s; root.dataset.state = s; };
  setState('idle');

  const open = () => {
    hiddenBefore = [...card.children].filter((c) => c !== root && !c.classList.contains('hidden'));
    hiddenBefore.forEach((c) => c.classList.add('hidden'));
    root.classList.remove('hidden');
  };
  const close = async () => {
    await net.stop().catch(() => {});
    reset();
    root.classList.add('hidden');
    hiddenBefore.forEach((c) => c.classList.remove('hidden'));
  };
  const reset = () => {
    setState('idle'); opponent = null;
    hostView.classList.add('hidden'); joinView.classList.add('hidden');
    actions.classList.remove('hidden');
    setStatus('Host a match and send your friend the room code, or join theirs.');
  };

  const host = async (tunnel: boolean) => {
    actions.classList.add('hidden');
    hostView.classList.remove('hidden');
    codeEl.textContent = '…'; addrEl.textContent = '';
    codeTag.textContent = tunnel ? 'room code' : 'join address';
    setState('busy');
    setStatus(tunnel ? 'starting tunnel…' : 'opening server…');
    try {
      const info = await net.host({ tunnel, lan: !tunnel, name: playerName, maxPeers: 1 });
      codeEl.textContent = info.code ?? info.addresses[info.addresses.length - 1] ?? `ws://127.0.0.1:${info.port}`;
      addrEl.textContent = info.code ? '' : info.addresses.join(' · ');
      root.dataset.address = info.addresses[0] ?? '';
      if (state === 'busy') { setState('waiting'); setStatus('ready — waiting for opponent…'); }
    } catch (err) {
      setState('failed');
      setStatus(`${(err as Error).message}`, 'bad');
      hostView.classList.add('hidden'); actions.classList.remove('hidden'); // let them retry / host on LAN
    }
  };

  const join = async () => {
    const target = codeIn.value.trim();
    if (!target) { codeIn.focus(); return; }
    setState('busy');
    setStatus('connecting…');
    try {
      const j = await net.join(target, playerName);
      opponent = j.hostName;
      setState('connected');
      renderConnected();
    } catch (err) {
      setState('failed');
      setStatus((err as Error).message, 'bad');
    }
  };

  const renderConnected = () => {
    if (state !== 'connected') return;
    const ping = Math.round(net.stats().rtt || net.stats().peers[0]?.rtt || 0);
    setStatus(`connected${opponent ? ` to ${opponent}` : ''} — ping ${ping} ms`, 'good');
  };

  net.onStatus((s: NetStatus) => {
    if (root.classList.contains('hidden')) return;
    if (s.role === 'host' && s.code && codeEl.textContent === '…') codeEl.textContent = s.code;
    if (s.state === 'tunnel-starting' && state === 'busy') setStatus(s.detail ?? 'starting tunnel…');
    if (s.state === 'reconnecting') setStatus(s.detail ?? 'reconnecting…', 'bad');
    if (s.state === 'failed' && state !== 'idle') { setState('failed'); setStatus(s.detail ?? 'connection failed', 'bad'); }
    if (s.state === 'closed' && state === 'connected') { setState('failed'); setStatus(s.detail ?? 'disconnected', 'bad'); }
  });
  net.onPeer((e: PeerEvent) => {
    if (e.kind === 'join' || e.kind === 'resume') {
      opponent = e.name;
      setState('connected');
      renderConnected();
      net.send('all', { t: 'greet', name: playerName });
      hud.toast('good', `${e.name} connected`, 3000);
    } else if (e.kind === 'lost') {
      setStatus(`${e.name} lost connection — waiting for them to come back…`, 'bad');
    } else if (e.kind === 'leave') {
      if (net.role === 'host') { setState('waiting'); setStatus(`${e.name} left — waiting for opponent…`); }
    }
  });
  net.onMessage((_from, msg) => {
    if (msg.t === 'greet' && typeof msg.name === 'string') { opponent = msg.name.slice(0, 24); root.dataset.greeted = opponent; renderConnected(); }
  });
  net.onStats(() => renderConnected());

  // clicks inside the lobby must not fall through to the menu's "click to capture the mouse"
  root.addEventListener('click', (e) => e.stopPropagation());
  openBtn.addEventListener('click', (e) => { e.stopPropagation(); open(); });
  hostBtn.onclick = () => void host(true);
  lanBtn.onclick = () => void host(false);
  joinToggle.onclick = () => { actions.classList.add('hidden'); joinView.classList.remove('hidden'); codeIn.focus(); };
  joinBtn.onclick = () => void join();
  codeIn.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') void join(); if (e.key === 'Escape') void close(); });
  codeIn.addEventListener('keyup', (e) => e.stopPropagation());
  copyBtn.onclick = () => {
    const text = codeEl.textContent ?? '';
    navigator.clipboard?.writeText(text).then(() => hud.toast('good', 'Room code copied', 2000), () => { codeIn.value = text; hud.toast('warn', 'Copy failed — select the code and press Ctrl+C', 3000); });
  };
  cancelBtn.onclick = () => void close();
}
