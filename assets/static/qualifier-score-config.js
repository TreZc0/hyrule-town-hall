document.addEventListener('DOMContentLoaded', function () {
    const kind = document.getElementById('qualifier_score_kind');
    const parameters = document.getElementById('qualifier_score_config');
    const jsonEditor = document.getElementById('qualifier-score-json');
    if (!kind || !parameters || !jsonEditor) return;

    const fields = document.createElement('div');
    fields.id = 'qualifier-score-fields';
    jsonEditor.before(fields);
    const saved = new Map();
    let currentKind = kind.value;

    function defaults(fresh) {
        const option = kind.selectedOptions[0];
        return JSON.parse((fresh ? option?.dataset.scoreNewDefaults : option?.dataset.scoreDefaults) || '{}');
    }

    function render() {
        fields.replaceChildren();
        fields.hidden = true;
        jsonEditor.hidden = false;
        if (kind.value !== 'time_relative') return;

        let config;
        try {
            const stored = JSON.parse(parameters.value || '{}');
            if (!stored || typeof stored !== 'object' || Array.isArray(stored)) return;
            const base = defaults(false);
            if (Object.keys(stored).some(key => !(key in base))) return;
            config = { ...base, ...stored };
        } catch (_) {
            // Keep malformed submitted JSON editable, alongside the server's error.
            return;
        }
        fields.hidden = false;
        jsonEditor.hidden = true;
        const explanation = document.createElement('p');
        explanation.className = 'help';
        explanation.textContent = 'Par is the average time of the fastest eligible finishers below. The first “max count” attempts supply your results. After optionally scrapping the best, the highest remaining scores are selected, up to “max results counted.” New configurations average those selected scores. DNFs score 0; results awaiting par remain pending.';
        fields.append(explanation);

        const controls = new Map();
        function add(parent, key, title, type, options = {}) {
            const row = document.createElement('div');
            row.style.marginBottom = '0.75em';
            const label = document.createElement('label');
            const input = document.createElement(type === 'select' ? 'select' : 'input');
            input.id = `qualifier-score-${key}`;
            label.htmlFor = input.id;
            label.textContent = title;
            if (type === 'select') {
                for (const [value, text] of options.choices) {
                    const option = document.createElement('option');
                    option.value = value;
                    option.textContent = text;
                    input.append(option);
                }
            } else {
                input.type = type;
            }
            if (type === 'checkbox') {
                input.checked = config[key];
                input.style.marginRight = '0.4em';
                row.append(input, label);
            } else {
                input.value = config[key] ?? '';
                input.required = !!options.required;
                if (type === 'number') {
                    input.step = options.step || '1';
                    if (options.min !== undefined) input.min = options.min;
                    if (options.max !== undefined) input.max = options.max;
                }
                row.append(label, document.createElement('br'), input);
            }
            if (options.help) {
                const help = document.createElement('p');
                help.className = 'help';
                help.textContent = options.help;
                row.append(help);
            }
            input.addEventListener('input', sync);
            parent.append(row);
            controls.set(key, input);
            return row;
        }

        add(fields, 'required_finishes', 'Min count', 'number', { required: true, min: 1, help: 'Minimum finishes required to qualify, before scrapping the best result.' });
        add(fields, 'counted_attempts', 'Max count', 'number', { required: true, min: 1, help: 'Only the first this many attempts count, including DNFs.' });
        add(fields, 'best_results', 'Max results counted', 'number', { required: true, min: 1, help: 'Maximum number of best remaining results to average, after optionally scrapping the best result.' });
        add(fields, 'par_finishers', 'Players counting towards par time', 'number', { required: true, min: 1, max: 255, help: 'Number of fastest eligible finishers to average. Scores stay pending until enough have finished.' });
        add(fields, 'ineligible_players_count_towards_average', 'Ineligible players count towards average', 'checkbox', { help: 'Include racers who already used their max count in later races’ par time. Their own later scores still do not count.' });
        add(fields, 'best_result_is_scrapped', 'Best result is scrapped', 'checkbox', { help: 'Remove exactly one highest score before calculating the overall average. Min count must be at least 2.' });
        add(fields, 'minimum', 'Min points (optional, default 100)', 'number', { min: 0.000001, step: 'any' });
        add(fields, 'maximum', 'Max points (optional, default 1100)', 'number', { min: 0.000001, step: 'any' });

        const advanced = document.createElement('details');
        const summary = document.createElement('summary');
        summary.textContent = 'Advanced formula and legacy aggregation';
        advanced.append(summary);
        fields.append(advanced);
        add(advanced, 'aggregation', 'Overall score', 'select', { choices: [
            ['average_counted', 'Average counted results'],
            ['sum', 'Sum best results (legacy)'],
            ['average', 'Average best results, with missing results as zero (legacy)'],
        ] });
        add(advanced, 'scale', 'Scale', 'number', { required: true, min: 0.000001, step: 'any' });
        add(advanced, 'offset', 'Offset', 'number', { required: true, step: 'any' });
        add(advanced, 'rounding', 'Rounding', 'select', { choices: [['none', 'None'], ['floor', 'Floor'], ['nearest', 'Nearest']] });
        add(advanced, 'extrapolation_score', 'Score used for qualification projections without max points', 'number', { required: true, min: 0.000001, step: 'any' });

        function sync() {
            for (const [key, input] of controls) {
                if (input.type === 'checkbox') config[key] = input.checked;
                else if (input.type === 'number') {
                    config[key] = input.value === '' ? null : Number(input.value);
                } else config[key] = input.value;
            }
            config.minimum ??= 100;
            if (config.aggregation === 'average_counted') config.maximum ??= 1100;
            controls.get('maximum').labels[0].textContent = config.aggregation === 'average_counted'
                ? 'Max points (optional, default 1100)' : 'Max points (optional, blank means no maximum)';
            controls.get('required_finishes').setCustomValidity(
                config.required_finishes > config.counted_attempts ? 'Min count cannot exceed max count.' :
                config.best_result_is_scrapped && config.required_finishes < 2 ? 'Scrapping the best result requires min count of at least 2.' : ''
            );
            controls.get('best_results').setCustomValidity(
                config.best_results > config.counted_attempts ? 'Max results counted cannot exceed max count.' : ''
            );
            controls.get('maximum').setCustomValidity(
                config.maximum !== null && config.maximum < config.minimum ? 'Max points cannot be less than min points.' : ''
            );
            parameters.value = JSON.stringify(config, null, 2);
        }
        // Normalize omitted properties without changing existing legacy settings.
        sync();
    }

    if (!parameters.value.trim()) parameters.value = JSON.stringify(defaults(true), null, 2);
    render();
    kind.addEventListener('change', () => {
        saved.set(currentKind, parameters.value);
        currentKind = kind.value;
        parameters.value = saved.get(currentKind) ?? JSON.stringify(defaults(true), null, 2);
        render();
    });
});
