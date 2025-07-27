function startEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;
    
    // Store original values
    const minCount = row.querySelector('.min-count').getAttribute('data-value');
    const maxCount = row.querySelector('.max-count').getAttribute('data-value');
    const autoApprove = row.querySelector('.auto-approve').getAttribute('data-value');
    const discordRole = row.querySelector('.discord-role').getAttribute('data-value');
    
    // Replace cells with input fields
    row.querySelector('.min-count').innerHTML = `<input type="number" name="min_count" value="${minCount}" min="1" style="width: 60px;">`;
    row.querySelector('.max-count').innerHTML = `<input type="number" name="max_count" value="${maxCount}" min="1" style="width: 60px;">`;
    row.querySelector('.auto-approve').innerHTML = `<input type="checkbox" name="auto_approve" ${autoApprove === 'true' ? 'checked' : ''}>`;
    row.querySelector('.discord-role').innerHTML = `<input type="text" name="discord_role_id" value="${discordRole}" placeholder="e.g. 123456789012345678" style="width: 150px;">`;
    
    // Replace edit button with save/cancel buttons
    const actionsDiv = row.querySelector('.actions');
    actionsDiv.innerHTML = `
        <button class="button save-btn" onclick="saveEdit(${bindingId})">Save</button>
        <button class="button cancel-btn" onclick="cancelEdit(${bindingId})">Cancel</button>
    `;
}

function cancelEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;
    
    // Restore original values
    const minCount = row.querySelector('.min-count').getAttribute('data-value');
    const maxCount = row.querySelector('.max-count').getAttribute('data-value');
    const autoApprove = row.querySelector('.auto-approve').getAttribute('data-value');
    const discordRole = row.querySelector('.discord-role').getAttribute('data-value');
    
    row.querySelector('.min-count').textContent = minCount;
    row.querySelector('.max-count').textContent = maxCount;
    row.querySelector('.auto-approve').innerHTML = autoApprove === 'true' ? 
        '<span style="color: green;">✓ Yes</span>' : 
        '<span style="color: red;">✗ No</span>';
    row.querySelector('.discord-role').textContent = discordRole || 'None';
    
    // Restore edit button
    const actionsDiv = row.querySelector('.actions');
    const currentPath = window.location.pathname;
    const basePath = currentPath.replace('/roles', '');
    actionsDiv.innerHTML = `
        <button class="button edit-btn" onclick="startEdit(${bindingId})">Edit</button>
        <form action="${basePath}/roles/binding/${bindingId}/delete" method="post" style="display: inline;">
            <input type="hidden" name="csrf" value="' + document.querySelector('input[name="csrf"]').value + '">
            <input type="submit" value="Delete" class="button">
        </form>
    `;
}

function saveEdit(bindingId) {
    const row = document.querySelector(`tr[data-binding-id="${bindingId}"]`);
    if (!row) return;
    
    const minCount = row.querySelector('input[name="min_count"]').value;
    const maxCount = row.querySelector('input[name="max_count"]').value;
    const autoApprove = row.querySelector('input[name="auto_approve"]').checked;
    const discordRole = row.querySelector('input[name="discord_role_id"]').value;
    
    // Create form data
    const formData = new FormData();
    formData.append('csrf', document.querySelector('input[name="csrf"]').value);
    formData.append('min_count', minCount);
    formData.append('max_count', maxCount);
    formData.append('auto_approve', autoApprove);
    formData.append('discord_role_id', discordRole);
    
    // Submit form
    const currentPath = window.location.pathname;
    const basePath = currentPath.replace('/roles', '');
    fetch(`${basePath}/roles/binding/${bindingId}/edit`, {
        method: 'POST',
        body: formData
    })
    .then(response => {
        if (response.ok) {
            // Update data attributes and display values
            row.querySelector('.min-count').setAttribute('data-value', minCount);
            row.querySelector('.max-count').setAttribute('data-value', maxCount);
            row.querySelector('.auto-approve').setAttribute('data-value', autoApprove.toString());
            row.querySelector('.discord-role').setAttribute('data-value', discordRole);
            
            // Update display
            row.querySelector('.min-count').textContent = minCount;
            row.querySelector('.max-count').textContent = maxCount;
            row.querySelector('.auto-approve').innerHTML = autoApprove ? 
                '<span style="color: green;">✓ Yes</span>' : 
                '<span style="color: red;">✗ No</span>';
            row.querySelector('.discord-role').textContent = discordRole || 'None';
            
            // Restore edit button
            const actionsDiv = row.querySelector('.actions');
            actionsDiv.innerHTML = `
                <button class="button edit-btn" onclick="startEdit(${bindingId})">Edit</button>
                <form action="${basePath}/roles/binding/${bindingId}/delete" method="post" style="display: inline;">
                    <input type="hidden" name="csrf" value="' + document.querySelector('input[name="csrf"]').value + '">
                    <input type="submit" value="Delete" class="button">
                </form>
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