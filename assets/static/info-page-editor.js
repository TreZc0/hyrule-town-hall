(function() {
    var initialContent = window.__initialContent || '';

    var icons = Quill.import('ui/icons');
    icons['organizers'] = '\u{1F465}';
    icons['table'] = '<svg viewBox="0 0 18 18"><rect class="ql-stroke" height="10" width="12" x="3" y="4"/><line class="ql-stroke" x1="3" x2="15" y1="8.5" y2="8.5"/><line class="ql-stroke" x1="9" x2="9" y1="4" y2="14"/></svg>';
    icons['spoiler'] = '<svg viewBox="0 0 18 18"><path class="ql-stroke" d="M1 9s3-5 8-5 8 5 8 5-3 5-8 5-8-5-8-5z"/><circle class="ql-stroke" cx="9" cy="9" r="2.5"/></svg>';

    // Helper: find the direct child of quill.root that contains a given node
    function findRootChild(quill, node) {
        var el = node;
        while (el && el.parentNode !== quill.root) {
            el = el.parentNode;
        }
        return el || null;
    }

    // Helper: insert an element after the block containing the current selection
    function insertAfterCurrentBlock(quill, el) {
        var range = quill.getSelection(true);
        var leaf = quill.getLeaf(range.index)[0];
        var blockNode = leaf && findRootChild(quill, leaf.domNode);
        if (blockNode) {
            quill.root.insertBefore(el, blockNode.nextSibling);
        } else {
            quill.root.appendChild(el);
        }
    }

    var quill = new Quill('#editor-container', {
        theme: 'snow',
        modules: {
            toolbar: {
                container: [
                    ['bold', 'italic', 'underline', 'strike'],
                    [{ 'header': [1, 2, 3, false] }],
                    ['link', 'image'],
                    [{ 'list': 'ordered' }, { 'list': 'bullet' }],
                    ['table', 'spoiler', 'organizers']
                ],
                handlers: {
                    image: function() {
                        var url = prompt('Image URL:');
                        if (url && url.trim()) {
                            var range = this.quill.getSelection(true);
                            this.quill.insertEmbed(range.index, 'image', url.trim(), Quill.sources.USER);
                            this.quill.setSelection(range.index + 1, 0);
                        }
                    },
                    table: function() {
                        var table = document.createElement('table');
                        table.setAttribute('border', '1');
                        table.style.borderCollapse = 'collapse';
                        table.innerHTML =
                            '<tbody>' +
                            '<tr><th style="padding:4px 8px">Header 1</th><th style="padding:4px 8px">Header 2</th></tr>' +
                            '<tr><td style="padding:4px 8px">Cell 1</td><td style="padding:4px 8px">Cell 2</td></tr>' +
                            '</tbody>';
                        insertAfterCurrentBlock(this.quill, table);
                    },
                    spoiler: function() {
                        var quill = this.quill;
                        var range = quill.getSelection(true);
                        var selectedText = range.length > 0
                            ? quill.getText(range.index, range.length).trim()
                            : '';
                        if (range.length > 0) {
                            quill.deleteText(range.index, range.length, Quill.sources.USER);
                        }

                        var details = document.createElement('details');
                        var summary = document.createElement('summary');
                        summary.textContent = 'Spoiler';
                        var p = document.createElement('p');
                        p.textContent = selectedText || 'Hidden content';
                        details.appendChild(summary);
                        details.appendChild(p);

                        insertAfterCurrentBlock(quill, details);
                    },
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
