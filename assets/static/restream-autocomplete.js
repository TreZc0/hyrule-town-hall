document.addEventListener('DOMContentLoaded', function() {
    // Cache for storing fetched video URL suggestions
    let videoUrlCache = null;

    // State for each input field
    const fieldStates = new Map();

    // Initialize autocomplete for all restream fields
    initializeAutocomplete();

    function initializeAutocomplete() {
        // Find all restreamer and video URL inputs
        const restreamerInputs = document.querySelectorAll('input[name^="restreamers."]');
        const videoUrlInputs = document.querySelectorAll('input[name^="video_urls."]');

        restreamerInputs.forEach(input => setupField(input, 'restreamers'));
        videoUrlInputs.forEach(input => setupField(input, 'videoUrls'));
    }

    function setupField(input, fieldType) {
        const fieldName = input.name.replace('.', '-');
        const suggestionContainer = document.getElementById(`suggestions-${fieldName}`);

        if (!suggestionContainer) {
            console.warn(`No suggestion container found for ${fieldName}`);
            return;
        }

        const state = {
            currentFocus: -1,
            input: input,
            container: suggestionContainer,
            fieldType: fieldType,
            searchTimeout: null
        };

        fieldStates.set(input, state);

        // Attach event listeners
        input.addEventListener('input', () => handleInput(state));
        input.addEventListener('keydown', (e) => handleKeydown(e, state));
        if (fieldType === 'videoUrls') {
            input.addEventListener('focus', () => handleFocus(state));
        }
    }

    async function handleInput(state) {
        const query = state.input.value.trim();

        if (state.fieldType === 'restreamers') {
            // For restreamers: search as you type
            handleRestreamerSearch(state, query);
        } else {
            // For video URLs: filter cached results
            handleVideoUrlFilter(state, query);
        }
    }

    async function handleFocus(state) {
        if (state.fieldType === 'videoUrls') {
            // Load video URL cache on first focus if not already loaded
            if (!videoUrlCache) {
                await loadVideoUrlSuggestions();
            }

            const query = state.input.value.trim().toLowerCase();
            filterVideoUrls(state, query);
        }
    }

    function handleRestreamerSearch(state, query) {
        // Clear existing timeout
        if (state.searchTimeout) {
            clearTimeout(state.searchTimeout);
        }

        // Minimum query length
        if (query.length < 2) {
            state.container.style.display = 'none';
            return;
        }

        // Debounce the search
        state.searchTimeout = setTimeout(() => {
            searchRestreamers(state, query);
        }, 300);
    }

    async function searchRestreamers(state, query) {
        try {
            const response = await fetch('/api/restreamers/search?query=' + encodeURIComponent(query));
            const users = await response.json();

            state.container.innerHTML = '';
            state.currentFocus = -1;

            if (users.length > 0) {
                users.forEach(user => {
                    const div = document.createElement('div');
                    div.className = 'suggestion-item';
                    div.innerHTML = `
                        <strong>${user.display_name}</strong>
                        ${user.racetime_id ? `<small>racetime.gg: ${user.racetime_id}</small>` : ''}
                        ${user.discord_username ? `<br><small>Discord: ${user.discord_username}</small>` : ''}
                    `;
                    div.addEventListener('click', () => {
                        // Use racetime_id when selecting a user
                        state.input.value = user.racetime_id || '';
                        state.container.style.display = 'none';
                        state.currentFocus = -1;
                    });
                    state.container.appendChild(div);
                });
                state.container.style.display = 'block';
            } else {
                state.container.style.display = 'none';
            }
        } catch (error) {
            console.error('Error searching restreamers:', error);
            state.container.style.display = 'none';
        }
    }

    async function handleVideoUrlFilter(state, query) {
        // Load cache on first input if not already loaded
        if (!videoUrlCache) {
            await loadVideoUrlSuggestions();
        }

        filterVideoUrls(state, query.toLowerCase());
    }

    async function loadVideoUrlSuggestions() {
        try {
            const response = await fetch('/api/video-urls/suggestions');
            const data = await response.json();
            videoUrlCache = data;
        } catch (error) {
            console.error('Error loading video URL suggestions:', error);
            videoUrlCache = [];
        }
    }

    function filterVideoUrls(state, query) {
        const suggestions = videoUrlCache || [];
        const filtered = query
            ? suggestions.filter(item => item && item.toLowerCase().includes(query))
            : suggestions;

        state.container.innerHTML = '';
        state.currentFocus = -1;

        if (filtered.length > 0) {
            filtered.forEach(item => {
                const div = document.createElement('div');
                div.className = 'suggestion-item';
                div.textContent = item;
                div.addEventListener('click', () => {
                    state.input.value = item;
                    state.container.style.display = 'none';
                    state.currentFocus = -1;
                });
                state.container.appendChild(div);
            });
            state.container.style.display = 'block';
        } else {
            state.container.style.display = 'none';
        }
    }

    function handleKeydown(e, state) {
        const items = state.container.getElementsByClassName('suggestion-item');

        if (e.key === 'ArrowDown') {
            e.preventDefault();
            state.currentFocus++;
            setActive(state, items);
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            state.currentFocus--;
            setActive(state, items);
        } else if (e.key === 'Enter') {
            if (state.currentFocus > -1 && items[state.currentFocus]) {
                e.preventDefault();
                items[state.currentFocus].click();
            }
        } else if (e.key === 'Escape') {
            state.container.style.display = 'none';
            state.currentFocus = -1;
        }
    }

    function setActive(state, items) {
        if (!items || items.length === 0) return;

        removeActive(items);

        // Wrap around
        if (state.currentFocus >= items.length) state.currentFocus = 0;
        if (state.currentFocus < 0) state.currentFocus = items.length - 1;

        items[state.currentFocus].classList.add('suggestion-active');

        // Scroll into view if needed
        items[state.currentFocus].scrollIntoView({ block: 'nearest' });
    }

    function removeActive(items) {
        for (let i = 0; i < items.length; i++) {
            items[i].classList.remove('suggestion-active');
        }
    }

    // Close suggestions when clicking outside
    document.addEventListener('click', function(e) {
        fieldStates.forEach((state, input) => {
            if (!input.contains(e.target) && !state.container.contains(e.target)) {
                state.container.style.display = 'none';
                state.currentFocus = -1;
            }
        });
    });
});
