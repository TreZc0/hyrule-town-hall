document.addEventListener('DOMContentLoaded', function() {
    document.querySelectorAll('.startgg-bulk-add-copy').forEach(function(btn) {
        var names = JSON.parse(btn.dataset.names);
        btn.addEventListener('click', function() {
            navigator.clipboard.writeText(names.join('\n')).then(function() {
                var orig = btn.textContent;
                btn.textContent = 'Copied!';
                setTimeout(function() { btn.textContent = orig; }, 2000);
            });
        });
    });
});
