document.addEventListener('DOMContentLoaded', () => {
    const source = document.getElementById('copy_from');
    if (!source) return;

    source.addEventListener('change', () => {
        source.form.requestSubmit();
    });
});
