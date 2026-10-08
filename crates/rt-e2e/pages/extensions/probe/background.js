chrome.runtime.onMessage.addListener((message, _sender, reply) => {
  if (message === "ping") reply("pong");
});
