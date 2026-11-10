/* The template's chart uses the same public measurements as the network page. */
(() => {
  'use strict';
  function draw() {
    const canvas = document.getElementById('cryptoChart');
    if (!canvas) return;
    const empty = document.getElementById('home-history-empty');
    const caption = document.getElementById('home-history-caption');
    const en = document.documentElement.lang !== 'fr';
    canvas.setAttribute('aria-label', en ? 'Hashrate over the last 24 hours. Use left and right arrows to inspect measurements.' : 'Hashrate sur les dernières 24 heures. Utilisez les flèches gauche et droite pour consulter les mesures.');
    const data = typeof summary === 'undefined' ? null : summary;
    const points = (data?.recent || []).filter(p => Number.isFinite(p.at) && p.at > data.at - 86400000 && p.at <= data.at && Number.isFinite(p.hashrate) && p.hashrate >= 0)
      .map(p => ({t: p.at, value: p.missing ? null : p.hashrate}));
    const heroRate = document.getElementById('hero-network-rate');
    if (heroRate) heroRate.textContent = Number.isFinite(data?.hashrate) ? window.BDLPulseChart?.rate(data.hashrate, en ? 'en-GB' : 'fr-FR') || '—' : '—';
    const spark = document.getElementById('hero-network-spark');
    if (spark) {
      const ordered = [...points].sort((a, b) => a.t - b.t);
      const valid = ordered.filter(p => p.value !== null);
      let path = '';
      if (valid.length > 1) {
        const first = ordered[0].t, span = Math.max(ordered.at(-1).t - first, 1);
        const peak = Math.max(...valid.map(p => p.value), 1);
        let previous = null;
        for (const point of ordered) {
          if (point.value === null) { previous = null; continue; }
          const start = !previous || point.t - previous.t > 750000;
          path += (start ? 'M' : 'L') + ((point.t - first) / span * 260).toFixed(2) + ' ' + (100 - point.value / peak * 88).toFixed(2) + ' ';
          previous = point;
        }
      }
      spark.innerHTML = '<path class="hero-spark-grid" d="M0 25H260M0 60H260M0 95H260"/><path class="hero-spark-under" d="' + path + '"/><path class="hero-spark-line" d="' + path + '"/>';
    }
    const count = points.filter(p => p.value !== null).length;
    const drawn = window.BDLPulseChart?.render(canvas, points, {locale: en ? 'en-GB' : 'fr-FR', label: 'Hashrate', gapMs: 750000}) || false;
    canvas.hidden = !drawn; empty.hidden = drawn;
    empty.textContent = count >= 2 ? (en ? 'Chart unavailable.' : 'Graphique indisponible.') : (en ? 'Waiting for hashrate measurements.' : 'En attente des mesures de hashrate.');
    caption.textContent = count ? (en ? 'Last 24 hours · ' : 'Dernières 24 heures · ') + count + (en ? ' measurements · Paris time' : ' mesures · heure de Paris') : '';
  }
  const canvas = document.getElementById('cryptoChart');
  canvas?.addEventListener('keydown', event => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    const chart = window.Chart?.getChart(canvas);
    const points = chart?.data.datasets[0].data || [];
    const available = points.map((point, i) => point.y === null ? null : i).filter(i => i !== null);
    if (!available.length) return;
    event.preventDefault();
    const current = available.indexOf(chart.getActiveElements()[0]?.index);
    const selected = event.key === 'Home' ? 0 : event.key === 'End' ? available.length - 1 : current < 0 ? (event.key === 'ArrowRight' ? 0 : available.length - 1) : Math.max(0, Math.min(available.length - 1, current + (event.key === 'ArrowRight' ? 1 : -1)));
    const index = available[selected], point = points[index], marker = chart.getDatasetMeta(0).data[index];
    const active = [{datasetIndex: 0, index}];
    chart.setActiveElements(active);
    chart.tooltip?.setActiveElements(active, {x: marker.x, y: marker.y});
    chart.update('none');
    const locale = document.documentElement.lang === 'fr' ? 'fr-FR' : 'en-GB';
    document.getElementById('home-history-inspect').textContent = new Intl.DateTimeFormat(locale, {
      day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit', timeZone: 'Europe/Paris',
    }).format(new Date(point.x)) + ' · ' + window.BDLPulseChart.rate(point.y, locale);
  });
  canvas?.addEventListener('blur', () => {
    const chart = window.Chart?.getChart(canvas);
    if (chart) {
      chart.setActiveElements([]);
      chart.tooltip?.setActiveElements([], {x: 0, y: 0});
      chart.update('none');
    }
    document.getElementById('home-history-inspect').textContent = '';
  });
  document.addEventListener('bdl-public-stats', draw);
  document.addEventListener('bdl-language-change', draw);
  new MutationObserver(draw).observe(document.documentElement, {attributes: true, attributeFilter: ['data-theme']});
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', draw, {once: true}); else draw();
})();
