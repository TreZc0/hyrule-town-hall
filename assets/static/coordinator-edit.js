var ALL_LANGUAGES = [
    { code: 'en', name: 'English' },
    { code: 'fr', name: 'French' },
    { code: 'de', name: 'German' },
    { code: 'pt', name: 'Portuguese' },
];

function startEditCoordinator(userId) {
    var row = document.querySelector('tr[data-user-id="' + userId + '"]');
    if (!row) return;

    var currentLangs = (row.getAttribute('data-languages') || '').split(',').filter(Boolean);
    var langCell = row.querySelector('.coordinator-languages');
    var actionsCell = row.querySelector('.coordinator-actions');

    // Replace languages cell with checkboxes
    var html = '';
    for (var i = 0; i < ALL_LANGUAGES.length; i++) {
        var lang = ALL_LANGUAGES[i];
        var checked = currentLangs.indexOf(lang.code) >= 0 ? ' checked' : '';
        html += '<label style="display: block;"><input type="checkbox" name="lang_' + lang.code + '" value="' + lang.code + '"' + checked + '> ' + lang.name + '</label>';
    }
    langCell.innerHTML = html;

    // Replace actions with Save/Cancel
    actionsCell.innerHTML = '<div style="display: flex; gap: 8px;">' +
        '<button class="button" onclick="saveEditCoordinator(\'' + userId + '\')">Save</button>' +
        '<button class="button" onclick="cancelEditCoordinator(\'' + userId + '\')">Cancel</button>' +
        '</div>';
}

function cancelEditCoordinator(userId) {
    var row = document.querySelector('tr[data-user-id="' + userId + '"]');
    if (!row) return;

    var currentLangs = (row.getAttribute('data-languages') || '').split(',').filter(Boolean);
    var langCell = row.querySelector('.coordinator-languages');
    var actionsCell = row.querySelector('.coordinator-actions');

    // Restore language display
    var names = currentLangs.map(function(code) {
        for (var i = 0; i < ALL_LANGUAGES.length; i++) {
            if (ALL_LANGUAGES[i].code === code) return ALL_LANGUAGES[i].name;
        }
        return code;
    });
    langCell.textContent = names.join(', ');

    // Restore action buttons
    restoreActionButtons(actionsCell, userId);
}

function saveEditCoordinator(userId) {
    var row = document.querySelector('tr[data-user-id="' + userId + '"]');
    if (!row) return;

    var checkboxes = row.querySelectorAll('.coordinator-languages input[type="checkbox"]');
    var selectedLangs = [];
    for (var i = 0; i < checkboxes.length; i++) {
        if (checkboxes[i].checked) {
            selectedLangs.push(checkboxes[i].value);
        }
    }

    if (selectedLangs.length === 0) {
        alert('Please select at least one language.');
        return;
    }

    var formData = new FormData();
    var csrfInput = document.querySelector('input[name="csrf"]');
    if (csrfInput) {
        formData.append('csrf', csrfInput.value);
    }
    for (var i = 0; i < selectedLangs.length; i++) {
        formData.append('languages', selectedLangs[i]);
    }

    var gameName = window.location.pathname.split('/')[2];
    fetch('/games/' + gameName + '/restreamers/' + userId + '/update-languages', {
        method: 'POST',
        body: formData
    })
    .then(function(response) {
        if (response.ok || response.redirected) {
            // Update data attribute and display
            row.setAttribute('data-languages', selectedLangs.join(','));

            var langCell = row.querySelector('.coordinator-languages');
            var names = selectedLangs.map(function(code) {
                for (var i = 0; i < ALL_LANGUAGES.length; i++) {
                    if (ALL_LANGUAGES[i].code === code) return ALL_LANGUAGES[i].name;
                }
                return code;
            });
            langCell.textContent = names.join(', ');

            var actionsCell = row.querySelector('.coordinator-actions');
            restoreActionButtons(actionsCell, userId);
        } else {
            alert('Failed to save changes. Please try again.');
        }
    })
    .catch(function(error) {
        console.error('Error:', error);
        alert('Failed to save changes. Please try again.');
    });
}

function restoreActionButtons(actionsCell, userId) {
    var csrfInput = document.querySelector('input[name="csrf"]');
    var csrfValue = csrfInput ? csrfInput.value : '';
    var gameName = window.location.pathname.split('/')[2];

    actionsCell.innerHTML = '<div style="display: flex; gap: 8px;">' +
        '<button class="button" onclick="startEditCoordinator(\'' + userId + '\')">Edit</button>' +
        '<form method="post" action="/games/' + gameName + '/restreamers/' + userId + '/remove" style="display: inline;" onsubmit="return confirm(\'Are you sure you want to remove this restream coordinator?\')">' +
        '<input type="hidden" name="csrf" value="' + csrfValue + '">' +
        '<input type="submit" value="Delete" class="button">' +
        '</form>' +
        '</div>';
}
