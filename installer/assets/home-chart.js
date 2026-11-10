/* The template's chart uses the same public measurements as the network page. */
(() => {
  'use strict';
  function draw() {
    const canvas = document.getElementById('cryptoChart');
    if (!canvas) return;
    const empty = document.getElementById('home-history-empty');
    const caption = document.getElementById('home-history-caption');
    const en = document.documentElement.lang !== 'fr';
    canvas.setAttribute('aria-label', en ? 'Hashrate over the last 24 hours' : 'Hashrate sur les dernières 24 heures');
    const data = typeof summary === 'undefined' ? null : summary;
    const points = (data?.recent || []).filter(p => Number.isFinite(p.at) && p.at > data.at - 86400000 && p.at <= data.at && Number.isFinite(p.hashrate) && p.hashrate >= 0)
      .map(p => ({t: p.at, value: p.missing ? null : p.hashrate}));
    const count = points.filter(p => p.value !== null).length;
    const drawn = window.BDLPulseChart?.render(canvas, points, {locale: en ? 'en-GB' : 'fr-FR', label: 'Hashrate', gapMs: 750000}) || false;
    canvas.hidden = !drawn; empty.hidden = drawn;
    empty.textContent = count >= 2 ? (en ? 'Chart unavailable.' : 'Graphique indisponible.') : (en ? 'Waiting for hashrate measurements.' : 'En attente des mesures de hashrate.');
    caption.textContent = count ? (en ? 'Last 24 hours · ' : 'Dernières 24 heures · ') + count + (en ? ' measurements · Paris time' : ' mesures · heure de Paris') : '';
  }
  document.addEventListener('bdl-public-stats', draw);
  document.addEventListener('bdl-language-change', draw);
  new MutationObserver(draw).observe(document.documentElement, {attributes: true, attributeFilter: ['data-theme']});
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', draw, {once: true}); else draw();
})();
