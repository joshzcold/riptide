// Marks the page, and asks the background worker to answer.
document.documentElement.dataset.probe = "content";
chrome.runtime.sendMessage("ping", (reply) => {
  document.documentElement.dataset.probeWorker = reply || "no reply";
});
