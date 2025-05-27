# Hyrule Town Hall
## A Mido's House Fork

This is the source code for <https://hth.zeldaspeedruns.com/>, a platform for organizing speedrunning and randomizer events like tournaments and community races. It integrates with other platforms to automate some aspects of event management, including:

* [racetime.gg](https://racetime.gg/) (creating official and practice race rooms, handling chat commands, handling results and [FPA](https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub) calls)
* [Discord](https://discord.com/) (creating scheduling threads, handling commands for scheduling and settings drafting, posting results, notifying organizers when their attention is needed)
* [start.gg](https://start.gg/) (handling matchups, reporting results)

Hyrule Town Hall is custom-built in close cooperation with event organizers to allow it to work with the features that make each event unique. It is a fork of Mido's House created by Fenhl.

# Contributing

If you're interested in contributing to the Hyrule Town Hall project, feel free to contact me on Discord (@trezc0_). Here are some ways you could help out:

## Code

The Hyrule Town Hall codebase is currently a one-person project, but it doesn't have to remain that way! If you're interested in contributing to the codebase but don't know where to start, let me know so we can discuss how you can help.

## Data archival

I'm also always looking for trusted members of the OoTR community willing to help out as “archivists”, i.e. for the task of manually adding race room and restream/vod links to races for events where this isn't automated. We have an invitational Discord channel for coordinating this.

## Translations

Hyrule Town Hall currently has an incomplete French translation which was created for the [Tournoi Francophone Saison 3](https://midos.house/event/fr/3) by that event's organizers. Let me know if you would like to proofread or extend this translation, or start a new one for a different language. A priority of the project is attention to detail, so I would include custom code to handle grammatical variations like case and gender where necessary. For example, the English Discord message for race results uses the word “defeat” or “defeats” depending on whether the winning team's name is grammatically singular or plural.

# Dev notes

Discord invite link with appropriate permissions (only useable by members of the Hyrule Town Hall Discord developer team):

* Dev: <https://discord.com/api/oauth2/authorize?client_id=1375404830037639189&scope=bot&permissions=318096427008>
* Production: <https://discord.com/api/oauth2/authorize?client_id=1375404601016324226&scope=bot&permissions=318096427008>
