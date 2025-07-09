document.addEventListener('DOMContentLoaded', function() {
    const input = document.getElementById('restreamer');
    const suggestions = document.getElementById('user-suggestions');
    let currentFocus = -1;
    
    input.addEventListener('input', function() {
        const query = this.value.trim();
        if (query.length < 2) {
            suggestions.style.display = 'none';
            return;
        }
        
        fetch('/api/users/search?query=' + encodeURIComponent(query))
            .then(response => response.json())
            .then(data => {
                suggestions.innerHTML = '';
                if (data.length > 0) {
                    data.forEach(user => {
                        const div = document.createElement('div');
                        div.className = 'suggestion-item';
                        div.innerHTML = `
                            <strong>${user.display_name}</strong>
                            <small>ID: ${user.id}</small>
                            ${user.racetime_id ? `<br><small>racetime.gg: ${user.racetime_id}</small>` : ''}
                            ${user.discord_username ? `<br><small>Discord: ${user.discord_username}</small>` : ''}
                        `;
                        div.addEventListener('click', function() {
                            input.value = user.id;
                            suggestions.style.display = 'none';
                        });
                        suggestions.appendChild(div);
                    });
                    suggestions.style.display = 'block';
                } else {
                    suggestions.style.display = 'none';
                }
            })
            .catch(error => {
                console.error('Error fetching suggestions:', error);
            });
    });
    
    input.addEventListener('keydown', function(e) {
        const items = suggestions.getElementsByClassName('suggestion-item');
        if (e.key === 'ArrowDown') {
            currentFocus++;
            addActive(items);
        } else if (e.key === 'ArrowUp') {
            currentFocus--;
            addActive(items);
        } else if (e.key === 'Enter') {
            e.preventDefault();
            if (currentFocus > -1) {
                if (items[currentFocus]) {
                    items[currentFocus].click();
                }
            }
        }
    });
    
    function addActive(items) {
        if (!items) return false;
        removeActive(items);
        if (currentFocus >= items.length) currentFocus = 0;
        if (currentFocus < 0) currentFocus = (items.length - 1);
        items[currentFocus].classList.add('suggestion-active');
    }
    
    function removeActive(items) {
        for (let i = 0; i < items.length; i++) {
            items[i].classList.remove('suggestion-active');
        }
    }
    
    document.addEventListener('click', function(e) {
        if (!input.contains(e.target) && !suggestions.contains(e.target)) {
            suggestions.style.display = 'none';
        }
    });
}); 