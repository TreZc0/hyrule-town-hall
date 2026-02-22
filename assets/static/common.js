document.querySelectorAll('.datetime').forEach(function(dateTime) {
    var longFormat = dateTime.dataset.long == 'true';
    dateTime.textContent = new Date(parseInt(dateTime.dataset.timestamp)).toLocaleString(['en'], {
        dateStyle: longFormat ? 'full' : 'medium',
        timeStyle: longFormat ? 'full' : 'short',
    });
});

document.querySelectorAll('.daterange').forEach(function(dateRange) {
    var start = new Date(parseInt(dateRange.dataset.start));
    var end = new Date(parseInt(dateRange.dataset.end));
    dateRange.textContent = Intl.DateTimeFormat([], {dateStyle: 'long'}).formatRange(start, end);
});

document.querySelectorAll('.recurring-time').forEach(function(recurringTime) {
    var date = new Date(parseInt(recurringTime.dataset.timestamp));
    var weekday = date.toLocaleDateString(['en'], {weekday: 'long'});
    var time = date.toLocaleTimeString(['en'], {hour: 'numeric', minute: '2-digit', timeZoneName: 'short'});
    recurringTime.textContent = weekday + ' at ' + time;
});

document.querySelectorAll('.timezone').forEach(function(timezone) {
    timezone.textContent = Intl.DateTimeFormat(['en'], {timeZoneName: 'longGeneric'}).formatToParts().find(part => part.type == 'timeZoneName').value;
});

document.querySelectorAll('.timezone-wrapper').forEach(function(timezoneWrapper) {
    timezoneWrapper.classList.remove('timezone-wrapper');
});

// Auto-detect timezone and set it in hidden timezone fields
document.addEventListener('DOMContentLoaded', function() {
    const timezoneField = document.getElementById('timezone-field');
    if (timezoneField) {
        try {
            const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
            timezoneField.value = timezone;
        } catch (e) {
            console.warn('Could not detect timezone:', e);
            timezoneField.value = 'UTC';
        }
    }

    // Update local timezone option in weekly schedule forms
    const localOption = document.getElementById('local-tz-option');
    if (localOption) {
        try {
            const userTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
            if (userTz) {
                localOption.value = userTz;
                localOption.textContent = 'Local timezone (' + userTz + ')';
            } else {
                localOption.remove();
            }
        } catch (e) {
            localOption.remove();
        }
    }

    // Fetch and populate racetime.gg goals dynamically
    const racetimeGoalSelect = document.getElementById('racetime_goal');
    const racetimeGoalCustom = document.getElementById('racetime_goal_custom');
    const racetimeGoalCustomFieldset = racetimeGoalCustom?.closest('fieldset');

    if (racetimeGoalSelect && racetimeGoalCustom) {
        // Fetch goals from racetime.gg
        const categorySlug = racetimeGoalSelect.dataset.racetimeCategory;
        const currentGoal = racetimeGoalSelect.dataset.currentGoal;

        if (categorySlug) {
            fetch(`/api/v1/racetime/${categorySlug}/goals`)
                .then(response => {
                    if (!response.ok) {
                        console.warn('Failed to fetch racetime.gg goals');
                        return null;
                    }
                    return response.json();
                })
                .then(goals => {
                    if (!goals || !Array.isArray(goals)) {
                        return;
                    }

                    // Clear existing options except the first one (None) and last one (Custom)
                    const firstOption = racetimeGoalSelect.options[0];
                    const customOption = racetimeGoalSelect.options[racetimeGoalSelect.options.length - 1];
                    racetimeGoalSelect.innerHTML = '';
                    racetimeGoalSelect.appendChild(firstOption);

                    // Add goals from racetime.gg
                    goals.forEach(goal => {
                        const option = document.createElement('option');
                        option.value = goal;
                        option.textContent = goal;
                        option.selected = currentGoal === goal;
                        racetimeGoalSelect.appendChild(option);
                    });

                    // Re-add custom option
                    racetimeGoalSelect.appendChild(customOption);
                })
                .catch(error => {
                    console.warn('Error fetching racetime.gg goals:', error);
                });
        }

        function updateGoalCustomField() {
            const isCustom = racetimeGoalSelect.value === 'custom';

            // Only hide/show — do NOT disable, since disabled fields aren't
            // submitted and Rocket will reject the form with "Missing".
            if (racetimeGoalCustomFieldset) {
                racetimeGoalCustomFieldset.style.display = isCustom ? '' : 'none';
            }
        }

        racetimeGoalSelect.addEventListener('change', updateGoalCustomField);
        updateGoalCustomField();
    }
});
