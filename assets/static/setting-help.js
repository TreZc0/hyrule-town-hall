document.addEventListener('DOMContentLoaded', function () {
    const triggers = document.querySelectorAll('.setting-help-trigger');
    let active = null;
    let frame = null;

    function position() {
        frame = null;
        if (!active || !active.panel.matches(':popover-open')) return;
        const {panel, trigger} = active;
        const bounds = trigger.getBoundingClientRect();
        const width = document.documentElement.clientWidth;
        const height = window.innerHeight;
        const margin = 12;
        panel.style.transform = 'none';
        const box = panel.getBoundingClientRect();
        const left = Math.max(margin, Math.min(bounds.left, width - box.width - margin));
        let top = bounds.bottom + 8;
        if (top + box.height > height - margin) top = bounds.top - box.height - 8;
        top = Math.max(margin, Math.min(top, height - box.height - margin));
        panel.style.left = `${left}px`;
        panel.style.top = `${top}px`;
    }

    function queuePosition() {
        if (active && frame === null) frame = requestAnimationFrame(position);
    }

    for (const trigger of triggers) {
        const panel = document.getElementById(trigger.getAttribute('popovertarget'));
        if (!panel) continue;
        trigger.setAttribute('aria-expanded', 'false');
        panel.addEventListener('toggle', function (event) {
            const open = event.newState === 'open';
            trigger.setAttribute('aria-expanded', String(open));
            if (open) {
                active = {panel, trigger};
                position();
            } else if (active?.panel === panel) {
                active = null;
            }
        });
        panel.querySelector('.setting-help-close').addEventListener('click', function () {
            trigger.focus({preventScroll: true});
        });
    }

    window.addEventListener('resize', queuePosition);
    window.addEventListener('scroll', queuePosition, true);
});
