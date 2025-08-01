-- Migration to move existing hardcoded event info content to database
-- This migration extracts content from the series files and inserts it into event_info_content table

-- First, let's create a temporary function to help with the migration
CREATE OR REPLACE FUNCTION migrate_event_info_content()
RETURNS void AS $$
BEGIN
    -- Standard Series (s) - Weeklies
    INSERT INTO event_info_content (series, event, content)
    VALUES (
        's', 'w',
        '<article><p>The Standard weeklies are a set of community races organized by the race mods and main tournament organizers in cooperation with ZeldaSpeedRuns. The races are open to all participants.</p><p>Starting from January 4, 2025, there will be alternating schedules:</p><ol><li>The Kokiri weekly, Saturdays of week A at 6PM Eastern Time</li><li>The Goron weekly, Sundays of week A at 2PM Eastern Time</li><li>The Zora weekly, Saturdays of week B at 2PM Eastern Time</li><li>The Gerudo weekly, Sundays of week B at 9AM Eastern Time</li></ol><p>Settings are typically changed once every 2 or 4 weeks and posted in <a href="https://discord.com/channels/274180765816848384/512053754015645696">#standard-announcements</a> on Discord.</p></article>'
    ) ON CONFLICT (series, event) DO NOTHING;

    -- Standard Series (s) - Season 8 (comprehensive content)
    INSERT INTO event_info_content (series, event, content)
    VALUES (
        's', '8',
        '<div class="toc"><article><h2>Welcome to the Ocarina of Time Randomizer Standard Tournament Season 8</h2><p>The tournament will be hosted through a partnership between ZeldaSpeedRuns and The Silver Gauntlets to give 96 players a chance to participate in Season 8.</p><p>This event is organized by the event organizers. Please contact us if you have any questions or concerns. We can be reached by pinging the <strong>@Tourney Organisation</strong> role on Discord.</p><h2 id="links">Important Links</h2><ul><li><a href="https://discord.gg/ootrandomizer">Ocarina of Time Randomizer Discord</a></li><li><a href="https://discord.gg/zsr">ZeldaSpeedRuns Discord</a></li><li><a href="https://discord.gg/qrGf6yNY4C">The Silver Gauntlets Discord</a></li><li><a href="/event/s/8/races">Qualifier Schedule</a></li><li><a href="https://www.start.gg/tournament/ocarina-of-time-randomizer-standard-tournament-season-8/event/main-tournament">Brackets</a></li><li><a href="/event/s/8cc">Challenge Cup</a></li><li><a href="https://wiki.ootrandomizer.com/index.php?title=Standard">OoTR Standard Racing Ruleset</a></li><li><a href="https://wiki.ootrandomizer.com/index.php?title=Rules#Universal_Rules">Universal Racing Rules</a></li><li><a href="https://docs.google.com/document/d/1BbvHJF8vtyrte76jpoCVQBTy9MYStpN3vr2PLdiCIMk/edit">Fair Play Agreement</a></li><li><a href="https://docs.google.com/document/d/1xJQ8DKFhBelfDSTih324h90mS1KNtugEf-b0O5hlsnw/edit">Hint Prioritization Document</a></li></ul><h2 id="format">Tournament Format</h2><p>Season 8 will include a <strong>qualifying stage</strong>, followed by a 1v1 format.</p><p>ZeldaSpeedRuns will be hosting the main tournament series. The <strong>top 32</strong> players after the qualifiers will be eligible to participate in the next phase of the tournament, featuring a double-elimination bracket.</p><p>The Silver Gauntlets will be hosting the <strong>Challenge Cup</strong>, a 64-player event that will include a <strong>group stage and a bracket stage</strong>. The Challenge Cup is available to players ranked 33–96 after the qualifiers.</p></article></div>'
    ) ON CONFLICT (series, event) DO NOTHING;

    -- Crosskeys Series - 2025
    INSERT INTO event_info_content (series, event, content)
    VALUES (
        'xkeys', '2025',
        '<article><p>Welcome back to ALttPR, welcome back Crosskeys! The 2025 tournament is organised by the event organizers. See <a href="https://zsr.link/xkeys2025">the official document</a> for details.</p></article>'
    ) ON CONFLICT (series, event) DO NOTHING;

    RAISE NOTICE 'Migration completed successfully. All existing event info content has been migrated to the database.';
END;
$$ LANGUAGE plpgsql;

-- Execute the migration
SELECT migrate_event_info_content();

-- Clean up the temporary function
DROP FUNCTION migrate_event_info_content(); 