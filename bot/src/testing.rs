//! An in-memory store for tests of the use cases and entrypoints.

use std::collections::BTreeMap;
use std::sync::Mutex;

use anyhow::Result;
use chrono::{DateTime, Utc};
use common::settings::{RecapSchedule, RecapSettings};

use crate::use_cases::manage_feed::FeedStore;
use crate::use_cases::manage_recaps::RecapStore;

#[derive(Default)]
struct State {
    destination: Option<String>,
    poll_interval_seconds: Option<u64>,
    /// Whether each channel, by username, is active.
    channels: BTreeMap<String, bool>,
    recap: Option<RecapSettings>,
    next_run_at: Option<DateTime<Utc>>,
    fail_next_destination_write: bool,
}

/// A store that keeps everything in memory, starting empty.
#[derive(Default)]
pub struct FakeStore {
    state: Mutex<State>,
}

impl FakeStore {
    /// Make the next destination write fail, as during a database outage.
    pub fn fail_next_destination_write(&self) {
        self.state.lock().unwrap().fail_next_destination_write = true;
    }

    pub fn add_channel(&self, username: &str, active: bool) {
        self.state.lock().unwrap().channels.insert(username.to_owned(), active);
    }

    pub fn destination(&self) -> Option<String> {
        self.state.lock().unwrap().destination.clone()
    }

    pub fn next_run_at(&self) -> Option<DateTime<Utc>> {
        self.state.lock().unwrap().next_run_at
    }

    pub fn set_next_run_at(&self, run_at: Option<DateTime<Utc>>) {
        self.state.lock().unwrap().next_run_at = run_at;
    }
}

impl FeedStore for FakeStore {
    async fn store_destination_chat_id(&self, chat_id: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if std::mem::take(&mut state.fail_next_destination_write) {
            anyhow::bail!("database is unavailable");
        }
        state.destination = Some(chat_id.to_owned());
        Ok(())
    }

    async fn load_poll_interval_seconds(&self) -> Result<Option<u64>> {
        Ok(self.state.lock().unwrap().poll_interval_seconds)
    }

    async fn store_poll_interval_seconds(&self, seconds: u64) -> Result<()> {
        self.state.lock().unwrap().poll_interval_seconds = Some(seconds);
        Ok(())
    }

    async fn list_active_channels(&self, after: &str, limit: usize) -> Result<Vec<String>> {
        let state = self.state.lock().unwrap();
        let active = state.channels.iter().filter(|(username, active)| **active && username.as_str() > after);
        Ok(active.map(|(username, _)| username.clone()).take(limit).collect())
    }

    async fn activate_channel(&self, username: &str) -> Result<()> {
        self.add_channel(username, true);
        Ok(())
    }

    async fn deactivate_channel(&self, username: &str) -> Result<bool> {
        let mut state = self.state.lock().unwrap();
        let Some(active) = state.channels.get_mut(username) else {
            return Ok(false);
        };
        *active = false;
        Ok(true)
    }
}

impl RecapStore for FakeStore {
    async fn load_recap(&self, default_interval_seconds: i64) -> Result<RecapSchedule> {
        let state = self.state.lock().unwrap();
        let settings = state.recap.clone().unwrap_or_else(|| RecapSettings {
            topics: Vec::new(),
            language: "English".to_owned(),
            interval_seconds: default_interval_seconds,
            enabled: false,
        });
        Ok(RecapSchedule { settings, next_run_at: state.next_run_at })
    }

    async fn store_recap(&self, settings: &RecapSettings, next_run_at: Option<DateTime<Utc>>) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.recap = Some(settings.clone());
        if next_run_at.is_some() {
            state.next_run_at = next_run_at;
        }
        Ok(())
    }

    async fn store_recap_next_run_at(&self, run_at: DateTime<Utc>) -> Result<()> {
        self.set_next_run_at(Some(run_at));
        Ok(())
    }
}
