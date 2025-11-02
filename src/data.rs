//! Management of application RSS data, all in memory.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

#[allow(unused_imports)]
use crate::{debug, error, info, log, warn};

/// An item to be displayed in the timeline
#[derive(Debug, Clone)]
pub struct TimelineItem {
    pub item: rss::Item,
    pub channel_title: String,
    pub channel_url: String,
}

/// The main data store for feeds and articles
/// NOTE: This struct should not be manually instantiated, use the static DATA_STORE instead
#[derive(Debug, Default)]
pub struct DataStoreType {
    /// Timeline of article IDs by timestamp
    pub timeline: BTreeMap<i64, TimelineItem>,
}

/// The global data store instance
/// See `data_store`
static DATA_STORE: LazyLock<Arc<Mutex<DataStoreType>>> = LazyLock::new(Default::default);

/// Get the global data store instance safely
/// NOTE: This is locking, so prefer multiple calls over keeping the returned guard when multithreading
pub fn data_store<'a>() -> MutexGuard<'a, DataStoreType> {
    DATA_STORE.lock().unwrap()
}

/// Add a timeline item to the data store
/// NOTE: Prefer `add_channel_items` (or manually wrap items and provide timestamps).
pub fn add_timeline_item(timestamp: i64, item: TimelineItem) {
    let mut store = data_store();
    store.timeline.insert(timestamp, item);
}

/// Add all items from a Channel to the data store timeline
pub fn add_channel_items(channel: &rss::Channel) {
    for item in channel.items() {
        let parsed_timestamp = item
            .pub_date()
            .and_then(|date| chrono::DateTime::parse_from_rfc2822(date).ok())
            .map(|dt| dt.timestamp());

        let timestamp = parsed_timestamp.unwrap_or_else(|| {
            warn!(
                "Failed to parse timestamp for item '{}', using current time -1s as fallback",
                item.title().unwrap_or("(No title)")
            );
            chrono::Utc::now().timestamp().saturating_sub(1) // default to 1s ago
        });

        let timeline_item = TimelineItem {
            item: item.clone(),
            channel_title: channel.title().to_string(),
            channel_url: channel.link().to_string(),
        };

        // debug!("added item with timestamp {timestamp} to timeline");
        add_timeline_item(timestamp, timeline_item);
    }
}

thread_local! {
    /// The thread-local reused RNG instance
   static RNG: Mutex<&'static mut rand::rngs::ThreadRng> = Mutex::new(Box::leak(Box::new(rand::rng())));
}

/// Open an RSS channel to a feed via URL
pub fn open_rss_channel(feed_url: &str) -> Result<rss::Channel, rss::Error> {
    let response = reqwest::blocking::get(feed_url);
    if let Err(e) = response {
        error!("GET-request failed: {e}");
        error!("Exiting...");
        std::process::exit(1);
    }

    let text = response.unwrap().text();
    if let Err(e) = text {
        error!("Failed to read response text: {e}");
        error!("Exiting...");
        std::process::exit(1);
    }

    let text = text.unwrap();

    rss::Channel::read_from(text.as_bytes())
}

impl TimelineItem {
    /// Get the title of the item, or "(No title)"
    pub fn title(&self) -> String {
        self.item.title().unwrap_or("(No title)").into()
    }

    /// Get the description of the item, or "(No description)"
    pub fn description(&self) -> String {
        self.item.description().unwrap_or("(No description)").into()
    }

    /// Get the source of the item
    pub fn source(&self) -> String {
        self.channel_title.clone()
    }

    /// Get the link of the item, or an empty string
    pub fn link(&self) -> String {
        self.item.link().unwrap_or_default().into()
    }

    /// Get the date of the item, or an empty string
    pub fn date(&self) -> String {
        self.item
            .pub_date()
            .map(|d| Self::format_datetime(d, "%Y-%m-%d"))
            .unwrap_or_default()
    }

    /// Get the time of the item, or an empty string
    pub fn time(&self) -> String {
        self.item
            .pub_date()
            .map(|d| Self::format_datetime(d, "%H:%M:%S"))
            .unwrap_or_default()
    }

    /// Helper to format a RFC2822 datetime string
    fn format_datetime(datetime: &str, fmt: &str) -> String {
        match chrono::DateTime::parse_from_rfc2822(datetime) {
            Ok(dt) => dt.format(fmt).to_string(),
            Err(_) => {
                error!("Failed to parse RFC2822 datetime '{datetime}'");
                "(Invalid date)".into()
            }
        }
    }
}
