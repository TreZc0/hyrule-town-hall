// Event Info WYSIWYG Editor
class EventInfoEditor {
    constructor(containerId, series, event) {
        this.container = document.getElementById(containerId);
        this.series = series;
        this.event = event;
        this.editor = null;
        this.isOrganizer = false;
        this.hasContent = false;
        
        this.init();
    }
    
    async init() {
        // Check if user is organizer and load existing content
        await this.loadContent();
        
        if (this.isOrganizer) {
            this.setupEditor();
            this.setupToolbar();
            this.setupMacros();
        } else {
            this.showReadOnly();
        }
    }
    
    async loadContent() {
        try {
            const response = await fetch(`/event/${this.series}/${this.event}/info-content`);
            const data = await response.json();
            
            this.isOrganizer = data.is_organizer;
            this.hasContent = data.has_content;
            
            if (this.hasContent) {
                this.container.innerHTML = data.content;
            }
        } catch (error) {
            console.error('Failed to load content:', error);
        }
    }
    
    setupEditor() {
        // Create editor container
        const editorContainer = document.createElement('div');
        editorContainer.className = 'event-info-editor';
        editorContainer.contentEditable = true;
        editorContainer.innerHTML = this.container.innerHTML || '<p>Start writing your event information here...</p>';
        
        // Clear container and add editor
        this.container.innerHTML = '';
        this.container.appendChild(editorContainer);
        
        this.editor = editorContainer;
        
        // Add event listeners
        this.editor.addEventListener('input', () => this.autoSave());
        this.editor.addEventListener('keydown', (e) => this.handleKeydown(e));
        
        // Focus editor
        this.editor.focus();
    }
    
    setupToolbar() {
        const toolbar = document.createElement('div');
        toolbar.className = 'editor-toolbar';
        toolbar.innerHTML = `
            <div class="toolbar-group">
                <button type="button" class="toolbar-btn" data-action="bold" title="Bold (Ctrl+B)">
                    <strong>B</strong>
                </button>
                <button type="button" class="toolbar-btn" data-action="italic" title="Italic (Ctrl+I)">
                    <em>I</em>
                </button>
                <button type="button" class="toolbar-btn" data-action="underline" title="Underline (Ctrl+U)">
                    <u>U</u>
                </button>
            </div>
            <div class="toolbar-group">
                <button type="button" class="toolbar-btn" data-action="h1" title="Heading 1">H1</button>
                <button type="button" class="toolbar-btn" data-action="h2" title="Heading 2">H2</button>
                <button type="button" class="toolbar-btn" data-action="h3" title="Heading 3">H3</button>
            </div>
            <div class="toolbar-group">
                <button type="button" class="toolbar-btn" data-action="ul" title="Unordered List">• List</button>
                <button type="button" class="toolbar-btn" data-action="ol" title="Ordered List">1. List</button>
                <button type="button" class="toolbar-btn" data-action="link" title="Insert Link">🔗</button>
            </div>
            <div class="toolbar-group">
                <button type="button" class="toolbar-btn" data-action="macro" title="Insert Macro">⚙️</button>
            </div>
            <div class="toolbar-group">
                <button type="button" class="toolbar-btn save-btn" data-action="save" title="Save (Ctrl+S)">💾 Save</button>
                <button type="button" class="toolbar-btn delete-btn" data-action="delete" title="Delete Content">🗑️ Delete</button>
            </div>
        `;
        
        this.container.insertBefore(toolbar, this.container.firstChild);
        
        // Add toolbar event listeners
        toolbar.addEventListener('click', (e) => {
            if (e.target.classList.contains('toolbar-btn')) {
                const action = e.target.dataset.action;
                this.handleToolbarAction(action);
            }
        });
    }
    
