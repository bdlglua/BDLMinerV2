/* Reveal the floating navigation on upward scroll or keyboard interaction. */
(() => {
  'use strict';
  const header = document.querySelector('.site-header, #__next > nav, body > nav');
  if (!header) return;
  const position = () => Math.max(0, Math.min(window.scrollY, document.documentElement.scrollHeight - innerHeight));
  let previous = position(), direction = 0, distance = 0, pending = false;
  const interacting = () => header.querySelector('[aria-expanded="true"], details[open], :focus-visible');
  function reveal() {
    header.classList.remove('nav-hidden');
    previous = position(); direction = 0; distance = 0;
  }
  function update() {
    pending = false;
    const y = position(), delta = y - previous;
    previous = y;
    if (y < 120 || interacting()) { reveal(); return; }
    if (!delta) return;
    const next = Math.sign(delta);
    if (next !== direction) { direction = next; distance = 0; }
    distance += Math.abs(delta);
    if (distance >= (direction > 0 ? 16 : 8)) header.classList.toggle('nav-hidden', direction > 0);
  }
  window.addEventListener('scroll', () => {
    if (!pending) { pending = true; requestAnimationFrame(update); }
  }, {passive: true});
  // Keyboard focus reveals the bar immediately, before the browser scrolls it into view.
  function revealForFocus() {
    header.classList.add('nav-focus-reveal');
    reveal();
    header.getBoundingClientRect(); // Apply the visible position before native focus scrolling.
    header.classList.remove('nav-focus-reveal');
  }
  header.addEventListener('focusin', revealForFocus);
  document.addEventListener('keydown', event => { if (event.key === 'Tab') revealForFocus(); });
  header.addEventListener('toggle', reveal, true);
  window.addEventListener('resize', reveal);
  window.addEventListener('pageshow', reveal);
})();
