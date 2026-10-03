document.addEventListener('DOMContentLoaded', function() {
    document.querySelectorAll('.ping-role-selection').forEach(function(container) {
        const language = document.getElementById(container.dataset.roleLanguage);
        const mode = container.querySelector('[name="role_selection"]');
        function update() {
            const selected = mode.value === 'selected';
            container.querySelector('.ping-role-choices').hidden = !selected;
            container.querySelectorAll('input[name="role_binding_ids"]').forEach(function(input) {
                const available = !language || input.parentElement.dataset.roleLanguage === language.value;
                input.parentElement.hidden = !available;
                input.disabled = !selected || !available;
            });
        }
        mode.addEventListener('change', update);
        if (language) language.addEventListener('change', update);
        container.updateRoleChoices = update;
        update();
        const form = container.closest('form');
        if (form) form.addEventListener('submit', function(event) {
            if (!validatePingRoles(container)) event.preventDefault();
        });
    });
    document.querySelectorAll('[data-ping-timezone]').forEach(function(select) {
        try {
            const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
            if (Array.from(select.options).some(option => option.value === timezone)) {
                select.value = timezone;
            }
        } catch (_) { /* Keep UTC if detection is unavailable. */ }
    });

    function toggleFields(container, visible) {
        if (!container) return;
        container.style.display = visible ? '' : 'none';
        container.querySelectorAll('input, select').forEach(field => { field.disabled = !visible; });
    }

    document.querySelectorAll('[data-ping-form-scheduled]').forEach(function(scheduledDiv) {
        const typeId = scheduledDiv.getAttribute('data-ping-form-scheduled');
        const typeSelect = document.getElementById(typeId);
        if (!typeSelect) return;
        const perRaceDiv = document.querySelector(`[data-ping-form-per-race="${typeId}"]`);
        const weeklyDiv = scheduledDiv.querySelector('[data-ping-form-weekly]');
        const intervalSelect = weeklyDiv && document.getElementById(weeklyDiv.getAttribute('data-ping-form-weekly'));
        function update() {
            const scheduled = typeSelect.value === 'scheduled';
            toggleFields(scheduledDiv, scheduled);
            toggleFields(perRaceDiv, !scheduled);
            toggleFields(weeklyDiv, scheduled && intervalSelect.value === 'weekly');
        }
        typeSelect.addEventListener('change', update);
        intervalSelect.addEventListener('change', update);
        update();
    });
});

function validatePingRoles(container) {
    if (container.querySelector('[name="role_selection"]').value === 'selected'
        && !container.querySelector('input[name="role_binding_ids"]:checked:not(:disabled)')) {
        alert('Select at least one role in the workflow’s language.');
        return false;
    }
    return true;
}

function renderScheduledEditor(row, cell) {
    cell.replaceChildren();
    function select(name, choices, value) {
        const field = document.createElement('select');
        field.name = name;
        choices.forEach(([key, text]) => field.add(new Option(text, key)));
        field.value = value;
        return field;
    }
    function input(name, type, value, min, max) {
        const field = document.createElement('input');
        field.name = name;
        field.type = type;
        field.value = value;
        field.required = true;
        if (min !== undefined) field.min = min;
        if (max !== undefined) field.max = max;
        return field;
    }
    function label(text, field) {
        const wrapper = document.createElement('label');
        wrapper.append(text + ' ', field);
        cell.append(wrapper, document.createElement('br'));
    }
    const interval = select('ping_interval', [['daily', 'Daily'], ['weekly', 'Weekly']], row.dataset.interval || 'daily');
    const time = input('schedule_time', 'time', row.dataset.scheduleTime || '18:00');
    const timezone = document.querySelector('.ping-timezone-picker').cloneNode(true);
    timezone.removeAttribute('id');
    timezone.removeAttribute('data-ping-timezone');
    timezone.disabled = false;
    timezone.value = row.dataset.scheduleTimezone || 'UTC';
    const day = input('schedule_day_of_week', 'number', row.dataset.scheduleDow || '0', 0, 6);
    const hours = input('cutoff_hours', 'number', row.dataset.cutoffHours || '', 1, 168);
    hours.required = false;
    hours.placeholder = 'Uses event request lead time';
    label('Interval:', interval);
    label('Ping time:', time);
    label('Timezone:', timezone);
    label('Weekday (0=Mon..6=Sun):', day);
    label('Race window (hours after ping):', hours);
    function update() {
        day.disabled = interval.value !== 'weekly';
        day.parentElement.style.display = day.disabled ? 'none' : '';
    }
    interval.addEventListener('change', update);
    update();
}

