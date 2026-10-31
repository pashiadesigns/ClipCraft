const features = [...document.querySelectorAll('.roadmap-feature')];
const filters = [...document.querySelectorAll('[data-status-filter]')];
const search = document.querySelector('#feature-search');
const category = document.querySelector('#feature-category');
let status = 'all';
function filterFeatures() {
  const query = search.value.toLocaleLowerCase().trim();
  let visible = 0;
  for (const feature of features) {
    const matches = (status === 'all' || feature.dataset.featureStatus === status) && (category.value === 'all' || feature.dataset.featureCategory === category.value) && (!query || feature.textContent.toLocaleLowerCase().includes(query));
    feature.hidden = !matches;
    if (matches) visible++;
  }
  for (const button of filters) button.setAttribute('aria-pressed', String(button.dataset.statusFilter === status));
  document.querySelector('#feature-count').textContent = `Showing ${visible} of ${features.length} features`;
  document.querySelector('#feature-empty').hidden = visible !== 0;
}
for (const button of filters) button.addEventListener('click', () => { status = button.dataset.statusFilter; filterFeatures(); });
search.addEventListener('input', filterFeatures);
category.addEventListener('change', filterFeatures);
document.querySelector('#feature-list').addEventListener('click', event => {
  if (!event.target.closest('[data-feature-link]')) return;
  status = 'all'; search.value = ''; category.value = 'all'; filterFeatures();
});
filterFeatures();
