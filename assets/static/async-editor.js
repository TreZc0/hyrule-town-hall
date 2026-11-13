(() => {
    const editor = document.getElementById('async-qualifiers');
    if (!editor) return;

    // Keep links back to this section useful on the standalone editor too.
    const reveal = () => {
        if (window.location.hash === '#async-qualifiers') editor.open = true;
    };
    reveal();
    window.addEventListener('hashchange', reveal);

    const panels = editor.querySelectorAll('.async-create');
    for (const panel of panels) {
        panel.addEventListener('toggle', () => {
            if (panel.open) {
                for (const sibling of panels) {
                    if (sibling !== panel) sibling.open = false;
                }
            }
        });
    }

    const count = editor.querySelector('#async_count');
    const preview = editor.querySelector('#async-batch-preview');
    if (!count || !preview) return;
    const updatePreview = () => {
        if (!count.validity.valid) {
            preview.textContent = 'Enter a count from 1 to 100.';
            return;
        }
        const first = count.dataset.nextName.match(/^Async (\d+)$/);
        if (!first) {
            preview.textContent = 'Unable to allocate names. Check the existing Async numbers.';
            return;
        }
        const start = BigInt(first[1]);
        const end = start + BigInt(count.valueAsNumber) - 1n;
        const names = start === end ? `Async ${start}` : `Async ${start}–${end}`;
        preview.textContent = `Creates ${names} with the same availability window. Numbering is checked again when saved.`;
    };
    count.addEventListener('input', updatePreview);
    updatePreview();
})();
