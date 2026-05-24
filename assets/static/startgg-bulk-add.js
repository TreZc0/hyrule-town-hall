document.addEventListener('DOMContentLoaded', function() {
    var btn = document.getElementById('startgg-bulk-add-copy');
    if (!btn) return;
    var names = JSON.parse(btn.dataset.names);
    btn.addEventListener('click', function() {
        navigator.clipboard.writeText(names.join('\n')).then(function() {
            var orig = btn.textContent;
            btn.textContent = 'Copied!';
            setTimeout(function() { btn.textContent = orig; }, 2000);
        });
    });
});
