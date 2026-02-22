tinymce.init({
    selector: '#editor-content',
    skin_url: 'https://cdnjs.cloudflare.com/ajax/libs/tinymce/8.1.2/skins/ui/oxide',
    content_css: 'https://cdnjs.cloudflare.com/ajax/libs/tinymce/8.1.2/skins/content/default/content.min.css',
    plugins: 'table image link lists code',
    toolbar: 'undo redo | bold italic underline strikethrough | blocks | link image | bullist numlist | table | spoiler organizers | code',
    promotion: false,
    branding: false,
    min_height: 400,
    menubar: false,
    table_toolbar: 'tableprops tabledelete | tableinsertrowbefore tableinsertrowafter tabledeleterow | tableinsertcolbefore tableinsertcolafter tabledeletecol',
    setup: function(editor) {
        editor.ui.registry.addButton('organizers', {
            text: '\u{1F465} Organizers',
            tooltip: 'Insert organizer list placeholder',
            onAction: function() {
                editor.insertContent('{{organizers}}');
            }
        });

        editor.ui.registry.addButton('spoiler', {
            text: '\u{1F441} Spoiler',
            tooltip: 'Wrap selection in a spoiler block',
            onAction: function() {
                var selected = editor.selection.getContent();
                var inner = selected || '<p>Hidden content</p>';
                editor.insertContent(
                    '<details><summary>Spoiler</summary>' + inner + '</details>'
                );
            }
        });
    }
});
