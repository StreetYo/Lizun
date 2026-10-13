// Plays the demo videos only while they are on screen, moves the island's eyes after the
// pointer, and on phones (or computers without Windows) offers to send the link to yourself.

const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

const videos = document.querySelectorAll('video[data-auto]');
if ('IntersectionObserver' in window && !reduceMotion) {
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      const v = e.target;
      if (e.isIntersecting) {
        if (v.preload === 'none') v.preload = 'auto';
        v.play().catch(() => {});
      } else {
        v.pause();
      }
    }
  }, { rootMargin: '120px 0px', threshold: 0.2 });
  videos.forEach((v) => io.observe(v));
} else {
  videos.forEach((v) => { v.controls = true; });
}

// Eyes look towards the pointer, and around now and then when there is no pointer.
const eyes = document.querySelectorAll('.pill .eye');
const pill = document.querySelector('.pill');
function look(dx, dy) {
  eyes.forEach((eye) => {
    eye.style.setProperty('--ex', `${dx}px`);
    eye.style.setProperty('--ey', `${dy}px`);
  });
}
if (pill && !reduceMotion) {
  let last = 0;
  addEventListener('pointermove', (e) => {
    last = Date.now();
    const r = pill.getBoundingClientRect();
    const x = e.clientX - (r.left + r.width / 2);
    const y = e.clientY - (r.top + r.height / 2);
    const len = Math.hypot(x, y) || 1;
    look((x / len) * 4, Math.max(-2, (y / len) * 3));
  }, { passive: true });
  const glances = [[-4, 1], [4, 1], [0, 2], [-3, -1], [3, 2], [0, 0]];
  let i = 0;
  setInterval(() => {
    if (Date.now() - last < 3000) return;
    const [dx, dy] = glances[i++ % glances.length];
    look(dx, dy);
  }, 1800);
}

// Phones and other systems: Lizun runs on a Windows PC, so offer to send the link over.
const ua = navigator.userAgent;
const isPhone = /Android|iPhone|iPad|iPod|Mobile/i.test(ua) ||
  (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
const isWindows = /Windows NT/.test(ua) && !isPhone;
const phone = document.getElementById('phone');
const other = document.getElementById('other');
if (isPhone && phone) phone.classList.add('on');
else if (!isWindows && other) other.classList.add('on');

const pageUrl = location.href.split('#')[0];
document.querySelectorAll('[data-share]').forEach((btn) => {
  if (!navigator.share) { btn.hidden = true; return; }
  btn.addEventListener('click', () => {
    navigator.share({ title: document.title, text: btn.dataset.share, url: pageUrl }).catch(() => {});
  });
});
document.querySelectorAll('[data-copy]').forEach((btn) => {
  btn.addEventListener('click', async () => {
    const done = btn.closest('.notice').querySelector('.done');
    try {
      await navigator.clipboard.writeText(pageUrl);
      done.textContent = btn.dataset.copy;
    } catch {
      done.textContent = pageUrl;
    }
  });
});
