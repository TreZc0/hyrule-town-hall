// Keep a successful load for the lifetime of the page; share in-flight requests.
const qualifierLoads = new WeakMap();

function loadQualifierSection(section) {
    if (!section.dataset.qualifierUrl) return Promise.resolve(true);
    if (qualifierLoads.has(section)) return qualifierLoads.get(section);
    const content = section.querySelector('.qualifier-lazy-content');
    const status = content.querySelector('[role="status"]');
    content.setAttribute('aria-busy', 'true');
    status.textContent = 'Loading results…';
    const request = (async () => {
        try {
            const response = await fetch(section.dataset.qualifierUrl, {
                credentials: 'same-origin',
                redirect: 'error',
                cache: 'no-store',
            });
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            content.innerHTML = await response.text();
            document.dispatchEvent(new CustomEvent('qualifier:loaded', {detail: content}));
            return true;
        } catch (_) {
            qualifierLoads.delete(section);
            status.textContent = 'Could not load results. ';
            const retry = document.createElement('button');
            retry.type = 'button';
            retry.textContent = 'Retry';
            retry.addEventListener('click', () => loadQualifierSection(section));
            status.append(retry);
            return false;
        } finally {
            content.removeAttribute('aria-busy');
        }
    })();
    qualifierLoads.set(section, request);
    return request;
}

for (const section of document.querySelectorAll('details[data-qualifier-url]')) {
    section.addEventListener('toggle', () => {
        if (section.open) loadQualifierSection(section);
    });
    if (section.open) loadQualifierSection(section);
}

// Redirects and previous-result links can point into a block not yet loaded.
async function revealQualifierSection() {
    const hash = window.location.hash;
    const id = hash.slice(1);
    let target = document.getElementById(id);
    if (!target && /^attempt-\d+$/.test(id)) {
        const attemptId = id.slice('attempt-'.length);
        target = Array.from(document.querySelectorAll('[data-attempt-ids]'))
            .find(section => section.dataset.attemptIds.split(' ').includes(attemptId));
    }
    if (!target || !target.closest('.qualifier-admin')) return;
    const loads = [];
    for (let element = target; element; element = element.parentElement) {
        if (element.tagName === 'DETAILS') {
            element.open = true;
            loads.push(loadQualifierSection(element));
        }
    }
    await Promise.all(loads);
    if (window.location.hash !== hash) return;
    target = document.getElementById(id) || target;
    requestAnimationFrame(() => target.scrollIntoView({block: 'start'}));
}

revealQualifierSection();
window.addEventListener('hashchange', revealQualifierSection);
document.addEventListener('click', event => {
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const link = event.target.closest('a[href^="#"]');
    if (link && link.hash === window.location.hash) revealQualifierSection();
});
