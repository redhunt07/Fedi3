/*
 * Fedi3 relay landing page — live mesh view.
 *
 * Polls the public aggregate endpoint `/_fedi3/relay/live` and renders an
 * animated relay mesh plus counters. Everything shown is a count or a
 * process-wide byte counter: no usernames, no peer ids.
 *
 * Constraints:
 * - no external libraries and no inline handlers, so the restrictive CSP
 *   documented in `.env.example` keeps working (`script-src 'self'`);
 * - values are written with `textContent`, never `innerHTML`;
 * - the animation stops when the tab is hidden or the canvas is off-screen,
 *   and respects `prefers-reduced-motion`.
 */
(() => {
  'use strict';

  const panel = document.querySelector('.live-panel');
  if (!panel) return;

  const endpoint = panel.getAttribute('data-live-endpoint') || '/_fedi3/relay/live';
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const meshCanvas = document.getElementById('live-mesh');
  const sparkCanvas = document.getElementById('live-spark');
  const meshCtx = meshCanvas ? meshCanvas.getContext('2d') : null;
  const sparkCtx = sparkCanvas ? sparkCanvas.getContext('2d') : null;

  const ui = {
    status: document.getElementById('live-status'),
    updated: document.getElementById('live-updated'),
    usersOnline: document.getElementById('live-users-online'),
    usersTotal: document.getElementById('live-users-total'),
    peersOnline: document.getElementById('live-peers-online'),
    peersSeen: document.getElementById('live-peers-seen'),
    relaysOnline: document.getElementById('live-relays-online'),
    relaysOffline: document.getElementById('live-relays-offline'),
    tunnelRate: document.getElementById('live-tunnel-rate'),
    tunnelTotal: document.getElementById('live-tunnel-total'),
    fetchRate: document.getElementById('live-fetch-rate'),
    fetchTotal: document.getElementById('live-fetch-total'),
    uptime: document.getElementById('live-uptime'),
    meshInfo: document.getElementById('live-mesh-info'),
    sparkRelays: document.getElementById('live-spark-relays'),
    sparkPeers: document.getElementById('live-spark-peers'),
  };

  const POLL_MS = 12000;
  const HISTORY_MAX = 75;

  const state = {
    timer: null,
    snapshot: null,
    snapshotAt: 0,
    relays: [],
    relayHost: '',
    peersOnline: 0,
    peersOffline: 0,
    targetRates: { tunnel: 0, fetch: 0 },
    rates: { tunnel: 0, fetch: 0 },
    history: [],
    uptimeBase: 0,
    uptimeAt: 0,
    running: false,
    polling: false,
    frame: 0,
    lastRatePaint: 0,
    lastUptimePaint: 0,
  };

  // ---------------------------------------------------------------- formatting

  const int = (n) => {
    const v = Number(n);
    return Number.isFinite(v) ? Math.round(v).toLocaleString() : '—';
  };

  const compact = (n) => {
    const v = Number(n);
    if (!Number.isFinite(v)) return '—';
    if (v < 1000) return String(Math.round(v));
    if (v < 1e6) return (v / 1e3).toFixed(v < 1e4 ? 1 : 0) + 'k';
    if (v < 1e9) return (v / 1e6).toFixed(1) + 'M';
    return (v / 1e9).toFixed(1) + 'G';
  };

  const bytes = (n) => {
    const v = Number(n);
    if (!Number.isFinite(v) || v <= 0) return '0 B';
    const units = ['B', 'kB', 'MB', 'GB', 'TB'];
    let i = 0;
    let value = v;
    while (value >= 1024 && i < units.length - 1) {
      value /= 1024;
      i += 1;
    }
    return value.toFixed(i === 0 ? 0 : 1) + ' ' + units[i];
  };

  const rate = (n) => bytes(n) + '/s';

  const duration = (secs) => {
    const s = Math.max(0, Math.floor(Number(secs) || 0));
    const d = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    const m = Math.floor((s % 3600) / 60);
    if (d > 0) return d + 'd ' + h + 'h';
    if (h > 0) return h + 'h ' + m + 'm';
    return m + 'm';
  };

  const clock = (ms) => {
    try {
      return new Date(ms).toLocaleTimeString(undefined, { hour12: false });
    } catch (e) {
      return '—';
    }
  };

  const hostOf = (url) => {
    try {
      return new URL(url).host;
    } catch (e) {
      return String(url || '').replace(/^https?:\/\//, '').split('/')[0];
    }
  };

  const hash = (str) => {
    let h = 2166136261;
    for (let i = 0; i < str.length; i += 1) {
      h ^= str.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return (h >>> 0) / 4294967295;
  };

  // ---------------------------------------------------------------- data intake

  const setStatus = (kind) => {
    if (!ui.status) return;
    ui.status.dataset.state = kind;
    if (kind === 'live') {
      ui.status.textContent = panel.getAttribute('data-label-live') || 'live';
    } else if (kind === 'retry') {
      ui.status.textContent = panel.getAttribute('data-label-retry') || 'reconnecting';
    } else {
      ui.status.textContent = panel.getAttribute('data-label-wait') || 'connecting';
    }
  };

  const applySnapshot = (data) => {
    const now = Date.now();
    const traffic = data.traffic || {};
    const tunnelTotal = Number(traffic.tunnel_request_bytes || 0) + Number(traffic.tunnel_response_bytes || 0);
    const fetchTotal = Number(traffic.fetch_bytes || 0);

    // Rates come from counter deltas. A decrease means the relay restarted.
    if (state.snapshot && state.snapshotAt > 0) {
      const elapsed = (now - state.snapshotAt) / 1000;
      if (elapsed > 1) {
        const prev = state.snapshot;
        const prevTunnel =
          Number(prev.tunnelTotal || 0);
        const prevFetch = Number(prev.fetchTotal || 0);
        const dTunnel = tunnelTotal - prevTunnel;
        const dFetch = fetchTotal - prevFetch;
        state.targetRates.tunnel = dTunnel >= 0 ? dTunnel / elapsed : 0;
        state.targetRates.fetch = dFetch >= 0 ? dFetch / elapsed : 0;
      }
    } else {
      state.targetRates.tunnel = 0;
      state.targetRates.fetch = 0;
    }

    state.snapshot = { tunnelTotal, fetchTotal };
    state.snapshotAt = now;
    state.relayHost = hostOf(data.relay_url || '');
    state.relays = Array.isArray(data.relays && data.relays.items) ? data.relays.items : [];
    state.peersOnline = Number(data.peers && data.peers.online) || 0;
    const peersSeen = Number(data.peers && data.peers.seen_30d) || 0;
    state.peersOffline = Math.max(0, peersSeen - state.peersOnline);
    state.uptimeBase = Number(data.uptime_secs) || 0;
    state.uptimeAt = now;

    state.history.push({
      t: now,
      peers: state.peersOnline,
      relays: Number(data.relays && data.relays.online) || 0,
    });
    while (state.history.length > HISTORY_MAX) state.history.shift();

    if (ui.updated) ui.updated.textContent = clock(now);

    tween(ui.usersOnline, Number(data.users && data.users.online) || 0, int);
    if (ui.usersTotal) ui.usersTotal.textContent = int(data.users && data.users.total) + ' total';
    tween(ui.peersOnline, state.peersOnline, int);
    if (ui.peersSeen) ui.peersSeen.textContent = int(peersSeen) + ' seen in 30 days';
    tween(ui.relaysOnline, Number(data.relays && data.relays.online) || 0, int);
    if (ui.relaysOffline) ui.relaysOffline.textContent = int(data.relays && data.relays.offline) + ' offline';
    if (ui.meshInfo) {
      const mesh = data.mesh || {};
      ui.meshInfo.textContent = 'sync ' + int(mesh.sync_secs) + 's · hops ' + int(mesh.max_hops);
    }
    if (ui.tunnelTotal) ui.tunnelTotal.textContent = bytes(tunnelTotal) + ' total';
    if (ui.fetchTotal) ui.fetchTotal.textContent = bytes(fetchTotal) + ' total';

    if (reduceMotion) {
      paintRates(true);
      paintUptime(true);
      drawMesh(performance.now());
      drawSpark();
    } else {
      start();
    }

    if (meshCanvas) {
      meshCanvas.setAttribute(
        'aria-label',
        'Relay mesh: ' +
          int(data.relays && data.relays.online) +
          ' relays online, ' +
          int(data.relays && data.relays.offline) +
          ' offline, ' +
          int(state.peersOnline) +
          ' peers online.'
      );
    }
  };

  const poll = async () => {
    if (document.hidden || state.polling) return;
    state.polling = true;
    setStatus('wait');
    try {
      const res = await fetch(endpoint, {
        cache: 'no-store',
        credentials: 'omit',
        headers: { Accept: 'application/json' },
      });
      if (!res.ok) throw new Error('HTTP ' + res.status);
      const data = await res.json();
      applySnapshot(data);
      setStatus('live');
    } catch (err) {
      setStatus('retry');
    } finally {
      state.polling = false;
      clearTimeout(state.timer);
      state.timer = setTimeout(poll, POLL_MS + Math.floor(Math.random() * 2000));
    }
  };

  // ---------------------------------------------------------------- tweening

  const tweens = new Map();

  const tween = (el, target, formatter) => {
    if (!el) return;
    const to = Number(target) || 0;
    const from = Number(el.getAttribute('data-value') || 0) || 0;
    el.setAttribute('data-value', String(to));
    const previous = tweens.get(el);
    if (previous) cancelAnimationFrame(previous);
    if (reduceMotion || from === to) {
      el.textContent = formatter(to);
      return;
    }
    const started = performance.now();
    const step = (now) => {
      const k = Math.min(1, (now - started) / 650);
      const eased = 1 - Math.pow(1 - k, 3);
      el.textContent = formatter(from + (to - from) * eased);
      if (k < 1) tweens.set(el, requestAnimationFrame(step));
      else tweens.delete(el);
    };
    tweens.set(el, requestAnimationFrame(step));
  };

  const paintRates = (force) => {
    const now = performance.now();
    if (!force && now - state.lastRatePaint < 450) return;
    state.lastRatePaint = now;
    if (ui.tunnelRate) ui.tunnelRate.textContent = rate(state.rates.tunnel);
    if (ui.fetchRate) ui.fetchRate.textContent = rate(state.rates.fetch);
  };

  const paintUptime = (force) => {
    const now = performance.now();
    if (!force && now - state.lastUptimePaint < 900) return;
    state.lastUptimePaint = now;
    const extra = state.uptimeAt ? Math.max(0, (Date.now() - state.uptimeAt) / 1000) : 0;
    if (ui.uptime) ui.uptime.textContent = duration(state.uptimeBase + extra);
  };

  // ---------------------------------------------------------------- mesh canvas

  const COLORS = {
    accent: '196, 161, 255',
    cyan: '125, 211, 252',
    offline: '110, 132, 158',
    text: '234, 242, 255',
  };

  let view = { w: 0, h: 0, dpr: 1 };

  const resize = () => {
    if (!meshCanvas) return;
    const rect = meshCanvas.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    view = { w: Math.max(1, rect.width), h: Math.max(1, rect.height), dpr };
    meshCanvas.width = Math.round(view.w * dpr);
    meshCanvas.height = Math.round(view.h * dpr);
    meshCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
    if (sparkCanvas) {
      const srect = sparkCanvas.getBoundingClientRect();
      sparkCanvas.width = Math.round(Math.max(1, srect.width) * dpr);
      sparkCanvas.height = Math.round(Math.max(1, srect.height) * dpr);
      sparkCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
  };

  const relayNodes = () => {
    const items = state.relays.slice(0, 18);
    const nodes = [];
    const base = Math.min(view.w, view.h) * 0.36;
    items.forEach((item, index) => {
      const seed = hash(item.url || String(index));
      const angle = (index / Math.max(1, items.length)) * Math.PI * 2 + seed * 0.7;
      const radius = base * (0.78 + seed * 0.22);
      nodes.push({
        x: view.w / 2 + Math.cos(angle) * radius * 1.25,
        y: view.h / 2 + Math.sin(angle) * radius,
        online: Boolean(item.online),
        seed,
        label: hostOf(item.url),
      });
    });
    return nodes;
  };

  const drawMesh = (now) => {
    if (!meshCtx) return;
    const ctx = meshCtx;
    const w = view.w;
    const h = view.h;
    ctx.clearRect(0, 0, w, h);

    // Faint grid so the panel does not look empty before the first snapshot.
    ctx.strokeStyle = 'rgba(38, 59, 82, 0.55)';
    ctx.lineWidth = 1;
    const gap = 42;
    for (let x = gap; x < w; x += gap) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, h);
      ctx.stroke();
    }
    for (let y = gap; y < h; y += gap) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    const cx = w / 2;
    const cy = h / 2;
    const nodes = relayNodes();
    const t = now / 1000;
    const speed = reduceMotion ? 0 : 0.35 + Math.min(2.2, state.rates.fetch / 2048 + state.rates.tunnel / 4096);
    const onlineRelays = nodes.filter((n) => n.online);
    const offlineRelays = nodes.filter((n) => !n.online);

    // Links: bright for online relays, dashed for offline ones.
    nodes.forEach((node, index) => {
      ctx.save();
      if (node.online) {
        const grad = ctx.createLinearGradient(cx, cy, node.x, node.y);
        grad.addColorStop(0, 'rgba(' + COLORS.accent + ', 0.55)');
        grad.addColorStop(1, 'rgba(' + COLORS.cyan + ', 0.22)');
        ctx.strokeStyle = grad;
        ctx.lineWidth = 1.4;
      } else {
        ctx.strokeStyle = 'rgba(' + COLORS.offline + ', 0.28)';
        ctx.lineWidth = 1;
        ctx.setLineDash([3, 5]);
      }
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(node.x, node.y);
      ctx.stroke();
      ctx.restore();

      if (node.online && !reduceMotion) {
        // Two packets per link, phase-shifted so the mesh always looks alive.
        for (let p = 0; p < 2; p += 1) {
          const phase = (t * speed * 0.22 + node.seed * 3.1 + p * 0.5 + index * 0.37) % 1;
          const px = cx + (node.x - cx) * phase;
          const py = cy + (node.y - cy) * phase;
          const alpha = Math.sin(phase * Math.PI) * 0.9;
          ctx.beginPath();
          ctx.fillStyle = 'rgba(' + COLORS.cyan + ', ' + alpha.toFixed(3) + ')';
          ctx.arc(px, py, 2.1, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    });

    // Relay nodes.
    nodes.forEach((node) => {
      if (node.online) {
        const pulse = reduceMotion ? 0.5 : 0.5 + Math.sin(t * 2 + node.seed * 9) * 0.5;
        ctx.beginPath();
        ctx.fillStyle = 'rgba(' + COLORS.accent + ', ' + (0.1 + pulse * 0.12).toFixed(3) + ')';
        ctx.arc(node.x, node.y, 12 + pulse * 6, 0, Math.PI * 2);
        ctx.fill();
        ctx.beginPath();
        ctx.fillStyle = 'rgb(' + COLORS.accent + ')';
        ctx.arc(node.x, node.y, 3.4, 0, Math.PI * 2);
        ctx.fill();
      } else {
        ctx.beginPath();
        ctx.fillStyle = 'rgba(' + COLORS.offline + ', 0.75)';
        ctx.arc(node.x, node.y, 2.2, 0, Math.PI * 2);
        ctx.fill();
      }
    });

    // Peer dots orbit the local relay: online peers bright, seen-but-offline
    // peers faint on a wider ring.
    const onlineDots = Math.min(36, state.peersOnline);
    const offlineDots = Math.min(30, state.peersOffline);
    const inner = Math.min(w, h) * 0.19;
    for (let i = 0; i < onlineDots; i += 1) {
      const a = (i / Math.max(1, onlineDots)) * Math.PI * 2 + (reduceMotion ? 0 : t * 0.25);
      const r = inner * (0.92 + hash('p' + i) * 0.16);
      ctx.beginPath();
      ctx.fillStyle = 'rgba(' + COLORS.cyan + ', 0.85)';
      ctx.arc(cx + Math.cos(a) * r, cy + Math.sin(a) * r, 1.9, 0, Math.PI * 2);
      ctx.fill();
    }
    for (let i = 0; i < offlineDots; i += 1) {
      const a = (i / Math.max(1, offlineDots)) * Math.PI * 2 + hash('q' + i) * 0.4;
      const r = inner * 1.42;
      ctx.beginPath();
      ctx.fillStyle = 'rgba(' + COLORS.offline + ', 0.5)';
      ctx.arc(cx + Math.cos(a) * r, cy + Math.sin(a) * r, 1.3, 0, Math.PI * 2);
      ctx.fill();
    }

    // Local relay at the centre.
    const pulse = reduceMotion ? 0.5 : 0.5 + Math.sin(t * 1.4) * 0.5;
    const halo = ctx.createRadialGradient(cx, cy, 0, cx, cy, 46 + pulse * 12);
    halo.addColorStop(0, 'rgba(' + COLORS.accent + ', 0.5)');
    halo.addColorStop(1, 'rgba(' + COLORS.accent + ', 0)');
    ctx.beginPath();
    ctx.fillStyle = halo;
    ctx.arc(cx, cy, 58 + pulse * 12, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.fillStyle = 'rgb(' + COLORS.accent + ')';
    ctx.arc(cx, cy, 6, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(' + COLORS.text + ', ' + (0.25 + pulse * 0.35).toFixed(3) + ')';
    ctx.lineWidth = 1;
    ctx.arc(cx, cy, 16 + pulse * 8, 0, Math.PI * 2);
    ctx.stroke();

    // Labels: centre host always, peer hosts only when there is room.
    ctx.font = '11px ui-monospace, SFMono-Regular, Menlo, monospace';
    ctx.textAlign = 'center';
    ctx.fillStyle = 'rgba(' + COLORS.text + ', 0.9)';
    if (state.relayHost) {
      ctx.fillText(state.relayHost, cx, cy + 34);
    }
    if (w > 560) {
      ctx.fillStyle = 'rgba(' + COLORS.text + ', 0.55)';
      nodes.slice(0, 6).forEach((node) => {
        ctx.fillText(node.label, node.x, node.y - 12);
      });
    }

    if (onlineRelays.length === 0 && state.relays.length > 0) {
      ctx.fillStyle = 'rgba(' + COLORS.text + ', 0.5)';
      ctx.fillText('no relay reachable yet', cx, cy - 34);
    } else if (onlineRelays.length < state.relays.length) {
      ctx.fillStyle = 'rgba(' + COLORS.text + ', 0.5)';
      ctx.fillText(offlineRelays.length + ' offline', cx, cy - 34);
    }

    if (state.relays.length > nodes.length) {
      ctx.fillStyle = 'rgba(' + COLORS.text + ', 0.45)';
      ctx.fillText('+' + (state.relays.length - nodes.length) + ' more relays', cx, cy + 52);
    }
  };

  // ---------------------------------------------------------------- sparkline

  const drawSpark = () => {
    if (!sparkCtx) return;
    const ctx = sparkCtx;
    const rect = sparkCanvas.getBoundingClientRect();
    const w = Math.max(1, rect.width);
    const h = Math.max(1, rect.height);
    ctx.clearRect(0, 0, w, h);

    ctx.strokeStyle = 'rgba(38, 59, 82, 0.7)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, h - 0.5);
    ctx.lineTo(w, h - 0.5);
    ctx.stroke();

    if (state.history.length < 2) {
      ctx.font = '10px ui-monospace, SFMono-Regular, Menlo, monospace';
      ctx.textAlign = 'left';
      ctx.fillStyle = 'rgba(' + COLORS.text + ', 0.5)';
      ctx.fillText(panel.getAttribute('data-label-collecting') || 'collecting samples…', 6, 14);
      return;
    }

    let max = 1;
    for (const point of state.history) {
      max = Math.max(max, point.peers, point.relays);
    }

    // Fill the available width with the samples collected so far, so the
    // chart is readable from the first minute instead of a tiny stub.
    const stepX = w / Math.max(1, state.history.length - 1);
    const series = [
      {
        key: 'peers',
        color: COLORS.cyan,
        points: state.history.map((point, index) => ({
          x: index * stepX,
          y: h - 6 - (point.peers / max) * (h - 20),
        })),
      },
      {
        key: 'relays',
        color: COLORS.accent,
        points: state.history.map((point, index) => ({
          x: index * stepX,
          y: h - 6 - (point.relays / max) * (h - 20),
        })),
      },
    ];

    const grad = ctx.createLinearGradient(0, 0, 0, h);
    grad.addColorStop(0, 'rgba(' + COLORS.cyan + ', 0.22)');
    grad.addColorStop(1, 'rgba(' + COLORS.cyan + ', 0.02)');
    ctx.beginPath();
    ctx.moveTo(series[0].points[0].x, h);
    series[0].points.forEach((p) => ctx.lineTo(p.x, p.y));
    ctx.lineTo(series[0].points[series[0].points.length - 1].x, h);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();

    series.forEach((line) => {
      ctx.beginPath();
      line.points.forEach((p, i) => (i === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y)));
      ctx.strokeStyle = 'rgba(' + line.color + ', 0.95)';
      ctx.lineWidth = 1.6;
      ctx.stroke();
      const last = line.points[line.points.length - 1];
      ctx.beginPath();
      ctx.fillStyle = 'rgb(' + line.color + ')';
      ctx.arc(last.x, last.y, 2.2, 0, Math.PI * 2);
      ctx.fill();
    });

    const last = state.history[state.history.length - 1];
    if (ui.sparkRelays) ui.sparkRelays.textContent = int(last.relays);
    if (ui.sparkPeers) ui.sparkPeers.textContent = int(last.peers);
  };

  // ---------------------------------------------------------------- animation

  const tick = (now) => {
    if (!state.running) return;
    state.frame = requestAnimationFrame(tick);
    state.rates.tunnel += (state.targetRates.tunnel - state.rates.tunnel) * 0.06;
    state.rates.fetch += (state.targetRates.fetch - state.rates.fetch) * 0.06;
    if (Math.abs(state.targetRates.tunnel - state.rates.tunnel) < 0.5) state.rates.tunnel = state.targetRates.tunnel;
    if (Math.abs(state.targetRates.fetch - state.rates.fetch) < 0.5) state.rates.fetch = state.targetRates.fetch;
    paintRates(false);
    paintUptime(false);
    drawMesh(now);
    drawSpark();
  };

  const start = () => {
    if (state.running || reduceMotion) return;
    state.running = true;
    state.frame = requestAnimationFrame(tick);
  };

  const stop = () => {
    state.running = false;
    cancelAnimationFrame(state.frame);
  };

  const onVisibility = () => {
    if (document.hidden) {
      stop();
    } else {
      if (!reduceMotion) start();
      poll();
    }
  };

  // Only animate while the panel is on screen.
  if ('IntersectionObserver' in window && meshCanvas) {
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.some((entry) => entry.isIntersecting);
        if (visible && !document.hidden) {
          if (!reduceMotion) start();
          if (!state.snapshot) poll();
        } else {
          stop();
        }
      },
      { threshold: 0.05 }
    );
    observer.observe(meshCanvas);
  }

  window.addEventListener('resize', () => {
    resize();
    if (reduceMotion) {
      drawMesh(performance.now());
      drawSpark();
    }
  });

  document.addEventListener('visibilitychange', onVisibility);

  // ---------------------------------------------------------------- bootstrap

  resize();
  setStatus('wait');
  if (reduceMotion) {
    drawMesh(performance.now());
    drawSpark();
  }
  poll();
})();
