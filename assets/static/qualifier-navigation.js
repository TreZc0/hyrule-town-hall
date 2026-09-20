// Redirects after saving link to the relevant section, including inside closed spoilers.
function revealQualifierSection() {
    const target = document.getElementById(window.location.hash.slice(1));
    if (!target || !target.closest('.qualifier-admin')) return;
    for (let element = target; element; element = element.parentElement) {
        if (element.tagName === 'DETAILS') element.open = true;
    }
    requestAnimationFrame(() => target.scrollIntoView({block: 'start'}));
}

revealQualifierSection();
window.addEventListener('hashchange', revealQualifierSection);
// Reopening a section should also work when its fragment is already in the URL.
document.addEventListener('click', event => {
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const link = event.target.closest('a[href^="#"]');
    if (link && link.hash === window.location.hash) revealQualifierSection();
});
