document.addEventListener('DOMContentLoaded', function () {
    const generator = document.getElementById('seed_gen_type');
    const field = document.getElementById('choice-resolution-field');
    const resolution = document.getElementById('choice_resolution');
    if (!generator || !field || !resolution) return;

    function updateVisibility() {
        const visible = ['owr', 'owr_tourney', 'alttpr_dr'].includes(generator.value);
        field.hidden = !visible;
        resolution.disabled = !visible;
    }

    updateVisibility();
    generator.addEventListener('change', updateVisibility);
    window.addEventListener('pageshow', updateVisibility);
    generator.form.addEventListener('reset', function () {
        setTimeout(updateVisibility, 0);
    });
});
