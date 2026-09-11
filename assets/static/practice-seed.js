document.addEventListener('DOMContentLoaded', function() {
    var button = document.getElementById('copy-practice-seed-link');
    var link = document.getElementById('practice-seed-link');
    var status = document.getElementById('practice-seed-copy-status');
    if (!button || !link || !status) return;

    button.addEventListener('click', async function() {
        try {
            await navigator.clipboard.writeText(link.value);
            status.textContent = 'Link copied.';
        } catch (_) {
            link.focus();
            link.select();
            status.textContent = 'Select and copy the link above.';
        }
    });
});
