use crate::prelude::*;

#[rocket::get("/mw")]
pub(crate) async fn index(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>) -> PageResult {
    let transaction = pool.begin().await?;
    page(transaction, &me, &uri, PageStyle { kind: PageKind::Center, mw_footer: true, ..PageStyle::default() }, "HTH Multiworld", html! {
        h1 : "HTH Multiworld";
        img(class = "banner icon", src = static_url!("mw.png"));
        p {
            : "HTH Multiworld is a tool that can be used to play ";
            a(href = "https://wiki.ootrandomizer.com/index.php?title=Multiworld") : "multiworld";
            : " seeds of the ";
            a(href = "https://ootrandomizer.com/") : "Ocarina of Time randomizer";
            : ". It supports cross-platform play between ";
            a(href = uri!(platforms)) : "different platforms";
            : ", and does not require port forwarding.";
        }
        div(class = "button-row large-button-row") {
            a(class = "button", href = "https://github.com/midoshouse/ootr-multiworld/releases/latest/download/multiworld-installer.exe") {
                : "Download for Windows";
                br;
                small : "supports EverDrive*, BizHawk, and Project64";
            }
            a(class = "button", href = uri!(install_macos)) {
                : "Install instructions for macOS";
                br;
                small : "supports EverDrive*";
            }
            a(class = "button", href = "https://github.com/midoshouse/ootr-multiworld/releases/latest/download/multiworld-installer-linux") {
                : "Download for Linux";
                br;
                small : "supports EverDrive* and BizHawk";
            }
        }
        p {
            : "*EverDrive support is currently experimental and requires ";
            a(href = "https://ootrandomizer.com/generatorDev?version=devFenhl_") : "Fenhl's branch of the randomizer";
            : ".";
        }
        p {
            : "If you need help, please ask in ";
            a(href = "https://discord.gg/BGRrKKn") : "#setup-support on the OoTR Discord";
            : " (feel free to ping @fenhl) or ";
            a(href = "https://github.com/midoshouse/ootr-multiworld/issues/new") : "open an issue";
            : ".";
        }
        p {
            a(href = "https://wiki.ootrandomizer.com/index.php?title=Mido%27s_House_Multiworld#Frequently_asked_questions") : "FAQ";
            : " • ";
            a(href = "https://github.com/midoshouse/ootr-multiworld") : "multiworld source code";
        }
    }).await
}

#[rocket::get("/mw/platforms")]
pub(crate) async fn platforms(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>) -> PageResult {
    let transaction = pool.begin().await?;
    page(transaction, &me, &uri, PageStyle { kind: PageKind::Center, mw_footer: true, ..PageStyle::default() }, "platform support — HTH Multiworld", html! {
        h1 {
            a(href = uri!(index)) : "HTH Multiworld";
            : " platform support status";
        }
        table {
            tr {
                th;
                th : "Windows";
                th : "Linux";
                th : "macOS";
            }
            tr {
                th : "EverDrive";
                td {
                    : "✓ (";
                    a(href = "https://github.com/midoshouse/ootr-multiworld/releases/latest/download/multiworld-installer.exe") : "download";
                    : ") *";
                }
                td {
                    : "✓ (";
                    a(href = "https://github.com/midoshouse/ootr-multiworld/releases/latest/download/multiworld-installer-linux") : "download";
                    : ") *";
                }
                td {
                    : "✓ (";
                    a(href = uri!(install_macos)) : "install instructions";
                    : ") *";
                }
            }
            tr {
                th : "Wii Virtual Console";
                td(colspan = "3") : "Would require a modification to Virtual Console itself. The “Multiworld 2.0” project claims to have solved this issue but has not shared any details out of concerns for competitive integrity.";
            }
            tr {
                th : "BizHawk";
                td {
                    : "✓ (";
                    a(href = "https://github.com/midoshouse/ootr-multiworld/releases/latest/download/multiworld-installer.exe") : "download";
                    : ")";
                }
                td {
                    : "✓ (";
                    a(href = "https://github.com/midoshouse/ootr-multiworld/releases/latest/download/multiworld-installer-linux") : "download";
                    : ")";
                }
                td {
                    a(href = "https://github.com/tasemulators/bizHawk#macos-legacy-bizhawk") : "Not supported by BizHawk itself";
                }
            }
            tr {
                th : "Project64";
                td {
                    : "✓ (";
                    a(href = "https://github.com/midoshouse/ootr-multiworld/releases/latest/download/multiworld-installer.exe") : "download";
                    : ")";
                }
                td(colspan = "2") : "Not supported by Project64 itself";
            }
            tr {
                th : "Project64-EM";
                td(colspan = "3") {
                    : "Not planned. Project64-EM is a modified version of Project64 created by the ";
                    a(href = "https://ootmm.com/") : "OoTMM";
                    : " community which removes the plugin system used by HTH Multiworld and replaces it with a different one. Note that HTH Multiworld does not support OoTMM — please follow ";
                    a(href = "https://ootmm.com/multiplayer") : "the OoTMM multiplayer setup guide";
                    : " instead.";
                }
            }
            tr {
                th : "RetroArch";
                td(colspan = "3") {
                    a(href = "https://github.com/midoshouse/ootr-multiworld/issues/25") : "Planned";
                }
            }
        }
        p {
            : "*EverDrive support is currently experimental and requires ";
            a(href = "https://ootrandomizer.com/generatorDev?version=devFenhl_") : "Fenhl's branch of the randomizer";
            : ".";
        }
        p {
            : "If your operating system, console, or emulator is not listed here, please ";
            a(href = "https://github.com/midoshouse/ootr-multiworld/issues/new") : "open an issue";
            : " to request support.";
        }
    }).await
}

#[rocket::get("/mw/install/macos")]
pub(crate) async fn install_macos(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>) -> PageResult {
    let transaction = pool.begin().await?;
    page(transaction, &me, &uri, PageStyle { mw_footer: true, ..PageStyle::default() }, "macOS install instructions — HTH Multiworld", html! {
        h1 {
            a(href = uri!(index)) : "HTH Multiworld";
            : " install instructions for macOS";
        }
        p : "You will need administrator permissions.";
        h2 : "Using Homebrew (recommended)";
        ol {
            li {
                : "Install ";
                a(href = "https://brew.sh/") : "Homebrew";
                : ".";
            }
            li {
                : "In Terminal, run the following command:";
                br;
                code : "brew install --no-quarantine midoshouse/tap/mhmw";
            }
        }
        h2 {
            : "Using ";
            a(href = "https://github.com/LnL7/nix-darwin") : "nix-darwin";
        }
        ol {
            li {
                : "Edit your configuration.nix to include the following:";
                pre : "{ config, pkgs, ... }: {
    homebrew = {
        enable = true;
        casks = [
            {
                name = \"midoshouse/tap/mhmw\";
                args.no_quarantine = true;
            }
        ];
        onActivation = {
            autoUpdate = true;
            upgrade = true;
        };
    };
}";
            }
            li {
                : "Run ";
                code : "darwin-rebuild switch";
            }
        }
        h2 : "Support";
        p {
            : "If you need help, please ask in ";
            a(href = "https://discord.gg/BGRrKKn") : "#setup-support on the OoTR Discord";
            : " (feel free to ping @fenhl) or ";
            a(href = "https://github.com/midoshouse/ootr-multiworld/issues/new") : "open an issue";
            : ".";
        }
    }).await
}
