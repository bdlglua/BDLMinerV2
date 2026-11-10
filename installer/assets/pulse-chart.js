/* Shared Chart.js presentation for real, timestamped hashrate measurements. */
(() => {
  'use strict';
  const charts = new WeakMap();
  const units = ['H/s', 'kH/s', 'MH/s', 'GH/s', 'TH/s', 'PH/s', 'EH/s'];
  function rate(value, locale) {
    if (!Number.isFinite(value) || value < 0) return '—';
    let unit = 0;
    while (value >= 1000 && unit < units.length - 1) { value /= 1000; unit++; }
    return new Intl.NumberFormat(locale, {maximumFractionDigits: 2}).format(value) + ' ' + units[unit];
  }
  function destroy(canvas) {
    charts.get(canvas)?.destroy();
    charts.delete(canvas);
  }
  function render(canvas, measurements, settings = {}) {
    if (!canvas || typeof window.Chart !== 'function') return false;
    const locale = settings.locale || document.documentElement.lang || 'en';
    const points = measurements.filter(p => Number.isFinite(p.t) && (p.value === null || (Number.isFinite(p.value) && p.value >= 0))).sort((a, b) => a.t - b.t);
    if (points.filter(p => p.value !== null).length < 2) { destroy(canvas); return false; }
    canvas.hidden = false;
    const palette = getComputedStyle(document.documentElement);
    const color = palette.getPropertyValue('--chart-color').trim() || '#00ff88';
    const muted = palette.getPropertyValue('--label').trim() || '#94a3b8';
    const grid = palette.getPropertyValue('--chart-grid').trim() || 'rgba(255,255,255,.05)';
    const background = palette.getPropertyValue('--bg').trim() || '#070913';
    const rgb = /^#[a-f0-9]{6}$/i.test(color) ? color.slice(1).match(/../g).map(v => parseInt(v, 16)).join(',') : '0,255,136';
    const formatTime = value => new Intl.DateTimeFormat(locale, {
      ...(points.at(-1).t - points[0].t > 86400000 ? {day: '2-digit', month: 'short'} : {}),
      hour: '2-digit', minute: '2-digit', timeZone: 'Europe/Paris',
    }).format(new Date(Number(value)));
    const peak = Math.max(...points.map(p => p.value || 0), 1);
    let axisUnit = 0;
    while (peak / 1000 ** axisUnit >= 1000 && axisUnit < units.length - 1) axisUnit++;
    const values = [];
    for (const point of points) {
      const previous = values.at(-1);
      if (previous && settings.gapMs && point.t - previous.x > settings.gapMs) values.push({x: previous.x + 1, y: null});
      values.push({x: point.t, y: point.value});
    }
    const dataset = {
      label: settings.label || 'Hashrate', data: values, borderColor: color, borderWidth: 3,
      backgroundColor: context => {
        const area = context.chart.chartArea;
        if (!area) return 'transparent';
        const gradient = context.chart.ctx.createLinearGradient(0, area.top, 0, area.bottom);
        gradient.addColorStop(0, `rgba(${rgb},.4)`);
        gradient.addColorStop(1, `rgba(${rgb},0)`);
        return gradient;
      },
      fill: true, tension: .4, cubicInterpolationMode: 'monotone', spanGaps: false,
      pointRadius: points.length > 40 ? 0 : 3, pointHoverRadius: 6,
      pointBackgroundColor: background, pointBorderColor: color, pointBorderWidth: 2,
    };
    const options = {
      responsive: true, maintainAspectRatio: false, parsing: false, normalized: false,
      animation: matchMedia('(prefers-reduced-motion: reduce)').matches ? false : {duration: 300},
      interaction: {mode: 'nearest', axis: 'x', intersect: false},
      plugins: {
        legend: {display: false},
        tooltip: {
          backgroundColor: 'rgba(19,23,42,.96)', titleColor: '#a4b0c5', bodyColor: '#fff',
          borderColor: `rgba(${rgb},.35)`, borderWidth: 1, padding: 12, displayColors: false,
          callbacks: {
            title: items => items.length ? formatTime(items[0].parsed.x) : '',
            label: item => rate(item.parsed.y, locale),
          },
        },
      },
      scales: {
        y: {beginAtZero: true, border: {display: false}, grid: {color: grid}, ticks: {
          color: muted, maxTicksLimit: 5,
          callback: value => new Intl.NumberFormat(locale, {maximumFractionDigits: 2}).format(Number(value) / 1000 ** axisUnit) + ' ' + units[axisUnit],
        }},
        x: {type: 'linear', min: points[0].t, max: points.at(-1).t, border: {display: false}, grid: {display: false}, ticks: {
          color: muted, maxTicksLimit: canvas.clientWidth < 500 ? 3 : 6, maxRotation: 0,
          callback: value => formatTime(value),
        }},
      },
    };
    canvas.hidden = false;
    const existing = charts.get(canvas);
    if (existing) {
      existing.data.datasets = [dataset]; existing.options = options; existing.update('none');
    } else {
      charts.set(canvas, new Chart(canvas.getContext('2d'), {type: 'line', data: {datasets: [dataset]}, options}));
    }
    return true;
  }
  window.BDLPulseChart = Object.freeze({render, destroy, rate});
})();
