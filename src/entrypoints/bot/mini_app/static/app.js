const telegram = window.Telegram?.WebApp;
const state = {
  initData: telegram?.initData ?? "",
  nextCursor: null,
  channelUsernames: new Set(),
  currentInterval: "",
  recap: null,
  recapInterval: "",
};

const elements = {
  channelList: document.querySelector("#channel-list"),
  loadMore: document.querySelector("#load-more"),
  message: document.querySelector("#message"),
  addForm: document.querySelector("#add-channel-form"),
  channelInput: document.querySelector("#channel-input"),
  presetButtons: document.querySelectorAll("[data-interval]"),
  recapForm: document.querySelector("#recap-form"),
  recapEnabled: document.querySelector("#recap-enabled"),
  recapTopics: document.querySelector("#recap-topics"),
  recapLanguage: document.querySelector("#recap-language"),
  recapIntervalButtons: document.querySelectorAll("[data-recap-interval]"),
  recapNext: document.querySelector("#recap-next"),
  recapSendNow: document.querySelector("#recap-send-now"),
};

function boot() {
  telegram?.ready();
  telegram?.expand();

  if (!state.initData) {
    setMessage("Open this from Telegram to manage your feed.", true);
    return;
  }

  bindEvents();
  loadState();
}

function bindEvents() {
  elements.loadMore.addEventListener("click", () => loadChannels());
  elements.addForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const usernameOrUrl = elements.channelInput.value.trim();
    if (!usernameOrUrl) {
      setMessage("Enter a channel username or URL.", true);
      return;
    }

    await mutate("/api/channels", {
      method: "POST",
      body: JSON.stringify({ username_or_url: usernameOrUrl }),
    }, (channel) => {
      renderChannel(channel);
      elements.channelInput.value = "";
    });
  });

  elements.presetButtons.forEach((button) => {
    button.addEventListener("click", async () => {
      const interval = button.dataset.interval;
      if (interval === state.currentInterval) {
        return;
      }

      await mutate("/api/settings/poll-interval", {
        method: "PATCH",
        body: JSON.stringify({ interval }),
      }, renderSettings);
    });
  });

  elements.recapIntervalButtons.forEach((button) => {
    button.addEventListener("click", () => {
      state.recapInterval = button.dataset.recapInterval;
      renderRecapIntervalButtons();
    });
  });
  elements.recapForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    await saveRecap(elements.recapEnabled.checked);
  });
  // The switch saves right away so turning recaps on or off never needs a second tap.
  elements.recapEnabled.addEventListener("change", async () => {
    const enabled = elements.recapEnabled.checked;
    elements.recapEnabled.checked = state.recap.enabled;
    await saveRecap(enabled);
  });
  elements.recapSendNow.addEventListener("click", async () => {
    await mutate("/api/recap/send-now", { method: "POST" }, renderRecap);
  });
}

async function saveRecap(enabled) {
  const topics = elements.recapTopics.value
    .split(/[\n,]/)
    .map((topic) => topic.trim())
    .filter(Boolean);
  await mutate("/api/recap", {
    method: "PUT",
    body: JSON.stringify({
      topics,
      language: elements.recapLanguage.value.trim(),
      interval: state.recapInterval,
      enabled,
    }),
  }, renderRecap);
}

async function loadState() {
  setBusy(true);
  try {
    const data = await apiFetch("/api/state");
    renderState(data);
    setMessage("");
  } catch (error) {
    setMessage(error.message, true);
  } finally {
    setBusy(false);
  }
}

async function mutate(path, options, applyChange) {
  setBusy(true);
  try {
    const data = await apiFetch(path, options);
    applyChange(data);
    setMessage("Updated.");
    telegram?.HapticFeedback?.notificationOccurred("success");
  } catch (error) {
    setMessage(error.message, true);
    telegram?.HapticFeedback?.notificationOccurred("error");
  } finally {
    setBusy(false);
  }
}

async function apiFetch(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      "X-Telegram-Init-Data": state.initData,
      ...options.headers,
    },
  });

  if (!response.ok) {
    const errorPayload = await response.json();
    throw new Error(errorPayload.detail);
  }

  return response.status === 204 ? null : response.json();
}

