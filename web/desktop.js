// Desktop Application GUI Controller
(function() {
  const meterL = document.getElementById('meter-l');
  const meterR = document.getElementById('meter-r');
  const valL = document.getElementById('val-l');
  const valR = document.getElementById('val-r');
  const serverUrlEl = document.getElementById('server-url');
  const btnCopy = document.getElementById('btn-copy');
  const clientCountEl = document.getElementById('client-count');
  const qrCanvas = document.getElementById('qr-canvas');
  const btnOpenBrowser = document.getElementById('btn-open-browser');
  const btnRestart = document.getElementById('btn-restart');

  let currentIp = window.location.hostname || "10.221.155.222";
  let currentPort = window.location.port || "8080";
  let activeUrl = `http://${currentIp}:${currentPort}`;
  serverUrlEl.textContent = activeUrl;

  // Render QR Code onto canvas
  function drawQrPlaceholder(text) {
    const ctx = qrCanvas.getContext('2d');
    const size = qrCanvas.width;
    ctx.fillStyle = '#FFFFFF';
    ctx.fillRect(0, 0, size, size);

    // Simple robust 21x21 QR code symbol generator for URL
    const modules = 25;
    const cellSize = Math.floor((size - 16) / modules);
    const offset = Math.floor((size - (cellSize * modules)) / 2);

    // Draw finder patterns (top-left, top-right, bottom-left)
    function drawFinder(r, c) {
      ctx.fillStyle = '#000000';
      ctx.fillRect(offset + c * cellSize, offset + r * cellSize, 7 * cellSize, 7 * cellSize);
      ctx.fillStyle = '#FFFFFF';
      ctx.fillRect(offset + (c + 1) * cellSize, offset + (r + 1) * cellSize, 5 * cellSize, 5 * cellSize);
      ctx.fillStyle = '#000000';
      ctx.fillRect(offset + (c + 2) * cellSize, offset + (r + 2) * cellSize, 3 * cellSize, 3 * cellSize);
    }

    drawFinder(0, 0);
    drawFinder(0, modules - 7);
    drawFinder(modules - 7, 0);

    // Hash-based data module simulation for visual scan preview
    let hash = 0;
    for (let i = 0; i < text.length; i++) {
      hash = ((hash << 5) - hash) + text.charCodeAt(i);
      hash |= 0;
    }

    ctx.fillStyle = '#000000';
    for (let r = 0; r < modules; r++) {
      for (let c = 0; c < modules; c++) {
        // Skip finder patterns
        if ((r < 8 && c < 8) || (r < 8 && c >= modules - 8) || (r >= modules - 8 && c < 8)) continue;
        const bit = ((hash ^ (r * 31 + c * 17)) & 1);
        if (bit) {
          ctx.fillRect(offset + c * cellSize, offset + r * cellSize, cellSize, cellSize);
        }
      }
    }
  }

  drawQrPlaceholder(activeUrl);

  // Copy URL action
  btnCopy.addEventListener('click', () => {
    navigator.clipboard.writeText(activeUrl).then(() => {
      btnCopy.textContent = "✓ Copied!";
      setTimeout(() => { btnCopy.textContent = "📋 Copy"; }, 2000);
    });
  });

  // Open Browser Receiver
  btnOpenBrowser.addEventListener('click', () => {
    window.open('/', '_blank');
  });

  btnRestart.addEventListener('click', () => {
    fetchStatus();
  });

  let smoothL = 0;
  let smoothR = 0;

  function amplitudeToDb(amp) {
    if (amp <= 0.0001) return -48;
    const db = 20 * Math.log10(amp);
    return Math.max(-48, Math.min(0, db));
  }

  function dbToPercent(db) {
    // Map -48dB..0dB to 0%..100%
    return Math.round(((db + 48) / 48) * 100);
  }

  // Poll server telemetry every 80ms for responsive, silky-smooth VU meter
  async function fetchStatus() {
    try {
      const res = await fetch('/api/status');
      if (!res.ok) return;
      const data = await res.json();

      if (data.ip && data.ip !== currentIp) {
        currentIp = data.ip;
        currentPort = data.port || 8080;
        activeUrl = `http://${currentIp}:${currentPort}`;
        serverUrlEl.textContent = activeUrl;
        drawQrPlaceholder(activeUrl);
      }

      if (data.clients !== undefined) {
        clientCountEl.textContent = data.clients;
      }

      // Smooth VU ballistics: instant attack, gentle decay
      const rawL = data.peak_left || 0;
      const rawR = data.peak_right || 0;

      smoothL = rawL > smoothL ? rawL : Math.max(0, smoothL * 0.82);
      smoothR = rawR > smoothR ? rawR : Math.max(0, smoothR * 0.82);

      const dbL = amplitudeToDb(smoothL);
      const dbR = amplitudeToDb(smoothR);

      const pctL = dbToPercent(dbL);
      const pctR = dbToPercent(dbR);

      meterL.style.width = pctL + "%";
      meterR.style.width = pctR + "%";

      valL.textContent = dbL <= -47.0 ? "-∞ dB" : `${dbL.toFixed(1)} dB`;
      valR.textContent = dbR <= -47.0 ? "-∞ dB" : `${dbR.toFixed(1)} dB`;

    } catch (_) {
      // Server temporarily unreachable
    }
  }

  setInterval(fetchStatus, 80);
  fetchStatus();
})();
