// Consolidated inline editing for role binding tables (event-level, game-level, and override rows).
// Rows must carry:
//   data-binding-id        — the binding ID
//   data-save-path         — POST endpoint for saving field edits
//   data-delete-path       — POST action for the Delete button
//
// Override rows additionally carry:
//   data-override-save-path        — POST endpoint for saving the override (upsert)
//   data-override-discord-role-id  — current override discord_role_id (empty = not overridden)
//   data-override-min-count        — current override min_count (empty = not overridden)
//   data-override-max-count        — current override max_count (empty = not overridden)

function getCsrf() {
    const el = document.querySelector('input[name="csrf"]');
    return el ? el.value : '';
}

// ── Regular role binding edit ────────────────────────────────────────────────

function startEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;

    const minCount    = row.querySelector('.min-count').getAttribute('data-value');
    const maxCount    = row.querySelector('.max-count').getAttribute('data-value');
    const autoApprove = row.querySelector('.auto-approve').getAttribute('data-value');
    const discordRole = row.querySelector('.discord-role').getAttribute('data-value');

    row.querySelector('.min-count').innerHTML =
        `<input type="number" name="min_count" value="${minCount}" min="1" style="width: 60px;">`;
    row.querySelector('.max-count').innerHTML =
        `<input type="number" name="max_count" value="${maxCount}" min="1" style="width: 60px;">`;
    row.querySelector('.auto-approve').innerHTML =
        `<input type="checkbox" name="auto_approve" ${autoApprove === 'true' ? 'checked' : ''}>`;
    row.querySelector('.discord-role').innerHTML =
        `<input type="text" name="discord_role_id" value="${discordRole}" placeholder="e.g. 123456789012345678" style="width: 220px;">`;

    const actionsDiv = row.querySelector('.actions');
    actionsDiv.innerHTML =
        `<button class="button save-btn" onclick="saveEdit(${bindingId})">Save</button>` +
        `<button class="button cancel-btn" onclick="cancelEdit(${bindingId})">Cancel</button>`;
}

function cancelEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;

    const minCount    = row.querySelector('.min-count').getAttribute('data-value');
    const maxCount    = row.querySelector('.max-count').getAttribute('data-value');
    const autoApprove = row.querySelector('.auto-approve').getAttribute('data-value');
    const discordRole = row.querySelector('.discord-role').getAttribute('data-value');

    row.querySelector('.min-count').textContent = minCount;
    row.querySelector('.max-count').textContent = maxCount;
    row.querySelector('.auto-approve').innerHTML = autoApprove === 'true'
        ? '<span style="color: green;">✓ Yes</span>'
        : '<span style="color: red;">✗ No</span>';
    row.querySelector('.discord-role').textContent = discordRole || 'None';

    restoreEditActions(row, bindingId);
}

function restoreEditActions(row, bindingId) {
    const deletePath = row.getAttribute('data-delete-path');
    const csrf = getCsrf();
    const actionsDiv = row.querySelector('.actions');
    actionsDiv.innerHTML =
        `<button class="button edit-btn config-edit-btn" onclick="startEdit(${bindingId})">Edit</button>` +
        `<form action="${deletePath}" method="post" style="display: inline;">` +
        `<input type="hidden" name="csrf" value="${csrf}">` +
        `<input type="submit" value="Delete" class="button">` +
        `</form>`;
}

function saveEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;

    const savePath    = row.getAttribute('data-save-path');
    const minCount    = row.querySelector('input[name="min_count"]').value;
    const maxCount    = row.querySelector('input[name="max_count"]').value;
    const autoApprove = row.querySelector('input[name="auto_approve"]').checked;
    const discordRole = row.querySelector('input[name="discord_role_id"]').value;

    const formData = new FormData();
    formData.append('csrf', getCsrf());
    formData.append('min_count', minCount);
    formData.append('max_count', maxCount);
    formData.append('auto_approve', autoApprove);
    formData.append('discord_role_id', discordRole);

    fetch(savePath, { method: 'POST', body: formData })
        .then(response => {
            if (response.ok) {
                row.querySelector('.min-count').setAttribute('data-value', minCount);
                row.querySelector('.max-count').setAttribute('data-value', maxCount);
                row.querySelector('.auto-approve').setAttribute('data-value', autoApprove.toString());
                row.querySelector('.discord-role').setAttribute('data-value', discordRole);
                cancelEdit(bindingId);
            } else {
                alert('Failed to save changes. Please try again.');
            }
        })
        .catch(() => alert('Failed to save changes. Please try again.'));
}

// ── Override edit (game binding rows in event context) ───────────────────────

function startOverrideEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;

    // Save current cell HTML for cancel restoration
    const cells = ['min-count', 'max-count', 'discord-role'];
    cells.forEach(cls => {
        const cell = row.querySelector(`.${cls}`);
        if (cell) cell._savedHtml = cell.innerHTML;
    });
    const actionsDiv = row.querySelector('.actions');
    if (actionsDiv) actionsDiv._savedHtml = actionsDiv.innerHTML;

    const overrideDiscordRole = row.getAttribute('data-override-discord-role-id') || '';
    const overrideMinCount    = row.getAttribute('data-override-min-count') || '';
    const overrideMaxCount    = row.getAttribute('data-override-max-count') || '';
    const savePath = row.getAttribute('data-override-save-path');
    const formId = `override-form-${bindingId}`;

    // Inputs live in separate <td>s, so a wrapping <form> won't reach them all;
    // associate them with a hidden <form> elsewhere via the HTML5 form="" attribute
    // instead, same as every other server-rendered form on this page.
    row.querySelector('.discord-role').innerHTML =
        `<input type="text" name="discord_role_id" form="${formId}" value="${overrideDiscordRole}" placeholder="leave blank to inherit" style="width: 200px;">`;
    row.querySelector('.min-count').innerHTML =
        `<input type="number" name="min_count" form="${formId}" value="${overrideMinCount}" min="1" placeholder="inherit" style="width: 70px;">`;
    row.querySelector('.max-count').innerHTML =
        `<input type="number" name="max_count" form="${formId}" value="${overrideMaxCount}" min="1" placeholder="inherit" style="width: 70px;">`;

    actionsDiv.innerHTML =
        `<form id="${formId}" action="${savePath}" method="post" style="display: inline;" onsubmit="return validateOverrideForm('${formId}')">` +
        `<input type="hidden" name="csrf" value="${getCsrf()}">` +
        `<input type="hidden" name="role_binding_id" value="${bindingId}">` +
        `</form>` +
        `<button type="submit" form="${formId}" class="button save-btn">Save Override</button>` +
        `<button type="button" class="button cancel-btn" onclick="cancelOverrideEdit(${bindingId})">Cancel</button>`;
}

function validateOverrideForm(formId) {
    const form = document.getElementById(formId);
    const discordRole = form.elements['discord_role_id'].value.trim();
    const minCount    = form.elements['min_count'].value.trim();
    const maxCount    = form.elements['max_count'].value.trim();
    if (!discordRole && !minCount && !maxCount) {
        alert('At least one of Discord role ID, minimum count, or maximum count must be provided.');
        return false;
    }
    return true;
}

function cancelOverrideEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;

    const cells = ['min-count', 'max-count', 'discord-role'];
    cells.forEach(cls => {
        const cell = row.querySelector(`.${cls}`);
        if (cell && cell._savedHtml !== undefined) {
            cell.innerHTML = cell._savedHtml;
            delete cell._savedHtml;
        }
    });
    const actionsDiv = row.querySelector('.actions');
    if (actionsDiv && actionsDiv._savedHtml !== undefined) {
        actionsDiv.innerHTML = actionsDiv._savedHtml;
        delete actionsDiv._savedHtml;
    }
}

