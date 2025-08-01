#!/usr/bin/env python3
"""
Script to remove hardcoded event info content from series files.
This script replaces the info() functions with simple placeholders since
content has been migrated to the database.
"""

import os
import re
from pathlib import Path

# Files to process
SERIES_FILES = [
    "src/series/s.rs",
    "src/series/rsl.rs", 
    "src/series/sgl.rs",
    "src/series/mp.rs",
    "src/series/mw.rs",
    "src/series/ohko.rs",
    "src/series/ndos.rs",
    "src/series/wttbb.rs",
    "src/series/xkeys.rs",
    "src/series/soh.rs",
    "src/series/league.rs",
    "src/series/br.rs",
    "src/series/scrubs.rs",
    "src/series/coop.rs",
    "src/series/pic.rs",
]

def replace_info_function(content: str, filename: str) -> str:
    """Replace the info() function with a simple placeholder."""
    
    # Pattern to match the info function
    pattern = r'pub\(crate\) async fn info\(transaction: &mut Transaction<.*?>, data: &Data<.*?>\) -> Result<Option<RawHtml<String>>, InfoError> \{[^}]*\}'
    
    # Replacement function
    replacement = '''pub(crate) async fn info(_transaction: &mut Transaction<'_, Postgres>, _data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    // Content has been migrated to database - see event_info_content table
    // Use the WYSIWYG editor in the event setup page to manage content
    Ok(None)
}'''
    
    # Try to replace the function
    new_content = re.sub(pattern, replacement, content, flags=re.DOTALL)
    
    if new_content == content:
        print(f"Warning: Could not find info() function in {filename}")
        return content
    
    return new_content

def process_file(filepath: str) -> bool:
    """Process a single file and replace the info function."""
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
        
        new_content = replace_info_function(content, filepath)
        
        if new_content != content:
            # Create backup
            backup_path = filepath + '.backup'
            with open(backup_path, 'w', encoding='utf-8') as f:
                f.write(content)
            print(f"Created backup: {backup_path}")
            
            # Write new content
            with open(filepath, 'w', encoding='utf-8') as f:
                f.write(new_content)
            print(f"Updated: {filepath}")
            return True
        else:
            print(f"No changes needed: {filepath}")
            return False
            
    except Exception as e:
        print(f"Error processing {filepath}: {e}")
        return False

def main():
    """Main function to process all series files."""
    print("Removing hardcoded event info content from series files...")
    print("This script will replace info() functions with database-driven placeholders.")
    print()
    
    # Check if we're in the right directory
    if not os.path.exists("src/series"):
        print("Error: Please run this script from the project root directory")
        return
    
    # Process each file
    updated_files = []
    for filepath in SERIES_FILES:
        if os.path.exists(filepath):
            if process_file(filepath):
                updated_files.append(filepath)
        else:
            print(f"Warning: File not found: {filepath}")
    
    print()
    print(f"Processing complete!")
    print(f"Updated {len(updated_files)} files:")
    for filepath in updated_files:
        print(f"  - {filepath}")
    
    print()
    print("Next steps:")
    print("1. Run 'cargo check' to ensure the code compiles")
    print("2. Test the event info pages to ensure they work correctly")
    print("3. Use the WYSIWYG editor in event setup pages to manage content")
    print("4. Remove the .backup files once you're satisfied with the changes")
    print("5. Note: The event_info_content table has been simplified to remove user tracking columns")

if __name__ == "__main__":
    main() 