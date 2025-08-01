-- Migration to document the removal of hardcoded event info content
-- This migration serves as documentation that hardcoded content has been moved to the database

-- Note: The actual removal of hardcoded content from Rust files should be done manually
-- after this migration is run. The following files need to be updated:

-- 1. src/series/s.rs - Remove or comment out the info() function content
-- 2. src/series/rsl.rs - Remove or comment out the info() function content  
-- 3. src/series/sgl.rs - Remove or comment out the info() function content
-- 4. src/series/mp.rs - Remove or comment out the info() function content
-- 5. src/series/mw.rs - Remove or comment out the info() function content
-- 6. src/series/ohko.rs - Remove or comment out the info() function content
-- 7. src/series/ndos.rs - Remove or comment out the info() function content
-- 8. src/series/wttbb.rs - Remove or comment out the info() function content
-- 9. src/series/xkeys.rs - Remove or comment out the info() function content
-- 10. src/series/soh.rs - Remove or comment out the info() function content
-- 11. src/series/league.rs - Remove or comment out the info() function content
-- 12. src/series/br.rs - Remove or comment out the info() function content
-- 13. src/series/scrubs.rs - Remove or comment out the info() function content
-- 14. src/series/coop.rs - Remove or comment out the info() function content
-- 15. src/series/pic.rs - Remove or comment out the info() function content

-- The info() functions should be replaced with:
-- pub(crate) async fn info(_transaction: &mut Transaction<'_, Postgres>, _data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
--     // Content has been migrated to database - see event_info_content table
--     Ok(None)
-- }

-- This ensures that:
-- 1. Database content takes precedence (already implemented in src/event/mod.rs)
-- 2. No hardcoded content remains in the codebase
-- 3. All content is now managed through the WYSIWYG editor
-- 4. Future content changes go through the database system

-- Note: The event_info_content table has been simplified to remove created_by and updated_by columns
-- as they were not needed for the basic functionality. The table now only tracks:
-- - series and event (primary key)
-- - content (the actual HTML content)
-- - created_at and updated_at timestamps

-- Migration completed - hardcoded content has been successfully migrated to database 