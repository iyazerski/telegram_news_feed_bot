const telegram = window.Telegram?.WebApp;
const state = {
  initData: telegram?.initData ?? "",
  busy: false,
  currentInterval: "",
};

const elements = {
  channelList: document.querySelector("#channel-list"),
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
    });
    elements.channelInput.value = "";
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
      });
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

async function mutate(path, options) {
  setBusy(true);
  try {
    const data = await apiFetch(path, options);
    renderState(data);
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

  return response.json();
}

function renderState(data) {
  state.currentInterval = formatInterval(data.poll_interval_seconds);
  renderIntervalButtons();
  renderChannels(data.channels);
}

function renderIntervalButtons() {
  elements.presetButtons.forEach((button) => {
    const isSelected = button.dataset.interval === state.currentInterval;
    button.classList.toggle("is-selected", isSelected);
    button.setAttribute("aria-pressed", String(isSelected));
  });
}

function renderChannels(channels) {
  elements.channelList.replaceChildren();

  if (channels.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = "Start by adding the first channel you want to follow.";
    elements.channelList.append(empty);
    return;
  }

  channels.forEach((channel) => {
    const row = document.createElement("article");
    row.className = "channel-row";

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
      });
    });

    main.append(link);
    row.append(main, removeButton);
    elements.channelList.append(row);
  });
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
  state.busy = isBusy;
  document.querySelectorAll("button, input").forEach((element) => {
    element.disabled = isBusy;
  });
}

function setMessage(text, isError = false) {
  elements.message.textContent = text;
  elements.message.className = isError ? "message error" : "message";
}

boot();
