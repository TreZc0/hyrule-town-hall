function apiKeyRow(userId) {
    return document.querySelector(`tr[data-api-key-user-id="${userId}"]`);
}

function setApiKeyButtons(row, editing) {
    row.querySelector('.api-key-edit').style.display = editing ? 'none' : '';
    row.querySelector('.api-key-save').style.display = editing ? '' : 'none';
    row.querySelector('.api-key-cancel').style.display = editing ? '' : 'none';
}

function editApiKeyScopes(userId) {
    const row = apiKeyRow(userId);
    if (!row) return;

    row.querySelectorAll('.api-key-scope-control').forEach(input => {
        input.dataset.originalChecked = input.checked ? 'true' : 'false';
        input.disabled = false;
    });
    row.querySelectorAll('.api-key-scope-edit-controls').forEach(controls => {
        controls.style.display = '';
    });
    row.querySelectorAll('.api-key-scope-state').forEach(state => {
        state.style.display = 'none';
    });
    setApiKeyButtons(row, true);
}

function cancelApiKeyScopes(userId) {
    const row = apiKeyRow(userId);
    if (!row) return;

    row.querySelectorAll('.api-key-scope-control').forEach(input => {
        input.checked = input.dataset.originalChecked === 'true';
        input.disabled = true;
    });
    row.querySelectorAll('.api-key-scope-edit-controls').forEach(controls => {
        controls.style.display = 'none';
    });
    row.querySelectorAll('.api-key-scope-state').forEach(state => {
        state.style.display = '';
    });
    setApiKeyButtons(row, false);
}