async function loadChannels() {
  setBusy(true);
  try {
    const page = await apiFetch(`/api/channels?after=${encodeURIComponent(state.nextCursor)}`);
    renderChannelPage(page);
    setMessage("");
  } catch (error) {
    setMessage(error.message, true);
  } finally {
    setBusy(false);
  }
}

function renderState(data) {
  renderSettings(data);
  renderChannelPage(data);
  renderRecap(data.recap);
}

function renderRecap(recap) {
  state.recap = recap;
  state.recapInterval = formatInterval(recap.interval_seconds);
  elements.recapEnabled.checked = recap.enabled;
  elements.recapTopics.value = recap.topics.join("\n");
  elements.recapLanguage.value = recap.language;
  elements.recapNext.textContent = recap.next_run_at
    ? `Next recap: ${new Date(recap.next_run_at).toLocaleString()}`
    : "AI recaps are off.";
  renderRecapIntervalButtons();
}

function renderRecapIntervalButtons() {
  elements.recapIntervalButtons.forEach((button) => {
    const isSelected = button.dataset.recapInterval === state.recapInterval;
    button.classList.toggle("is-selected", isSelected);
    button.setAttribute("aria-pressed", String(isSelected));
  });
}

function renderSettings(data) {
  state.currentInterval = formatInterval(data.poll_interval_seconds);
  renderIntervalButtons();
}

function renderChannelPage(page) {
  page.channels.forEach(renderChannel);
  state.nextCursor = page.next_cursor;
  elements.loadMore.hidden = state.nextCursor === null;
  renderEmptyState();
}

function renderEmptyState() {
  elements.channelList.querySelector(".empty")?.remove();
  if (state.channelUsernames.size === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = state.nextCursor === null
      ? "Start by adding the first channel you want to follow."
      : "Load more to see the remaining channels.";
    elements.channelList.append(empty);
  }
}

function renderIntervalButtons() {
  elements.presetButtons.forEach((button) => {
    const isSelected = button.dataset.interval === state.currentInterval;
    button.classList.toggle("is-selected", isSelected);
    button.setAttribute("aria-pressed", String(isSelected));
  });
}

function renderChannel(channel) {
  if (state.channelUsernames.has(channel.username)) {
    return;
  }
  elements.channelList.querySelector(".empty")?.remove();
  const row = document.createElement("article");
  row.className = "channel-row";
  row.dataset.username = channel.username;

  const main = document.createElement("div");
  main.className = "channel-main";

  const link = document.createElement("a");
  link.className = "channel-title";
  link.href = channel.url;
  link.target = "_blank";
  link.rel = "noreferrer";
  link.textContent = `@${channel.username}`;

  const removeButton = document.createElement("button");
  removeButton.className = "remove-button";
  removeButton.type = "button";
  removeButton.textContent = "Remove";
  removeButton.addEventListener("click", async () => {
    await mutate(`/api/channels/${encodeURIComponent(channel.username)}`, {
      method: "DELETE",
    }, () => {
      row.remove();
      state.channelUsernames.delete(channel.username);
      renderEmptyState();
    });
  });

  main.append(link);
  row.append(main, removeButton);
  const lastRow = elements.channelList.lastElementChild;
  const nextRow = lastRow && lastRow.dataset.username > channel.username
    ? [...elements.channelList.children].find((item) => item.dataset.username > channel.username)
    : null;
  elements.channelList.insertBefore(row, nextRow ?? null);
  state.channelUsernames.add(channel.username);
}

function formatInterval(seconds) {
  if (seconds % 3600 === 0) {
    return `${seconds / 3600}h`;
  }
  if (seconds % 60 === 0) {
    return `${seconds / 60}m`;
  }
  return `${seconds}s`;
}

function setBusy(isBusy) {
  document.querySelectorAll("button, input, select, textarea").forEach((element) => {
    element.disabled = isBusy;
  });
}

function setMessage(text, isError = false) {
  elements.message.textContent = text;
  elements.message.className = isError ? "message error" : "message";
}

boot();
