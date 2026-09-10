document.addEventListener('DOMContentLoaded', function () {
    const kind = document.getElementById('qualifier_score_kind');
    const parameters = document.getElementById('qualifier_score_config');
    if (!kind || !parameters) return;

    function showDefaults() {
        parameters.value = kind.selectedOptions[0]?.dataset.scoreDefaults || '';
    }

    // Keep submitted edits when the server redisplays a form with validation errors.
    if (!parameters.value.trim()) showDefaults();
    kind.addEventListener('change', showDefaults);
});
