(function() {
    var initialContent = window.__initialContent || '';

    var icons = Quill.import('ui/icons');
    icons['organizers'] = '\u{1F465}';

    var quill = new Quill('#editor-container', {
        theme: 'snow',
        modules: {
            toolbar: {
                container: [
                    ['bold', 'italic', 'underline', 'strike'],
                    [{ 'header': [1, 2, 3, false] }],
                    ['link'],
                    [{ 'list': 'ordered' }, { 'list': 'bullet' }],
                    ['organizers']
                ],
                handlers: {
                    organizers: function() {
                        var range = this.quill.getSelection(true);
                        this.quill.insertText(range.index, '{{organizers}}', Quill.sources.USER);
                        this.quill.setSelection(range.index + '{{organizers}}'.length, 0);
                    }
                }
            }
        }
    });

    if (initialContent.trim()) {
        quill.clipboard.dangerouslyPasteHTML(initialContent);
    }

    var form = document.querySelector('form');
    form.addEventListener('submit', function(e) {
        var clickedReset = e.submitter && e.submitter.name === 'reset';
        if (!clickedReset) {
            document.getElementById('editor-content').value = quill.root.innerHTML;
        }
    });
})();