function scheduledWorkflowText(row) {
    let text = `${row.dataset.scheduleTime} ${row.dataset.scheduleTimezone || 'UTC'} (${row.dataset.interval}`;
    if (row.dataset.interval === 'weekly') text += `, day ${row.dataset.scheduleDow}`;
    text += ') — ';
    if (row.dataset.cutoffHours) return text + `Next ${row.dataset.cutoffHours} hours`;
    return text + 'Uses event request lead time';
}

function startEditWorkflow(id) {
    const row = document.querySelector(`tr[data-workflow-id="${id}"]`);
    if (!row) return;

    row.querySelector('.wf-role-summary').hidden = true;
    row.querySelector('.wf-role-editor').style.display = '';

    const type = row.getAttribute('data-type');
    const channel = row.querySelector('.wf-channel').getAttribute('data-value');
    const deleteAfterRace = row.querySelector('.wf-delete-after').getAttribute('data-value');

    // Replace channel cell
    row.querySelector('.wf-channel').innerHTML =
        `<input type="text" name="discord_ping_channel" value="${channel}" placeholder="channel ID" style="width:180px;">`;

    // Replace delete-after cell
    row.querySelector('.wf-delete-after').innerHTML =
        `<input type="checkbox" name="delete_after_race" ${deleteAfterRace === 'true' ? 'checked' : ''}>`;

    // Replace details cell based on type
    const detailsCell = row.querySelector('.wf-details');
    if (type === 'scheduled') {
        renderScheduledEditor(row, detailsCell);
    } else {
        const leadTimes = row.getAttribute('data-lead-times') || '';
        detailsCell.innerHTML =
            `<input type="text" name="lead_times" value="${leadTimes}" placeholder="e.g. 24,48,72" style="width:150px;"> hours (comma-separated)`;
    }

    // Replace actions cell
    const actionsDiv = row.querySelector('.wf-actions');
    actionsDiv.innerHTML =
        `<button class="button save-btn" onclick="saveEditWorkflow(${id})">Save</button> ` +
        `<button class="button cancel-btn" onclick="cancelEditWorkflow(${id})">Cancel</button>`;
}

function cancelEditWorkflow(id) {
    const row = document.querySelector(`tr[data-workflow-id="${id}"]`);
    if (!row) return;

    const roleCell = row.querySelector('.wf-roles');
    const roleMode = roleCell.querySelector('[name="role_selection"]');
    roleMode.value = Array.from(roleMode.options).find(option => option.defaultSelected).value;
    roleCell.querySelectorAll('input[name="role_binding_ids"]').forEach(input => {
        input.checked = input.defaultChecked;
    });
    roleCell.querySelector('.ping-role-selection').updateRoleChoices();
    roleCell.querySelector('.wf-role-editor').style.display = 'none';
    roleCell.querySelector('.wf-role-summary').hidden = false;

    const type = row.getAttribute('data-type');
    const channel = row.querySelector('.wf-channel').getAttribute('data-value');
    const deleteAfterRace = row.querySelector('.wf-delete-after').getAttribute('data-value');

    // Restore channel cell
    row.querySelector('.wf-channel').textContent = channel || 'Uses volunteer info channel';

    // Restore delete-after cell
    row.querySelector('.wf-delete-after').innerHTML = deleteAfterRace === 'true'
        ? '<span style="color: green;">✓ Yes</span>'
        : '<span style="color: red;">✗ No</span>';

    // Restore details cell
    const detailsCell = row.querySelector('.wf-details');
    if (type === 'scheduled') {
        detailsCell.textContent = scheduledWorkflowText(row);
    } else {
        const leadTimes = row.getAttribute('data-lead-times') || '';
        detailsCell.textContent = leadTimes ? `Lead times: ${leadTimes}h` : 'No lead times configured';
    }

    // Restore actions
    restoreWorkflowActions(row, id);
}