    setupMacros() {
        // Create macro dropdown
        const macroDropdown = document.createElement('div');
        macroDropdown.className = 'macro-dropdown';
        macroDropdown.style.display = 'none';
        macroDropdown.innerHTML = `
            <div class="macro-item" data-macro="organizers">📋 Event Organizers</div>
            <div class="macro-item" data-macro="toc">📑 Table of Contents</div>
            <div class="macro-item" data-macro="links">🔗 Important Links</div>
            <div class="macro-item" data-macro="rules">📜 Tournament Rules</div>
            <div class="macro-item" data-macro="schedule">📅 Event Schedule</div>
            <div class="macro-item" data-macro="prizes">🏆 Prizes</div>
            <div class="macro-item" data-macro="contact">📞 Contact Information</div>
        `;
        
        this.container.appendChild(macroDropdown);
        
        // Show/hide macro dropdown
        const macroBtn = this.container.querySelector('[data-action="macro"]');
        macroBtn.addEventListener('click', () => {
            macroDropdown.style.display = macroDropdown.style.display === 'none' ? 'block' : 'none';
        });
        
        // Handle macro selection
        macroDropdown.addEventListener('click', (e) => {
            if (e.target.classList.contains('macro-item')) {
                const macro = e.target.dataset.macro;
                this.insertMacro(macro);
                macroDropdown.style.display = 'none';
            }
        });
        
        // Hide dropdown when clicking outside
        document.addEventListener('click', (e) => {
            if (!macroDropdown.contains(e.target) && !macroBtn.contains(e.target)) {
                macroDropdown.style.display = 'none';
            }
        });
    }
    
    handleToolbarAction(action) {
        switch (action) {
            case 'bold':
                document.execCommand('bold', false, null);
                break;
            case 'italic':
                document.execCommand('italic', false, null);
                break;
            case 'underline':
                document.execCommand('underline', false, null);
                break;
            case 'h1':
                this.insertHeading(1);
                break;
            case 'h2':
                this.insertHeading(2);
                break;
            case 'h3':
                this.insertHeading(3);
                break;
            case 'ul':
                this.insertList('ul');
                break;
            case 'ol':
                this.insertList('ol');
                break;
            case 'link':
                this.insertLink();
                break;
            case 'save':
                this.saveContent();
                break;
            case 'delete':
                this.deleteContent();
                break;
        }
    }
    
    insertHeading(level) {
        const selection = window.getSelection();
        if (selection.rangeCount > 0) {
            const range = selection.getRangeAt(0);
            const heading = document.createElement(`h${level}`);
            heading.textContent = 'Heading ' + level;
            range.deleteContents();
            range.insertNode(heading);
            heading.focus();
        }
    }
    
    insertList(type) {
        const selection = window.getSelection();
        if (selection.rangeCount > 0) {
            const range = selection.getRangeAt(0);
            const list = document.createElement(type);
            const item = document.createElement('li');
            item.textContent = 'List item';
            list.appendChild(item);
            range.deleteContents();
            range.insertNode(list);
            item.focus();
        }
    }
    
    insertLink() {
        const url = prompt('Enter URL:');
        if (url) {
            const text = prompt('Enter link text:', url);
            if (text) {
                document.execCommand('createLink', false, url);
            }
        }
    }
    
    insertMacro(macro) {
        const macroContent = this.getMacroContent(macro);
        if (macroContent) {
            const selection = window.getSelection();
            if (selection.rangeCount > 0) {
                const range = selection.getRangeAt(0);
                const tempDiv = document.createElement('div');
                tempDiv.innerHTML = macroContent;
                range.deleteContents();
                range.insertNode(tempDiv.firstChild);
            }
        }
    }
    
    getMacroContent(macro) {
        const macros = {
            organizers: `
                <h2 id="organizers">Event Organizers</h2>
                <p>This event is organized by the following people:</p>
                <ul>
                    <li>Organizer Name 1</li>
                    <li>Organizer Name 2</li>
                </ul>
            `,
            toc: `
                <div class="toc">
                    <h2>Table of Contents</h2>
                    <ul>
                        <li><a href="#organizers">Event Organizers</a></li>
                        <li><a href="#links">Important Links</a></li>
                        <li><a href="#rules">Tournament Rules</a></li>
                        <li><a href="#schedule">Event Schedule</a></li>
                    </ul>
                </div>
            `,
            links: `
                <h2 id="links">Important Links</h2>
                <ul>
                    <li><a href="https://discord.gg/ootrandomizer">Ocarina of Time Randomizer Discord</a></li>
                    <li><a href="https://racetime.gg/">Racetime.gg</a></li>
                    <li><a href="https://wiki.ootrandomizer.com/">OoTR Wiki</a></li>
                </ul>
            `,
            rules: `
                <h2 id="rules">Tournament Rules</h2>
                <p>Please read and follow the tournament rules:</p>
                <ul>
                    <li>Rule 1</li>
                    <li>Rule 2</li>
                    <li>Rule 3</li>
                </ul>
            `,
            schedule: `
                <h2 id="schedule">Event Schedule</h2>
                <p>Important dates and times:</p>
                <ul>
                    <li>Registration Deadline: TBD</li>
                    <li>Tournament Start: TBD</li>
                    <li>Finals: TBD</li>
                </ul>
            `,
            prizes: `
                <h2 id="prizes">Prizes</h2>
                <p>Tournament prizes:</p>
                <ul>
                    <li>1st Place: TBD</li>
                    <li>2nd Place: TBD</li>
                    <li>3rd Place: TBD</li>
                </ul>
            `,
            contact: `
                <h2 id="contact">Contact Information</h2>
                <p>For questions or concerns, please contact:</p>
                <ul>
                    <li>Discord: @username</li>
                    <li>Email: contact@example.com</li>
                </ul>
            `
        };
        
        return macros[macro] || '';
    }
    
