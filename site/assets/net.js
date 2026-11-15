// Fedi3 site: the network diagram.
//
// It is a map, not a counter: the device with the local core at the centre,
// the public relays from the project directory, peers and ActivityPub
// instances. Packets travel along the edges to show that delivery has more
// than one route. The animation stops when the canvas leaves the screen, when
// the tab is hidden, and under prefers-reduced-motion it is drawn once.

(() => {
  const canvas = document.getElementById('fedi3-net');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const stage = canvas.parentElement;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');

  const instances = [
    { label: 'Mastodon', kind: 'instance', angle: -2.3 },
    { label: 'Misskey', kind: 'instance', angle: -0.55 },
    { label: 'PeerTube', kind: 'instance', angle: 2.45 },
  ];

  let relays = [
    { label: 'relay 1', kind: 'relay', angle: -1.35 },
    { label: 'relay 2', kind: 'relay', angle: 1.2 },
  ];
  let peers = [
    { label: 'peer', kind: 'peer', angle: 0.35 },
    { label: 'peer', kind: 'peer', angle: 2.95 },
  ];

  // Real relay hosts from relays/relays.json when the file is reachable.
  fetch(canvas.dataset.relays || 'assets/relays.json')
    .then((r) => (r.ok ? r.json() : null))
    .then((data) => {
      const list = (data && data.relays) || [];
      if (list.length) {
        relays = list.slice(0, 4).map((rel, i, all) => ({
          label: rel.url.replace(/^https?:\/\//, ''),
          kind: 'relay',
          angle: -1.4 + (i * 2.8) / Math.max(1, all.length - 1),
        }));
      }
      schedule();
    })
    .catch(() => {});

  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  let w = 0;
  let h = 0;
  let raf = 0;
  let visible = true;
  let running = false;
  let t = 0;
  let paused = false;
  const pauseBtn = stage.querySelector('.net-pause');

  const COLORS = {
    core: '#c4a1ff',
    relay: '#7dd3fc',
    peer: '#34d399',
    instance: '#f59e0b',
    edge: 'rgba(155, 176, 200, 0.35)',
    edgeLit: 'rgba(196, 161, 255, 0.55)',
    text: '#9bb0c8',
  };

  function resize() {
    const box = stage.getBoundingClientRect();
    w = Math.max(320, Math.floor(box.width));
    h = Math.max(280, Math.floor(box.height));
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    canvas.style.width = w + 'px';
    canvas.style.height = h + 'px';
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function nodePos(node, radiusX, radiusY) {
    return {
      x: w / 2 + Math.cos(node.angle) * radiusX,
      y: h / 2 + Math.sin(node.angle) * radiusY * 0.72,
    };
  }

  function drawStatic() {
    ctx.clearRect(0, 0, w, h);
    const cx = w / 2;
    const cy = h / 2;
    const rx = Math.min(w * 0.4, 420);
    const ry = Math.min(h * 0.42, 260);

    ctx.strokeStyle = 'rgba(35, 55, 77, 0.7)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.ellipse(cx, cy, rx, ry * 0.72, 0, 0, Math.PI * 2);
    ctx.stroke();

    const nodes = [...relays, ...peers, ...instances];
    for (const n of nodes) {
      const p = nodePos(n, rx, ry);
      const midX = (cx + p.x) / 2;
      const midY = (cy + p.y) / 2 - 18;
      ctx.strokeStyle = n.kind === 'relay' ? COLORS.edgeLit : COLORS.edge;
      ctx.lineWidth = n.kind === 'relay' ? 1.6 : 1;
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.quadraticCurveTo(midX, midY, p.x, p.y);
      ctx.stroke();
    }

    // core
    const coreR = 16 + Math.sin(t / 22) * 2;
    const glow = ctx.createRadialGradient(cx, cy, 2, cx, cy, coreR * 4);
    glow.addColorStop(0, 'rgba(196, 161, 255, 0.45)');
    glow.addColorStop(1, 'rgba(196, 161, 255, 0)');
    ctx.fillStyle = glow;
    ctx.beginPath();
    ctx.arc(cx, cy, coreR * 4, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = COLORS.core;
    ctx.beginPath();
    ctx.arc(cx, cy, 7, 0, Math.PI * 2);
    ctx.fill();

    ctx.font = '600 10px ui-monospace, Menlo, monospace';
    ctx.textAlign = 'center';
    ctx.fillStyle = COLORS.text;
    ctx.fillText('CORE LOCALE', cx, cy + 30);

    for (const n of nodes) {
      const p = nodePos(n, rx, ry);
      const color = COLORS[n.kind] || COLORS.text;
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.arc(p.x, p.y, 5, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = 'rgba(155, 176, 200, 0.85)';
      ctx.font = '500 9.5px ui-monospace, Menlo, monospace';
      const label = n.label.length > 26 ? n.label.slice(0, 25) + '…' : n.label;
      ctx.fillText(label, p.x, p.y + 16);
    }

    // packets
    if (!reduced.matches) {
      for (let i = 0; i < nodes.length; i++) {
        const n = nodes[i];
        const p = nodePos(n, rx, ry);
        const phase = (t / 90 + i * 0.37) % 1;
        const q = phase < 0.5 ? phase * 2 : (1 - phase) * 2;
        const midX = (cx + p.x) / 2;
        const midY = (cy + p.y) / 2 - 18;
        const x = (1 - q) * (1 - q) * cx + 2 * (1 - q) * q * midX + q * q * p.x;
        const y = (1 - q) * (1 - q) * cy + 2 * (1 - q) * q * midY + q * q * p.y;
        ctx.fillStyle = n.kind === 'relay' ? COLORS.relay : COLORS.peer;
        ctx.globalAlpha = phase < 0.5 ? 0.9 : 0.5;
        ctx.beginPath();
        ctx.arc(x, y, 2.4, 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 1;
      }
    }
  }

  function frame() {
    if (!running) return;
    t += 1;
    drawStatic();
    raf = requestAnimationFrame(frame);
  }

  function start() {
    if (paused) return;
    if (running || reduced.matches || !visible) return;
    running = true;
    raf = requestAnimationFrame(frame);
  }

  function stop() {
    running = false;
    cancelAnimationFrame(raf);
  }

  function schedule() {
    resize();
    drawStatic();
    if (reduced.matches || paused) return;
    start();
  }

  if (pauseBtn) {
    pauseBtn.addEventListener('click', () => {
      paused = !paused;
      pauseBtn.setAttribute('aria-pressed', String(paused));
      pauseBtn.textContent = paused ? 'Riprendi animazione' : 'Pausa animazione';
      if (paused) stop();
      else if (visible) start();
    });
  }

  const io = new IntersectionObserver((entries) => {
    visible = entries.some((e) => e.isIntersecting);
    if (visible) start();
    else stop();
  }, { threshold: 0.05 });
  io.observe(canvas);

  document.addEventListener('visibilitychange', () => {
    if (document.hidden) stop();
    else if (visible) start();
  });

  reduced.addEventListener('change', () => {
    if (reduced.matches) stop();
    else if (visible) start();
    drawStatic();
  });

  let resizeTimer = 0;
  window.addEventListener('resize', () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(schedule, 180);
  });

  schedule();
})();
