(function () {
    const customTeams = document.getElementById('multi_teams');
    if (!customTeams) return;

    const standardTeams = ['team1', 'team2', 'team3']
        .map((name) => document.querySelector(`[name="${name}"]`))
        .filter(Boolean);
    const customTitle = document.querySelector('[name="custom_title"]');
    const modeFields = document.getElementById('custom-mode-fields');
    const modeTemplate = modeFields?.querySelector('select')?.cloneNode(true);
    const gameCount = document.querySelector('[name="game_count"]');
    let standardGameCount = gameCount?.value;
    const modeValues = [];

    function syncModes(custom, count) {
        if (!modeTemplate) return;
        modeFields.querySelectorAll('select').forEach((select, index) => {
            modeValues[index] = select.value;
        });
        const fields = [];
        for (let index = 0; index < count; index++) {
            const field = document.createElement('div');
            field.className = 'custom-mode-field';
            const select = modeTemplate.cloneNode(true);
            select.id = `custom_baseline_${index + 1}`;
            select.value = modeValues[index] || '';
            select.disabled = !custom;
            select.required = false;
            const label = document.createElement('label');
            label.htmlFor = select.id;
            label.textContent = count === 1 ? 'Mode:' : `Game ${index + 1} mode:`;
            field.append(label, select);
            fields.push(field);
        }
        modeFields.replaceChildren(...fields);
    }

    function syncCustomRace() {
        const match = customTeamsSelected();
        const custom = match || Boolean(customTitle?.value.trim());
        const open = custom && !match;
        if (gameCount) {
            if (open && !gameCount.disabled) {
                standardGameCount = gameCount.value;
                gameCount.value = '1';
            } else if (!open && gameCount.disabled) {
                gameCount.value = standardGameCount;
            }
            gameCount.disabled = open;
        }
        const count = Math.min(255, Math.max(1, Number.parseInt(gameCount?.value, 10) || 1));
        syncModes(custom, count);
    }

    function customTeamsSelected() {
        return Array.from(customTeams.options).some((option) => option.selected);
    }

    function syncMatchupFields(clearStandardTeams) {
        const custom = customTeamsSelected();
        for (const select of standardTeams) {
            if (custom && clearStandardTeams) select.value = '';
            select.disabled = custom;
        }
        syncCustomRace();
    }

    customTitle?.addEventListener('input', syncCustomRace);
    gameCount?.addEventListener('input', syncCustomRace);
    customTeams.addEventListener('change', () => syncMatchupFields(true));
    for (const select of standardTeams) {
        select.addEventListener('change', () => {
            if (select.value) {
                for (const option of customTeams.options) option.selected = false;
                syncMatchupFields(false);
            }
        });
    }
    syncMatchupFields(true);
})();
