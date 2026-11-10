/* Reveal the floating navigation on upward scroll or keyboard interaction. */
(() => {
  'use strict';
  const header = document.querySelector('.site-header, #__next > nav, body > nav');
  if (!header) return;
  const position = () => Math.max(0, Math.min(window.scrollY, document.documentElement.scrollHeight - innerHeight));
  let previous = position(), direction = 0, distance = 0, pending = false, focusFrame = 0;
  const interacting = () => [...header.querySelectorAll('[aria-expanded="true"], details[open], :focus-visible')]
    .some(control => control.getClientRects().length > 0);
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
  // Focus must ignore the anchor inset while the browser brings a header control into view.
  function finishFocus() {
    cancelAnimationFrame(focusFrame); focusFrame = 0;
    document.documentElement.classList.remove('nav-focusing');
  }
  function revealForFocus() {
    document.documentElement.classList.add('nav-focusing');
    header.classList.add('nav-focus-reveal');
    reveal();
    header.getBoundingClientRect(); // Apply the visible position before native focus scrolling.
    header.classList.remove('nav-focus-reveal');
    cancelAnimationFrame(focusFrame);
    focusFrame = requestAnimationFrame(finishFocus);
  }
  header.addEventListener('focusin', revealForFocus);
  header.addEventListener('click', finishFocus);
  document.addEventListener('keydown', event => { if (event.key === 'Tab') revealForFocus(); else finishFocus(); });
  header.addEventListener('toggle', reveal, true);
  function resetForViewport() {
    finishFocus();
    const toggle = header.querySelector('.menu-toggle, #menu');
    if (toggle?.getAttribute('aria-expanded') === 'true' && !toggle.getClientRects().length) toggle.click();
    reveal();
  }
  window.addEventListener('resize', resetForViewport);
  window.addEventListener('pageshow', resetForViewport);
})();
