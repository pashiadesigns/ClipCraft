const menu = document.querySelector('.menu-toggle');
const nav = document.querySelector('#navigation');
menu.addEventListener('click', () => {
  const open = menu.getAttribute('aria-expanded') !== 'true';
  menu.setAttribute('aria-expanded', String(open));
  nav.classList.toggle('open', open);
});
nav.addEventListener('click', event => {
  if (event.target.closest('a')) { menu.setAttribute('aria-expanded', 'false'); nav.classList.remove('open'); }
});
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && nav.classList.contains('open')) { menu.setAttribute('aria-expanded', 'false'); nav.classList.remove('open'); menu.focus(); }
});
for (const button of document.querySelectorAll('[data-preview]')) {
  button.addEventListener('click', () => {
    const annotated = button.dataset.preview === 'after';
    for (const item of document.querySelectorAll('[data-preview]')) item.setAttribute('aria-pressed', String(item === button));
    const image = document.querySelector('#example-image');
    image.src = annotated ? 'assets/annotated.png' : 'assets/original.png';
    image.alt = annotated ? 'Synthetic interface with a red arrow and rectangle highlighting its blue heading' : 'Original synthetic interface before annotations';
    document.querySelector('#preview-caption').textContent = annotated ? 'Arrow + rectangle · Flattened PNG export' : 'Original image · Before annotations';
  });
}
const toast = document.querySelector('.toast');
let toastTimer;
document.querySelector('#copy-checksum')?.addEventListener('click', async event => {
  try {
    await navigator.clipboard.writeText(event.currentTarget.dataset.checksum);
    toast.textContent = 'SHA-256 checksum copied';
  } catch { toast.textContent = 'Open SHA-256 checksums to select and copy the value.'; }
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { toast.hidden = true; }, 5000);
});
const marker = document.querySelector('#scroll-position');
function updateProgress() {
  if (!marker) return;
  const available = document.documentElement.scrollHeight - window.innerHeight;
  const progress = available > 0 ? Math.min(1, Math.max(0, window.scrollY / available)) : 0;
  marker.style.top = `calc(${progress * 100}% - ${progress * 16}px)`;
}
window.addEventListener('scroll', updateProgress, {passive:true});
window.addEventListener('resize', updateProgress);
updateProgress();
