// Enhance the existing edit route so validation and scheduling side effects stay shared.
document.addEventListener('click', async function(event) {
    const link = event.target.closest('a.qualifier-race-edit');
    if (!link || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    const row = link.closest('tr');
    if (row.dataset.editing) return;
    row.dataset.editing = 'loading';
    const actions = row.querySelector('.qualifier-race-actions');
    actions.querySelector('.qualifier-race-status')?.remove();
    const status = document.createElement('p');
    status.className = 'qualifier-race-status';
    status.setAttribute('role', 'status');
    status.textContent = 'Loading editor…';
    actions.append(status);

    function errorMessage(response) {
        if (response.status === 403) return 'You no longer have permission to edit this race, or the event has started. Reload the page to check.';
        if (response.status === 409) return 'Pause qualifier requests before editing. A pool cannot be changed once seed generation or entries have started.';
        if (response.status === 400) return 'Check that an enabled qualifier mode is selected.';
        return 'Could not save the race. Your changes are still here; please try again.';
    }

    try {
        const response = await fetch(link.href);
        if (!response.ok) throw new Error('Could not load the race editor. Reload the page to check your access.');
        const page = new DOMParser().parseFromString(await response.text(), 'text/html');
        const source = Array.from(page.forms).find(form => form.getAttribute('action') === new URL(link.href).pathname);
        if (!source) throw new Error('Could not load the race editor. Reload the page to check your access.');
        status.remove();
        const original = Array.from(row.cells, cell => [cell, Array.from(cell.childNodes)]);
        const form = document.createElement('form');
        form.id = `qualifier-race-edit-${row.dataset.qualifierRace}`;
        form.action = link.href;
        form.method = 'post';
        form.className = 'qualifier-race-edit-form';
        const fields = Array.from(source.querySelectorAll('input[name], select[name]'));
        for (const field of fields) {
            const label = field.labels && field.labels[0];
            if (label) field.setAttribute('aria-label', label.textContent.trim());
            field.id = `${form.id}-${field.name}`;
            field.setAttribute('form', form.id);
            const cell = Array.from(row.cells).find(cell => cell.dataset.raceField === field.name);
            if (cell && field.type !== 'hidden') {
                cell.replaceChildren(field);
            } else {
                form.append(field);
            }
        }
        const save = document.createElement('button');
        save.type = 'submit';
        save.className = 'button';
        save.textContent = 'Save';
        const cancel = document.createElement('button');
        cancel.type = 'button';
        cancel.className = 'button';
        cancel.textContent = 'Cancel';
        status.textContent = '';
        form.append(save, cancel, status);
        actions.replaceChildren(form);
        row.dataset.editing = 'ready';

        function restore() {
            row.removeEventListener('keydown', escape);
            for (const [cell, nodes] of original) cell.replaceChildren(...nodes);
            delete row.dataset.editing;
            link.focus();
        }
        cancel.addEventListener('click', restore);
        // The inputs are in neighboring cells, associated with the form by ID.
        function escape(event) {
            if (event.key === 'Escape' && row.dataset.editing === 'ready') {
                restore();
            }
        }
        row.addEventListener('keydown', escape);
        form.addEventListener('submit', async event => {
            event.preventDefault();
            if (row.dataset.editing === 'saving') return;
            const body = new URLSearchParams(new FormData(form));
            row.dataset.editing = 'saving';
            const controls = Array.from(form.elements);
            controls.forEach(control => { control.disabled = true; });
            status.textContent = 'Saving…';
            try {
                const response = await fetch(form.action, {method: 'POST', body});
                if (!response.ok) throw new Error(errorMessage(response));
                const page = new DOMParser().parseFromString(await response.text(), 'text/html');
                const orderedRows = Array.from(page.querySelectorAll('tr[data-qualifier-race]'));
                const updated = orderedRows.find(candidate => candidate.dataset.qualifierRace === row.dataset.qualifierRace);
                if (response.redirected && updated) {
                    row.replaceWith(updated);
                    // Use the server's start-time order without discarding other rows' unsaved edits.
                    const body = updated.parentElement;
                    const currentRows = new Map(Array.from(body.children, candidate => [candidate.dataset.qualifierRace, candidate]));
                    for (const ordered of orderedRows) {
                        const current = currentRows.get(ordered.dataset.qualifierRace);
                        if (current) body.append(current);
                    }
                    const message = document.createElement('p');
                    message.className = 'qualifier-race-status';
                    message.setAttribute('role', 'status');
                    message.textContent = 'Saved.';
                    updated.querySelector('.qualifier-race-actions')?.append(message);
                    updated.querySelector('.qualifier-race-edit')?.focus();
                    return;
                }
                const errors = Array.from(page.querySelectorAll('form p.error'), error => error.textContent.trim());
                throw new Error(errors.join(' ') || 'Could not confirm that the race was saved. Reload the page to check before trying again.');
            } catch (error) {
                status.textContent = error instanceof TypeError
                    ? 'Connection interrupted. Your changes are still here. Reload the page to check whether they were saved before trying again.'
                    : error.message;
            } finally {
                row.dataset.editing = 'ready';
                controls.forEach(control => { control.disabled = false; });
            }
        });
        fields.find(field => field.type !== 'hidden')?.focus();
    } catch (error) {
        status.textContent = error.message;
        delete row.dataset.editing;
    }
});
