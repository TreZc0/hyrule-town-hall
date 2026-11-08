# Hyrule Town Hall
## A Mido's House Fork

This is the source code for <https://hyruletownhall.com/>, a platform for organizing speedrunning and randomizer events like tournaments and community races. It integrates with other platforms to automate some aspects of event management, including:

* [racetime.gg](https://racetime.gg/) (creating official and practice race rooms, handling chat commands, handling results and [FPA](https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub) calls)
* [Discord](https://discord.com/) (creating scheduling threads, handling commands for scheduling and settings drafting, posting results, notifying organizers when their attention is needed, volunteer handling)
* [start.gg](https://start.gg/) (handling matchups, reporting results)
* [challonge](https://challonge.com/) (basic tournament lifecycle support)

Hyrule Town Hall is custom-built in close cooperation with event organizers to allow it to work with the features that make each event unique. It is a fork of Mido's House created by Fenhl.

# AI Disclosure

We have used and use specific coding agents and models for work that we otherwise have no time to do. We will continue to ensure that the quality of the codebase is maintained, even through the use of agentic ai models.

In the case of this service, AI was used extensively to make the forked code base understandable and free of hard codes inherited by upstream and its dedicated use cases. In addition, together with the very strong and opionated Rust compiler, it is used for feature development and runtime optimization, especially in regards to stability and crash resilience.

No form of AI code was committed or delivered to production without previous human review or significant manual testing efforts. 

Any current contributor to the project is vehemently against using generative AI for creative work including creative writing, art, and music.

Wherever possible, locally hosted AI was used to fulfill the required tasks.

# Contributing

If you're interested in contributing to the Hyrule Town Hall project, feel free to contact me on Discord (@trezc0_) or contact through Issues/Pull Requests.
Note that while AI assisted contributions are allowed, our coding guidelines require every contributor to be fully aware of the scope of their contribution, to be able to defend the design philosophy, and provide support for the implementation. Purely vibecoded contributions will be rejected. 

## Discord

Discord invite link with appropriate permissions (only useable by members of the Hyrule Town Hall Discord developer team):

* Dev: <https://discord.com/api/oauth2/authorize?client_id=1375404830037639189&scope=bot&permissions=2269795995502592>
* Production: <https://discord.com/api/oauth2/authorize?client_id=1375404601016324226&scope=bot&permissions=2269795995502592>
