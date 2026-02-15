document.addEventListener('DOMContentLoaded', function() {
    // Cache for storing fetched suggestions
    const cache = {
        restreamers: null,
        videoUrls: null
    };

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
            fieldType: fieldType
        };

        fieldStates.set(input, state);

        // Attach event listeners
        input.addEventListener('input', () => handleInput(state));
        input.addEventListener('keydown', (e) => handleKeydown(e, state));
        input.addEventListener('focus', () => handleFocus(state));
    }

    async function handleInput(state) {
        const query = state.input.value.trim().toLowerCase();

        // Load suggestions on first input if not cached
        if (!cache[state.fieldType]) {
            await loadSuggestions(state.fieldType);
        }

        // Filter and display suggestions
        filterAndDisplaySuggestions(state, query);
    }

    async function handleFocus(state) {
        // Load suggestions when field is focused if not already loaded
        if (!cache[state.fieldType]) {
            await loadSuggestions(state.fieldType);
        }

        // Show all suggestions if input is empty
        const query = state.input.value.trim().toLowerCase();
        filterAndDisplaySuggestions(state, query);
    }

    async function loadSuggestions(fieldType) {
        try {
            const endpoint = fieldType === 'restreamers'
                ? '/api/restreamers/suggestions'
                : '/api/video-urls/suggestions';

            const response = await fetch(endpoint);
            const data = await response.json();
            cache[fieldType] = data;
        } catch (error) {
            console.error(`Error loading ${fieldType} suggestions:`, error);
            cache[fieldType] = [];
        }
    }

    function filterAndDisplaySuggestions(state, query) {
        const suggestions = cache[state.fieldType] || [];
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
                div.addEventListener('click', () => selectSuggestion(state, item));
                state.container.appendChild(div);
            });
            state.container.style.display = 'block';
        } else {
            state.container.style.display = 'none';
        }
    }

    function selectSuggestion(state, value) {
        state.input.value = value;
        state.container.style.display = 'none';
        state.currentFocus = -1;
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
