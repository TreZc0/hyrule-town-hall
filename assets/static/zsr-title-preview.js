function buildZsrTitle(eventName, phase, round, includePhase) {
    const matchup = 'Player1 vs Player2';
    if (includePhase && phase) {
        const shortPhase = phase.includes(' - ') ? phase.split(' - ')[0] : phase;
        if (round) {
            return `${eventName}: ${shortPhase} ${round.replace('Round ', 'R')} - ${matchup}`;
        }
        return `${eventName}: ${shortPhase} - ${matchup}`;
    }
    if (round) return `${eventName}: ${round} - ${matchup}`;
    if (phase) return `${eventName}: ${phase} - ${matchup}`;
    return `${eventName}: ${matchup}`;
}

document.addEventListener('DOMContentLoaded', function() {
    const exportPreviewEl = document.getElementById('zsr-title-preview');
    if (exportPreviewEl) {
        const defaultEventName = exportPreviewEl.getAttribute('data-event-name');
        const samplePhase = exportPreviewEl.getAttribute('data-sample-phase') || '';
        const sampleRound = exportPreviewEl.getAttribute('data-sample-round') || '';
        const titleInput = document.getElementById('title');
        const includePhaseCheckbox = document.getElementById('include_phase');

        function updateExport() {
            const customTitle = titleInput ? titleInput.value.trim() : '';
            const includePhase = includePhaseCheckbox ? includePhaseCheckbox.checked : false;
            exportPreviewEl.textContent = buildZsrTitle(customTitle || defaultEventName, samplePhase, sampleRound, includePhase);
        }

        if (titleInput) titleInput.addEventListener('input', updateExport);
        if (includePhaseCheckbox) includePhaseCheckbox.addEventListener('change', updateExport);
        updateExport();
    }

    const labelPreviewEl = document.getElementById('round-label-preview');
    if (labelPreviewEl) {
        const eventName = labelPreviewEl.getAttribute('data-event-name');
        const phaseInput = document.getElementById('mapped_phase');
        const roundInput = document.getElementById('mapped_round');

        function updateLabel() {
            const phase = phaseInput ? phaseInput.value.trim() : '';
            const round = roundInput ? roundInput.value.trim() : '';

            if (!phase && !round) {
                labelPreviewEl.textContent = '(enter mapped phase/round values above to see preview)';
                return;
            }

            const standard = buildZsrTitle(eventName, phase, round, false);
            const withPhase = buildZsrTitle(eventName, phase, round, true);

            if (standard === withPhase) {
                labelPreviewEl.innerHTML = '<em>Example title:</em> ' + standard;
            } else {
                labelPreviewEl.innerHTML =
                    '<em>Standard:</em> ' + standard +
                    '<br><em>With include_phase:</em> ' + withPhase;
            }
        }

        if (phaseInput) phaseInput.addEventListener('input', updateLabel);
        if (roundInput) roundInput.addEventListener('input', updateLabel);
        updateLabel();
    }
});
