const telegram = window.Telegram?.WebApp;
const state = {
  initData: telegram?.initData ?? "",
  nextCursor: null,
  channelUsernames: new Set(),
  currentInterval: "",
};

const elements = {
  channelList: document.querySelector("#channel-list"),
  loadMore: document.querySelector("#load-more"),
  message: document.querySelector("#message"),
  addForm: document.querySelector("#add-channel-form"),
  channelInput: document.querySelector("#channel-input"),
  presetButtons: document.querySelectorAll("[data-interval]"),
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
  document.querySelectorAll("button, input").forEach((element) => {
    element.disabled = isBusy;
  });
}

function setMessage(text, isError = false) {
  elements.message.textContent = text;
  elements.message.className = isError ? "message error" : "message";
}

boot();
