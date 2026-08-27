(function () {
    const customTeams = document.getElementById('multi_teams');
    if (!customTeams) return;

    const standardTeams = ['team1', 'team2', 'team3']
        .map((name) => document.querySelector(`[name="${name}"]`))
        .filter(Boolean);

    function customTeamsSelected() {
        return Array.from(customTeams.options).some((option) => option.selected);
    }

    function syncMatchupFields(clearStandardTeams) {
        const custom = customTeamsSelected();
        for (const select of standardTeams) {
            if (custom && clearStandardTeams) select.value = '';
            select.disabled = custom;
        }
    }

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
