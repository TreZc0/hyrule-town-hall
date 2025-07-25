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
});
