const telegram = window.Telegram?.WebApp;
const state = {
  initData: telegram?.initData ?? "",
  busy: false,
};

const elements = {
  status: document.querySelector("#connection-status"),
  destination: document.querySelector("#destination-value"),
  interval: document.querySelector("#interval-value"),
  channelCount: document.querySelector("#channel-count-value"),
  channelList: document.querySelector("#channel-list"),
  message: document.querySelector("#message"),
  addForm: document.querySelector("#add-channel-form"),
  channelInput: document.querySelector("#channel-input"),
  intervalForm: document.querySelector("#interval-form"),
  intervalInput: document.querySelector("#interval-input"),
  presetButtons: document.querySelectorAll("[data-interval]"),
};

function boot() {
  telegram?.ready();
  telegram?.expand();

  if (!state.initData) {
    setStatus("Open in Telegram", "error");
    setMessage("Telegram authentication data is unavailable.", true);
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

  elements.intervalForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const interval = elements.intervalInput.value.trim();
    if (!interval) {
      setMessage("Enter a polling interval.", true);
      return;
    }

    await mutate("/api/settings/poll-interval", {
      method: "PATCH",
      body: JSON.stringify({ interval }),
    });
  });

  elements.presetButtons.forEach((button) => {
    button.addEventListener("click", () => {
      elements.intervalInput.value = button.dataset.interval;
      elements.intervalInput.focus();
    });
  });
}

async function loadState() {
  setBusy(true);
  try {
    const data = await apiFetch("/api/state");
    renderState(data);
    setStatus("Ready", "ready");
    setMessage("");
  } catch (error) {
    setStatus("Error", "error");
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
    setStatus("Saved", "ready");
    setMessage("Saved.");
    telegram?.HapticFeedback?.notificationOccurred("success");
  } catch (error) {
    setStatus("Error", "error");
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
  elements.destination.textContent = data.destination_chat_id ? "Configured" : "Not set";
  elements.destination.title = data.destination_chat_id ?? "";
  elements.interval.textContent = formatInterval(data.poll_interval_seconds);
  elements.channelCount.textContent = String(data.channels.length);
  elements.intervalInput.value = formatInterval(data.poll_interval_seconds);
  renderChannels(data.channels);
}

function renderChannels(channels) {
  elements.channelList.replaceChildren();

  if (channels.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = "No source channels configured.";
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

    const meta = document.createElement("span");
    meta.className = "channel-meta";
    meta.textContent = `Last committed: ${channel.last_committed_message_id}`;

    const removeButton = document.createElement("button");
    removeButton.className = "remove-button";
    removeButton.type = "button";
    removeButton.textContent = "Remove";
    removeButton.addEventListener("click", async () => {
      await mutate(`/api/channels/${encodeURIComponent(channel.username)}`, {
        method: "DELETE",
      });
    });

    main.append(link, meta);
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

function setStatus(text, className) {
  elements.status.textContent = text;
  elements.status.className = `status-pill ${className}`;
}

function setMessage(text, isError = false) {
  elements.message.textContent = text;
  elements.message.className = isError ? "message error" : "message";
}

boot();
