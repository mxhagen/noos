//! Management of application RSS data, all in memory.

use std::{
    path::Path,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

use opml::*;

#[allow(unused_imports)]
use crate::{debug, error, info, log, warn};

/// An item to be displayed in the timeline
#[derive(Debug, Clone)]
pub struct TimelineItem {
    pub item: rss::Item,
    pub channel_title: String,
    pub channel_url: String,
    pub timestamp: i64,
}

/// The main data store for feeds and articles
/// NOTE: This struct should not be manually instantiated, use the static DATA_STORE instead
#[derive(Debug, Default)]
pub struct DataStoreType {
    /// Timeline of article IDs by timestamp
    pub timeline: Vec<TimelineItem>,
}

/// The global data store instance
/// See `data_store`
static DATA_STORE: LazyLock<Arc<Mutex<DataStoreType>>> = LazyLock::new(Default::default);

/// Get the global data store instance safely
/// NOTE: This is locking, so prefer multiple calls over keeping the returned guard when multithreading
pub fn data_store<'a>() -> MutexGuard<'a, DataStoreType> {
    DATA_STORE.lock().unwrap()
}

/// Add all items from a Channel to the data store timeline
pub fn add_channel_items(channel: &rss::Channel) {
    let channel_name = channel.title();
    let (mut missing_ts_count, mut added_count) = (0, 0);

    for item in channel.items() {
        let parsed_timestamp = item
            .pub_date()
            .and_then(|date| chrono::DateTime::parse_from_rfc2822(date).ok())
            .map(|dt| dt.timestamp());

        let timestamp = parsed_timestamp.unwrap_or_else(|| {
            missing_ts_count += 1;
            chrono::Utc::now().timestamp().saturating_sub(60) // default to 1m ago
        });

        let timeline_item = TimelineItem {
            item: item.clone(),
            channel_title: channel.title().to_string(),
            channel_url: channel.link().to_string(),
            timestamp,
        };

        data_store().timeline.push(timeline_item);
        added_count += 1;
    }

    if missing_ts_count > 0 {
        warn!(
            "Failed to parse timestamp for {missing_ts_count} items from '{channel_name}', using 1m ago as fallback"
        );
    }

    debug!("added {added_count} items from {channel_name} to timeline");
}

thread_local! {
    /// The thread-local reused RNG instance
   static RNG: Mutex<&'static mut rand::rngs::ThreadRng> = Mutex::new(Box::leak(Box::new(rand::rng())));
}

/// Open an RSS channel to a feed via URL
pub fn open_rss_channel(feed_url: &str) -> Result<rss::Channel, String> {
    // TODO: Async requests, retries/timeout arguments?
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5)) // flat 5 second timeout for now
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(feed_url).send();
    if let Err(e) = response {
        error!("GET-request failed: {e}. Skipping channel '{feed_url}'...");
        return Err(e.to_string());
    }

    let text = response.unwrap().text();
    if let Err(e) = text {
        error!("Failed to read response text: {e}");
        error!("Exiting...");
        std::process::exit(1);
    }

    let text = text.unwrap();

    rss::Channel::read_from(text.as_bytes()).map_err(|e| e.to_string())
}

