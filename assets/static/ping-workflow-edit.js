document.addEventListener('DOMContentLoaded', function() {
    // Auto-wire show/hide for ping workflow add forms.
    // Divs with data-ping-form-scheduled="<typeSelectId>" are shown only when type = "scheduled".
    // Divs with data-ping-form-per-race="<typeSelectId>" are shown only when type = "per_race".
    // Divs with data-ping-form-weekly="<intervalSelectId>" are shown only when interval = "weekly".

    document.querySelectorAll('[data-ping-form-scheduled]').forEach(function(scheduledDiv) {
        const typeId = scheduledDiv.getAttribute('data-ping-form-scheduled');
        const typeSelect = document.getElementById(typeId);
        if (!typeSelect) return;

        // Find the per-race counterpart driven by the same type select
        const perRaceDiv = document.querySelector(`[data-ping-form-per-race="${typeId}"]`);

        // Find the weekly sub-div inside the scheduled div (if any)
        const weeklyDiv = scheduledDiv.querySelector('[data-ping-form-weekly]');
        const intervalSelectId = weeklyDiv ? weeklyDiv.getAttribute('data-ping-form-weekly') : null;
        const intervalSelect = intervalSelectId ? document.getElementById(intervalSelectId) : null;

        function update() {
            const isScheduled = typeSelect.value === 'scheduled';
            scheduledDiv.style.display = isScheduled ? '' : 'none';
            if (perRaceDiv) perRaceDiv.style.display = isScheduled ? 'none' : '';
            if (weeklyDiv && intervalSelect) {
                weeklyDiv.style.display = (isScheduled && intervalSelect.value === 'weekly') ? '' : 'none';
            }
        }

        typeSelect.addEventListener('change', update);
        if (intervalSelect) intervalSelect.addEventListener('change', update);
        update();
    });
});

function startEditWorkflow(id) {
    const row = document.querySelector(`tr[data-workflow-id="${id}"]`);
    if (!row) return;

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
        const interval = row.getAttribute('data-interval') || 'daily';
        const scheduleTime = row.getAttribute('data-schedule-time') || '';
        const scheduleDow = row.getAttribute('data-schedule-dow') || '';
        detailsCell.innerHTML =
            `<select name="ping_interval">` +
            `<option value="daily" ${interval === 'daily' ? 'selected' : ''}>Daily</option>` +
            `<option value="weekly" ${interval === 'weekly' ? 'selected' : ''}>Weekly</option>` +
            `</select> ` +
            `<input type="time" name="schedule_time" value="${scheduleTime}" style="width:8em;"> UTC ` +
            `<input type="number" name="schedule_day_of_week" value="${scheduleDow}" min="0" max="6" placeholder="0–6 (weekly)" style="width:5em;">`;
    } else {
        const leadTimes = row.getAttribute('data-lead-times') || '';
        detailsCell.innerHTML =
            `<input type="text" name="lead_times" value="${leadTimes}" placeholder="e.g. 24,48,72" style="width:150px;"> hours (comma-separated)`;
    }

    // Replace actions cell
    const editPath = row.getAttribute('data-edit-path');
    const actionsDiv = row.querySelector('.wf-actions');
    actionsDiv.innerHTML =
        `<button class="button save-btn" onclick="saveEditWorkflow(${id})">Save</button> ` +
        `<button class="button cancel-btn" onclick="cancelEditWorkflow(${id})">Cancel</button>`;
}

function cancelEditWorkflow(id) {
    const row = document.querySelector(`tr[data-workflow-id="${id}"]`);
    if (!row) return;

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
        const interval = row.getAttribute('data-interval') || 'daily';
        const scheduleTime = row.getAttribute('data-schedule-time') || '';
        const scheduleDow = row.getAttribute('data-schedule-dow') || '';
        let text = `${scheduleTime} UTC (${interval}`;
        if (interval === 'weekly' && scheduleDow !== '') {
            text += `, day ${scheduleDow}`;
        }
        text += ')';
        detailsCell.textContent = text;
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

    const channelInput = row.querySelector('input[name="discord_ping_channel"]');
    formData.append('discord_ping_channel', channelInput ? channelInput.value : '');

    const deleteInput = row.querySelector('input[name="delete_after_race"]');
    formData.append('delete_after_race', deleteInput ? deleteInput.checked : false);

    if (type === 'scheduled') {
        const intervalSelect = row.querySelector('select[name="ping_interval"]');
        const timeInput = row.querySelector('input[name="schedule_time"]');
        const dowInput = row.querySelector('input[name="schedule_day_of_week"]');
        formData.append('ping_interval', intervalSelect ? intervalSelect.value : 'daily');
        formData.append('schedule_time', timeInput ? timeInput.value : '');
        formData.append('schedule_day_of_week', dowInput ? dowInput.value : '');
    } else {
        const ltInput = row.querySelector('input[name="lead_times"]');
        formData.append('lead_times', ltInput ? ltInput.value : '');
    }

    fetch(editPath, { method: 'POST', body: formData })
        .then(response => {
            if (response.ok) {
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
