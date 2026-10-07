import { debug, media, ready, storage } from '/__sdk/index.js';

const track = document.getElementById('track');
const counter = document.getElementById('counter');
const checks = document.getElementById('checks');

function showTrack(nowPlaying) {
  track.textContent = nowPlaying ? `${nowPlaying.title} — ${nowPlaying.artist}` : 'Nothing playing';
}

/** Things a widget must not be able to do. Each entry should end up "blocked". */
async function sandboxChecks() {
  const results = {};
  const attempt = async (name, action) => {
    try {
      await action();
      results[name] = 'allowed';
    } catch (error) {
      results[name] = `blocked (${error?.code ?? error?.name ?? 'error'})`;
    }
  };
  await attempt('island page', () => window.parent.document.title);
  await attempt('tauri ipc', async () => {
    const ipc = window.__TAURI_INTERNALS__;
    if (!ipc) throw Object.assign(new Error('not injected'), { name: 'NotInjected' });
    // Tauri injects its IPC object into frames too, but never answers calls from a sandboxed one.
    const noAnswer = new Promise((_, reject) =>
      setTimeout(() => reject(Object.assign(new Error('no answer'), { name: 'NoAnswer' })), 1000),
    );
    await Promise.race([ipc.invoke('media_snapshot'), noAnswer]);
  });
  await attempt('network', () => fetch('https://example.com', { mode: 'no-cors' }));
  await attempt('localStorage', () => localStorage.setItem('probe', '1'));
  await attempt('media control', () => media.togglePlayPause());
  return results;
}

const context = await ready();

let clicks = (await storage.get('clicks')) ?? 0;
counter.textContent = `Clicked ${clicks}×`;
counter.addEventListener('click', async () => {
  clicks += 1;
  counter.textContent = `Clicked ${clicks}×`;
  await storage.set('clicks', clicks);
});

showTrack(await media.getNowPlaying());
media.subscribe(showTrack);

const results = await sandboxChecks();
for (const [name, result] of Object.entries(results)) {
  const item = document.createElement('li');
  item.className = result === 'allowed' ? 'bad' : 'ok';
  item.textContent = `${name}: ${result}`;
  checks.append(item);
}
await debug.report({ widgetId: context.widgetId, checks: results });
