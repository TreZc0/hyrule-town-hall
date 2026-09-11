document.addEventListener('DOMContentLoaded', function() {
    var baseline = document.getElementById('baseline');
    if (baseline) {
        var choices = baseline.form.querySelectorAll('[data-baselines]');
        function updateChoices() {
            choices.forEach(function(choice) {
                var scopes = choice.dataset.baselines;
                var applies = !scopes || JSON.parse(scopes).includes(baseline.value);
                choice.hidden = !applies;
                choice.querySelectorAll('input').forEach(function(input) {
                    input.disabled = !applies;
                });
            });
        }
        baseline.addEventListener('change', updateChoices);
        updateChoices();
    }
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
