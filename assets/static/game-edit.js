function startEditGame(gameName) {
    const row = document.querySelector(`tr[data-game-name="${gameName}"]`);
    if (!row) return;

    const displayName = row.querySelector('.game-display-name').getAttribute('data-value');
    const description = row.querySelector('.game-description').getAttribute('data-value');

    row.querySelector('.game-display-name').innerHTML = `<input type="text" name="display_name" value="${displayName}" style="width: 200px;">`;
    row.querySelector('.game-description').innerHTML = `<input type="text" name="description" value="${description}" style="width: 300px;">`;

    const actionsDiv = row.querySelector('.actions');
    actionsDiv.innerHTML = `
        <button class="button save-btn" onclick="saveEditGame('${gameName}')">Save</button>
        <button class="button cancel-btn" onclick="cancelEditGame('${gameName}')">Cancel</button>
    `;
}

function cancelEditGame(gameName) {
    const row = document.querySelector(`tr[data-game-name="${gameName}"]`);
    if (!row) return;

    const displayName = row.querySelector('.game-display-name').getAttribute('data-value');
    const description = row.querySelector('.game-description').getAttribute('data-value');

    row.querySelector('.game-display-name').textContent = displayName;
    row.querySelector('.game-description').textContent = description;

    const actionsDiv = row.querySelector('.actions');
    actionsDiv.innerHTML = `
        <button class="button edit-btn" onclick="startEditGame('${gameName}')">Edit</button>
        <a href="/games/${gameName}">Manage</a>
    `;
}

function saveEditGame(gameName) {
    const row = document.querySelector(`tr[data-game-name="${gameName}"]`);
    if (!row) return;

    const displayName = row.querySelector('input[name="display_name"]').value;
    const description = row.querySelector('input[name="description"]').value;

    const formData = new FormData();
    formData.append('csrf', document.querySelector('input[name="csrf"]').value);
    formData.append('display_name', displayName);
    formData.append('description', description);

    fetch(`/admin/game/${gameName}/edit`, {
        method: 'POST',
        body: formData,
    })
    .then(response => {
        if (response.ok) {
            row.querySelector('.game-display-name').setAttribute('data-value', displayName);
            row.querySelector('.game-description').setAttribute('data-value', description);
            row.querySelector('.game-display-name').textContent = displayName;
            row.querySelector('.game-description').textContent = description;

            const actionsDiv = row.querySelector('.actions');
            actionsDiv.innerHTML = `
                <button class="button edit-btn" onclick="startEditGame('${gameName}')">Edit</button>
                <a href="/games/${gameName}">Manage</a>
            `;
        } else {
            alert('Failed to save changes. Please try again.');
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('Failed to save changes. Please try again.');
    });
}
