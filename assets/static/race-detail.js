"use strict";

document.querySelectorAll(".race-stream[data-twitch-query]").forEach((stream) => {
  stream.addEventListener("toggle", () => {
    const container = stream.querySelector(".race-stream-player");
    if (!stream.open) {
      container.replaceChildren();
      return;
    }
    if (container.firstChild) return;
    const url = new URL("https://player.twitch.tv/");
    const query = new URLSearchParams(stream.dataset.twitchQuery);
    for (const key of ["channel", "video"]) {
      if (query.has(key)) url.searchParams.set(key, query.get(key));
    }
    url.searchParams.set("parent", window.location.hostname);
    url.searchParams.set("autoplay", "false");
    const player = document.createElement("iframe");
    player.src = url.href;
    player.title = container.dataset.playerTitle;
    player.allowFullscreen = true;
    container.append(player);
  });
});