/// Open multiple RSS channels from a list of feed URLs with logging
/// Skipping any that fail to open
pub fn open_rss_channels(feed_urls: &[String]) -> Vec<rss::Channel> {
    feed_urls
        .iter()
        .flat_map(|url| match open_rss_channel(url) {
            Err(e) => {
                error!("Failed to open RSS channel at URL '{url}': {e}. Skipping channel...");
                None
            }
            Ok(c) => Some(c),
        })
        .collect::<Vec<_>>()
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

/// Import feed urls from a line-separated text file
pub fn import_channel_urls<P>(file_path: P) -> Result<Vec<String>, String>
where
    P: AsRef<Path>,
{
    let content = std::fs::read_to_string(file_path).map_err(|e| e.to_string())?;
    let urls: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Ok(urls)
}

/// Read URLs from the channels file in the config directory
/// Exits on failure
pub fn read_urls_from_config_channels_file() -> Vec<String> {
    let path = dirs::config_dir()
        .unwrap()
        .join("noos")
        .join("channels.txt");

    if !path.exists() {
        warn!(
            "Channels file '{}' does not exist. Creating an empty one...",
            path.display()
        );

        if let Err(e) = std::fs::create_dir_all(path.parent().unwrap())
            .and_then(|_| std::fs::File::create(&path))
        {
            error!(
                "Failed to create channels file in config directory '{}': {e}.",
                path.display()
            );
            std::process::exit(1);
        }
    }

    let contents = std::fs::read_to_string(&path);
    if let Err(e) = contents {
        error!("Failed to read URLs from file '{}': {e}.", path.display());
        std::process::exit(1);
    }

    match import_channel_urls(&path) {
        Ok(urls) => urls,
        Err(e) => {
            error!("Failed to import URLs from file '{}': {e}.", path.display());
            std::process::exit(1);
        }
    }
}

/// Export feed urls to a line-separated text file
pub fn export_channel_urls<P, S>(file_path: P, urls: &[S]) -> Result<(), String>
where
    P: AsRef<Path>,
    S: ToString,
{
    let content = urls.iter().map(S::to_string).collect::<Vec<_>>().join("\n");
    std::fs::write(file_path, content).map_err(|e| e.to_string())
}

/// Export feed urls to a line-separated text file in the config directory (with logging)
/// Exits on failure
pub fn export_channel_urls_to_config<S>(urls: &[S])
where
    S: ToString,
{
    let config_dir = match dirs::config_dir() {
        Some(dir) => dir.join("noos"),
        None => {
            error!("Fatal: Failed to get config directory");
            std::process::exit(1);
        }
    };

    let config_channels_file = config_dir.join("channels.txt");

    if config_channels_file.exists() {
        // Backup existing channels file to 'channels_{iso-date}.txt.bak'
        // Meaning we keep one backup per day
        let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let backup_path = config_dir.join(format!("channels_{now}.txt.bak"));

        if backup_path.exists() {
            warn!(
                "Backup file for today '{}' already exists, overwriting...",
                backup_path.display()
            );
        }

        if let Err(e) = std::fs::copy(&config_channels_file, &backup_path) {
            error!("Failed to backup existing channels file: {e}");
            std::process::exit(1);
        }

        warn!(
            "Channels already existed at '{}', original file was backed up to '{}'...",
            config_channels_file.display(),
            backup_path.display(),
        );
    }

    match export_channel_urls(config_channels_file, urls) {
        Ok(_) => info!(
            "Imported {} URLs to channels file from OPML file",
            urls.len()
        ),
        Err(e) => {
            error!("Failed to update channels file: {e}");
            std::process::exit(1);
        }
    }
}

/// Import urls of RSS channels from an OPML file (with logging)
/// Exits on failure
/// NOTE: this is a compatability option, prefer `import_channel_urls`
pub fn import_opml_channel_urls<P>(file_path: P) -> Vec<String>
where
    P: AsRef<Path>,
{
    info!(
        "Importing feeds from OPML file: '{}'",
        file_path.as_ref().display()
    );

    let file = std::fs::File::open(file_path).map_err(|e| e.to_string());

    let opml = file.and_then(|mut f| OPML::from_reader(&mut f).map_err(|e| e.to_string()));
    let opml = match opml {
        Ok(o) => o,
        Err(e) => {
            error!("Fatal: Failed to parse OPML file: {e}");
            std::process::exit(1);
        }
    };

    opml.body
        .outlines
        .into_iter()
        .filter_map(|outline| outline.xml_url)
        .collect()
}

/// Export RSS channels to an OPML file (with logging)
/// Exits on failure
/// NOTE: this is a compatability option, prefer `export_channel_urls`
pub fn export_opml<P>(file_path: P, channels: Vec<rss::Channel>)
where
    P: AsRef<Path>,
{
    let now = chrono::Utc::now().to_rfc2822();

    let outlines: Vec<Outline> = channels
        .into_iter()
        .map(|channel| Outline {
            text: channel.title().into(),
            title: Some(channel.title().into()),
            description: match channel.description() {
                "" => None,
                d => Some(d.into()),
            },
            xml_url: Some(channel.link().into()),
            created: Some(now.clone()),
            category: channel.categories().first().map(|cat| cat.name().into()),
            ..Default::default()
        })
        .collect();

    let opml = OPML {
        head: Some(Head {
            title: "Noos Exported Subscriptions".to_string().into(),
            date_created: Some(now.clone()),
            date_modified: Some(now.clone()),
            ..Default::default()
        }),
        body: Body { outlines },
        ..Default::default()
    };

    let file = std::fs::File::create(file_path).map_err(|e| e.to_string());

    let write_result = file.and_then(|mut f| opml.to_writer(&mut f).map_err(|e| e.to_string()));
    match write_result {
        Ok(_) => info!("Successfully exported URLs to OPML file"),
        Err(e) => {
            error!("Fatal: Failed to export OPML file: {e}");
            std::process::exit(1);
        }
    }
}