function saveEditWorkflow(id) {
    const row = document.querySelector(`tr[data-workflow-id="${id}"]`);
    if (!row) return;

    const type = row.getAttribute('data-type');
    const editPath = row.getAttribute('data-edit-path');
    const csrf = document.querySelector('input[name="csrf"]').value;

    const formData = new FormData();
    formData.append('csrf', csrf);

    const roleCell = row.querySelector('.wf-roles');
    if (!validatePingRoles(roleCell)) return;
    formData.append('role_selection', roleCell.querySelector('[name="role_selection"]').value);
    roleCell.querySelectorAll('input[name="role_binding_ids"]:checked:not(:disabled)').forEach(input => {
        formData.append('role_binding_ids', input.value);
    });

    const channelInput = row.querySelector('input[name="discord_ping_channel"]');
    formData.append('discord_ping_channel', channelInput ? channelInput.value : '');

    const deleteInput = row.querySelector('input[name="delete_after_race"]');
    formData.append('delete_after_race', deleteInput ? deleteInput.checked : false);

    if (type === 'scheduled') {
        for (const name of ['ping_interval', 'schedule_time', 'schedule_timezone', 'schedule_day_of_week',
            'cutoff_hours']) {
            const field = row.querySelector(`[name="${name}"]`);
            if (!field.disabled && !field.reportValidity()) return;
            formData.append(name, field.disabled ? '' : field.value);
        }
    } else {
        const ltInput = row.querySelector('input[name="lead_times"]');
        formData.append('lead_times', ltInput ? ltInput.value : '');
    }

    fetch(editPath, { method: 'POST', body: formData })
        .then(response => {
            if (response.ok) {
                const allRoles = formData.get('role_selection') === 'all';
                roleCell.querySelectorAll('[name="role_selection"] option').forEach(option => {
                    option.defaultSelected = option.value === formData.get('role_selection');
                });
                const selectedIds = formData.getAll('role_binding_ids');
                const names = [];
                roleCell.querySelectorAll('input[name="role_binding_ids"]').forEach(input => {
                    input.defaultChecked = selectedIds.includes(input.value);
                    if (input.defaultChecked) names.push(input.parentElement.textContent.trim());
                });
                roleCell.querySelector('.wf-role-summary').textContent = allRoles
                    ? 'All roles in this language' : names.join(', ');
                // Update data attributes with new values
                const newChannel = formData.get('discord_ping_channel');
                const newDeleteAfter = formData.get('delete_after_race') === 'true'
                    || formData.get('delete_after_race') === true;

                row.querySelector('.wf-channel').setAttribute('data-value', newChannel);
                row.querySelector('.wf-delete-after').setAttribute('data-value', newDeleteAfter.toString());

                if (type === 'scheduled') {
                    row.setAttribute('data-interval', formData.get('ping_interval'));
                    row.setAttribute('data-schedule-time', formData.get('schedule_time'));
                    row.setAttribute('data-schedule-dow', formData.get('schedule_day_of_week'));
                    row.setAttribute('data-schedule-timezone', formData.get('schedule_timezone'));
                    row.setAttribute('data-cutoff-hours', formData.get('cutoff_hours'));
                } else {
                    row.setAttribute('data-lead-times', formData.get('lead_times'));
                }

                cancelEditWorkflow(id); // re-renders display from updated data attrs
            } else {
                alert('Failed to save changes. Please try again.');
            }
        })
        .catch(() => {
            alert('Failed to save changes. Please try again.');
        });
}

function restoreWorkflowActions(row, id) {
    const deletePath = row.getAttribute('data-delete-path');
    const csrf = document.querySelector('input[name="csrf"]').value;
    const actionsDiv = row.querySelector('.wf-actions');
    actionsDiv.innerHTML =
        `<button class="button edit-btn" onclick="startEditWorkflow(${id})">Edit</button> ` +
        `<form action="${deletePath}" method="post" style="display:inline;">` +
        `<input type="hidden" name="csrf" value="${csrf}">` +
        `<input type="submit" value="Delete" class="button">` +
        `</form>`;
}
