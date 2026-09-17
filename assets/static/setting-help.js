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
        if (trigger.hasAttribute('data-hover-help')) {
            let hoverOpened = false;
            let hideTimer;
            trigger.addEventListener('pointerenter', function (event) {
                clearTimeout(hideTimer);
                if (event.pointerType === 'mouse' && !panel.matches(':popover-open')) {
                    hoverOpened = true;
                    panel.showPopover();
                }
            });
            // Clicking a hover preview pins it open; touch and keyboard retain
            // the normal popover button behavior.
            trigger.addEventListener('click', function (event) {
                if (hoverOpened) {
                    event.preventDefault();
                    hoverOpened = false;
                }
            });
            function leave() {
                clearTimeout(hideTimer);
                hideTimer = setTimeout(function () {
                    if (hoverOpened && !trigger.matches(':hover') && !panel.matches(':hover')
                        && !panel.contains(document.activeElement)) {
                        panel.hidePopover();
                    }
                }, 180);
            }
            trigger.addEventListener('pointerleave', leave);
            panel.addEventListener('pointerleave', leave);
            panel.addEventListener('pointerenter', () => clearTimeout(hideTimer));
            panel.addEventListener('toggle', function (event) {
                if (event.newState === 'closed') {
                    hoverOpened = false;
                    clearTimeout(hideTimer);
                }
            });
        }
    }

    window.addEventListener('resize', queuePosition);
    window.addEventListener('scroll', queuePosition, true);
});