    handleKeydown(e) {
        // Ctrl+S to save
        if (e.ctrlKey && e.key === 's') {
            e.preventDefault();
            this.saveContent();
        }
        
        // Ctrl+B for bold
        if (e.ctrlKey && e.key === 'b') {
            e.preventDefault();
            document.execCommand('bold', false, null);
        }
        
        // Ctrl+I for italic
        if (e.ctrlKey && e.key === 'i') {
            e.preventDefault();
            document.execCommand('italic', false, null);
        }
        
        // Ctrl+U for underline
        if (e.ctrlKey && e.key === 'u') {
            e.preventDefault();
            document.execCommand('underline', false, null);
        }
    }
    
    async saveContent() {
        if (!this.editor) return;
        
        const content = this.editor.innerHTML;
        
        try {
            const response = await fetch(`/event/${this.series}/${this.event}/info-content`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/x-www-form-urlencoded',
                },
                body: new URLSearchParams({
                    content: content,
                    csrf: this.getCsrfToken()
                })
            });
            
            const result = await response.json();
            
            if (result.success) {
                this.showMessage('Content saved successfully!', 'success');
                this.hasContent = true;
            } else {
                this.showMessage(result.error || 'Failed to save content', 'error');
            }
        } catch (error) {
            console.error('Save error:', error);
            this.showMessage('Failed to save content', 'error');
        }
    }
    
    async deleteContent() {
        if (!confirm('Are you sure you want to delete the event info content? This action cannot be undone.')) {
            return;
        }
        
        try {
            const response = await fetch(`/event/${this.series}/${this.event}/info-content/delete`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/x-www-form-urlencoded',
                },
                body: new URLSearchParams({
                    csrf: this.getCsrfToken()
                })
            });
            
            const result = await response.json();
            
            if (result.success) {
                this.showMessage('Content deleted successfully!', 'success');
                this.editor.innerHTML = '<p>Start writing your event information here...</p>';
                this.hasContent = false;
            } else {
                this.showMessage('Failed to delete content', 'error');
            }
        } catch (error) {
            console.error('Delete error:', error);
            this.showMessage('Failed to delete content', 'error');
        }
    }
    
    autoSave() {
        // Debounce auto-save
        clearTimeout(this.autoSaveTimeout);
        this.autoSaveTimeout = setTimeout(() => {
            this.saveContent();
        }, 2000);
    }
    
    showReadOnly() {
        if (!this.hasContent) {
            this.container.innerHTML = '<p>No custom content available for this event.</p>';
        }
    }
    
    showMessage(message, type) {
        const messageDiv = document.createElement('div');
        messageDiv.className = `message message-${type}`;
        messageDiv.textContent = message;
        
        this.container.appendChild(messageDiv);
        
        setTimeout(() => {
            messageDiv.remove();
        }, 3000);
    }
    
    getCsrfToken() {
        const csrfInput = document.querySelector('input[name="csrf"]');
        return csrfInput ? csrfInput.value : '';
    }
}

// Initialize editor when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    const editorContainer = document.getElementById('event-info-editor');
    if (editorContainer) {
        const series = editorContainer.dataset.series;
        const event = editorContainer.dataset.event;
        new EventInfoEditor('event-info-editor', series, event);
    }
}); 